//! One bounded image reconstruction using the unchanged Assistant backend.
use reshiki::assistant::{self, codex, settings::Preferences};
use serde_json::json;
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: reshiki_process_heap::BoundedHeap = reshiki_process_heap::BoundedHeap;

fn main() -> anyhow::Result<()> {
    // Chemical helpers relaunch current_exe. Dispatch before creating Tokio,
    // just like the application, so the frozen benchmark runner is sufficient.
    match std::env::args_os().nth(1).as_deref() {
        Some(flag) if flag == std::ffi::OsStr::new("--geometry-worker") => {
            reshiki::geometry::worker::run();
            return Ok(());
        }
        Some(flag) if flag == std::ffi::OsStr::new("--inchi-worker") => {
            reshiki::chemistry::inchi::worker::run();
            return Ok(());
        }
        _ => {}
    }
    tokio::runtime::Runtime::new()?.block_on(benchmark())
}

async fn benchmark() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let option = |key: &str| {
        args.iter()
            .position(|arg| arg == key)
            .and_then(|i| args.get(i + 1))
    };
    let source =
        PathBuf::from(option("--image").ok_or_else(|| anyhow::anyhow!("--image required"))?);
    let output =
        PathBuf::from(option("--output").ok_or_else(|| anyhow::anyhow!("--output required"))?);
    let requested_model = option("--model")
        .map(String::as_str)
        .unwrap_or("gpt-6.1-sol");
    let requested_effort = option("--effort").map(String::as_str).unwrap_or("xhigh");
    let seconds: u64 = option("--seconds")
        .map(String::as_str)
        .unwrap_or("300")
        .parse()?;
    std::fs::create_dir_all(&output)?;
    let started = Instant::now();
    let cancel = codex::Cancel::default();
    let account =
        tokio::time::timeout(Duration::from_secs(seconds), codex::connect(cancel.clone()))
            .await
            .map_err(|_| anyhow::anyhow!("Connection exceeded benchmark budget"))?
            .map_err(anyhow::Error::msg)?;
    if !account.connected {
        anyhow::bail!("Codex is not signed in");
    }
    let mut preferences = Preferences {
        model: Some(requested_model.into()),
        ..Default::default()
    };
    preferences
        .efforts
        .insert(requested_model.into(), requested_effort.into());
    // Availability is a guard, never permission to change the authorized model.
    let availability_error = match account
        .models
        .iter()
        .find(|model| model.id == requested_model)
    {
        None => Some("The exact requested model is absent from the Codex catalog"),
        Some(model)
            if !model
                .efforts
                .iter()
                .any(|effort| effort.id == requested_effort) =>
        {
            Some("The exact requested reasoning effort is not offered for the requested model")
        }
        Some(_) => None,
    };
    if let Some(reason) = availability_error {
        let report = json!({"status":"not_run","stage":"availability","reason":reason,
            "elapsed_seconds":started.elapsed().as_secs_f64(),
            "configuration":{"provider":"Codex app-server","requested_model":requested_model,
                "requested_effort":requested_effort,"auto_apply":false,"request_timeout_seconds":seconds}});
        std::fs::write(output.join("run.json"), serde_json::to_vec_pretty(&report)?)?;
        println!("{}", report);
        return Ok(());
    }
    let model = preferences
        .resolve(&account.models)
        .map_err(anyhow::Error::msg)?;
    let mut configuration = json!({"provider":"Codex app-server", "requested_model":requested_model,
        "model":model.id,"requested_effort":requested_effort,"effort":preferences.effort(model),
        "service_tier":preferences.tier(model),"auto_apply":false,"request_timeout_seconds":seconds});
    let image = reshiki::pictures::Picture::open(&source).map_err(anyhow::Error::msg)?;
    let blank = reshiki::document::Document::default();
    let settings = assistant::DrawingSettings::for_document(&blank);
    let canvas = assistant::canvas_tools::CanvasTools {
        canvas: std::sync::Arc::new(std::sync::RwLock::new(assistant::canvas_tools::Snapshot {
            document: blank.clone(),
            ..Default::default()
        })),
        settings: settings.clone(),
        replace: vec![],
        epoch: 0,
        revision: 0,
    };
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let progress_output = output.clone();
    let progress = tokio::spawn(async move {
        let mut snapshots = 0;
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            let elapsed = started.elapsed().as_secs_f64();
            match event {
                codex::Progress::Preview(doc) => {
                    snapshots += 1;
                    std::fs::write(progress_output.join(format!("preview-{snapshots}.rsk")), serde_json::to_vec_pretty(&doc)?)?;
                    std::fs::write(progress_output.join("last-preview.rsk"), serde_json::to_vec_pretty(&doc)?)?;
                    events.push(json!({"elapsed_seconds":elapsed,"event":"preview","atoms":doc.atoms.len(),"bonds":doc.bonds.len()}));
                }
                codex::Progress::Proposal(proposal) => {
                    std::fs::write(progress_output.join("proposal.json"), serde_json::to_vec_pretty(&proposal)?)?;
                    events.push(json!({"elapsed_seconds":elapsed,"event":"proposal"}));
                }
                codex::Progress::Started { model, effort } => events.push(json!({"elapsed_seconds":elapsed,"event":"started","model_label":model,"effort":effort})),
                codex::Progress::Checking { pass } => events.push(json!({"elapsed_seconds":elapsed,"event":"review","pass":pass})),
                _ => {}
            }
        }
        std::fs::write(
            progress_output.join("events.json"),
            serde_json::to_vec_pretty(&events)?,
        )?;
        Ok::<_, anyhow::Error>(events)
    });
    // The expected graph, name and reference annotations never enter this prompt.
    let prompt = "Reconstruct only the molecular structures visible in this image as editable objects. Preserve atom labels, connectivity, bond orders, formal charges, stereochemistry, abbreviations and disconnected fragments. Report unreadable or ambiguous assignments instead of guessing. Do not add a title.";
    let request = json!({"request":prompt,"conversation":[],"previous_proposal":null,
        "drawing_summary":{"atoms":0,"bonds":0,"arrows":0},"selected_ids":[],
        "placement":"add new drawing objects",
        "style":{"name":blank.drawing_style.name,"bond_length_pt":settings.bond_length * reshiki::style::DEFAULT.points_per_world(),"text":settings.format,"bond_color":settings.bond_color}}).to_string();
    let outcome = tokio::time::timeout(
        Duration::from_secs(seconds).saturating_sub(started.elapsed()),
        codex::propose_image(
            request,
            preferences,
            cancel.clone(),
            tx,
            Some(canvas),
            image,
        ),
    )
    .await;
    let (status, error, review) = match outcome {
        Ok(Ok(outcome)) => {
            std::fs::write(
                output.join("drawing.rsk"),
                serde_json::to_vec_pretty(&outcome.document)?,
            )?;
            std::fs::write(
                output.join("proposal.json"),
                serde_json::to_vec_pretty(&outcome.proposal)?,
            )?;
            let review = serde_json::to_value(&outcome.review)?;
            std::fs::write(
                output.join("review.json"),
                serde_json::to_vec_pretty(&review)?,
            )?;
            let candidate = assistant::candidate(&Default::default(), &outcome.document, &[]);
            std::fs::write(
                output.join("apply-validation.json"),
                serde_json::to_vec_pretty(
                    &json!({"accepted":candidate.is_ok(),"error":candidate.err()}),
                )?,
            )?;
            if !outcome.document.atoms.is_empty() {
                for (index, (_, png)) in assistant::review::images(&outcome.document)
                    .map_err(anyhow::Error::msg)?
                    .into_iter()
                    .enumerate()
                {
                    std::fs::write(output.join(format!("result-{index}.png")), png)?;
                }
            }
            (
                if outcome.document.atoms.is_empty() {
                    "clarification"
                } else {
                    "completed"
                },
                None,
                Some(review),
            )
        }
        Ok(Err(error)) => ("failed", Some(error), None),
        Err(_) => {
            cancel.stop();
            (
                "timeout",
                Some("Per-request benchmark budget exhausted".to_string()),
                None,
            )
        }
    };
    let events = progress.await??;
    configuration["generation_started"] = json!(
        events
            .iter()
            .filter(|event| event["event"] == "started")
            .collect::<Vec<_>>()
    );
    let retained = output.join("last-preview.rsk");
    if !output.join("apply-validation.json").exists() && retained.exists() {
        let document = reshiki::document::Document::from_json(&std::fs::read(retained)?)
            .map_err(anyhow::Error::msg)?;
        let candidate = assistant::candidate(&Default::default(), &document, &[]);
        std::fs::write(
            output.join("apply-validation.json"),
            serde_json::to_vec_pretty(
                &json!({"accepted":candidate.is_ok(),"error":candidate.err(),"retained_preview":true}),
            )?,
        )?;
    }
    let report = json!({"status":status,"error":error,"elapsed_seconds":started.elapsed().as_secs_f64(),"configuration":configuration,"review":review});
    std::fs::write(output.join("run.json"), serde_json::to_vec_pretty(&report)?)?;
    writeln!(std::io::stdout(), "{}", serde_json::to_string(&report)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::{Document, Point};

    #[test]
    fn ordinary_apply_validation_rejects_malformed_graph_without_mutating_base() {
        let mut base = Document::default();
        base.add_atom("C", Point::default());
        let original = base.clone();
        let mut ambiguous = Document::default();
        let a = ambiguous.add_atom("N", Point::default());
        let b = ambiguous.add_atom("O", Point::new(42., 0.));
        ambiguous.add_bond(a, b, 1, "plain");
        // A chemically wrong but structurally valid assignment can still be
        // applied. Candidate acceptance is explicitly not graph correctness.
        assert!(assistant::candidate(&base, &ambiguous, &[]).is_ok());
        ambiguous.bonds[0].b = 999;
        assert!(assistant::candidate(&base, &ambiguous, &[]).is_err());
        assert_eq!(base, original);
    }
}
