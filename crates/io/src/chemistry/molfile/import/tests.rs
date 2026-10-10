use super::*;
use crate::{
    chemistry::{molfile, smiles},
    document::Document,
};

const ALKANE_V2: &str =
    include_str!("../../../../../../tests/fixtures/mol-import-3d/alkane-3d-v2000.mol");
const ALKANE_V3: &str =
    include_str!("../../../../../../tests/fixtures/mol-import-3d/alkane-3d-v3000.mol");
const CHIRAL_V2: &str =
    include_str!("../../../../../../tests/fixtures/mol-import-3d/chiral-3d-v2000.mol");
const CHIRAL_V3: &str =
    include_str!("../../../../../../tests/fixtures/mol-import-3d/chiral-3d-v3000.mol");
const ALKENE_V2: &str =
    include_str!("../../../../../../tests/fixtures/mol-import-3d/alkene-3d-v2000.mol");
const ALKENE_V3: &str =
    include_str!("../../../../../../tests/fixtures/mol-import-3d/alkene-3d-v3000.mol");

fn drawing(imported: &Imported) -> anyhow::Result<Document> {
    let draft = imported.drawing()?;
    let labels = draft.labels()?;
    Ok(draft.finish_with(labels, |document| imported.restore_xyz(document))?)
}

fn smiles(imported: &Imported) -> anyhow::Result<String> {
    Ok(smiles::write::write(&imported.molecule.state, Default::default())?.text)
}

fn xyz(document: &Document) -> Vec<[f64; 3]> {
    document
        .atoms
        .iter()
        .map(|atom| {
            [
                f64::from(atom.position.x) / 28.,
                -f64::from(atom.position.y) / 28.,
                f64::from(atom.depth) / 28.,
            ]
        })
        .collect()
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn distances(points: &[[f64; 3]]) -> Vec<f64> {
    points
        .iter()
        .enumerate()
        .flat_map(|(index, &point)| {
            points[index + 1..]
                .iter()
                .map(move |&other| dot(sub(point, other), sub(point, other)).sqrt())
        })
        .collect()
}

fn torsion(points: &[[f64; 3]]) -> f64 {
    let a = sub(points[1], points[0]);
    let b = sub(points[2], points[1]);
    let c = sub(points[3], points[2]);
    let n = cross(a, b);
    let m = cross(b, c);
    let y = dot(cross(n, m), b) / dot(b, b).sqrt();
    y.atan2(dot(n, m))
}

#[test]
fn v2000_v3000_keep_xyz_and_fold_only_ordinary_hydrogens() -> anyhow::Result<()> {
    for text in [ALKANE_V2, ALKANE_V3] {
        let raw = molfile::read(text)?;
        let snapshot = serde_json::to_value(&raw)?;
        assert_eq!(raw.molecule.state.graph.atoms.len(), 20);
        let imported = raw.for_display()?;
        assert_eq!(imported.molecule.state.graph.atoms.len(), 6);
        assert_eq!(smiles(&imported)?, "CCCCCC");
        assert_eq!(
            inventory(&raw.molecule.state)?,
            inventory(&imported.molecule.state)?
        );
        let document = drawing(&imported)?;
        assert!(document.atoms.iter().any(|atom| atom.depth != 0.));
        for ((atom, position), &id) in document
            .atoms
            .iter()
            .zip(&imported.molecule.positions)
            .zip(&imported.molecule.ids)
        {
            assert_eq!(atom.id, id);
            assert_eq!(atom.position.x, (position.x * 28.) as f32);
            assert_eq!(atom.position.y, (-position.y * 28.) as f32);
            assert_eq!(atom.depth, (position.z * 28.) as f32);
        }
        assert_eq!(serde_json::to_value(&raw)?, snapshot);
        let bytes = document.file_json().map_err(anyhow::Error::msg)?;
        let native = Document::from_json(&bytes).map_err(anyhow::Error::msg)?;
        assert_eq!(native, document.current());
        let before = xyz(&document);
        let ids = document
            .atoms
            .iter()
            .map(|atom| atom.id)
            .collect::<Vec<_>>();
        let mut rotated = document.clone();
        crate::projection::tilt(&mut rotated, &ids, 37., true);
        let after = xyz(&rotated);
        assert!(
            before
                .iter()
                .zip(&after)
                .any(|(a, b)| (a[1] - b[1]).abs() > 0.01)
        );
        for (a, b) in distances(&before).into_iter().zip(distances(&after)) {
            assert!((a - b).abs() < 2e-6);
        }
        assert!((torsion(&before) - torsion(&after)).abs() < 2e-6);
    }
    Ok(())
}

#[test]
fn captured_chiral_and_alkene_stereo_survive_h_folding_and_edge_on_rotation() -> anyhow::Result<()>
{
    for (text, expected) in [
        (CHIRAL_V2, "F[C@H](Cl)Br"),
        (CHIRAL_V3, "F[C@H](Cl)Br"),
        (
            include_str!("../../../../../../tests/fixtures/mol-import-3d/chiral-s-3d-v3000.mol"),
            "F[C@@H](Cl)Br",
        ),
        (
            include_str!("../../../../../../tests/fixtures/mol-import-3d/chiral-h-first.mol"),
            "F[C@H](Cl)Br",
        ),
        (
            include_str!("../../../../../../tests/fixtures/mol-import-3d/chiral-h-middle.mol"),
            "F[C@H](Cl)Br",
        ),
        (ALKENE_V2, "F/C=C/F"),
        (ALKENE_V3, "F/C=C/F"),
        (
            include_str!("../../../../../../tests/fixtures/mol-import-3d/alkene-z-3d-v3000.mol"),
            "F/C=C\\F",
        ),
    ] {
        let imported = molfile::read(text)?.for_display()?;
        assert_eq!(smiles(&imported)?, expected);
        let document = drawing(&imported)?;
        let bytes = document.file_json().map_err(anyhow::Error::msg)?;
        let mut document = Document::from_json(&bytes).map_err(anyhow::Error::msg)?;
        for atom in &mut document.atoms {
            atom.cip_label = None;
        }
        for bond in &mut document.bonds {
            bond.cip_label = None;
        }
        let ids = document
            .atoms
            .iter()
            .map(|atom| atom.id)
            .collect::<Vec<_>>();
        for (angle, around_x) in [
            (45., true),
            (45., true),
            (-45., true),
            (-45., true),
            (45., false),
            (45., false),
            (-45., false),
            (-45., false),
        ] {
            crate::projection::tilt(&mut document, &ids, angle, around_x);
            let prepared = document::prepare(&document)?;
            assert_eq!(
                smiles::write::write(&prepared.state, Default::default())?.text,
                expected
            );
            for force_v3000 in [false, true] {
                let output =
                    molfile::write_projected_absolute(&prepared, molfile::Options { force_v3000 })?;
                assert_eq!(smiles(&molfile::read(&output)?.for_display()?)?, expected);
            }
        }
    }
    Ok(())
}

#[test]
fn mixed_ordinary_isotopic_and_unknown_h_keep_stable_stereo_and_ids() -> anyhow::Result<()> {
    for text in [
        include_str!("../../../../../../tests/fixtures/mol-import-3d/mixed-isotope-r-v2000.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/mixed-isotope-r-v3000.mol"),
    ] {
        let raw = molfile::read(text)?;
        let imported = raw.for_display()?;
        assert_eq!(raw.molecule.ids.len(), 5);
        assert_eq!(imported.molecule.ids.len(), 4);
        assert_eq!(smiles(&imported)?, "[2H][C@H](F)Cl");
        let isotope = imported
            .molecule
            .state
            .graph
            .atoms
            .iter()
            .position(|atom| atom.isotope == 2)
            .unwrap();
        let old = raw
            .molecule
            .ids
            .iter()
            .position(|id| *id == imported.molecule.ids[isotope])
            .unwrap();
        assert_eq!(
            imported.molecule.positions[isotope],
            raw.molecule.positions[old]
        );
        let mut document = drawing(&imported)?;
        let ids = document
            .atoms
            .iter()
            .map(|atom| atom.id)
            .collect::<Vec<_>>();
        for around_x in [true, false] {
            crate::projection::tilt(&mut document, &ids, 37., around_x);
            let prepared = document::prepare(&document)?;
            assert_eq!(
                smiles::write::write(&prepared.state, Default::default())?.text,
                "[2H][C@H](F)Cl"
            );
        }
    }
    for text in [
        include_str!("../../../../../../tests/fixtures/mol-import-3d/chiral-wavy-h-3d-v2000.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/chiral-wavy-h-3d-v3000.mol"),
    ] {
        let raw = molfile::read(text)?;
        let imported = raw.for_display()?;
        assert_eq!(imported.molecule.ids, raw.molecule.ids);
        let mut document = drawing(&imported)?;
        let ids = document
            .atoms
            .iter()
            .map(|atom| atom.id)
            .collect::<Vec<_>>();
        for around_x in [true, false] {
            crate::projection::tilt(&mut document, &ids, 37., around_x);
            assert!(
                document
                    .bonds
                    .iter()
                    .any(|bond| bond.order == 1 && bond.display == "wavy" && !bond.projection)
            );
            let prepared = document::prepare(&document)?;
            assert!(
                prepared
                    .state
                    .directions
                    .contains(&crate::chemistry::kekulize::Direction::Unknown)
            );
            assert!(
                prepared
                    .state
                    .metadata
                    .atoms
                    .iter()
                    .all(|atom| atom.chiral_tag == 0)
            );
        }
    }
    Ok(())
}

#[test]
fn essential_hydrogens_and_their_annotations_stay_visible() -> anyhow::Result<()> {
    for text in [
        include_str!("../../../../../../tests/fixtures/mol-import-3d/isotope-3d-v2000.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/isotope-3d-v3000.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/isotope-isolated.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/charged-h.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/charged-h-attached.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/radical-h.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/metal-h.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/bridging-h.mol"),
    ] {
        let raw = molfile::read(text)?;
        let imported = raw.for_display()?;
        assert_eq!(
            serde_json::to_value(&imported)?,
            serde_json::to_value(&raw)?
        );
        let document = drawing(&imported)?;
        assert!(document.atoms.iter().any(|atom| atom.element == "H"));
    }
    // The source's ordinary chiral H becomes essential when it has a map or
    // an attachment role. Those protections never rewrite its ID or property.
    for mapped in [true, false] {
        let mut raw = molfile::read(CHIRAL_V3)?;
        if mapped {
            raw.molecule.state.metadata.atoms[4].map_present = true;
            raw.molecule.state.metadata.atoms[4].map_number = 27;
        } else {
            raw.annotations.attachment_points[4] = Some(1);
        }
        assert_eq!(
            serde_json::to_value(raw.for_display()?)?,
            serde_json::to_value(&raw)?
        );
    }
    Ok(())
}

#[test]
fn ordinary_2d_controls_keep_their_current_drawing() -> anyhow::Result<()> {
    for text in [
        include_str!("../../../../../../tests/fixtures/mol-import-3d/alkane-2d.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/chiral-2d.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/alkene-2d.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/isotope-2d.mol"),
    ] {
        let raw = molfile::read(text)?;
        let old = raw.drawing()?;
        let labels = old.labels()?;
        let old = old.finish(labels)?;
        let imported = raw.for_display()?;
        assert_eq!(drawing(&imported)?, old);
    }
    Ok(())
}

#[test]
fn a_nonzero_z_is_retained_even_below_the_readers_stereo_threshold() -> anyhow::Result<()> {
    let mut imported = molfile::read(include_str!(
        "../../../../../../tests/fixtures/mol-import-3d/alkane-2d.mol"
    ))?;
    imported.molecule.positions[0].z = 0.000_01;
    assert!(!imported.annotations.is_3d);
    let document = drawing(&imported)?;
    assert_eq!(document.atoms[0].depth, (0.000_01 * 28.) as f32);
    Ok(())
}

#[test]
fn explicit_unknown_double_stereo_survives_native_and_projected_interchange() -> anyhow::Result<()>
{
    fn unknown(state: &perception::State) {
        let double = state
            .graph
            .bonds
            .iter()
            .position(|bond| bond.order == 2)
            .unwrap();
        assert_eq!(state.metadata.bonds[double].stereo, 1);
    }
    for text in [
        include_str!("../../../../../../tests/fixtures/mol-import-3d/alkene-unknown-3d-v2000.mol"),
        include_str!("../../../../../../tests/fixtures/mol-import-3d/alkene-unknown-3d-v3000.mol"),
    ] {
        let raw = molfile::read(text)?;
        unknown(&raw.molecule.state);
        let imported = raw.for_display()?;
        unknown(&imported.molecule.state);
        let document = drawing(&imported)?;
        let bytes = document.file_json().map_err(anyhow::Error::msg)?;
        let mut document = Document::from_json(&bytes).map_err(anyhow::Error::msg)?;
        let ids = document
            .atoms
            .iter()
            .map(|atom| atom.id)
            .collect::<Vec<_>>();
        for angle in [0., 37., 53., -53., -37.] {
            crate::projection::tilt(&mut document, &ids, angle, true);
            assert!(document.bonds.iter().any(|bond| bond.order == 2
                && bond.stereo.as_deref() == Some("any")
                && bond.stereo_authoritative));
            let prepared = document::prepare(&document)?;
            unknown(&prepared.state);
            for force_v3000 in [false, true] {
                let output =
                    molfile::write_projected_absolute(&prepared, molfile::Options { force_v3000 })?;
                unknown(&molfile::read(&output)?.molecule.state);
            }
        }
    }
    Ok(())
}
