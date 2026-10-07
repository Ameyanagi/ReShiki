use super::*;
use crate::document::Point;

#[test]
fn every_preset_can_replace_an_isolated_or_attached_atom() {
    for preset in presets().unwrap() {
        for outside in [
            None,
            Some(Point::new(42.0, 0.0)),
            Some(Point::new(-30.0, 30.0)),
        ] {
            let mut input = Document::default();
            let anchor = input.add_atom("C", Point::new(0.0, 0.0));
            if let Some(position) = outside {
                let neighbor = input.add_atom("C", position);
                input.add_bond(anchor, neighbor, 1, "plain");
            }
            let output = replace(&input, &[anchor], &preset.label)
                .unwrap_or_else(|error| panic!("{} at {outside:?}: {error}", preset.label));
            document::prepare(&output)
                .unwrap_or_else(|error| panic!("{} chemistry: {error}", preset.label));
            assert_eq!(output.abbreviations.len(), 1);
            assert_eq!(output.abbreviations[0].anchor, anchor);
            assert_eq!(output.atom(anchor).unwrap().position, Point::new(0.0, 0.0));
            assert_eq!(
                output.atoms.len(),
                preset.atoms.len() - 1 + usize::from(outside.is_some())
            );
            assert!(
                output
                    .atoms
                    .iter()
                    .all(|atom| atom.position.x.is_finite() && atom.position.y.is_finite())
            );
        }
    }
}
