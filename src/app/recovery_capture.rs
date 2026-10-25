//! Opt-in real-renderer recovery fixtures, also runnable on the unmodified base.
use super::App;
use iced::advanced::{renderer::Headless, widget::Tree};
use reshiki::{
    engine::{LocalEngine, Request},
    recovery::{Recovery, Snapshot},
};
use std::path::Path;

pub(super) fn snapshot(app: &mut App, renderer: &mut iced::Renderer, path: &Path, width: f32) {
    let size = iced::Size::new(width, 820.);
    let mut tree = Tree::empty();
    for _ in 0..3 {
        let messages =
            super::workspace::layout_snapshots::pass(app, renderer, &mut tree, size, None);
        if messages.is_empty() {
            break;
        }
        for message in messages {
            let _ = app.update(message);
        }
    }
    let _ = super::workspace::layout_snapshots::pass(app, renderer, &mut tree, size, Some(path));
}

#[tokio::test]
#[ignore = "Capture with isolated RESHIKI_DATA_DIR, RESHIKI_RECOVERY_QA_INPUT and RESHIKI_RECOVERY_QA_OUTPUT"]
async fn recovery_banner_evidence() -> anyhow::Result<()> {
    let input = std::path::PathBuf::from(
        std::env::var_os("RESHIKI_RECOVERY_QA_INPUT").expect("Fixture folder"),
    );
    let output = std::path::PathBuf::from(
        std::env::var_os("RESHIKI_RECOVERY_QA_OUTPUT").expect("Capture folder"),
    );
    std::env::var_os("RESHIKI_DATA_DIR").expect("Isolated app data");
    std::fs::create_dir_all(&output)?;
    let engine = LocalEngine::default();
    let saved_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .saturating_sub(86400);
    for count in [1, 3, 100] {
        let drafts = input.join(format!("data-{count}")).join("recovery");
        std::fs::create_dir_all(&drafts)?;
        let molecules = [
            (Some("benzene.rsk"), "c1ccccc1"),
            (Some("aspirin.rsk"), "CC(=O)Oc1ccccc1C(=O)O"),
            (None, "CCO"),
        ];
        for index in 0..count {
            let (source, smiles) = molecules[index % molecules.len()];
            let source = if count == 100 && index == 0 {
                Some("benzene-recovery-review-after-a-very-long-session-name.rsk")
            } else {
                source
            };
            let path = drafts.join(format!("4294967294-{index}.json"));
            if path.exists() {
                continue;
            }
            let document = engine
                .request(Request::import_smiles(smiles))
                .await
                .map_err(anyhow::Error::msg)?
                .document
                .expect("Fixture molecule");
            let snapshot = Snapshot {
                document,
                source: source.map(Into::into),
                saved_at: saved_at.saturating_sub(index as u64 * 86400),
            };
            std::fs::write(path, serde_json::to_vec(&snapshot)?)?;
        }
        let store = Recovery::in_directory(&drafts).map_err(anyhow::Error::msg)?;
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.recovered = store.candidates();
        assert_eq!(app.recovered.len(), count);
        app.status.clear();
        app.tab.camera.zoom = 1.;
        let mut renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .expect("Renderer");
        for width in [1280., 1040.] {
            snapshot(
                &mut app,
                &mut renderer,
                &output.join(format!("drafts-{count}-{width}.png")),
                width,
            );
        }
    }
    Ok(())
}
