//! UNRUN private regression for map-label opacity in the composed model.
use reshiki::{
    atom_labels::{self, Number, Owner},
    depth_appearance,
    document::{Document, Point},
    palette::Color,
    rear_opacity,
    scene::{self, Primitive},
    typography::TextStyle,
};

fn text_alpha(parts: &[Primitive], wanted: &str) -> Vec<(f32, [u8; 3])> {
    fn collect(part: &Primitive, wanted: &str, alpha: f32, out: &mut Vec<(f32, [u8; 3])>) {
        match part {
            Primitive::Opacity {
                alpha: own,
                primitive,
            } => {
                collect(primitive, wanted, alpha * own, out);
            }
            Primitive::Text { text, color, .. } if text == wanted => out.push((alpha, *color)),
            _ => {}
        }
    }
    let mut out = vec![];
    for part in parts {
        collect(part, wanted, 1., &mut out);
    }
    out
}

#[test]
fn composed_map_labels_share_owning_atom_alpha_without_replacing_number_or_stereo_ink() {
    let mut doc = Document::from_native_file(include_bytes!(
        "fixtures/rear-opacity/c60-rear-opacity-25.rsk"
    ))
    .unwrap();
    assert_eq!(doc.version, 22, "the historical native fixture remains v22");
    assert_eq!((doc.atoms.len(), doc.bonds.len()), (60, 90));
    let ids = doc.all_ids();
    let original_paint = rear_opacity::Paint::new(&doc);
    let rear = ids
        .iter()
        .copied()
        .find(|id| original_paint.atom(*id) == 0.25)
        .unwrap();
    let front = ids
        .iter()
        .copied()
        .find(|id| original_paint.atom(*id) == 1.)
        .unwrap();
    assert_ne!(
        rear, front,
        "exercise actual obscured and exposed cage sites"
    );
    // Isolate drawing alpha from the separately tested RGB-depth fade. These
    // labels are declared in-memory extensions of the unchanged captured cage.
    for scope in &mut doc.depth_appearance {
        scope.strength = 0.;
    }
    let color = [23, 91, 167];
    for (id, map, custom, stereo) in [
        (rear, 941, "rear-custom", "R"),
        (front, 982, "front-custom", "S"),
    ] {
        let atom = doc.atom_mut(id).unwrap();
        atom.map_num = map;
        atom.display.mapping.show = Some(true);
        atom.display.mapping.offset = Some(Point::new(-20., -20.));
        atom.display.mapping.style.color = Color::Custom(color);
        atom.display.number = Some(Number {
            text: custom.into(),
            offset: Some(Point::new(0., 20.)),
            style: TextStyle {
                color: Color::Custom(color),
                ..Default::default()
            },
        });
        atom.cip_label = Some(stereo.into());
        atom.display.stereo.show = Some(true);
        atom.display.stereo.offset = Some(Point::new(20., -20.));
        atom.display.stereo.style.color = Color::Custom(color);
    }
    doc.validate().unwrap();
    let owned = atom_labels::indicators(&doc);
    for id in [rear, front] {
        assert!(owned.iter().any(|label| label.owner == Owner::Mapping(id)));
        assert!(owned.iter().any(|label| label.owner == Owner::Number(id)));
        assert!(
            owned
                .iter()
                .any(|label| label.owner == Owner::AtomStereo(id))
        );
    }
    let atoms = doc.atoms.clone();
    let bonds = doc.bonds.clone();
    for alpha in [0., 0.25, 1.] {
        depth_appearance::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
        let paint = rear_opacity::Paint::new(&doc);
        assert_eq!(paint.atom(rear), alpha);
        assert_eq!(paint.atom(front), 1.);
        let parts = scene::primitives(&doc);
        for text in ["941", "rear-custom", "(R)"] {
            let expected = if alpha == 0. {
                vec![]
            } else {
                vec![(alpha, color)]
            };
            assert_eq!(
                text_alpha(&parts, text),
                expected,
                "rear label {text} at alpha {alpha}"
            );
        }
        for text in ["982", "front-custom", "(S)"] {
            assert_eq!(
                text_alpha(&parts, text),
                vec![(1., color)],
                "exposed label {text} at alpha {alpha}"
            );
        }
        assert_eq!(
            doc.atoms, atoms,
            "drawing alpha must not mutate independent owner data"
        );
        assert_eq!(
            doc.bonds, bonds,
            "drawing alpha must not mutate the chemical graph"
        );
    }
}
