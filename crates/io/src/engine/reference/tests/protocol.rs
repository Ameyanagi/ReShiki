//! Exercise the private JSON-lines transport with an independent fake child.
use super::super::{PythonEngine, Worker, reference_python};
use anyhow::Context;
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{io::BufReader, process::Command};

async fn fixture_engine() -> anyhow::Result<PythonEngine> {
    let root = &crate::repository_root();
    let explicit = crate::compatibility::environment("REFERENCE_PYTHON")
        .or_else(|| crate::compatibility::environment("PYTHON"))
        .map(std::path::PathBuf::from);
    let python = reference_python(root, explicit)?;
    let mut child = Command::new(python)
        .arg("-u")
        .arg(root.join("reference/worker_exchange_fixture.py"))
        // Match the production worker: Windows pipes otherwise use the ANSI code page.
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let worker = Worker {
        input: child.stdin.take().context("Missing fixture input")?,
        output: BufReader::new(child.stdout.take().context("Missing fixture output")?),
        _child: child,
        next_id: 1,
    };
    let engine = PythonEngine::default();
    *engine.worker.lock().await = Some(worker);
    Ok(engine)
}

#[tokio::test]
async fn results_preserve_json_shapes_and_sequential_worker_identity() -> anyhow::Result<()> {
    let engine = fixture_engine().await?;
    let child_id = engine.worker.lock().await.as_ref().unwrap()._child.id();
    let values = [
        Value::Null,
        json!(true),
        json!(1.25),
        json!("分子"),
        json!([null, 42, {"empty": []}]),
        json!({"document": {"graphics": []}, "output": "x".repeat(256 * 1024)}),
    ];
    for (index, expected) in values.into_iter().enumerate() {
        let actual = engine
            .exchange(json!({"action": "echo", "payload": expected}))
            .await
            .map_err(anyhow::Error::msg)?;
        anyhow::ensure!(actual == expected, "JSON result changed in case {index}");
        let slot = engine.worker.lock().await;
        let worker = slot
            .as_ref()
            .context("Successful exchange removed worker")?;
        anyhow::ensure!(
            worker._child.id() == child_id,
            "Successful exchange replaced worker"
        );
        anyhow::ensure!(
            worker.next_id == index as u64 + 2,
            "Request ID sequence changed"
        );
    }
    Ok(())
}

#[tokio::test]
async fn response_errors_preserve_validation_order_and_reset_worker() -> anyhow::Result<()> {
    let cases = [
        (
            json!({"id": 99, "ok": false, "error": "wrong status"}),
            "Chemistry response ID mismatch",
        ),
        (json!({"ok": false, "error": "Rejected"}), "Rejected"),
        (json!({"ok": false, "error": 42}), "Chemistry error"),
        (json!({"ok": true}), "Missing chemistry result"),
        (json!({"result": "ignored"}), "Chemistry error"),
        (json!({"ok": "true", "result": null}), "Chemistry error"),
        (Value::Null, "Chemistry response ID mismatch"),
    ];
    for (response, expected) in cases {
        let engine = fixture_engine().await?;
        let actual = engine
            .exchange(json!({"action": "reply", "response": response}))
            .await;
        anyhow::ensure!(
            actual.err().as_deref() == Some(expected),
            "Response error changed"
        );
        anyhow::ensure!(
            engine.worker.lock().await.is_none(),
            "Response error retained worker"
        );
    }
    let engine = fixture_engine().await?;
    let missing_id = engine
        .exchange(json!({
            "action": "reply", "attach_id": false,
            "response": {"ok": false, "error": "Rejected"}
        }))
        .await;
    anyhow::ensure!(missing_id.err().as_deref() == Some("Chemistry response ID mismatch"));
    anyhow::ensure!(engine.worker.lock().await.is_none());
    Ok(())
}

#[tokio::test]
async fn transport_and_request_errors_reset_worker() -> anyhow::Result<()> {
    for action in ["malformed", "eof"] {
        let engine = fixture_engine().await?;
        let error = engine
            .exchange(json!({"action": action}))
            .await
            .expect_err("Broken transport unexpectedly succeeded");
        if action == "eof" {
            anyhow::ensure!(error == "Chemistry worker exited unexpectedly");
        } else {
            anyhow::ensure!(error.contains("EOF"), "Malformed JSON error changed");
        }
        anyhow::ensure!(
            engine.worker.lock().await.is_none(),
            "Transport error retained worker"
        );
    }
    let engine = fixture_engine().await?;
    let error = engine.exchange(Value::Null).await;
    anyhow::ensure!(error.err().as_deref() == Some("Invalid chemistry request envelope"));
    anyhow::ensure!(engine.worker.lock().await.is_none());
    Ok(())
}

#[tokio::test]
async fn concurrent_exchanges_keep_responses_with_their_requests() -> anyhow::Result<()> {
    let engine = fixture_engine().await?;
    let (left, right) = tokio::join!(
        engine.exchange(json!({"action": "echo", "payload": "left"})),
        engine.exchange(json!({"action": "echo", "payload": "right"})),
    );
    anyhow::ensure!(left.map_err(anyhow::Error::msg)? == json!("left"));
    anyhow::ensure!(right.map_err(anyhow::Error::msg)? == json!("right"));
    anyhow::ensure!(engine.worker.lock().await.as_ref().unwrap().next_id == 3);
    Ok(())
}

#[tokio::test]
async fn cancelled_exchange_releases_lock_and_preserves_worker() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let marker = directory.path().join("ready");
    let engine = fixture_engine().await?;
    let child_id = engine.worker.lock().await.as_ref().unwrap()._child.id();
    let mut exchange = Box::pin(engine.exchange(json!({"action": "stall", "ready": marker})));
    let ready = async {
        while !tokio::fs::try_exists(&marker).await? {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, std::io::Error>(())
    };
    tokio::select! {
        result = &mut exchange => anyhow::bail!("Stalled exchange completed: {result:?}"),
        ready = tokio::time::timeout(Duration::from_secs(5), ready) => ready??,
    }
    drop(exchange);
    {
        let slot = engine.worker.lock().await;
        let worker = slot.as_ref().context("Cancellation removed worker")?;
        anyhow::ensure!(worker._child.id() == child_id);
        anyhow::ensure!(worker.next_id == 2);
    }
    let result = engine
        .exchange(json!({"action": "echo", "payload": "after cancellation"}))
        .await
        .map_err(anyhow::Error::msg)?;
    anyhow::ensure!(result == json!("after cancellation"));
    Ok(())
}

#[tokio::test]
#[ignore = "Run alone with --test-threads=1 to measure process-wide Rust allocations"]
async fn worker_exchange_allocation_workload() -> anyhow::Result<()> {
    const OPERATIONS: usize = 30;
    const PAYLOAD_BYTES: usize = 1024 * 1024;
    let fixture = json!({
        "action": "echo",
        "payload": {
            "document": {"graphics": [{"id": 1, "picture": "A".repeat(PAYLOAD_BYTES)}]},
            "output": null,
            "warnings": [],
        },
    });
    let engine = fixture_engine().await?;
    let warm = engine
        .exchange(fixture.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    anyhow::ensure!(warm == fixture["payload"], "Warm response changed");
    drop(warm);
    let baseline = crate::allocation_metrics::reset();
    for _ in 0..OPERATIONS {
        let response = engine
            .exchange(fixture.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            response == fixture["payload"],
            "Large worker response changed"
        );
    }
    let metrics = crate::allocation_metrics::snapshot();
    eprintln!(
        "worker_exchange operations={OPERATIONS} payload_bytes={PAYLOAD_BYTES} allocated_bytes={} allocation_count={} live_bytes={} baseline_live_bytes={baseline} peak_bytes={} peak_extra_bytes={} retained_extra_bytes={}",
        metrics.allocated_bytes,
        metrics.allocation_count,
        metrics.live_bytes,
        metrics.peak_bytes,
        metrics.peak_bytes.saturating_sub(baseline),
        metrics.live_bytes as i128 - baseline as i128,
    );
    eprintln!(
        "Rust requested allocation bytes only; fixture construction and warmup excluded; per-request fixture clones included; Python child, native allocations and RSS excluded."
    );
    Ok(())
}
