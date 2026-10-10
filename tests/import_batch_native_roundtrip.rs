//! Combined import-batch persistence using original native captures and MOL input.
use anyhow::Context;
use reshiki::{
    document::{Atom, Document, Point, VERSION},
    editing,
    engine::{LocalEngine, Request},
};

const ALKANE_NATIVE: &[u8] =
    include_bytes!("../docs/changes/evidence/mol-import-3d/candidate-alkane-3d-desktop.rsk");
const EMF_NATIVE: &[u8] = include_bytes!(
    "../docs/changes/fixtures/emf-import/windows-native-20261010/desktop-import-20261010.rsk"
);
const ORIGINAL_EMF: &[u8] =
    include_bytes!("../docs/changes/fixtures/emf-import/windows-native-20261010/source.emf");
const PROTECTED_H_MOL: &str = include_str!("fixtures/mol-import-3d/mixed-isotope-r-v2000.mol");

fn atom(document: &Document, id: u64) -> anyhow::Result<&Atom> {
    document
        .atom(id)
        .with_context(|| format!("Missing atom {id}"))
}

#[tokio::test]
async fn saved_mol_xyz_and_retained_emf_share_a_native_append_roundtrip() -> anyhow::Result<()> {
    // These are actual saved files from PR282's desktop and PR285's Windows
    // worker acceptance. Loading the latter uses its portable stored preview.
    let mut combined = Document::from_native_file(ALKANE_NATIVE).map_err(anyhow::Error::msg)?;
    let emf = Document::from_native_file(EMF_NATIVE).map_err(anyhow::Error::msg)?;
    assert_eq!(combined.version, 19);
    assert_eq!(emf.version, 20);
    assert_eq!((combined.atoms.len(), combined.bonds.len()), (6, 5));
    assert!(combined.graphics.is_empty());
    assert!(emf.atoms.is_empty() && emf.bonds.is_empty());
    assert_eq!(emf.graphics.len(), 1);

    // Golden canvas XYZ and folded H counts are from the original saved
    // hexane, independently checked against the source MOL in its receipt.
    let expected_xyz: [(u64, f32, f32, f32, u32); 6] = [
        (1, -83.3868, -14.1484, 6.2692, 3),
        (2, -42.7336, -8.6688, 17.7352, 2),
        (3, -21.3808, 14.6244, -11.1552, 2),
        (4, 18.8636, 23.6236, 0.5852, 2),
        (5, 44.0244, -10.934, 2.9904, 2),
        (6, 84.3332, -0.2128, 11.6228, 3),
    ];
    for (id, x, y, z, h) in expected_xyz {
        let a = atom(&combined, id)?;
        assert_eq!(a.element, "C");
        assert_eq!((a.position.x, a.position.y, a.depth), (x, y, z));
        assert_eq!((a.explicit_h, a.label_h), (h, h));
        assert!(!a.no_implicit);
    }
    let source_graphic = emf.graphics.first().context("Missing EMF graphic")?;
    let source_picture = source_graphic.picture.as_ref().context("Missing preview")?;
    assert_eq!(source_picture.emf(), Some(ORIGINAL_EMF));
    assert_eq!(
        (source_picture.width(), source_picture.height()),
        (4724, 2834)
    );

    // The saved hexane has no visible H. This authentic MOL separately proves
    // that ordinary H folds while isotope-protected H and its stereo survive.
    let response = LocalEngine::default()
        .request(Request::import("mol", PROTECTED_H_MOL))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        response
            .analysis
            .as_ref()
            .context("Missing analysis")?
            .smiles,
        "[2H][C@H](F)Cl"
    );
    let protected = response.document.context("Missing protected-H drawing")?;
    assert_eq!(protected.all_ids(), [1, 2, 3, 4]);
    assert_eq!((protected.atoms.len(), protected.bonds.len()), (4, 3));
    let d = atom(&protected, 4)?;
    assert_eq!(
        (&*d.element, d.isotope, d.charge, d.radical_electrons),
        ("H", 2, 0, 0)
    );
    assert_eq!(
        (d.position.x, d.position.y, d.depth),
        (-10.696, 16.0132, -26.1828)
    );
    assert_eq!(
        protected.atoms.iter().filter(|a| a.element == "H").count(),
        1
    );
    let source_carbon = atom(&protected, 2)?;
    assert_eq!(source_carbon.explicit_h, 1);
    let source_stereo = source_carbon
        .stereo
        .as_ref()
        .context("Missing protected-H stereo")?;

    let picture_offset = Point::new(200., 100.);
    assert_eq!(editing::append(&mut combined, &emf, picture_offset), [7]);
    let protected_offset = Point::new(-200., 100.);
    assert_eq!(
        editing::append(&mut combined, &protected, protected_offset),
        [8, 9, 10, 11]
    );
    combined.validate().map_err(anyhow::Error::msg)?;
    assert_eq!(
        (
            combined.atoms.len(),
            combined.bonds.len(),
            combined.graphics.len()
        ),
        (10, 8, 1)
    );
    assert_eq!(combined.all_ids(), [1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 7]);
    assert_eq!(
        combined
            .bonds
            .iter()
            .map(|b| (b.a, b.b, b.order))
            .collect::<Vec<_>>(),
        [
            (1, 2, 1),
            (2, 3, 1),
            (3, 4, 1),
            (4, 5, 1),
            (5, 6, 1),
            (8, 9, 1),
            (9, 10, 1),
            (9, 11, 1)
        ]
    );
    for before in &protected.atoms {
        let after = atom(&combined, before.id + 7)?;
        assert_eq!(
            after.position,
            before
                .position
                .offset(protected_offset.x, protected_offset.y)
        );
        assert_eq!(after.depth, before.depth);
        assert_eq!(
            (&after.element, after.isotope, after.explicit_h),
            (&before.element, before.isotope, before.explicit_h)
        );
    }
    let appended_stereo = atom(&combined, 9)?
        .stereo
        .as_ref()
        .context("Missing appended stereo")?;
    assert_eq!(appended_stereo.winding, source_stereo.winding);
    assert_eq!(
        appended_stereo.neighbors,
        source_stereo
            .neighbors
            .iter()
            .map(|id| id + 7)
            .collect::<Vec<_>>()
    );
    let mut expected_graphic = source_graphic.clone();
    expected_graphic.id = 7;
    expected_graphic.origin = source_graphic
        .origin
        .offset(picture_offset.x, picture_offset.y);
    assert_eq!(combined.graphics.first(), Some(&expected_graphic));

    // Exercise disk bytes and the public native loader, including the format
    // upgrade. The loader intentionally clears derived CIP labels only.
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("mol-xyz-with-retained-emf.rsk");
    std::fs::write(&file, combined.file_json().map_err(anyhow::Error::msg)?)?;
    let reopened =
        Document::from_native_file(&std::fs::read(&file)?).map_err(anyhow::Error::msg)?;
    assert_eq!(reopened.version, VERSION);
    let mut expected = combined.current();
    reshiki::atom_labels::clear_computed(&mut expected);
    assert_eq!(reopened, expected);
    let reopened_picture = reopened
        .graphics
        .first()
        .and_then(|g| g.picture.as_ref())
        .context("Missing reopened EMF")?;
    assert_eq!(reopened_picture.emf(), Some(ORIGINAL_EMF));
    assert_eq!(reopened_picture.png(), source_picture.png());
    assert_eq!(
        (reopened_picture.width(), reopened_picture.height()),
        (4724, 2834)
    );
    assert_eq!(atom(&reopened, 11)?.isotope, 2);
    assert_eq!(atom(&reopened, 9)?.explicit_h, 1);
    Ok(())
}
