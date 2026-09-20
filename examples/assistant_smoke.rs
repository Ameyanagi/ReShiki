//! Explicit, headless integration check. Does not open windows or touch clipboard.
use reshiki::{
    assistant::{self, codex},
    engine::PythonEngine,
};
use std::io::Write;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let account = codex::connect(Default::default()).await?;
    writeln!(
        std::io::stdout(),
        "Codex connected: {}; available models: {}",
        account.connected,
        account.models.len()
    )?;
    if !std::env::args().any(|a| a == "--generate") {
        return Ok(());
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let progress = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let _ = writeln!(std::io::stdout(), "{message:?}");
        }
    });
    let args: Vec<_> = std::env::args().collect();
    let option = |key: &str| {
        args.iter()
            .position(|a| a == key)
            .and_then(|i| args.get(i + 1))
    };
    let prompt = option("--prompt").cloned().unwrap_or_else(|| "Draw the esterification of acetic acid with ethanol to ethyl acetate and water. Label the molecules and put H2SO4, heat above the arrow.".into());
    let document = match option("--canvas") {
        Some(path) => serde_json::from_slice(&std::fs::read(path)?)?,
        None => Default::default(),
    };
    let canvas = assistant::canvas_tools::CanvasTools {
        canvas: std::sync::Arc::new(std::sync::RwLock::new(assistant::canvas_tools::Snapshot {
            document,
            ..Default::default()
        })),
        settings: Default::default(),
        replace: vec![],
        epoch: 0,
        revision: 0,
    };
    let proposal = codex::propose(
        prompt,
        Default::default(),
        Default::default(),
        tx,
        Some(canvas),
    )
    .await?;
    let output = std::path::PathBuf::from(
        option("--output")
            .map(String::as_str)
            .unwrap_or("artifacts/assistant-qa"),
    );
    let document =
        assistant::render(&PythonEngine::default(), &proposal, &Default::default()).await?;
    document.validate()?;
    std::fs::create_dir_all(&output)?;
    std::fs::write(
        output.join("proposal.json"),
        serde_json::to_string_pretty(&proposal)?,
    )?;
    std::fs::write(
        output.join("proposal.reshiki"),
        serde_json::to_string_pretty(&document)?,
    )?;
    std::fs::write(output.join("proposal.svg"), reshiki::scene::svg(&document))?;
    std::fs::write(
        output.join("proposal.png"),
        reshiki::export::drawing(&document, "png")?,
    )?;
    writeln!(
        std::io::stdout(),
        "Validated proposal: {} atoms, {} bonds, {} arrows, {} captions",
        document.atoms.len(),
        document.bonds.len(),
        document.arrows.len(),
        document.annotations.len()
    )?;
    let _ = progress.await;
    Ok(())
}
