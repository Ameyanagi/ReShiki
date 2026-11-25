//! The save dialog counts metadata lost from the original ChemDraw file.
//! The writer can build new schemes from the imported drawing.
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki_io::{
    engine::LocalEngine,
    formats::{self, ChemdrawLoss},
};
use std::path::Path;

/// The loss the dialog reports for the file, after checking it against the
/// file as the writer saves it again.
async fn loss(name: &str, rebuilt: (usize, usize)) -> ChemdrawLoss {
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
        assert!(document.reactions.is_empty(), "{name}.{format}");
        for saved in ["cdxml", "cdx"] {
            let bytes = formats::write_with_engine(document.clone(), saved, &engine)
                .await
                .unwrap();
            let contents = reshiki_io::compatibility::import_text(saved, bytes).unwrap();
            if saved == "cdxml" {
                assert_eq!(contents.matches("<page").count(), 1, "{name}.{format}");
            }
            assert_eq!(
                formats::chemdraw_loss(saved, &contents),
                ChemdrawLoss {
                    schemes: rebuilt.0,
                    steps: rebuilt.1,
                    ..Default::default()
                },
                "{name}.{format} saved as {saved}"
            );
        }
    }
    expected
}

#[tokio::test]
async fn a_chemdraw_save_rebuilds_schemes_and_flattens_pages() {
    assert_eq!(
        loss("075-multischeme-4-schemes-3-steps-3", (4, 12)).await,
        ChemdrawLoss {
            schemes: 4,
            steps: 12,
            pages: 0,
            unsupported: 0,
        }
    );
    assert_eq!(
        loss("105-freshmix-seeded-mixed-001", (9, 18)).await,
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
    assert_eq!(
        loss("001-molecule-chain", (0, 0)).await,
        ChemdrawLoss::default()
    );
    assert_eq!(
        formats::chemdraw_loss("mol", "not ChemDraw"),
        ChemdrawLoss::default()
    );
    assert_eq!(
        formats::chemdraw_loss("cdxml", "<CDXML"),
        ChemdrawLoss::default()
    );
}
