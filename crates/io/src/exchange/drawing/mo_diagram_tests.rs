use super::*;
use crate::{
    graphics::{Graphic, MoDiagram},
    models3d::surface::{Figure, OrbitalRef},
    orbital_levels::Column,
};
use reshiki_qcdata::Spin;

fn diagram() -> Document {
    let mut doc = Document::default();
    let molecule = doc.add_atom("C", Point::default());
    let result = "0123456789abcdef".to_owned();
    let mut mo = MoDiagram::new(
        molecule,
        result.clone(),
        vec![Column {
            spin: Spin::Alpha,
            label: String::new(),
            energies_ev: vec![-15., -10.95, -6.02, -1.],
            occupations: vec![2., 2., 0., 0.],
            occupied: Some(2),
        }],
    );
    mo.pictures.push(Figure {
        orbital: OrbitalRef {
            result,
            spin: Spin::Alpha,
            index: 1,
        },
        iso: 0.05,
        look: Default::default(),
        snapshot: None,
    });
    let id = doc.next_id();
    doc.graphics.push(Graphic::mo_diagram(
        id,
        Point::new(40., 20.),
        Point::new(360., 300.),
        mo,
        Default::default(),
    ));
    doc
}

#[test]
fn mo_diagram_exports_grouped_lines_and_text_through_cdxml_and_cdx() {
    let doc = diagram();
    doc.validate().unwrap();
    let expected: Vec<_> = doc.graphics[0]
        .mo_parts(&doc.drawing_style)
        .1
        .into_iter()
        .map(|a| a.text)
        .collect();
    assert!(expected.iter().any(|text| text.contains("HOMO")));
    assert!(expected.iter().any(|text| text.contains("gap 4.93 eV")));
    // No caller-supplied vector geometry is needed for the diagram.
    let xml = write(&doc, Options::default()).unwrap();
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &decoded] {
        let tree = roxmltree::Document::parse(xml).unwrap();
        let groups: Vec<_> = tree
            .descendants()
            .filter(|n| n.has_tag_name("group"))
            .collect();
        assert_eq!(groups.len(), 1);
        let group = groups[0];
        assert!(group.children().any(|n| n.has_tag_name("curve")));
        let labels: Vec<_> = group
            .children()
            .filter(|n| n.has_tag_name("t"))
            .map(|n| {
                assert_eq!(n.attribute("InterpretChemically"), Some("no"));
                n.descendants()
                    .filter(|n| n.has_tag_name("s"))
                    .filter_map(|n| n.text())
                    .collect::<String>()
            })
            .collect();
        assert_eq!(labels, expected);
        assert!(!tree.descendants().any(|n| n.has_tag_name("embeddedobject")));
    }
}

#[test]
fn mo_diagram_loss_notice_reaches_editable_files_and_clipboard() {
    use crate::engine::export_notices::{Route, notices};
    let doc = diagram();
    for format in ["cdxml", "cdx", "mol"] {
        let warnings = notices(
            &doc,
            format,
            Route::Prepared {
                source: &doc,
                figure_only: false,
            },
        );
        assert_eq!(
            warnings.iter().any(|w| w == MO_DIAGRAM_EXPORT_NOTICE),
            format != "mol"
        );
    }
    let (_, warnings) = write_clipboard(&doc).unwrap();
    assert!(warnings.iter().any(|w| w == MO_DIAGRAM_EXPORT_NOTICE));
    assert!(mo_diagram_export_notice(&Document::default()).is_none());
}
