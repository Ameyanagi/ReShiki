use super::*;
use crate::chemistry::stereo::cip::label;

const NOTICE: &str = "The drawing cannot keep the imported atropisomer axis; editing, Clean Up or export of this drawing will not include it";
const SPATIAL: &str =
    include_str!("../../../../../tests/fixtures/mol-atrop-import/geometry-824.mol");
const BINOL: &str = include_str!("../../../../../tests/fixtures/mol-atrop-import/binol-2d.mol");

#[tokio::test]
async fn molfile_atrop_warns_only_when_drawing_changes_axes() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    // The named counterexamples in #1053/#1059, plus the 3D MOL case from
    // #1065: standalone MOL sanitization keeps the 2D axes that RXN loses
    // or adds to, while the 3D MOL projections still lose their axes.
    for (name, text, changed) in [
        ("geometry/824", SPATIAL, true),
        (
            "geometry/1160",
            include_str!("../../../../../tests/fixtures/mol-atrop-import/geometry-1160.mol"),
            true,
        ),
        (
            "geometry/4327",
            include_str!("../../../../../tests/fixtures/mol-atrop-import/geometry-4327.mol"),
            false,
        ),
        (
            "geometry/3583",
            include_str!("../../../../../tests/fixtures/mol-atrop-import/geometry-3583.mol"),
            false,
        ),
        (
            "geometry/1506",
            include_str!("../../../../../tests/fixtures/mol-atrop-import/geometry-1506.mol"),
            true,
        ),
        (
            "geometry/4110",
            include_str!("../../../../../tests/fixtures/mol-atrop-import/geometry-4110.mol"),
            true,
        ),
        (
            "rings/pyrrole/17",
            include_str!("../../../../../tests/fixtures/mol-atrop-import/rings-pyrrole-17.mol"),
            true,
        ),
        (
            "rings/biaryl/31",
            include_str!("../../../../../tests/fixtures/mol-atrop-import/rings-biaryl-31.mol"),
            true,
        ),
        (
            "rings/amide/21",
            include_str!("../../../../../tests/fixtures/mol-atrop-import/rings-amide-21.mol"),
            true,
        ),
        (
            "rings/stilbene/34",
            include_str!("../../../../../tests/fixtures/mol-atrop-import/rings-stilbene-34.mol"),
            false,
        ),
    ] {
        let imported = molfile::read(text)?;
        let expected = axes(&imported.molecule)?;
        let file_xyz = imported.annotations.is_3d;
        assert!(!expected.is_empty(), "{name}");
        if name == "geometry/824" {
            assert!(imported.annotations.is_3d);
            assert_eq!(expected, [(4, 6, 6)]);
            let labeled = label::assign(&imported.molecule.state, &Default::default())?;
            assert_eq!(labeled.state.properties.bond_codes[1].as_deref(), Some("P"));
        }
        let (document, molecule, _) = imported.display_document()?;
        let drawn = axes(&molecular::prepare(&document)?)?;
        assert_eq!(drawn != expected, changed, "{name}");
        let warnings = if changed { vec![NOTICE] } else { vec![] };
        let Preparation::Complete(prepared) = prepare(Request::import("mol", text))? else {
            panic!("{name}: import unexpectedly deferred");
        };
        assert_eq!(prepared.warnings, warnings, "{name}");
        assert_eq!(prepared.file_xyz, file_xyz, "{name}");
        // Keep the old display construction and reader-based analysis state
        // byte-for-byte, including the file XYZ and projection flags.
        assert_eq!(
            serde_json::to_vec(&prepared.document)?,
            serde_json::to_vec(&document)?,
            "{name}"
        );
        assert_eq!(
            serde_json::to_vec(&prepared.molecule)?,
            serde_json::to_vec(&molecule)?,
            "{name}"
        );
        let response = engine
            .request(Request::import("mol", text))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(response.warnings, warnings, "{name}");
        assert_eq!(
            serde_json::to_vec(response.document.as_ref().unwrap())?,
            serde_json::to_vec(&document)?,
            "{name}"
        );
        assert!(response.analysis.is_some(), "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn molfile_atrop_kept_2d_axis_and_axis_free_3d_are_unchanged() -> anyhow::Result<()> {
    let engine = crate::engine::LocalEngine::in_process();
    for (text, expected_axes, spatial) in [
        (BINOL, vec![(1, 11, 6)], false),
        // Folding the first, ordinary terminal H changes indices but keeps
        // atom IDs, so matching axes by graph index would warn incorrectly.
        (
            include_str!("../../../../../tests/fixtures/mol-atrop-import/binol-2d-explicit-h.mol"),
            vec![(2, 12, 6)],
            false,
        ),
        (
            include_str!("../../../../../tests/fixtures/mol-import-3d/alkane-3d-v2000.mol"),
            vec![],
            true,
        ),
        (
            include_str!("../../../../../tests/fixtures/mol-import-3d/alkane-3d-v3000.mol"),
            vec![],
            true,
        ),
    ] {
        let imported = molfile::read(text)?;
        assert_eq!(axes(&imported.molecule)?, expected_axes);
        assert_eq!(imported.annotations.is_3d, spatial);
        let (document, molecule, _) = imported.display_document()?;
        assert_eq!(axes(&molecular::prepare(&document)?)?, expected_axes);
        let Preparation::Complete(prepared) = prepare(Request::import("mol", text))? else {
            panic!("Import unexpectedly deferred");
        };
        assert!(prepared.warnings.is_empty());
        assert_eq!(prepared.file_xyz, spatial);
        assert_eq!(
            serde_json::to_vec(&prepared.document)?,
            serde_json::to_vec(&document)?
        );
        assert_eq!(
            serde_json::to_vec(&prepared.molecule)?,
            serde_json::to_vec(&molecule)?
        );
        let response = engine
            .request(Request::import("mol", text))
            .await
            .map_err(anyhow::Error::msg)?;
        assert!(response.warnings.is_empty());
        assert_eq!(
            serde_json::to_vec(response.document.as_ref().unwrap())?,
            serde_json::to_vec(&document)?
        );
        if spatial {
            assert_eq!(response.analysis.unwrap().smiles, "CCCCCC");
        }
    }
    Ok(())
}

#[tokio::test]
async fn molfile_atrop_unpreparable_named_dummy_keeps_importing() -> anyhow::Result<()> {
    // #1063 initially rejected named dummy atoms when preparing the drawing.
    let text = SPATIAL
        .replace("COUNTS 6 5", "COUNTS 7 5")
        .replace("M  V30 END ATOM", "M  V30 7 R1 5 5 0 0\nM  V30 END ATOM");
    let imported = molfile::read(&text)?;
    assert_eq!(axes(&imported.molecule)?, [(4, 6, 6)]);
    let (document, _, _) = imported.display_document()?;
    assert!(
        molecular::prepare(&document)
            .unwrap_err()
            .to_string()
            .contains("Unknown element R1")
    );
    let response = crate::engine::LocalEngine::in_process()
        .request(Request::import("mol", &text))
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(response.warnings.is_empty());
    assert_eq!(
        serde_json::to_vec(response.document.as_ref().unwrap())?,
        serde_json::to_vec(&document)?
    );
    Ok(())
}

#[tokio::test]
async fn molfile_atrop_sdf_record_uses_the_same_warning() -> anyhow::Result<()> {
    let text = format!("{SPATIAL}>  <reference>\ngeometry/824\n\n$$$$\n");
    let engine = crate::engine::LocalEngine::in_process();
    let mol = engine
        .request(Request::import("mol", SPATIAL))
        .await
        .map_err(anyhow::Error::msg)?;
    let sdf = engine
        .request(Request::import("mol", &text))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(sdf.warnings, [NOTICE]);
    assert_eq!(serde_json::to_vec(&mol)?, serde_json::to_vec(&sdf)?);
    Ok(())
}
