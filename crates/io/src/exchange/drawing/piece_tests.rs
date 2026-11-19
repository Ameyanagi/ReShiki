use super::*;
use crate::chemistry::cdxml;
use crate::equipment::{builtin, placed};

#[test]
fn a_piece_exports_as_one_group_of_curves_in_paint_order() {
    let mut document = Document::default();
    let scale = placed::world_per_mm(&document, placed::DEFAULT_PT_PER_MM);
    let flask = builtin::find("reshiki/rbf-100-15").unwrap();
    document
        .graphics
        .push(placed::graphic(1, flask, Point::new(100., 100.), scale));
    document.validate().unwrap();
    let painted = placed::primitives(&document.graphics[0]).len();
    let request = Request::molecule("export", document.clone());
    let xml = write(&document, (&request).into()).unwrap();
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &decoded] {
        let imported = cdxml::import_cdxml(xml).unwrap().document;
        assert!(imported.graphics.iter().all(|g| g.piece.is_none()));
        assert_eq!(imported.graphics.len(), painted);
        assert!(imported.groups.iter().any(|g| g.members.len() == painted));
        // White glass over its doubled outline, as on the canvas.
        assert!(
            imported
                .graphics
                .iter()
                .any(|g| { g.style.fill == Some(crate::palette::Color::Custom([255; 3])) })
        );
    }
}
