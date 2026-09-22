//! Explicit, headless integration check. Does not open windows or touch clipboard.
use reshiki::assistant::{self, codex};
use std::io::Write;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let option = |key: &str| {
        args.iter()
            .position(|a| a == key)
            .and_then(|i| args.get(i + 1))
    };
    if let Some(path) = option("--render") {
        let doc = serde_json::from_slice(&std::fs::read(path)?)?;
        let output = std::path::PathBuf::from(
            option("--output")
                .map(String::as_str)
                .unwrap_or("artifacts/assistant-qa/render"),
        );
        std::fs::create_dir_all(&output)?;
        for (index, (_, png)) in assistant::review::images(&doc)
            .map_err(anyhow::Error::msg)?
            .into_iter()
            .enumerate()
        {
            std::fs::write(output.join(format!("image-{index}.png")), png)?;
        }
        return Ok(());
    }
    let account = codex::connect(Default::default())
        .await
        .map_err(anyhow::Error::msg)?;
    writeln!(
        std::io::stdout(),
        "Codex connected: {}; available models: {}",
        account.connected,
        account.models.len()
    )?;
    if !std::env::args().any(|a| a == "--generate" || a == "--improve") {
        return Ok(());
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let progress = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let summary = match message {
                codex::Progress::Preview(doc) => format!(
                    "Draft: {} atoms, {} panels",
                    doc.atoms.len(),
                    doc.reactions.len()
                ),
                codex::Progress::Proposal(_) => "Structured proposal received".into(),
                other => format!("{other:?}"),
            };
            let _ = writeln!(std::io::stdout(), "{summary}");
        }
    });
    let prompt = option("--prompt").cloned().unwrap_or_else(|| "Draw the esterification of acetic acid with ethanol to ethyl acetate and water. Label the molecules and put H2SO4, heat above the arrow.".into());
    let document: reshiki::document::Document = match option("--canvas") {
        Some(path) => serde_json::from_slice(&std::fs::read(path)?)?,
        None => Default::default(),
    };
    let canvas = assistant::canvas_tools::CanvasTools {
        canvas: std::sync::Arc::new(std::sync::RwLock::new(assistant::canvas_tools::Snapshot {
            document: document.clone(),
            ..Default::default()
        })),
        settings: Default::default(),
        replace: vec![],
        epoch: 0,
        revision: 0,
    };
    let outcome = if args.iter().any(|s| s == "--improve") {
        let proposal = assistant::Proposal {
            explanation: "Layout review".into(),
            ..Default::default()
        };
        codex::improve(
            prompt,
            Default::default(),
            Default::default(),
            tx,
            canvas,
            assistant::review::Outcome {
                proposal,
                document,
                review: Default::default(),
            },
        )
        .await
    } else {
        codex::propose(
            prompt,
            Default::default(),
            Default::default(),
            tx,
            Some(canvas),
        )
        .await
    }
    .map_err(anyhow::Error::msg)?;
    let output = std::path::PathBuf::from(
        option("--output")
            .map(String::as_str)
            .unwrap_or("artifacts/assistant-qa"),
    );
    let document = outcome.document;
    let proposal = outcome.proposal;
    document.validate().map_err(anyhow::Error::msg)?;
    std::fs::create_dir_all(&output)?;
    std::fs::write(
        output.join("review.json"),
        serde_json::to_string_pretty(&outcome.review)?,
    )?;
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
        assistant::canvas_tools::image(&document).map_err(anyhow::Error::msg)?,
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
