use super::*;
use reshiki::arrows::{ArrowStyle, Preset};
use reshiki::document::Arrow;
use reshiki::graphics::{EnergyProfile, EnergyProfileState, EnergyProfileStep};
use reshiki::qcdata::Frame;
use reshiki::reactions::Participant;
use std::sync::Arc;

fn fixture() -> (Document, u64, Vec<u64>, Vec<u64>) {
    let mut doc = Document::default();
    let reactants: Vec<_> = (0..3)
        .map(|i| doc.add_atom("C", Point::new(i as f32 * 25., 0.)))
        .collect();
    let products: Vec<_> = (0..3)
        .map(|i| doc.add_atom("C", Point::new(160. + i as f32 * 25., 0.)))
        .collect();
    for ids in [&reactants, &products] {
        for (i, &id) in ids.iter().enumerate() {
            doc.atom_mut(id).unwrap().map_num = i as u32 + 1
        }
    }
    doc.add_bond(reactants[0], reactants[1], 1, "plain");
    doc.add_bond(products[0], products[2], 1, "plain");
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(75., 0.),
        Point::new(130., 0.),
        Preset::Forward,
        ArrowStyle::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(Participant {
        atoms: reactants.clone(),
        coefficient: 1,
    });
    reaction.products.push(Participant {
        atoms: products.clone(),
        coefficient: 1,
    });
    doc.reactions.push(reaction);
    // All three atoms are carbon. Their non-drawing order deliberately differs
    // from both map order and document order.
    let mut frame = Frame {
        numbers: vec![6, 6, 6],
        positions_angstrom: vec![[2., 0., 0.], [0., 0., 0.], [1., 1., 0.]],
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![Some(products[2]), Some(products[0]), Some(products[1])],
    };
    doc.science.frames.push(Arc::new(frame.clone()));
    frame.drawing_atoms.clear();
    let geometry = EnergyProfileGeometry {
        frame,
        bonds: vec![[0, 1], [1, 2]],
        forming: vec![[0, 1]],
        breaking: vec![[1, 2]],
    };
    let states = [
        (Kind::Reactant, 0.),
        (Kind::TransitionState, 20.),
        (Kind::Product, -5.),
    ]
    .into_iter()
    .map(|(kind, energy_kcal)| EnergyProfileState {
        kind,
        energy_kcal,
        label: String::new(),
        geometry: Some(geometry.clone()),
    })
    .collect();
    let key = crate::app::xtb::ts::profile::source_key(&doc, &doc.reactions[0]);
    let profile = EnergyProfile::new(
        arrow,
        key,
        "GFN2-xTB".into(),
        states,
        vec![EnergyProfileStep {
            from: 0,
            to: 2,
            transition: Some(1),
            source_key: None,
            irc: vec![],
        }],
    );
    let (w, h) = profile.natural_size(&doc.drawing_style);
    let id = doc.next_id();
    doc.graphics.push(Graphic::energy_profile(
        id,
        Point::new(-30., 70.),
        Point::new(w, h),
        profile,
        GraphicStyle::default(),
    ));
    (doc, id, reactants, products)
}

#[test]
fn scheme_boxes_reactant_and_product_and_maps_ts_bonds_in_exact_saved_order() {
    let (doc, id, reactants, products) = fixture();
    let g = doc.graphics.iter().find(|g| g.id == id).unwrap();
    let artwork = Artwork::new(&doc, g);
    assert_eq!(artwork.species[0][0].owner, reactants[0]);
    assert_eq!(artwork.species[2][0].owner, products[0]);
    assert_eq!(
        artwork.changes[1].forming,
        vec![[reactants[2], reactants[0]]]
    );
    assert_eq!(
        artwork.changes[1].breaking,
        vec![[reactants[0], reactants[1]]]
    );
    for state in [0, 2] {
        assert_eq!(artwork.overlay(&doc, g, state, None, None, true).len(), 3);
        assert_eq!(artwork.overlay(&doc, g, state, None, None, false).len(), 2);
    }
    let overlay = artwork.overlay(&doc, g, 1, None, None, true);
    assert_eq!(overlay.len(), 5);
    let Primitive::Path {
        commands, style, ..
    } = &overlay[2]
    else {
        panic!("Missing forming bond")
    };
    assert_eq!(
        commands,
        &vec![
            PathCommand::Move(doc.atom(reactants[2]).unwrap().position),
            PathCommand::Line(doc.atom(reactants[0]).unwrap().position)
        ]
    );
    assert_eq!(style.pattern, LinePattern::Dashed);
    assert_eq!(style.stroke, Color::Palette(Hue::Green, Row::Strong));
    let Primitive::Path {
        commands, style, ..
    } = &overlay[4]
    else {
        panic!("Missing crossed bond")
    };
    assert_eq!(style.stroke, Color::Palette(Hue::Red, Row::Strong));
    assert_eq!(style.pattern, LinePattern::Solid);
    assert_eq!(commands.len(), 4);
}

#[test]
fn mapped_attachments_take_priority_over_creation_ids_after_paste() {
    let (mut doc, id, reactants, products) = fixture();
    let hash = doc.science.frames[0].hash();
    doc.science.attachments.push(reshiki::science::Attachment {
        id: 1,
        frame: hash,
        atoms: vec![Some(reactants[2]), Some(reactants[0]), Some(reactants[1])],
        baseline: String::new(),
    });
    // The original immutable frame still has IDs from the creation document.
    Arc::make_mut(&mut doc.science.frames[0]).drawing_atoms =
        products.into_iter().map(|id| Some(id + 1000)).collect();
    let g = doc.graphics.iter().find(|g| g.id == id).unwrap();
    let artwork = Artwork::new(&doc, g);
    assert_eq!(
        artwork.changes[1].forming,
        vec![[reactants[2], reactants[0]]]
    );
}

#[test]
fn five_states_check_the_saved_chemistry_of_each_arrow() {
    let (mut doc, id, _, intermediate) = fixture();
    let final_atoms: Vec<_> = (0..3)
        .map(|i| doc.add_atom("C", Point::new(320. + i as f32 * 25., 0.)))
        .collect();
    for (i, &atom) in final_atoms.iter().enumerate() {
        doc.atom_mut(atom).unwrap().map_num = i as u32 + 1;
    }
    doc.add_bond(final_atoms[0], final_atoms[1], 1, "plain");
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(230., 0.),
        Point::new(290., 0.),
        Preset::Forward,
        ArrowStyle::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(Participant {
        atoms: intermediate.clone(),
        coefficient: 1,
    });
    reaction.products.push(Participant {
        atoms: final_atoms.clone(),
        coefficient: 1,
    });
    let key = source_key(&doc, &reaction);
    doc.reactions.push(reaction);
    let g = doc.graphics.iter_mut().find(|g| g.id == id).unwrap();
    let profile = g.energy_profile.as_deref_mut().unwrap();
    profile.states[2].kind = Kind::Intermediate;
    let mut second_ts = profile.states[1].geometry.clone().unwrap();
    std::mem::swap(&mut second_ts.forming, &mut second_ts.breaking);
    profile.states.extend([
        EnergyProfileState {
            kind: Kind::TransitionState,
            label: "TS2‡".into(),
            energy_kcal: 16.,
            geometry: Some(second_ts),
        },
        EnergyProfileState {
            kind: Kind::Product,
            label: "P".into(),
            energy_kcal: -10.,
            geometry: None,
        },
    ]);
    profile.steps.push(EnergyProfileStep {
        from: 2,
        to: 4,
        transition: Some(3),
        source_key: Some(key),
        irc: vec![],
    });
    let g = &doc.graphics[0];
    let artwork = Artwork::new(&doc, g);
    assert_eq!(artwork.species[2][0].owner, intermediate[0]);
    assert_eq!(artwork.species[4][0].owner, final_atoms[0]);
    assert!(artwork.selection.contains(&arrow));
    let count = |doc: &Document, artwork: &Artwork, state| {
        artwork
            .overlay(doc, &doc.graphics[0], state, None, None, true)
            .len()
    };
    assert_eq!(count(&doc, &artwork, 3), 5);
    assert_eq!(count(&doc, &artwork, 4), 3);

    doc.atom_mut(final_atoms[0]).unwrap().charge = 1;
    let artwork = Artwork::new(&doc, &doc.graphics[0]);
    assert!(!artwork.selection.contains(&arrow));
    assert_eq!(
        (0..5)
            .map(|state| count(&doc, &artwork, state))
            .collect::<Vec<_>>(),
        [3, 5, 3, 2, 2],
    );
    let g = &doc.graphics[0];
    let sample = Some((
        1,
        EnergyProfileSample {
            x: 0.1,
            energy_kcal: -4.,
        },
    ));
    assert_eq!(
        artwork.overlay(&doc, g, 2, sample, None, true).len(),
        artwork.overlay(&doc, g, 2, sample, None, false).len(),
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn linked_models_do_not_contribute_to_scheme_highlights() {
    use reshiki::models3d::{Display, ForceField, Model3d, Placed};

    let (mut doc, id, reactants, _) = fixture();
    let bounds = scene::selection_bounds(&doc, &reactants).unwrap();
    let mut model = Model3d::new(
        &doc,
        reactants
            .iter()
            .enumerate()
            .map(|(i, &id)| Placed {
                id,
                xyz: [i as f32, 0., 0.],
            })
            .collect(),
        vec![],
        ForceField::default(),
    );
    model.display = Display::Both;
    model.offset = Point::new(1000., 0.);
    doc.models.push(model);
    assert_ne!(scene::selection_bounds(&doc, &reactants), Some(bounds));
    let renderer = FrameRenderer::new(&doc, id, 480, true).unwrap();
    let live = Artwork::new(&doc, &doc.graphics[0]);
    for artwork in [&live, &renderer.artwork] {
        let species = &artwork.species[0][0];
        assert_eq!((species.lo, species.hi), bounds);
        assert_eq!(artwork.changes[1].forming.len(), 1);
        assert_eq!(artwork.changes[1].breaking.len(), 1);
    }

    doc.models[0].display = Display::ThreeD;
    let renderer = FrameRenderer::new(&doc, id, 480, true).unwrap();
    let live = Artwork::new(&doc, &doc.graphics[0]);
    for (artwork, doc, graphic) in [
        (&live, &doc, &doc.graphics[0]),
        (&renderer.artwork, &renderer.doc, &renderer.graphic),
    ] {
        assert_eq!(artwork.overlay(doc, graphic, 0, None, None, true).len(), 2);
        assert_eq!(artwork.overlay(doc, graphic, 1, None, None, true).len(), 2);
        assert_eq!(artwork.overlay(doc, graphic, 2, None, None, true).len(), 3);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn offscreen_frames_include_the_scheme_and_preserve_background_and_document() {
    let (doc, id, _, _) = fixture();
    let before = serde_json::to_string(&doc).unwrap();
    let mut white = FrameRenderer::new(&doc, id, 480, false).unwrap();
    let mut clear = FrameRenderer::new(&doc, id, 480, true).unwrap();
    assert!(
        white.bounds.0.y < 0.,
        "Scheme above the profile is included"
    );
    let r = white.render(0, None, None, true).unwrap();
    let ts = white.render(1, None, None, true).unwrap();
    let transparent = clear.render(0, None, None, true).unwrap();
    assert_eq!(r.len(), 480 * 480 * 4);
    assert!(r.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));
    assert!(
        transparent
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] == 0)
    );
    assert_ne!(r, ts);
    assert_eq!(serde_json::to_string(&doc).unwrap(), before);
    for size in [720, 1080] {
        let mut renderer = FrameRenderer::new(&doc, id, size, true).unwrap();
        assert_eq!(
            renderer.render(2, None, None, false).unwrap().len(),
            usize::from(size) * usize::from(size) * 4
        );
    }
    assert!(FrameRenderer::new(&doc, id, 640, false).is_err());
}

#[test]
fn changed_source_chemistry_disables_scheme_highlights() {
    let (mut doc, id, reactants, _) = fixture();
    doc.atom_mut(reactants[0]).unwrap().element = "O".into();
    let g = doc.graphics.iter().find(|g| g.id == id).unwrap();
    let artwork = Artwork::new(&doc, g);
    assert_eq!(artwork.selection, vec![id]);
    for state in 0..3 {
        assert_eq!(artwork.overlay(&doc, g, state, None, None, true).len(), 2);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn table_owned_profile_keeps_its_clip_without_exporting_other_cells() {
    use reshiki::graphics::{
        GraphicKind,
        table::{Cell, Table},
    };
    let (mut doc, id, _, _) = fixture();
    let unrelated = doc.add_atom("N", Point::new(900., 900.));
    let mut table = doc.graphics.iter().find(|g| g.id == id).unwrap().clone();
    table.id = doc.next_id();
    table.kind = GraphicKind::Table;
    table.energy_profile = None;
    table.origin = Point::new(0., 50.);
    table.axis_x = Point::new(500., 0.);
    table.axis_y = Point::new(0., 200.);
    table.table = Some(Box::new(Table {
        cells: vec![
            Cell {
                id: 1,
                lo: Point::new(0., 0.),
                hi: Point::new(0.5, 1.),
                members: vec![id],
                borders: vec![],
            },
            Cell {
                id: 2,
                lo: Point::new(0.5, 0.),
                hi: Point::new(1., 1.),
                members: vec![unrelated],
                borders: vec![],
            },
        ],
    }));
    let corners = table.table.as_ref().unwrap().cells[0]
        .frame(&table)
        .unwrap()
        .corners();
    doc.graphics.push(table);
    let g = doc.graphics.iter().find(|g| g.id == id).unwrap();
    let artwork = Artwork::new(&doc, g);
    let overlay = artwork.overlay(&doc, g, 1, None, None, false);
    let [
        Primitive::Clip {
            corners: actual,
            primitives,
        },
    ] = overlay.as_slice()
    else {
        panic!("Unclipped table overlay")
    };
    assert_eq!(*actual, corners);
    assert_eq!(primitives.len(), 2);
    let mut renderer = FrameRenderer::new(&doc, id, 480, true).unwrap();
    let table = renderer
        .doc
        .graphics
        .iter()
        .find_map(|g| g.table.as_deref())
        .unwrap();
    assert_eq!(table.cells.len(), 1);
    assert_eq!(table.cells[0].members, vec![id]);
    assert!(!renderer.doc.atoms.iter().any(|atom| atom.id == unrelated));
    assert_eq!(
        renderer.render(1, None, None, false).unwrap().len(),
        480 * 480 * 4
    );
}
