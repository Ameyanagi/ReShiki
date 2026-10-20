use super::*;
use crate::chemistry::abbreviations::presets;
use crate::document::Point;

#[test]
fn every_preset_can_replace_an_isolated_or_attached_atom() {
    for preset in presets().unwrap().iter() {
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

/// The 62 groups bundled before the catalog were laid out by RDKit 2026.03.6
/// and captured per platform. Those frozen captures now guard the catalog's
/// own depiction against layout changes in existing documents. Only the
/// matching host's file applies: Intel macOS compares with the Apple capture
/// it used before, so it is held to a tolerance rather than to the bit.
#[test]
fn own_depiction_reproduces_the_frozen_rdkit_captures() {
    #[derive(serde::Deserialize)]
    struct Template {
        label: String,
        smiles: String,
        positions: Vec<Point3>,
    }
    #[derive(serde::Deserialize)]
    struct Captures {
        rdkit_version: String,
        templates: Vec<Template>,
    }
    let data = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", _) => include_str!("rdkit-geometry-macos-aarch64.json"),
        ("linux", "x86_64") => include_str!("rdkit-geometry-linux-x86_64.json"),
        ("linux", "aarch64") => include_str!("rdkit-geometry-linux-aarch64.json"),
        ("windows", "x86_64") => include_str!("rdkit-geometry-windows-x86_64.json"),
        ("windows", "aarch64") => include_str!("rdkit-geometry-windows-aarch64.json"),
        _ => return,
    };
    let captures: Captures = serde_json::from_str(data).unwrap();
    assert_eq!(captures.rdkit_version, RDKIT_VERSION);
    assert_eq!(captures.templates.len(), 62);
    let exact_host = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    for template in captures.templates {
        let (preset, _) = lookup(&template.label).unwrap().unwrap();
        assert_eq!(preset.smiles, template.smiles);
        let positions = preset.positions().unwrap();
        assert_eq!(positions.len(), template.positions.len());
        for (own, captured) in positions.iter().zip(&template.positions) {
            if exact_host {
                assert_eq!(
                    (own.x.to_bits(), own.y.to_bits()),
                    (captured.x.to_bits(), captured.y.to_bits()),
                    "{}",
                    template.label
                );
            }
            assert!(
                (own.x - captured.x).abs() <= 1e-9 && (own.y - captured.y).abs() <= 1e-9,
                "{}: {own:?} != {captured:?}",
                template.label
            );
        }
    }
}

/// N-linked aromatic roots (azolyl, pyridinium) borrow a hydrogen while the
/// template stands alone; it must not survive once the bond is restored.
#[test]
fn aromatic_nitrogen_roots_attach_without_an_extra_hydrogen() {
    for smiles in ["*n1cccc1", "*n1ccnc1", "*[n+]1ccccc1"] {
        let entry = crate::chemistry::abbreviations::Entry {
            label: "Ring".into(),
            reverse: String::new(),
            smiles: smiles.into(),
            formula: None,
            category: String::new(),
            names: Vec::new(),
            aliases: Vec::new(),
            contract: true,
        };
        let preset = std::sync::Arc::new(Preset::from_entry(entry, true).unwrap());
        let mut input = Document::default();
        let anchor = input.add_atom("C", Point::new(0.0, 0.0));
        let neighbor = input.add_atom("C", Point::new(42.0, 0.0));
        input.add_bond(anchor, neighbor, 1, "plain");
        let output = replace_preset(
            &input,
            &[anchor],
            "Ring",
            (preset, Spelling::Label),
            AttachmentPolicy::Terminal,
        )
        .unwrap_or_else(|error| panic!("{smiles}: {error}"));
        assert_eq!(output.atom(anchor).unwrap().explicit_h, 0, "{smiles}");
        document::prepare(&output).unwrap_or_else(|error| panic!("{smiles}: {error}"));
    }
}
