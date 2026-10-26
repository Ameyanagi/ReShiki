//! #495/#545 (5ec483aed, 61a5b5aed): imported reactions use final label ink,
//! rather than the frozen worker's 36-world-unit participant padding.
//!
//! The frozen document is the sole source of chemistry and internal geometry.
//! Predict only the specified horizontal placement, then compare the complete
//! response normally. Never align to actual coordinates or remove coordinates.
use anyhow::{Context, ensure};
use reshiki_model::{document::Document, reactions::Participant};
use serde_json::Value;

/// Call only with the original request, before the ordinary strict comparison.
#[allow(dead_code)] // The direct drawing target has an explicit RXN boundary.
pub fn adapt_import(
    operation: &str,
    format: Option<&str>,
    actual: &Value,
    expected: &mut Value,
) -> anyhow::Result<()> {
    if operation == "import" && matches!(format, Some("rxn" | "rsmi")) {
        adapt_document(&actual["document"], &mut expected["document"])?;
    }
    Ok(())
}

pub fn adapt_document(actual: &Value, expected: &mut Value) -> anyhow::Result<()> {
    let source: Document = serde_json::from_value(expected.clone())?;
    ensure!(
        source.reactions.len() == 1,
        "[reaction-ink] Expected one imported reaction"
    );
    let projected = project(source)?;
    // Patch only the documented x fields. Keeping the original JSON preserves
    // even fields not represented by the typed geometry used for measurement.
    for (index, atom) in projected.atoms.iter().enumerate() {
        expected["atoms"][index]["position"]["x"] = serde_json::to_value(atom.position.x)?;
    }
    let reaction = &projected.reactions[0];
    for (index, arrow) in projected.arrows.iter().enumerate() {
        if arrow.id == reaction.arrow {
            expected["arrows"][index]["start"]["x"] = serde_json::to_value(arrow.start.x)?;
            expected["arrows"][index]["end"]["x"] = serde_json::to_value(arrow.end.x)?;
        }
    }
    for (index, sign) in projected.annotations.iter().enumerate() {
        if reaction.annotations.contains(&sign.id) {
            expected["annotations"][index]["position"]["x"] =
                serde_json::to_value(sign.position.x)?;
        }
    }
    // Independently check the resulting rendered spacing, in addition to exact
    // equality against the projected frozen document at the caller.
    check_spacing(&serde_json::from_value(actual.clone())?)?;
    static REPORTED: std::sync::Once = std::sync::Once::new();
    REPORTED.call_once(|| eprintln!(
        "Reaction presentation contract: #495/#545 / 5ec483aed + 61a5b5aed; frozen participant geometry, predicted x placement, strict remaining fields"
    ));
    Ok(())
}

fn extent(doc: &Document, part: &Participant) -> anyhow::Result<(f32, f32)> {
    let points = part
        .atoms
        .iter()
        .map(|id| {
            doc.atom(*id)
                .map(|atom| atom.position.x)
                .context("[reaction-ink] Missing participant atom")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let (lo, hi) = points
        .into_iter()
        .chain(
            reshiki_model::scene::participant_label_ink_boxes(doc, &part.atoms)
                .into_iter()
                .flat_map(|(lo, hi)| [lo.x, hi.x]),
        )
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), x| {
            (lo.min(x), hi.max(x))
        });
    ensure!(
        lo.is_finite() && hi.is_finite(),
        "[reaction-ink] Empty or nonfinite participant"
    );
    Ok((lo, hi))
}

/// Independent horizontal projection from the legacy reference. Row origins,
/// label widths, separator gaps, arrow margins and agent centering are explicit
/// contract values; the production reaction assembler is never invoked.
fn project(mut doc: Document) -> anyhow::Result<Document> {
    let reaction = doc.reactions[0].clone();
    let mut signs = reaction.annotations.iter();
    let mut row = |doc: &mut Document, parts: &[Participant], origin: f32| -> anyhow::Result<f32> {
        let mut right = origin;
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                let id = signs
                    .next()
                    .context("[reaction-ink] Missing separator identity")?;
                let gap = doc.drawing_style.world(6.);
                let sign = doc
                    .annotations
                    .iter_mut()
                    .find(|a| a.id == *id)
                    .context("[reaction-ink] Missing separator")?;
                ensure!(sign.text == "+", "[reaction-ink] Nonseparator annotation");
                sign.position.x = right + gap;
                right +=
                    gap * 2. + reshiki_model::typography::layout(&sign.text, &sign.format).width;
            }
            let (left, old_right) = extent(doc, part)?;
            let translation = right - left;
            for atom in doc
                .atoms
                .iter_mut()
                .filter(|atom| part.atoms.contains(&atom.id))
            {
                atom.position.x += translation;
            }
            right += old_right - left;
        }
        Ok(right)
    };
    let reactants_right = row(&mut doc, &reaction.reactants, 0.)?;
    let agents_width = row(&mut doc, &reaction.agents, 0.)?;
    let arrow_left = reactants_right + 42.;
    let arrow_width = 140_f32.max(agents_width + 42.);
    let agents_left = arrow_left + (arrow_width - agents_width) * 0.5;
    for atom in doc
        .atoms
        .iter_mut()
        .filter(|atom| reaction.agents.iter().any(|p| p.atoms.contains(&atom.id)))
    {
        atom.position.x += agents_left;
    }
    for id in reaction
        .annotations
        .iter()
        .skip(reaction.reactants.len().saturating_sub(1))
        .take(reaction.agents.len().saturating_sub(1))
    {
        doc.annotations
            .iter_mut()
            .find(|a| a.id == *id)
            .context("[reaction-ink] Missing agent separator")?
            .position
            .x += agents_left;
    }
    row(&mut doc, &reaction.products, arrow_left + arrow_width + 42.)?;
    ensure!(
        signs.next().is_none(),
        "[reaction-ink] Unexpected separator identity"
    );
    let arrow = doc
        .arrows
        .iter_mut()
        .find(|a| a.id == reaction.arrow)
        .context("[reaction-ink] Missing reaction arrow")?;
    arrow.start.x = arrow_left;
    arrow.end.x = arrow_left + arrow_width;
    Ok(doc)
}

pub(super) fn check_spacing(doc: &Document) -> anyhow::Result<()> {
    ensure!(
        doc.reactions.len() == 1,
        "[reaction-ink] Actual reaction count changed"
    );
    let reaction = &doc.reactions[0];
    let arrow = doc
        .arrows
        .iter()
        .find(|a| a.id == reaction.arrow)
        .context("[reaction-ink] Actual arrow missing")?;
    // Only for rendered-spacing assertions, not the strict JSON comparison.
    let epsilon = doc
        .atoms
        .iter()
        .map(|a| a.position.x.abs())
        .fold(1_f32, f32::max)
        * f32::EPSILON
        * 16.
        + 0.001;
    let near = |a: f32, b: f32| -> anyhow::Result<()> {
        ensure!(
            (a - b).abs() <= epsilon,
            "[reaction-ink] Spacing {a} != {b}"
        );
        Ok(())
    };
    let mut signs = reaction.annotations.iter();
    let mut rows = Vec::new();
    for parts in [&reaction.reactants, &reaction.agents, &reaction.products] {
        let bounds = parts
            .iter()
            .map(|part| extent(doc, part))
            .collect::<anyhow::Result<Vec<_>>>()?;
        for pair in bounds.windows(2) {
            let id = signs
                .next()
                .context("[reaction-ink] Actual separator missing")?;
            let sign = doc
                .annotations
                .iter()
                .find(|a| a.id == *id)
                .context("[reaction-ink] Actual sign missing")?;
            let width = reshiki_model::typography::layout(&sign.text, &sign.format).width;
            let gap = doc.drawing_style.world(6.);
            near(sign.position.x - pair[0].1, gap)?;
            near(pair[1].0 - (sign.position.x + width), gap)?;
            near(sign.position.x + width * 0.5, (pair[0].1 + pair[1].0) * 0.5)?;
        }
        rows.push(
            bounds
                .first()
                .zip(bounds.last())
                .map(|(first, last)| (first.0, last.1)),
        );
    }
    if let Some((left, right)) = rows[0] {
        near(left, 0.)?;
        near(arrow.start.x - right, 42.)?;
    } else {
        near(arrow.start.x, 42.)?;
    }
    if let Some((left, right)) = rows[1] {
        near((left + right) * 0.5, (arrow.start.x + arrow.end.x) * 0.5)?;
        near(arrow.end.x - arrow.start.x, 140_f32.max(right - left + 42.))?;
    } else {
        near(arrow.end.x - arrow.start.x, 140.)?;
    }
    if let Some((left, _)) = rows[2] {
        near(left - arrow.end.x, 42.)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki_model::{
        document::{Annotation, Arrow, Point},
        reactions::Reaction,
    };
    use serde_json::json;

    fn fixture() -> Document {
        let mut doc = Document::default();
        let atoms: Vec<_> = (0..7)
            .map(|i| {
                doc.add_atom(
                    if i % 2 == 0 { "Cl" } else { "N" },
                    Point::new(36. + i as f32 * 120., i as f32 * 3.),
                )
            })
            .collect();
        doc.add_bond(atoms[0], atoms[1], 1, "plain");
        doc.atoms[0].cip_label = Some("S".into());
        let arrow = 100;
        doc.arrows.push(Arrow::new(
            arrow,
            Point::new(500., 0.),
            Point::new(640., 0.),
            Default::default(),
            Default::default(),
        ));
        let part = |ids: &[u64]| Participant {
            atoms: ids.to_vec(),
            coefficient: 1,
        };
        let mut reaction = Reaction::new(arrow);
        reaction.reactants = vec![part(&atoms[..2]), part(&atoms[2..3])];
        reaction.agents = vec![part(&atoms[3..4]), part(&atoms[4..5])];
        reaction.products = vec![part(&atoms[5..6]), part(&atoms[6..])];
        for id in [101, 102, 103] {
            doc.annotations.push(Annotation {
                id,
                text: "+".into(),
                position: Point::new(id as f32, 9.),
                format: Default::default(),
            });
            reaction.annotations.push(id);
        }
        doc.annotations.push(Annotation {
            id: 104,
            text: "conditions".into(),
            position: Point::new(75., -50.),
            format: Default::default(),
        });
        doc.reactions.push(reaction);
        doc
    }

    fn responses() -> (Value, Value) {
        let source = fixture();
        let actual = json!({"document": project(source.clone()).unwrap(), "analysis": {"smiles": "strict", "inchikey": "oracle"}});
        let expected =
            json!({"document": source, "analysis": {"smiles": "strict", "inchikey": "oracle"}});
        (actual, expected)
    }

    fn accepts(actual: &Value, expected: &Value) -> bool {
        let mut expected = expected.clone();
        adapt_import("import", Some("rxn"), actual, &mut expected).is_ok() && *actual == expected
    }

    #[test]
    fn reaction_projection_preserves_all_non_x_fields_and_internal_geometry() {
        let (actual, expected) = responses();
        assert!(accepts(&actual, &expected));
        let before: Document = serde_json::from_value(expected["document"].clone()).unwrap();
        let after: Document = serde_json::from_value(actual["document"].clone()).unwrap();
        for part in &before.reactions[0].reactants {
            for ids in part.atoms.windows(2) {
                let b = before.atom(ids[1]).unwrap().position.x
                    - before.atom(ids[0]).unwrap().position.x;
                let a =
                    after.atom(ids[1]).unwrap().position.x - after.atom(ids[0]).unwrap().position.x;
                assert!((a - b).abs() < 0.001);
            }
        }
        check_spacing(&after).unwrap();
        assert_eq!(
            actual["document"]["annotations"][3],
            expected["document"]["annotations"][3]
        );
    }

    #[test]
    fn reaction_projection_rejects_unpredicted_coordinates_and_chemistry() {
        let (actual, expected) = responses();
        for pointer in [
            "/document/atoms/0/position/x",
            "/document/atoms/0/position/y",
            "/document/annotations/0/position/x",
            "/document/arrows/0/start/x",
            "/document/arrows/0/end/x",
            "/document/atoms/0/id",
            "/document/bonds/0/order",
            "/document/reactions/0/reactants/0/coefficient",
        ] {
            let mut changed = actual.clone();
            let field = changed.pointer_mut(pointer).unwrap();
            *field = if let Some(value) = field.as_u64() {
                json!(value + 1)
            } else {
                json!(field.as_f64().unwrap() + 1.)
            };
            assert!(!accepts(&changed, &expected), "accepted mutation {pointer}");
        }
        for pointer in [
            "/document/atoms/0/element",
            "/analysis/smiles",
            "/analysis/inchikey",
        ] {
            let mut changed = actual.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("changed");
            assert!(!accepts(&changed, &expected), "accepted mutation {pointer}");
        }
        let mut changed_cip = actual.clone();
        changed_cip["document"]["atoms"][0]["cip_label"] = json!("R");
        let changed: Document = serde_json::from_value(changed_cip["document"].clone()).unwrap();
        changed.validate().unwrap();
        check_spacing(&changed).unwrap();
        assert!(!accepts(&changed_cip, &expected));
        let mut translated = actual.clone();
        for index in [0, 1] {
            let field = &mut translated["document"]["atoms"][index]["position"]["x"];
            *field = json!(field.as_f64().unwrap() + 10.);
        }
        assert!(
            !accepts(&translated, &expected),
            "accepted arbitrary participant translation"
        );
        let mut swapped = actual.clone();
        swapped["document"]["reactions"][0]["reactants"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert!(
            !accepts(&swapped, &expected),
            "accepted participant reordering"
        );
    }

    #[test]
    fn reaction_projection_requires_explicit_import_format() {
        let (actual, expected) = responses();
        for (operation, format) in [
            ("analyze", Some("rxn")),
            ("export", Some("rsmi")),
            ("import", Some("smiles")),
            ("import", None),
        ] {
            let mut unchanged = expected.clone();
            adapt_import(operation, format, &actual, &mut unchanged).unwrap();
            assert_eq!(unchanged, expected);
            assert_ne!(actual, unchanged);
        }
    }

    #[test]
    fn reaction_projection_includes_owned_map_and_stereo_ink() {
        let mut source = fixture();
        source.atom_labels.maps = true;
        source.atom_labels.stereo = true;
        source.atoms[0].map_num = 123456;
        source.atoms[0].cip_label = Some("R".into());
        // This contract projects rigid participant ink. Automatic indicators
        // can move around the fixture's fixed captions when an atom translates.
        source.atoms[0].display.mapping.offset = Some(Point::new(-80., -80.));
        source.atoms[0].display.stereo.offset = Some(Point::new(200., -80.));
        let part = &source.reactions[0].reactants[0];
        let mut visible = source.clone();
        visible.atom_labels.maps = false;
        visible.atom_labels.stereo = false;
        let atom_labels = extent(&visible, part).unwrap();
        visible.atom_labels.maps = true;
        assert!(
            extent(&visible, part).unwrap().0 < atom_labels.0,
            "map ink must extend the participant's left edge"
        );
        visible.atom_labels.maps = false;
        visible.atom_labels.stereo = true;
        assert!(
            extent(&visible, part).unwrap().1 > atom_labels.1,
            "stereo ink must extend the participant's right edge"
        );
        let projected = project(source).unwrap();
        check_spacing(&projected).unwrap();
        let reaction = &projected.reactions[0];
        let left = extent(&projected, &reaction.reactants[0]).unwrap();
        assert!(left.1 > left.0);
        assert_eq!(projected.atoms[0].map_num, 123456);
        assert_eq!(projected.atoms[0].cip_label.as_deref(), Some("R"));
    }

    #[test]
    fn rendered_spacing_independently_rejects_sign_arrow_and_agent_errors() {
        let correct = project(fixture()).unwrap();
        let mut sign = correct.clone();
        sign.annotations[0].position.x += 2.;
        assert!(check_spacing(&sign).is_err());
        let mut arrow = correct.clone();
        arrow.arrows[0].end.x += 2.;
        assert!(check_spacing(&arrow).is_err());
        let mut agents = correct;
        let ids = agents.reactions[0]
            .agents
            .iter()
            .flat_map(|part| part.atoms.iter().copied())
            .collect::<Vec<_>>();
        for atom in agents.atoms.iter_mut().filter(|a| ids.contains(&a.id)) {
            atom.position.x += 2.;
        }
        agents.annotations[1].position.x += 2.;
        assert!(check_spacing(&agents).is_err());
    }

    #[test]
    fn reaction_projection_keeps_valid_stereo_winding_strict() {
        use reshiki_model::document::AtomStereo;
        let mut source = fixture();
        let centre = source.atoms[0].id;
        source.atoms[0].element = "C".into();
        let mut neighbors = vec![source.atoms[1].id];
        for (element, point) in [
            ("F", Point::new(0., 42.)),
            ("Br", Point::new(36., -42.)),
            ("I", Point::new(-42., -42.)),
        ] {
            let id = source.add_atom(element, point);
            source.add_bond(centre, id, 1, "plain");
            source.reactions[0].reactants[0].atoms.push(id);
            neighbors.push(id);
        }
        source.atoms[0].stereo = Some(AtomStereo {
            winding: "cw".into(),
            neighbors,
        });
        source.validate().unwrap();
        let expected = json!({"document": source});
        let source: Document = serde_json::from_value(expected["document"].clone()).unwrap();
        let mut actual = json!({"document": project(source).unwrap()});
        assert!(accepts(&actual, &expected));
        actual["document"]["atoms"][0]["stereo"]["winding"] = json!("ccw");
        let changed: Document = serde_json::from_value(actual["document"].clone()).unwrap();
        changed.validate().unwrap();
        check_spacing(&changed).unwrap();
        assert!(!accepts(&actual, &expected));
    }

    #[test]
    fn reaction_projection_grows_arrow_for_wide_agent_row() {
        let mut source = fixture();
        let agent = source.reactions[0].agents[0].atoms[0];
        let id = source.add_atom("F", Point::new(1300., 80.));
        source.add_bond(agent, id, 1, "plain");
        source.reactions[0].agents[0].atoms.push(id);
        let after = project(source).unwrap();
        assert!(after.arrows[0].end.x - after.arrows[0].start.x > 140.);
        check_spacing(&after).unwrap();
    }
}
