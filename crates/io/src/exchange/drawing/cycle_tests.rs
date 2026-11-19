use super::*;
use crate::{chemistry::cdxml, document::Annotation, typography::TextFormat};

/// Three text intermediates made into a cycle, the first step named.
fn cycle_document() -> Document {
    let mut document = Document::default();
    let mut ids = vec![];
    for (x, y, text) in [
        (0., -200., "Pd(0)L2"),
        (170., 90., "ArPdX"),
        (-230., 90., "ArPdAr"),
    ] {
        let id = document.next_id();
        document.annotations.push(Annotation {
            id,
            position: Point::new(x, y),
            text: text.into(),
            format: TextFormat {
                style: document.drawing_style.text_style(),
                ..Default::default()
            },
        });
        ids.push(id);
    }
    let (cycle, _) = crate::cycles::make(&mut document, &ids).unwrap();
    if let Some(c) = document
        .graphics
        .iter_mut()
        .find(|g| g.id == cycle)
        .and_then(|g| g.cycle.as_deref_mut())
    {
        c.steps[0].name = "Oxidative addition".into();
    }
    // A reagent coming in (a headless curve) and a product going out.
    for (role, text, x, y) in [
        (crate::cycles::Role::Reagent(0), "ArX", 250., -200.),
        (crate::cycles::Role::Product(2), "ArAr", -250., -200.),
    ] {
        let id = document.next_id();
        document.annotations.push(Annotation {
            id,
            position: Point::new(x, y),
            text: text.into(),
            format: TextFormat {
                style: document.drawing_style.text_style(),
                ..Default::default()
            },
        });
        let ink = crate::scene::selection_bounds(&document, &[id]).unwrap();
        crate::cycles::assign(
            &mut document,
            &crate::cycles::Target {
                cycle,
                role,
                part: vec![id],
                ink,
            },
        );
    }
    document.validate().unwrap();
    document
}

#[test]
fn a_cycle_exports_as_its_arrows_and_text_grouped_with_its_parts() {
    let document = cycle_document();
    let request = Request::molecule("export", document.clone());
    let xml = write(&document, (&request).into()).unwrap();
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &decoded] {
        let tree = roxmltree::Document::parse(xml).unwrap();
        // No oval for the circle itself.
        assert!(
            !tree
                .descendants()
                .any(|n| n.has_tag_name("graphic") && n.attribute("GraphicType") == Some("Oval"))
        );
        let imported = cdxml::import_cdxml(xml).unwrap().document;
        // Three arc arrows and the product's; the reagent's headless curve
        // goes as a plain path.
        assert_eq!(imported.arrows.len(), 4);
        assert_eq!(
            imported.arrows.iter().filter(|a| a.arc.is_some()).count(),
            3
        );
        // Everything the cycle drew stays grouped with its parts.
        assert!(imported.groups.iter().any(|g| {
            imported
                .annotations
                .iter()
                .all(|a| g.members.contains(&a.id))
                && imported.arrows.iter().all(|a| g.members.contains(&a.id))
        }));
        assert!(
            imported
                .annotations
                .iter()
                .any(|a| a.text.replace('\n', " ") == "Oxidative addition")
        );
        assert!(imported.graphics.iter().all(|g| g.cycle.is_none()));
    }
    let notice = crate::arrows::export_notice(&document).unwrap();
    assert!(notice.contains("1 catalytic cycle as plain arrows, text and reaction steps"));
}

/// A chain of `n` carbons from (`x`, `y`), as one molecule's atom IDs.
fn chain(document: &mut Document, x: f32, y: f32, n: usize) -> Vec<u64> {
    let ids: Vec<u64> = (0..n)
        .map(|i| {
            let dy = if i % 2 == 0 { 0. } else { -15. };
            document.add_atom("C", Point::new(x + 26. * i as f32, y + dy))
        })
        .collect();
    for pair in ids.windows(2) {
        document.add_bond(pair[0], pair[1], 1, "plain");
    }
    for &id in &ids {
        document.atom_mut(id).unwrap().label_h = 2;
    }
    ids
}

/// Three structures made into a cycle, a text reagent coming into step 1
/// and a structure going out of step 3.
fn structure_cycle() -> (Document, u64) {
    let mut document = Document::default();
    let parts = [
        chain(&mut document, -20., -220., 2),
        chain(&mut document, 200., 110., 3),
        chain(&mut document, -260., 110., 4),
    ];
    let (cycle, _) = crate::cycles::make(&mut document, &parts.concat()).unwrap();
    let c = document
        .graphics
        .iter_mut()
        .find(|g| g.id == cycle)
        .and_then(|g| g.cycle.as_deref_mut())
        .unwrap();
    c.steps[0].name = "Oxidative addition".into();
    c.steps[1].reversible = true;
    let text = document.next_id();
    document.annotations.push(Annotation {
        id: text,
        position: Point::new(240., -220.),
        text: "ArX".into(),
        format: TextFormat {
            style: document.drawing_style.text_style(),
            ..Default::default()
        },
    });
    let product = chain(&mut document, -330., -200., 2);
    for (role, part) in [
        (crate::cycles::Role::Reagent(0), vec![text]),
        (crate::cycles::Role::Product(2), product),
    ] {
        let ink = crate::scene::selection_bounds(&document, &part).unwrap();
        crate::cycles::assign(
            &mut document,
            &crate::cycles::Target {
                cycle,
                role,
                part,
                ink,
            },
        );
    }
    document.validate().unwrap();
    (document, cycle)
}

async fn import(format: &str, text: &str) -> crate::engine::Response {
    crate::engine::LocalEngine::in_process()
        .request(Request::import(format, text))
        .await
        .unwrap()
}

fn cdx(xml: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(crate::exchange::to_cdx(xml).unwrap())
}

#[test]
fn each_cycle_step_is_a_chemdraw_reaction_step() {
    let (document, _) = structure_cycle();
    let xml = write(&document, Default::default()).unwrap();
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &decoded] {
        let tree = roxmltree::Document::parse(xml).unwrap();
        let by_id = |id: &str| {
            tree.descendants()
                .find(|n| n.is_element() && n.attribute("id") == Some(id))
                .unwrap()
        };
        let list = |n: roxmltree::Node, name: &str| -> Vec<String> {
            n.attribute(name)
                .map(|v| v.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default()
        };
        let steps: Vec<_> = tree
            .descendants()
            .filter(|n| n.has_tag_name("step"))
            .collect();
        assert_eq!(steps.len(), 3);
        assert_eq!(
            tree.descendants()
                .filter(|n| n.has_tag_name("scheme"))
                .count(),
            1
        );
        // Step 1: one structure to the next, the text reagent above its arc.
        let first = steps[0];
        for name in ["ReactionStepReactants", "ReactionStepProducts"] {
            let ids = list(first, name);
            assert_eq!(ids.len(), 1, "{name}");
            assert!(by_id(&ids[0]).has_tag_name("fragment"));
        }
        let above = list(first, "ReactionStepObjectsAboveArrow");
        assert_eq!(above.len(), 1);
        assert!(by_id(&above[0]).has_tag_name("t"));
        assert_eq!(list(first, "ReactionStepArrows").len(), 1);
        // A reversible step has both its arcs; the product comes out of step 3.
        assert_eq!(list(steps[1], "ReactionStepArrows").len(), 2);
        assert_eq!(list(steps[2], "ReactionStepProducts").len(), 2);
        for id in steps.iter().flat_map(|s| list(*s, "ReactionStepArrows")) {
            assert!(by_id(&id).has_tag_name("arrow"));
        }
        // The steps hand on: step 1's product is step 2's reactant.
        assert_eq!(
            list(steps[0], "ReactionStepProducts"),
            list(steps[1], "ReactionStepReactants")
        );
    }
    // The file passes the reader's own check of reaction references.
    let (rest, found) = crate::exchange::cycles::take_cdxml(&xml);
    assert!(matches!(
        found,
        Some(crate::exchange::cycles::Found::Fresh(_))
    ));
    assert!(crate::chemistry::cdxml::schemes::validate(&rest).unwrap());
}

#[tokio::test]
async fn an_unchanged_file_reopens_with_its_cycle() {
    let (document, cycle) = structure_cycle();
    let original = document.graphics.iter().find(|g| g.id == cycle).unwrap();
    let (center, radius) = original.cycle_circle();
    let first = crate::scene::selection_bounds(
        &document,
        &original.cycle.as_ref().unwrap().intermediates[0],
    )
    .unwrap();
    let offset = Point::new(
        center.x - (first.0.x + first.1.x) / 2.,
        center.y - (first.0.y + first.1.y) / 2.,
    );
    let xml = write(&document, Default::default()).unwrap();
    for (format, text) in [("cdxml", xml.clone()), ("cdx", cdx(&xml))] {
        let response = import(format, &text).await;
        assert!(
            !response
                .warnings
                .iter()
                .any(|w| w == crate::chemistry::cdxml::schemes::NOTICE
                    || w == crate::exchange::cycles::EDITED_NOTICE),
            "{format}: {:?}",
            response.warnings
        );
        let doc = response.document.unwrap();
        doc.validate().unwrap();
        let cycles: Vec<_> = doc.graphics.iter().filter(|g| g.cycle.is_some()).collect();
        assert_eq!(cycles.len(), 1, "{format}");
        let g = cycles[0];
        let c = g.cycle.as_deref().unwrap();
        // Each intermediate is its whole structure again.
        let sizes: Vec<usize> = c.intermediates.iter().map(Vec::len).collect();
        assert_eq!(sizes, [2, 3, 4], "{format}");
        assert_eq!(c.steps[0].name, "Oxidative addition");
        assert!(c.steps[1].reversible);
        assert_eq!(c.steps[0].reagents.len(), 1);
        assert_eq!(c.steps[2].products.len(), 1);
        assert_eq!(c.steps[2].products[0].len(), 2);
        // What it drew is gone: no loose arrows, shapes or step names.
        assert!(doc.arrows.is_empty(), "{format}");
        assert_eq!(doc.graphics.len(), 1, "{format}");
        assert_eq!(doc.annotations.len(), 1, "{format}");
        assert_eq!(doc.annotations[0].text, "ArX");
        // Grouped with its parts, where it was.
        assert!(doc.groups.iter().any(|group| {
            group.members.contains(&g.id)
                && c.parts()
                    .flat_map(|(_, ids)| ids)
                    .all(|id| group.members.contains(id))
        }));
        let (at, r) = g.cycle_circle();
        assert!((r - radius).abs() < 0.5, "{format}: {r} {radius}");
        let first = crate::scene::selection_bounds(&doc, &c.intermediates[0]).unwrap();
        let moved = Point::new(
            at.x - (first.0.x + first.1.x) / 2.,
            at.y - (first.0.y + first.1.y) / 2.,
        );
        assert!(
            moved.distance(offset) < 0.5,
            "{format}: {moved:?} {offset:?}"
        );
        // Its steps are reactions again.
        assert!(crate::cycles::steps::step(&doc, g.id, 0).is_ok());
    }
}

#[tokio::test]
async fn a_file_changed_elsewhere_opens_as_saved_with_a_note() {
    let (document, _) = structure_cycle();
    let xml = write(&document, Default::default()).unwrap();
    let edited = xml.replace(">ArX<", ">ArY<");
    assert_ne!(edited, xml);
    let mut bytes = crate::exchange::to_cdx(&xml).unwrap();
    let at = bytes.windows(3).position(|w| w == b"ArX").unwrap();
    bytes[at + 2] = b'Y';
    let binary = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    };
    for (format, text) in [("cdxml", edited), ("cdx", binary)] {
        let response = import(format, &text).await;
        assert!(
            response
                .warnings
                .iter()
                .any(|w| w == crate::exchange::cycles::EDITED_NOTICE),
            "{format}: {:?}",
            response.warnings
        );
        let doc = response.document.unwrap();
        assert!(doc.graphics.iter().all(|g| g.cycle.is_none()));
        assert!(doc.arrows.len() >= 4, "{format}");
        assert!(doc.annotations.iter().any(|a| a.text == "ArY"));
    }
    // A file without cycle data says nothing about cycles.
    let (_, found) = crate::exchange::cycles::take_cdxml(&xml.replace(" ReShikiCycles=", " X="));
    assert_eq!(found, None);
}

#[test]
fn a_fresh_cycle_file_loses_no_steps_on_save() {
    let (document, _) = structure_cycle();
    let xml = write(&document, Default::default()).unwrap();
    for (format, text) in [("cdxml", xml.clone()), ("cdx", cdx(&xml))] {
        let loss = crate::formats::chemdraw_loss(format, &text);
        assert_eq!((loss.schemes, loss.steps), (0, 0), "{format}");
    }
    let loss = crate::formats::chemdraw_loss("cdxml", &xml.replace(">ArX<", ">ArY<"));
    assert_eq!((loss.schemes, loss.steps), (1, 3));
}

#[tokio::test]
async fn an_ungrouped_cycle_comes_back_with_each_structure_apart() {
    let (mut document, _) = structure_cycle();
    document.groups.clear();
    let xml = write(&document, Default::default()).unwrap();
    let doc = import("cdxml", &xml).await.document.unwrap();
    let c = doc
        .graphics
        .iter()
        .find_map(|g| g.cycle.as_deref())
        .unwrap();
    let sizes: Vec<usize> = c.intermediates.iter().map(Vec::len).collect();
    assert_eq!(sizes, [2, 3, 4]);
    assert!(doc.groups.is_empty());
}

#[tokio::test]
async fn a_binary_copy_of_an_edited_file_still_reads_as_edited() {
    let (document, _) = structure_cycle();
    let xml = write(&document, Default::default()).unwrap();
    let response = import("cdx", &cdx(&xml.replace(">ArX<", ">ArY<"))).await;
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w == crate::exchange::cycles::EDITED_NOTICE),
        "{:?}",
        response.warnings
    );
    let doc = response.document.unwrap();
    assert!(doc.graphics.iter().all(|g| g.cycle.is_none()));
}

#[tokio::test]
async fn cycle_data_that_does_not_fit_its_file_opens_the_drawing_as_saved() {
    use crate::exchange::cycles::{Found, encode, seal_cdxml, take_cdxml};
    let (document, _) = structure_cycle();
    let xml = write(&document, Default::default()).unwrap();
    let (rest, found) = take_cdxml(&xml);
    let Some(Found::Fresh(mut payload)) = found else {
        panic!("fresh data");
    };
    // A step short, sealed as if ReShiki wrote it.
    if let Some(c) = payload.cycles[0].graphic.cycle.as_deref_mut() {
        c.steps.pop();
    }
    let forged = seal_cdxml(rest.into_owned(), &encode(&payload).unwrap());
    let response = import("cdxml", &forged).await;
    assert!(
        response
            .warnings
            .iter()
            .any(|w| w == crate::exchange::cycles::EDITED_NOTICE)
    );
    let doc = response.document.unwrap();
    assert!(doc.graphics.iter().all(|g| g.cycle.is_none()));
    assert!(doc.arrows.len() >= 4);
}

#[test]
fn a_lone_intermediate_starts_and_ends_its_step() {
    let mut document = Document::default();
    let part = chain(&mut document, 0., -200., 2);
    let cycle = crate::cycles::insert(&mut document, Point::new(0., 0.), 200.);
    let ink = crate::scene::selection_bounds(&document, &part).unwrap();
    crate::cycles::assign(
        &mut document,
        &crate::cycles::Target {
            cycle,
            role: crate::cycles::Role::Intermediate(0),
            part,
            ink,
        },
    );
    let xml = write(&document, Default::default()).unwrap();
    let tree = roxmltree::Document::parse(&xml).unwrap();
    let step = tree.descendants().find(|n| n.has_tag_name("step")).unwrap();
    assert_eq!(
        step.attribute("ReactionStepReactants"),
        step.attribute("ReactionStepProducts")
    );
}

#[tokio::test]
async fn a_cycle_drawn_on_the_dark_canvas_comes_back() {
    let (mut document, _) = structure_cycle();
    document.canvas_theme = crate::canvas_theme::CanvasTheme::Dark;
    let xml = write(&document, Default::default()).unwrap();
    let doc = import("cdxml", &xml).await.document.unwrap();
    assert!(doc.graphics.iter().any(|g| g.cycle.is_some()));
    assert!(doc.arrows.is_empty());
}

#[tokio::test]
async fn a_part_split_in_two_opens_as_drawn() {
    let (mut document, cycle) = structure_cycle();
    // Delete the middle bond of the four-carbon intermediate; both pieces
    // stay in its part.
    let part = document
        .graphics
        .iter()
        .find(|g| g.id == cycle)
        .and_then(|g| g.cycle.as_deref())
        .unwrap()
        .intermediates[2]
        .clone();
    document
        .bonds
        .retain(|b| !(part.contains(&b.a) && part.contains(&b.b) && b.a == part[1]));
    let xml = write(&document, Default::default()).unwrap();
    assert!(!xml.contains(crate::exchange::cycles::ATTRIBUTE));
    let response = import("cdxml", &xml).await;
    let doc = response.document.unwrap();
    assert!(doc.graphics.iter().all(|g| g.cycle.is_none()));
}
