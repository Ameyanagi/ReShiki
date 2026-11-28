use crate::{
    document::{Annotation, Document, Point},
    editing::preserved_expansion_selection,
    graphics::{Graphic, GraphicKind},
};

fn drawing() -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let a = doc.add_atom("N", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let c = doc.add_atom("C", Point::new(300., 0.));
    let d = doc.add_atom("C", Point::new(342., 0.));
    doc.add_bond(c, d, 1, "plain");
    for (id, text) in [(100, "Hydroxylamine"), (101, "Unselected caption")] {
        doc.annotations.push(Annotation {
            id,
            text: text.into(),
            position: Point::new(id as f32, 70.),
            angle: 13.,
            format: Default::default(),
        });
    }
    for id in [200, 201] {
        doc.graphics.push(Graphic::dragged(
            id,
            GraphicKind::Rectangle,
            Point::new(id as f32, 100.),
            Point::new(id as f32 + 80., 150.),
            Default::default(),
            Default::default(),
            false,
        ));
    }
    let selected = vec![a, b, 100, 200];
    doc.groups.push(crate::grouping::Group {
        id: 300,
        members: selected.clone(),
        integral: true,
        layout: None,
    });
    doc.molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation: 100,
            atoms: vec![a, b],
            smiles: "NO".into(),
            text: "Hydroxylamine".into(),
        });
    doc.canvas_theme = crate::canvas_theme::CanvasTheme::Dark;
    doc.color_theme = crate::canvas_theme::ColorTheme::Presentation;
    doc.drawing_style.font_size_pt = 18.;
    doc.page_layout = Some(Default::default());
    doc.recent_colors = vec![[1, 2, 3], [120, 130, 140]];
    // This transient page-resolution metadata is copied as stored, just as
    // themes, page layout and embedded definitions are.
    doc.on_other_canvas.extend([a, c]);
    (doc, selected)
}

#[test]
fn preserved_fragment_keeps_selected_ink_order_links_and_document_metadata() {
    let (doc, selected) = drawing();
    doc.validate().unwrap();
    let original = doc.clone();
    let mut expected = doc.clone();
    expected.atoms.retain(|a| selected.contains(&a.id));
    expected
        .bonds
        .retain(|b| selected.contains(&b.a) && selected.contains(&b.b));
    expected.annotations.retain(|a| selected.contains(&a.id));
    expected.arrows.retain(|a| selected.contains(&a.id));
    expected.graphics.retain(|g| selected.contains(&g.id));
    for ids in [
        selected.clone(),
        vec![selected[0], selected[0], selected[1], 100, 200, u64::MAX],
    ] {
        let part = preserved_expansion_selection(&doc, &ids).unwrap();
        assert_eq!(part, expected);
        part.validate().unwrap();
        assert_eq!(
            format!("{:?}", crate::scene::primitives(&part)),
            format!("{:?}", crate::scene::primitives(&expected))
        );
    }
    assert_eq!(doc, original);
}

#[test]
fn preserved_fragment_keeps_full_and_empty_selection_behavior() {
    let (doc, _) = drawing();
    let full = preserved_expansion_selection(&doc, &doc.all_ids()).unwrap();
    assert_eq!(full, doc);
    let empty = preserved_expansion_selection(&doc, &[]).unwrap();
    assert!(empty.object_ids().next().is_none());
    assert!(empty.groups.is_empty());
    assert!(empty.molecule_names.is_empty());
    assert_eq!(empty.drawing_style, doc.drawing_style);
    assert_eq!(empty.canvas_theme, doc.canvas_theme);
    assert_eq!(empty.page_layout, doc.page_layout);
    assert_eq!(empty.on_other_canvas, doc.on_other_canvas);
}

#[test]
fn preserved_fragment_still_refuses_cut_components_and_native_groups() {
    let (mut doc, selected) = drawing();
    assert!(preserved_expansion_selection(&doc, &selected[..1]).is_err());
    doc.groups[0].members.push(doc.atoms[2].id);
    assert!(preserved_expansion_selection(&doc, &selected).is_err());
    assert!(preserved_expansion_selection(&doc, &doc.all_ids()).is_ok());
}
