//! The save dialog names what a save in ChemDraw format loses from the file:
//! the count must match what the writer leaves out.
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki_io::{
    engine::LocalEngine,
    formats::{self, ChemdrawLoss},
};
use std::path::Path;

/// The loss the dialog reports for the file, after checking it against the
/// file as the writer saves it again.
async fn loss(name: &str) -> ChemdrawLoss {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/chemdraw-import-corpus/supported");
    let engine = LocalEngine::in_process();
    let source = std::fs::read_to_string(root.join(format!("{name}.cdxml"))).unwrap();
    let expected = formats::chemdraw_loss("cdxml", &source);
    let binary = std::fs::read(root.join(format!("{name}.cdx"))).unwrap();
    assert_eq!(
        formats::chemdraw_loss("cdx", &STANDARD.encode(&binary)),
        expected,
        "{name}.cdx"
    );
    for (format, data) in [("cdxml", source.into_bytes()), ("cdx", binary)] {
        let document = formats::read_with_engine(data, format, &engine)
            .await
            .unwrap();
        for saved in ["cdxml", "cdx"] {
            let bytes = formats::write_with_engine(document.clone(), saved, &engine)
                .await
                .unwrap();
            let contents = reshiki_io::compatibility::import_text(saved, bytes).unwrap();
            if saved == "cdxml" {
                assert_eq!(contents.matches("<page").count(), 1, "{name}.{format}");
                assert!(!contents.contains("<scheme") && !contents.contains("<step"));
            }
            assert_eq!(
                formats::chemdraw_loss(saved, &contents),
                ChemdrawLoss::default(),
                "{name}.{format} saved as {saved} keeps none of it"
            );
        }
    }
    expected
}

#[tokio::test]
async fn a_chemdraw_save_loses_the_schemes_steps_and_pages_it_counts() {
    assert_eq!(
        loss("075-multischeme-4-schemes-3-steps-3").await,
        ChemdrawLoss {
            schemes: 4,
            steps: 12,
            pages: 0,
            unsupported: 0,
        }
    );
    assert_eq!(
        loss("105-freshmix-seeded-mixed-001").await,
        ChemdrawLoss {
            schemes: 9,
            steps: 18,
            pages: 2,
            unsupported: 0,
        }
    );
}

#[tokio::test]
async fn a_file_without_schemes_or_extra_pages_loses_nothing() {
    assert_eq!(loss("001-molecule-chain").await, ChemdrawLoss::default());
    assert_eq!(
        formats::chemdraw_loss("mol", "not ChemDraw"),
        ChemdrawLoss::default()
    );
    assert_eq!(
        formats::chemdraw_loss("cdxml", "<CDXML"),
        ChemdrawLoss::default()
    );
}
