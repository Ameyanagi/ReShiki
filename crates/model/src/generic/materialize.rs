//! Concrete fragment insertion and generic-site materializers.
use super::{
    Alternative, LinkNode, RepeatPattern, RepeatUnit, Work, bonds_at,
    definitions::pair,
    expansion::{Mode, Site},
    guards::reject_affected_stereo,
    variable,
};
use crate::document::{Document, Point};
use std::collections::{HashMap, HashSet};

/// Allocation is checked once before appending. No growing-document scans occur
/// inside a replication loop, and the reserved sentinel ID is never produced.
struct Ids {
    next: u64,
    remaining: usize,
}
impl Ids {
    fn new(doc: &Document, count: usize) -> Result<Self, String> {
        let maximum = doc
            .object_ids()
            .chain(doc.groups.iter().map(|g| g.id))
            .max()
            .unwrap_or(0);
        let next = maximum
            .checked_add(1)
            .ok_or("Generic object ID limit exceeded")?;
        let count64 = u64::try_from(count).map_err(|_| "Generic object ID limit exceeded")?;
        if next.checked_add(count64).is_none() {
            return Err("Generic object ID limit exceeded".into());
        }
        Ok(Self {
            next,
            remaining: count,
        })
    }
    fn take(&mut self) -> Result<u64, String> {
        if self.remaining == 0 || self.next == u64::MAX {
            return Err("Generic object ID limit exceeded".into());
        }
        let id = self.next;
        self.next += 1;
        self.remaining -= 1;
        Ok(id)
    }
}
fn ordinary_bond(a: u64, b: u64) -> crate::document::Bond {
    crate::document::Bond::plain(a, b, 1, "plain")
}
pub(super) fn clear_after_expansion(doc: &mut Document) {
    // The explicit-stereo guard has already checked every affected center and
    // bond. Cached labels need clearing, but unrelated captured stereo remains.
    crate::atom_labels::clear_computed(doc);
    for a in &mut doc.atoms {
        a.label_h = 0;
    }
}
pub(super) fn materialize(
    doc: &Document,
    site: &Site,
    index: usize,
    fragments: &HashMap<String, Document>,
    mode: Mode<'_>,
    work: &mut Work,
) -> Result<Document, String> {
    match site {
        Site::Alternative(s) => {
            let choice = s.choices.get(index).ok_or("Missing alternative choice")?;
            match choice {
                Alternative::Element { symbol, isotope } => {
                    reject_affected_stereo(doc, &[s.atom])?;
                    let mut result = doc.clone();
                    let a = result.atom_mut(s.atom).ok_or("Missing alternative atom")?;
                    a.element = symbol.clone();
                    a.isotope = *isotope;
                    a.no_implicit = false;
                    a.display.variable = None;
                    result.invalidate_chemistry(&[s.atom]);
                    Ok(result)
                }
                Alternative::Group { label } => {
                    insert_fragment(doc, s.atom, fragments.get(label).ok_or("Missing fragment")?)
                }
                Alternative::Unresolved { .. } => Err("Unresolved generic alternative".into()),
            }
        }
        Site::Variable(id, targets) => {
            let target = *targets.get(index).ok_or("Missing attachment target")?;
            let contacts = bonds_at(doc, *id);
            if contacts.len() != 1 || contacts.iter().any(|b| b.order != 1) {
                return Err(
                    "Finite variable attachments require one ordinary single connecting bond"
                        .into(),
                );
            }
            let contact = contacts.first().ok_or("Missing variable attachment bond")?;
            reject_affected_stereo(doc, &[*id, target, contact.a, contact.b])?;
            let mut result = doc.clone();
            if let Mode::Drawing(_) = mode {
                let root = if contact.a == *id {
                    contact.b
                } else {
                    contact.a
                };
                variable::pose_variable(&mut result, *id, target, root, work)?;
            }
            result.atoms.retain(|a| a.id != *id);
            for b in &mut result.bonds {
                if b.a == *id {
                    b.a = target;
                }
                if b.b == *id {
                    b.b = target;
                }
            }
            result.groups.retain(|g| !g.members.contains(id));
            result.abbreviations.retain(|g| !g.members.contains(id));
            crate::projection::sync_centroids(&mut result);
            result.invalidate_chemistry(&[target]);
            Ok(result)
        }
        Site::Link(s) => link(doc, s, s.min + index as u32),
        Site::Repeat(s) => {
            let (min, _) = s.count.range()?;
            repeat(doc, s, min + index as u32)
        }
    }
}
pub(super) fn group(doc: &Document, id: u64, label: &str) -> Result<Document, String> {
    let mut clean = doc.clone();
    clean
        .atom_mut(id)
        .ok_or("Missing alternative atom")?
        .display
        .variable = None;
    if label != "OPh" {
        let mut result = crate::chemistry::abbreviations::replace(&clean, &[id], label)
            .map_err(|e| e.to_string())?;
        result.expand_abbreviations(&[id]);
        return Ok(result);
    }
    let mut result = crate::chemistry::abbreviations::replace(&clean, &[id], "OMe")
        .map_err(|e| e.to_string())?;
    let members = result
        .abbreviation(id)
        .ok_or("Missing methoxy fragment")?
        .members
        .clone();
    let carbon = *members
        .iter()
        .find(|&&n| n != id)
        .ok_or("Missing methoxy carbon")?;
    result.expand_abbreviations(&[id]);
    result = crate::chemistry::abbreviations::replace(&result, &[carbon], "Ph")
        .map_err(|e| e.to_string())?;
    result.expand_abbreviations(&[carbon]);
    Ok(result)
}
/// A concrete catalog alternative, used by expansion and editable producers.
pub fn alternative_fragment(choice: &Alternative) -> Result<Document, String> {
    let mut doc = Document::default();
    let id = doc.add_atom("*", Point::default());
    match choice {
        Alternative::Group { label } => group(&doc, id, label),
        Alternative::Element { symbol, isotope } => {
            let a = doc.atom_mut(id).ok_or("Missing fragment root")?;
            a.element = symbol.clone();
            a.isotope = *isotope;
            a.no_implicit = false;
            Ok(doc)
        }
        Alternative::Unresolved { .. } => {
            Err("An unresolved nickname has no concrete alternative fragment".into())
        }
    }
}
fn insert_fragment(doc: &Document, id: u64, fragment: &Document) -> Result<Document, String> {
    reject_affected_stereo(doc, &[id])?;
    let anchor = doc.atom(id).ok_or("Missing alternative atom")?;
    let contacts = bonds_at(doc, id);
    if contacts.len() > 1 || contacts.iter().any(|b| b.order != 1) {
        return Err(
            "A group alternative requires one single attachment bond or an isolated atom".into(),
        );
    }
    let root = fragment.atoms.first().ok_or("Empty alternative fragment")?;
    // Cached fragments use the same existing template framework, pointing away
    // from an isolated site's default outside direction. Rotate that geometry
    // to the drawing's explicit attachment and scale by its bond length.
    let (angle, scale) = if let Some(b) = contacts.first() {
        let outside = doc
            .atom(if b.a == id { b.b } else { b.a })
            .ok_or("Missing group endpoint")?;
        let delta = Point::new(
            outside.position.x - anchor.position.x,
            outside.position.y - anchor.position.y,
        );
        let length = delta.distance(Point::default());
        if length < 0.1 {
            return Err("The group attachment bond has invalid geometry".into());
        }
        (
            (delta.y as f64).atan2(delta.x as f64) - std::f64::consts::PI,
            length as f64 / 42.,
        )
    } else {
        (0., 1.)
    };
    let (sin, cos) = angle.sin_cos();
    let mut allocator = Ids::new(doc, fragment.atoms.len().saturating_sub(1))?;
    let mut ids = HashMap::new();
    let mut atoms = Vec::with_capacity(fragment.atoms.len());
    for a in &fragment.atoms {
        let mut a = a.clone();
        let old = a.id;
        a.id = if old == root.id {
            id
        } else {
            allocator.take()?
        };
        let x = (a.position.x - root.position.x) as f64 * scale;
        let y = (a.position.y - root.position.y) as f64 * scale;
        a.position = anchor
            .position
            .offset((x * cos - y * sin) as f32, (x * sin + y * cos) as f32);
        if old == root.id {
            a.display = anchor.display.clone();
            a.display.variable = None;
            a.text_style = anchor.text_style.clone();
            a.map_num = anchor.map_num;
            a.depth = anchor.depth;
            a.marks = anchor.marks.clone();
        }
        ids.insert(old, a.id);
        atoms.push(a);
    }
    let members: Vec<_> = atoms.iter().map(|a| a.id).collect();
    let mut result = doc.clone();
    result.atoms.retain(|a| a.id != id);
    result.atoms.extend(atoms);
    for b in &fragment.bonds {
        let mut b = b.clone();
        b.a = *ids.get(&b.a).ok_or("Missing fragment bond")?;
        b.b = *ids.get(&b.b).ok_or("Missing fragment bond")?;
        result.bonds.push(b);
    }
    for g in &mut result.groups {
        if g.members.contains(&id) {
            g.members.retain(|n| *n != id);
            g.members.extend(&members);
        }
    }
    for reaction in &mut result.reactions {
        for role in [
            &mut reaction.reactants,
            &mut reaction.products,
            &mut reaction.agents,
        ] {
            for participant in role {
                if participant.atoms.contains(&id) {
                    participant.atoms.retain(|n| *n != id);
                    participant.atoms.extend(&members);
                }
            }
        }
    }
    result.abbreviations.retain(|g| !g.members.contains(&id));
    crate::ring_fills::prune(&mut result);
    clear_after_expansion(&mut result);
    Ok(result)
}
pub(super) fn link(doc: &Document, node: &LinkNode, count: u32) -> Result<Document, String> {
    let atom = doc.atom(node.atom).ok_or("Missing link node")?;
    if atom.element == "*" {
        return Err("A generic link-node atom needs a concrete element before expansion".into());
    }
    let contacts = bonds_at(doc, node.atom);
    let [head, tail] = contacts.as_slice() else {
        return Err("A link node requires exactly two bonds".into());
    };
    if head.order != 1 || tail.order != 1 || atom.stereo.is_some() {
        return Err(
            "Link expansion supports ordinary single bonds without specified atom stereochemistry"
                .into(),
        );
    }
    let outside = if tail.a == node.atom { tail.b } else { tail.a };
    reject_affected_stereo(doc, &[node.atom, outside])?;
    let finish = doc.atom(outside).ok_or("Missing link endpoint")?.position;
    let mut result = doc.clone();
    result
        .atom_mut(node.atom)
        .ok_or("Missing link atom")?
        .display
        .variable = None;
    if count == 1 {
        return Ok(result);
    }
    result
        .bonds
        .retain(|b| pair(b.a, b.b) != pair(node.atom, outside));
    let mut previous = node.atom;
    let mut allocator = Ids::new(doc, count.saturating_sub(1) as usize)?;
    let mut atoms = Vec::with_capacity(count.saturating_sub(1) as usize);
    let mut bonds = Vec::with_capacity(count as usize);
    for i in 1..count {
        let mut copy = atom.clone();
        copy.id = allocator.take()?;
        copy.display.variable = None;
        copy.cip_label = None;
        copy.stereo = None;
        let ratio = i as f32 / count as f32;
        copy.position = Point::new(
            atom.position.x + (finish.x - atom.position.x) * ratio,
            atom.position.y + (finish.y - atom.position.y) * ratio,
        );
        let id = copy.id;
        atoms.push(copy);
        bonds.push(ordinary_bond(previous, id));
        previous = id;
    }
    let mut end = (*tail).clone();
    if end.a == node.atom {
        end.a = previous
    } else {
        end.b = previous
    }
    bonds.push(end);
    result.atoms.extend(atoms);
    result.bonds.extend(bonds);
    clear_after_expansion(&mut result);
    Ok(result)
}
pub(super) fn repeat(doc: &Document, unit: &RepeatUnit, count: u32) -> Result<Document, String> {
    if !matches!(unit.usage.as_str(), "SRU" | "Generic")
        || unit.pattern != RepeatPattern::HeadToTail
        || !matches!(unit.flip.as_str(), "" | "NoFlip" | "Unspecified")
    {
        return Err("This polymer usage, repeat pattern or flip type is retained but cannot be finitely expanded".into());
    }
    let [head, tail] = unit.crossings.as_slice() else {
        return Err("Finite polymer expansion requires exactly two ordered crossing bonds; ladder polymers remain generic".into());
    };
    if unit.head_count != Some(1) {
        return Err(
            "Finite polymer expansion requires explicitly ordered head and tail ports".into(),
        );
    }
    let members: HashSet<_> = unit.members.iter().copied().collect();
    let atom_index: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a)).collect();
    if unit.members.iter().any(|id| {
        atom_index
            .get(id)
            .is_none_or(|a| a.attachment.is_some() || a.stereo.is_some())
    }) {
        return Err("Finite polymer expansion requires an ordinary repeat unit without specified atom stereochemistry".into());
    }
    let head_bond = doc
        .bonds
        .iter()
        .find(|b| pair(b.a, b.b) == pair(head.inside, head.outside))
        .ok_or("Missing head crossing bond")?;
    let tail_bond = doc
        .bonds
        .iter()
        .find(|b| pair(b.a, b.b) == pair(tail.inside, tail.outside))
        .ok_or("Missing tail crossing bond")?;
    if head_bond.order != 1 || tail_bond.order != 1 {
        return Err("Finite polymer expansion requires single crossing bonds".into());
    }
    let mut affected = unit.members.clone();
    affected.extend(unit.crossings.iter().map(|c| c.outside));
    reject_affected_stereo(doc, &affected)?;
    // Overlapping/nested definitions and disconnected unit graphs cannot infer
    // a head/tail chain from the bracket's appearance.
    let reached = component(doc, head.inside, &members);
    if reached.len() != members.len() {
        return Err("Repeating-unit atoms must form one connected fragment".into());
    }
    let mut result = doc.clone();
    if let Some(id) = unit.bracket {
        result.graphics.retain(|g| g.id != id);
        result.groups.retain(|g| !g.members.contains(&id));
    }
    result
        .graphics
        .retain(|g| !unit.bracket_objects.contains(&g.id));
    result
        .groups
        .retain(|g| !g.members.iter().any(|id| unit.bracket_objects.contains(id)));
    if count == 1 {
        return Ok(result);
    }
    let head_pos = doc.atom(head.inside).ok_or("Missing head atom")?.position;
    let end_pos = doc.atom(tail.outside).ok_or("Missing tail atom")?.position;
    let shift = Point::new(end_pos.x - head_pos.x, end_pos.y - head_pos.y);
    if shift.distance(Point::default()) < 0.01 {
        return Err("The repeating unit has no usable head-to-tail drawing direction".into());
    }
    result
        .bonds
        .retain(|b| pair(b.a, b.b) != pair(tail.inside, tail.outside));
    let mut previous = tail.inside;
    let extra_atoms = unit
        .members
        .len()
        .checked_mul(count.saturating_sub(1) as usize)
        .ok_or("Generic atom count overflow")?;
    let mut allocator = Ids::new(doc, extra_atoms)?;
    let mut atoms = Vec::with_capacity(extra_atoms);
    let internal: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| members.contains(&b.a) && members.contains(&b.b))
        .collect();
    let mut bonds = Vec::with_capacity((internal.len() + 1).saturating_mul(count as usize));
    for repeat_index in 1..count {
        let mut ids = HashMap::new();
        for &old in &unit.members {
            let mut a = (**atom_index.get(&old).ok_or("Missing repeat member")?).clone();
            a.id = allocator.take()?;
            a.position = a
                .position
                .offset(shift.x * repeat_index as f32, shift.y * repeat_index as f32);
            a.cip_label = None;
            a.stereo = None;
            ids.insert(old, a.id);
            atoms.push(a);
        }
        for b in &internal {
            let mut b = (**b).clone();
            b.a = *ids.get(&b.a).ok_or("Missing repeat bond")?;
            b.b = *ids.get(&b.b).ok_or("Missing repeat bond")?;
            b.stereo = None;
            b.stereo_atoms.clear();
            b.cip_label = None;
            bonds.push(b);
        }
        let first = *ids.get(&head.inside).ok_or("Missing repeat head")?;
        bonds.push(ordinary_bond(previous, first));
        previous = *ids.get(&tail.inside).ok_or("Missing repeat tail")?;
    }
    result.atoms.extend(atoms);
    result.bonds.extend(bonds);
    let outside: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| !members.contains(&a.id))
        .map(|a| a.id)
        .collect();
    let tail_component = component(doc, tail.outside, &outside);
    if !tail_component.contains(&head.outside) {
        for a in &mut result.atoms {
            if tail_component.contains(&a.id) {
                a.position = a
                    .position
                    .offset(shift.x * (count - 1) as f32, shift.y * (count - 1) as f32);
            }
        }
    }
    let mut end = tail_bond.clone();
    if end.a == tail.inside {
        end.a = previous
    } else {
        end.b = previous
    }
    result.bonds.push(end);
    clear_after_expansion(&mut result);
    Ok(result)
}
fn component(doc: &Document, start: u64, allowed: &HashSet<u64>) -> HashSet<u64> {
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    for b in &doc.bonds {
        if allowed.contains(&b.a) && allowed.contains(&b.b) {
            adjacent.entry(b.a).or_default().push(b.b);
            adjacent.entry(b.b).or_default().push(b.a);
        }
    }
    let mut pending = vec![start];
    let mut found = HashSet::new();
    while let Some(id) = pending.pop() {
        if allowed.contains(&id) && found.insert(id) {
            pending.extend(adjacent.get(&id).into_iter().flatten().copied());
        }
    }
    found
}
