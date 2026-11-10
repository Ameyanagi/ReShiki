//! The imported drawing must give naming the same E/Z as the bare SMILES.
//! Naming uses this source's built binary through RESHIKI_NAMING_HELPER, as CI
//! configures for the other naming integration tests.
#![cfg(not(target_arch = "wasm32"))]

use anyhow::Context as _;
use reshiki::{
    engine::{LocalEngine, Request},
    naming::{self, Cancel, Record},
};

async fn imported_name(engine: &LocalEngine, text: &str) -> anyhow::Result<Record> {
    let imported = engine
        .request(Request::import_smiles(text))
        .await
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("Importing {text}"))?;
    let document = imported.document.context("Missing imported drawing")?;
    let identity = naming::document_identity(&document)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("Reading naming identity for {text}"))?;
    naming::generate_name(&identity, Cancel::default())
        .await
        .with_context(|| format!("Generating name for {text}"))
}

#[tokio::test]
async fn wedge_flags_keep_stated_alkene_names_through_imported_drawings() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    for (text, expected) in [
        ("[H]/N=C/F |wU:1.0|", None),
        ("[H]/N=C/F |wD:1.0|", None),
        (r"[H]\N=C/F |wU:1.0|", None),
        (
            "[H]/N=C/F |(0,0,;1,0,;0,1,;-1,-1,;1,1,;0,-1,),wU:1.0|",
            None,
        ),
        ("[H]/N=C/F |(-1,1,;0,0,;1,0,;2,-1,),wU:1.0|", None),
        ("C/C=C/C |wU:0.0|", Some("(2E)-but-2-ene")),
        (r"C/C=C\C |wU:0.0|", Some("(2Z)-but-2-ene")),
        ("C/C=C/C |wU:1.0|", Some("(2E)-but-2-ene")),
    ] {
        let bare = text.split_once(' ').context("Missing CX suffix")?.0;
        let expected_name = imported_name(&engine, bare).await?;
        let actual = imported_name(&engine, text).await?;
        assert_eq!(actual.title, expected_name.title, "{text}");
        assert_eq!(
            actual.systematic_name, expected_name.systematic_name,
            "{text}"
        );
        assert_eq!(
            actual.canonical_smiles, expected_name.canonical_smiles,
            "{text}"
        );
        if let Some(expected) = expected {
            assert_eq!(actual.title, expected, "{text}");
        }
    }
    Ok(())
}
