//! Certify preserved drawing data and exact assembly translations.
use super::{
    errors::retained,
    geometry::{at, finite},
};
use crate::document::{Document, Point};
use std::collections::HashMap;

pub(super) fn preserve(
    before: &Document,
    after: &Document,
    affected: &[bool],
) -> Result<(), String> {
    if before.atoms.len() != after.atoms.len()
        || before.bonds != after.bonds
        || affected.len() != after.atoms.len()
    {
        return Err("Generated layout changed the molecular graph".into());
    }
    let mut restored = after.clone();
    for (i, (a, original)) in restored.atoms.iter_mut().zip(&before.atoms).enumerate() {
        if a.id != original.id || !*at(affected, i)? && a.position != original.position {
            return Err("Generated layout moved an unrelated atom or changed its identity".into());
        }
        a.position = original.position;
        if a != original {
            return Err("Generated layout changed an atom's chemistry or presentation".into());
        }
    }
    if &restored != before {
        return Err("Generated layout changed noncoordinate drawing data".into());
    }
    Ok(())
}

/// Independent equality proof for the strict assembly seam. Restore IDs and
/// positions in a detached comparison copy; require the exact common shift
/// first, so restoration cannot hide a geometry or default-style mutation.
pub(super) fn certify_translation(
    source: &Document,
    part: &Document,
    mapping: &HashMap<u64, u64>,
    shift: Point,
) -> Result<(), String> {
    if source.atoms.len() != part.atoms.len()
        || source.bonds.len() != part.bonds.len()
        || source.annotations.len() != part.annotations.len()
        || source.arrows.len() != part.arrows.len()
        || source.graphics.len() != part.graphics.len()
    {
        return Err("Prepared tile changed object counts".into());
    }
    let inverse: HashMap<_, _> = mapping.iter().map(|(&a, &b)| (b, a)).collect();
    if inverse.len() != mapping.len() {
        return Err("Prepared tile has an ambiguous ID remap".into());
    }
    let undo = |id: &mut u64| -> Result<(), String> {
        *id = *inverse.get(id).ok_or("Prepared tile has an unmapped ID")?;
        Ok(())
    };
    let check_shift = |old: Point, new: Point| -> Result<(), String> {
        finite(new)?;
        let error_x = (f64::from(new.x) - f64::from(old.x) - f64::from(shift.x)).abs();
        let error_y = (f64::from(new.y) - f64::from(old.y) - f64::from(shift.y)).abs();
        if error_x > 0.01 || error_y > 0.01 {
            return Err(retained(
                "Prepared tile loses local geometry at these finite coordinates",
            ));
        }
        Ok(())
    };
    let relative = |a: Point, b: Point, x: Point, y: Point| -> Result<(), String> {
        let dx = ((f64::from(y.x) - f64::from(x.x)) - (f64::from(b.x) - f64::from(a.x))).abs();
        let dy = ((f64::from(y.y) - f64::from(x.y)) - (f64::from(b.y) - f64::from(a.y))).abs();
        if dx > 0.01 || dy > 0.01 {
            return Err(retained(
                "Prepared tile loses local geometry at these finite coordinates",
            ));
        }
        Ok(())
    };
    // Checking only old+shift can hide cancellation even in f64. Independently
    // compare local bond/control/caption vectors, before restoration or repair.
    for bond in &source.bonds {
        let a = source
            .atom(bond.a)
            .ok_or("Missing original shape endpoint")?;
        let b = source
            .atom(bond.b)
            .ok_or("Missing original shape endpoint")?;
        let x = part
            .atom(*mapping.get(&bond.a).ok_or("Missing shape remap")?)
            .ok_or("Missing translated shape endpoint")?;
        let y = part
            .atom(*mapping.get(&bond.b).ok_or("Missing shape remap")?)
            .ok_or("Missing translated shape endpoint")?;
        relative(a.position, b.position, x.position, y.position)?;
    }
    for (old, new) in source.arrows.iter().zip(&part.arrows) {
        relative(old.start, old.end, new.start, new.end)?;
        if let Some((a, b)) = old.control.zip(new.control) {
            relative(old.start, a, new.start, b)?;
        }
        if let Some((a, b)) = old.cubic.zip(new.cubic) {
            for (a, b) in a.into_iter().zip(b) {
                relative(old.start, a, new.start, b)?;
            }
        }
    }
    for name in &source.molecule_names {
        let atom = *name.atoms.first().ok_or("Empty named shape")?;
        let old = source
            .annotations
            .iter()
            .find(|a| a.id == name.annotation)
            .ok_or("Missing named shape caption")?;
        let new_id = *mapping
            .get(&name.annotation)
            .ok_or("Missing named shape remap")?;
        let new = part
            .annotations
            .iter()
            .find(|a| a.id == new_id)
            .ok_or("Missing translated name caption")?;
        let a = source.atom(atom).ok_or("Missing named shape atom")?;
        let z = part
            .atom(*mapping.get(&atom).ok_or("Missing named atom remap")?)
            .ok_or("Missing translated named atom")?;
        relative(a.position, old.position, z.position, new.position)?;
    }
    let mut restored = part.clone();
    // Fields bound to `_` are compared verbatim by the final document equality.
    let Document {
        generic,
        canvas_theme: _,
        color_theme: _,
        custom_theme: _,
        drawing_style: _,
        page_layout: _,
        abbreviations,
        atom_labels: _,
        ring_fills,
        depth_appearance,
        version: _,
        atoms,
        bonds,
        annotations,
        molecule_names,
        arrows,
        graphics,
        groups,
        reactions,
        recent_colors: _,
    } = &mut restored;
    for (actual, old) in atoms.iter_mut().zip(&source.atoms) {
        check_shift(old.position, actual.position)?;
        if actual.position != old.position.offset(shift.x, shift.y) {
            return Err("Prepared atom lost its common translation".into());
        }
        undo(&mut actual.id)?;
        actual.position = old.position;
        for id in &mut actual.centroid {
            undo(id)?;
        }
        if let Some(stereo) = &mut actual.stereo {
            for id in &mut stereo.neighbors {
                undo(id)?;
            }
        }
    }
    for b in bonds {
        undo(&mut b.a)?;
        undo(&mut b.b)?;
        for id in &mut b.stereo_atoms {
            undo(id)?;
        }
    }
    for (actual, old) in annotations.iter_mut().zip(&source.annotations) {
        check_shift(old.position, actual.position)?;
        if actual.position != old.position.offset(shift.x, shift.y) {
            return Err("Prepared annotation lost its common translation".into());
        }
        undo(&mut actual.id)?;
        actual.position = old.position;
    }
    for (actual, old) in arrows.iter_mut().zip(&source.arrows) {
        check_shift(old.start, actual.start)?;
        check_shift(old.end, actual.end)?;
        if let Some((a, b)) = old.control.zip(actual.control) {
            check_shift(a, b)?;
        }
        if let Some((a, b)) = old.cubic.zip(actual.cubic) {
            for (a, b) in a.into_iter().zip(b) {
                check_shift(a, b)?;
            }
        }
        if actual.start != old.start.offset(shift.x, shift.y)
            || actual.end != old.end.offset(shift.x, shift.y)
            || actual.control != old.control.map(|p| p.offset(shift.x, shift.y))
            || actual.cubic != old.cubic.map(|v| v.map(|p| p.offset(shift.x, shift.y)))
        {
            return Err(
                "Prepared arrow changed its explicit controls or common translation".into(),
            );
        }
        for (new, prior) in [
            (&mut actual.start_anchor, &old.start_anchor),
            (&mut actual.end_anchor, &old.end_anchor),
        ] {
            match (new.as_mut(), prior.as_ref()) {
                (Some(new), Some(prior)) => {
                    // Validated bond targets have distinct atoms, and the
                    // injective mapping above keeps their remapped IDs distinct.
                    let mut target = prior.target.clone();
                    if !target.remap(mapping) {
                        return Err("Missing anchor remap".into());
                    }
                    if new.target != target
                        || new.offset != prior.offset
                        || new.direction != prior.direction
                        || new.gap != prior.gap
                    {
                        return Err("Prepared tile changed stable arrow attachment metadata".into());
                    }
                    new.target = prior.target.clone();
                }
                (None, None) => {}
                _ => return Err("Prepared tile changed arrow attachment presence".into()),
            }
        }
        undo(&mut actual.id)?;
        actual.start = old.start;
        actual.end = old.end;
        actual.control = old.control;
        actual.cubic = old.cubic;
    }
    for (actual, old) in graphics.iter_mut().zip(&source.graphics) {
        check_shift(old.origin, actual.origin)?;
        if actual.origin != old.origin.offset(shift.x, shift.y) {
            return Err("Prepared graphic lost its common translation".into());
        }
        undo(&mut actual.id)?;
        actual.origin = old.origin;
    }
    for g in groups {
        undo(&mut g.id)?;
        for id in &mut g.members {
            undo(id)?;
        }
    }
    for g in abbreviations {
        undo(&mut g.anchor)?;
        for id in &mut g.members {
            undo(id)?;
        }
    }
    for f in ring_fills {
        for id in &mut f.atoms {
            undo(id)?;
        }
    }
    for r in reactions {
        r.remap(&inverse).ok_or("Missing reaction remap")?;
    }
    for n in molecule_names {
        undo(&mut n.annotation)?;
        for id in &mut n.atoms {
            undo(id)?;
        }
    }
    *depth_appearance = crate::depth_appearance::remap(depth_appearance, &inverse);
    generic.remap(&inverse).ok_or("Missing generic remap")?;
    if &restored != source {
        return Err("Prepared tile changed noncoordinate native data".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
