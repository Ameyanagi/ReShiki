use super::*;

#[tokio::test]
async fn allene_inchi_import_surfaces_the_native_loss_diagnostic() -> anyhow::Result<()> {
    let config = if std::env::var_os("RESHIKI_INCHI_HELPER").is_some() {
        native_response::Config::new(helper::discover()?)
    } else {
        assert!(std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none());
        native_response::Config::in_process()
    };
    let outcome = execute(
        Request::import("inchi", "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3/t5-/m0/s1"),
        Some(config),
    )
    .await?;
    let Outcome::Complete(response) = outcome else {
        panic!("InChI import unexpectedly deferred");
    };
    assert!(response.document.is_some());
    assert!(
        response.warnings.iter().any(|warning| warning
            .contains("Allene-style stereochemistry is not supported yet and will be ignored.")),
        "{:?}",
        response.warnings
    );
    Ok(())
}
