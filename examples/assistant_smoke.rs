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
    if let Some(path) = option("--prepare-image") {
        let output = std::path::PathBuf::from(
            option("--output")
                .map(String::as_str)
                .unwrap_or("artifacts/image-handoff"),
        );
        let picture = reshiki::pictures::Picture::open(std::path::Path::new(path))
            .map_err(anyhow::Error::msg)?;
        let png = picture.png_on_white().map_err(anyhow::Error::msg)?;
        std::fs::create_dir_all(&output)?;
        std::fs::write(output.join("source.png"), png)?;
        writeln!(
            std::io::stdout(),
            "Prepared opaque source: {} × {} pixels",
            picture.width(),
            picture.height()
        )?;
        return Ok(());
    }
    if let Some(path) = option("--render").or_else(|| option("--render-proposal")) {
        let doc = if option("--render-proposal").is_some() {
            let proposal: assistant::Proposal = serde_json::from_slice(&std::fs::read(path)?)?;
            proposal.validate().map_err(anyhow::Error::msg)?;
            assistant::render(&Default::default(), &proposal, &Default::default())
                .await
                .map_err(anyhow::Error::msg)?
        } else {
            serde_json::from_slice(&std::fs::read(path)?)?
        };
        let output = std::path::PathBuf::from(
            option("--output")
                .map(String::as_str)
                .unwrap_or("artifacts/assistant-qa/render"),
        );
        std::fs::create_dir_all(&output)?;
        std::fs::write(
            output.join("drawing.rsk"),
            serde_json::to_string_pretty(&doc)?,
        )?;
        for (index, (_, png)) in assistant::review::images(&doc)
            .map_err(anyhow::Error::msg)?
            .into_iter()
            .enumerate()
        {
            std::fs::write(output.join(format!("image-{index}.png")), png)?;
        }
        return Ok(());
    }
    let provider: assistant::provider::Provider = option("--provider")
        .map(|s| match s.to_lowercase().as_str() {
            "openai" | "openai-compatible" | "custom" => assistant::provider::Provider::OpenAI,
            "anthropic" | "claude" => assistant::provider::Provider::Anthropic,
            _ => assistant::provider::Provider::Codex,
        })
        .unwrap_or(assistant::provider::Provider::Codex);
    let mut preferences = assistant::settings::Preferences {
        provider,
        ..Default::default()
    };
    if let Some(model) = option("--model") {
        match provider {
            assistant::provider::Provider::OpenAI => preferences.openai_model = Some(model.clone()),
            assistant::provider::Provider::Anthropic => {
                preferences.anthropic_model = Some(model.clone())
            }
            assistant::provider::Provider::Codex => preferences.model = Some(model.clone()),
        }
    }
    if let Some(base_url) = option("--base-url") {
        match provider {
            assistant::provider::Provider::OpenAI => preferences.openai_base_url = base_url.clone(),
            assistant::provider::Provider::Anthropic => {
                preferences.anthropic_base_url = base_url.clone()
            }
            assistant::provider::Provider::Codex => {}
        }
    }
    let account = assistant::http::connect(provider, &preferences, Default::default())
        .await
        .map_err(anyhow::Error::msg)?;
    writeln!(
        std::io::stdout(),
        "{} connected: {}; available models: {}",
        provider.label(),
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
        let source = option("--image")
            .map(|path| reshiki::pictures::Picture::open(std::path::Path::new(path)))
            .transpose()
            .map_err(anyhow::Error::msg)?;
        assistant::http::improve_with_image(
            provider,
            prompt,
            preferences,
            Default::default(),
            tx,
            canvas,
            assistant::review::Outcome {
                proposal,
                document,
                review: Default::default(),
            },
            source,
        )
        .await
    } else if let Some(path) = option("--image") {
        assistant::http::propose_image(
            provider,
            option("--prompt").cloned().unwrap_or_else(|| "Reconstruct and clean up the chemical drawing in this image, preserving the depicted chemistry and arrangement.".into()),
            preferences, Default::default(), tx, Some(canvas),
            reshiki::pictures::Picture::open(std::path::Path::new(path)).map_err(anyhow::Error::msg)?,
        ).await
    } else {
        assistant::http::propose(
            provider,
            prompt,
            preferences,
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
        output.join("proposal.rsk"),
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
