use super::*;

fn transparent_source_image() -> anyhow::Result<crate::pictures::Picture> {
    let rgba = image::RgbaImage::from_fn(8, 6, |x, y| {
        image::Rgba([0, 0, 0, if x == y { 255 } else { 0 }])
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut bytes, image::ImageFormat::Png)?;
    crate::pictures::Picture::import(bytes.get_ref()).map_err(anyhow::Error::msg)
}

#[tokio::test]
async fn source_handoff_is_opaque_without_changing_chat_picture() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let image = transparent_source_image()?;
    let original = image.png().to_vec();
    let source = prepare_source_image(directory.path(), image.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(source, directory.path().join("source.png"));
    let decoded = image::load_from_memory(&tokio::fs::read(source).await?)?;
    assert_eq!(decoded.color(), image::ColorType::Rgb8);
    assert_eq!((decoded.width(), decoded.height()), (8, 6));
    for (x, y, pixel) in decoded.to_rgb8().enumerate_pixels() {
        assert_eq!(pixel.0, if x == y { [0; 3] } else { [255; 3] });
    }
    assert_eq!(image.png(), original);

    let error = prepare_source_image(&directory.path().join("missing"), image)
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("Writing to a missing directory should fail"))?;
    assert!(error.contains("Could not write the source image"));
    Ok(())
}

#[test]
fn active_turns_survive_four_minutes_but_stalls_and_total_runtime_are_bounded() {
    let start = tokio::time::Instant::now();
    let mut timeout = Timeout::new(start, "Visual review", TURN_LIMIT);
    assert!(timeout.error(start + Duration::from_secs(240)).is_none());
    for minute in [4, 8, 12, 16] {
        let now = start + Duration::from_secs(minute * 60);
        assert!(timeout.error(now).is_none());
        timeout.progress(now, "model working");
    }
    assert!(
        timeout
            .error(start + TURN_LIMIT)
            .is_some_and(|e| e.contains("Visual review") && e.contains("time limit"))
    );
    let idle = Timeout::new(start, "Drawing generation", TURN_LIMIT);
    assert!(
        idle.error(start + IDLE_LIMIT)
            .is_some_and(|e| e.contains("No progress") && e.contains("drawing generation"))
    );
}

#[test]
fn progress_notifications_refresh_idle_deadline_without_exposing_reasoning() {
    let start = tokio::time::Instant::now();
    let mut timeout = Timeout::new(
        start - Duration::from_secs(240),
        "Visual review",
        TURN_LIMIT,
    );
    let old_idle = timeout.idle;
    timeout.observe(&json!({"method":"account/rateLimits/updated"}));
    assert_eq!(timeout.idle, old_idle);
    timeout
        .observe(&json!({"method":"item/reasoning/textDelta","params":{"delta":"private text"}}));
    assert!(timeout.idle > old_idle);
    assert_eq!(timeout.activity, "model working");
    assert_eq!(timeout.hard, start - Duration::from_secs(240) + TURN_LIMIT);
}
#[test]
fn streaming_explanations_are_unicode_safe_and_never_show_json() {
    assert_eq!(
        explanation_prefix(r#"{"explanation":"反応を描"#).as_deref(),
        Some("反応を描")
    );
    assert_eq!(
        explanation_prefix(r#"{"explanation":"Water\nH₂O","molecules":[]}"#).as_deref(),
        Some("Water\nH₂O")
    );
    assert_eq!(
        explanation_prefix(r#"{"explanation":"hello\u65"#).as_deref(),
        Some("hello")
    );
    assert_eq!(explanation_prefix(r#"{"molecules":[]}"#), None);
}
#[cfg(unix)]
async fn fake_review(always_edit: bool) -> anyhow::Result<(Server, tempfile::TempDir)> {
    let evidence = tempfile::tempdir()?;
    let script = r#"
import sys, json, pathlib, hashlib
count = 0
root = pathlib.Path(sys.argv[1])
for line in sys.stdin:
    event = json.loads(line)
    if event.get('method') != 'turn/start': continue
    count += 1
    inputs = event['params']['input']
    images = [pathlib.Path(i['path']) for i in inputs if i['type'] == 'localImage']
    assert images and all(p.read_bytes().startswith(b'\x89PNG') for p in images)
    text = json.loads(inputs[0]['text'])
    assert text['original_request'] == 'Review this test scheme'
    assert text['editable_document'] and text['editable_targets']
    draft = next(p for p in images if p.name.startswith('review-'))
    root.joinpath(str(count)).write_text(hashlib.sha256(draft.read_bytes()).hexdigest())
    for p in images:
        if p.name == 'source.png': root.joinpath('source-' + str(count)).write_bytes(p.read_bytes())
    edits = [{'action':'arrow_length','target':'arrow:1','length_pt':20 + count}] if count == 1 or sys.argv[2] == 'true' else []
    result = {'summary':'Adjusted arrow spacing' if edits else 'Exact final image checked', 'issues':[], 'edits':edits}
    print(json.dumps({'method':'item/completed','params':{'item':{'type':'agentMessage','text':json.dumps(result)}}}), flush=True)
    print(json.dumps({'method':'turn/completed','params':{'turn':{'status':'completed'}}}), flush=True)
"#;
    let directory = tempfile::tempdir()?;
    let mut child = Command::new("python3")
        .arg("-u")
        .arg("-c")
        .arg(script)
        .arg(evidence.path())
        .arg(always_edit.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let input = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("Missing mock stdin"))?;
    let output = BufReader::new(
        child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Missing mock stdout"))?,
    );
    Ok((
        Server {
            child,
            input,
            output,
            directory,
            next_id: 1,
            cancel: Cancel::default(),
            timeout: Timeout::new(
                tokio::time::Instant::now(),
                "Test connection",
                Duration::from_secs(30),
            ),
        },
        evidence,
    ))
}

#[cfg(unix)]
#[tokio::test]
async fn corrections_are_rerendered_and_exact_final_images_are_required() -> anyhow::Result<()> {
    for (always_edit, with_source) in [(false, false), (true, false), (false, true)] {
        let (mut server, evidence) = fake_review(always_edit).await?;
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        let mut doc = crate::document::Document::default();
        doc.arrows.push(crate::document::Arrow::new(
            1,
            crate::document::Point::new(0., 0.),
            crate::document::Point::new(160., 0.),
            crate::arrows::Preset::Forward,
            Default::default(),
        ));
        doc.annotations.push(crate::document::Annotation {
            id: 2,
            position: crate::document::Point::new(70., -80.),
            text: "Test".into(),
            format: Default::default(),
        });
        let mut outcome = super::super::review::Outcome {
            proposal: Default::default(),
            document: doc,
            review: Default::default(),
        };
        let source_path = server.directory.path().join("source.png");
        let source_png = transparent_source_image()?
            .png_on_white()
            .map_err(anyhow::Error::msg)?;
        if with_source {
            let prepared =
                prepare_source_image(server.directory.path(), transparent_source_image()?)
                    .await
                    .map_err(anyhow::Error::msg)?;
            assert_eq!(prepared, source_path);
        }
        let turn = Turn {
            thread: "test",
            effort: "low",
            tier: None,
            progress: &tx,
            canvas: None,
            source: with_source.then_some(source_path.as_path()),
        };
        review_draft(&mut server, &turn, "Review this test scheme", &mut outcome)
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(outcome.review.passes, if always_edit { 3 } else { 2 });
        assert_eq!(outcome.review.verified, !always_edit);
        assert_ne!(
            std::fs::read_to_string(evidence.path().join("1"))?,
            std::fs::read_to_string(evidence.path().join("2"))?
        );
        assert!(!evidence.path().join("4").exists());
        for pass in 1..=outcome.review.passes {
            let source = evidence.path().join(format!("source-{pass}"));
            assert_eq!(source.exists(), with_source);
            if with_source {
                assert_eq!(std::fs::read(source)?, source_png);
            }
        }
        let mut previews = 0;
        while let Ok(p) = rx.try_recv() {
            if matches!(p, Progress::Preview(_)) {
                previews += 1;
            }
        }
        assert_eq!(previews, if always_edit { 2 } else { 1 });
        server.shutdown().await;
    }
    Ok(())
}

#[test]
fn tool_response_keeps_codex_content_bytes() {
    assert_eq!(
        tool_response(ToolOutput {
            text: "{\"a\":1} H₂O".into(),
            png: vec![0x89, b'P', b'N', b'G'],
        })
        .to_string(),
        r#"{"contentItems":[{"text":"{\"a\":1} H₂O","type":"inputText"},{"imageUrl":"data:image/png;base64,iVBORw==","type":"inputImage"}],"success":true}"#
    );
}

#[test]
fn dynamic_tools_keep_slice_order_and_omit_title_and_hints() {
    let spec = |name| ToolSpec {
        name,
        title: Some("Title"),
        description: "Draws.",
        input_schema: || json!({"type":"object"}),
        hints: Some(reshiki_agent::tool_spec::Hints {
            read_only: true,
            destructive: false,
            idempotent: true,
            open_world: false,
        }),
    };
    assert_eq!(dynamic_tools(&[]).to_string(), "[]");
    assert_eq!(
        dynamic_tools(&[spec("b"), spec("a")]).to_string(),
        r#"[{"description":"Draws.","inputSchema":{"type":"object"},"name":"b","type":"function"},{"description":"Draws.","inputSchema":{"type":"object"},"name":"a","type":"function"}]"#
    );
}

#[test]
fn progress_events_map_to_codex_variants_without_reallocation() {
    let document = Box::new(crate::document::Document::default());
    let document_ptr = &*document as *const _;
    let proposal = Box::new(super::super::Proposal::default());
    let proposal_ptr = &*proposal as *const _;
    assert!(matches!(
        Progress::from(Event::Preview(document)),
        Progress::Preview(converted) if std::ptr::eq(&*converted, document_ptr)
    ));
    assert!(matches!(
        Progress::from(Event::Proposal(proposal)),
        Progress::Proposal(converted) if std::ptr::eq(&*converted, proposal_ptr)
    ));
    assert!(matches!(
        Progress::from(Event::Structures {
            completed: 2,
            total: 5
        }),
        Progress::Structures {
            completed: 2,
            total: 5
        }
    ));
}

#[cfg(unix)]
async fn fake_connection(account: Value, models: Value) -> anyhow::Result<Server> {
    let script = r#"
import json, sys
account = json.loads(sys.argv[1])
models = json.loads(sys.argv[2])
for line in sys.stdin:
    event = json.loads(line)
    method = event.get('method')
    if method == 'account/read': result = account
    elif method == 'model/list':
        if account.get('result', {}).get('account') is None: raise RuntimeError('signed-out must not request models')
        result = models
    else: raise RuntimeError('unexpected method')
    print(json.dumps({'id':event['id'], **result}), flush=True)
"#;
    let directory = tempfile::tempdir()?;
    let mut child = Command::new("python3")
        .arg("-u")
        .arg("-c")
        .arg(script)
        .arg(serde_json::to_string(&account)?)
        .arg(serde_json::to_string(&models)?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let input = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("missing stdin"))?;
    let output = BufReader::new(
        child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("missing stdout"))?,
    );
    Ok(Server {
        child,
        input,
        output,
        directory,
        next_id: 1,
        cancel: Cancel::default(),
        timeout: Timeout::new(
            tokio::time::Instant::now(),
            "Fixture connection",
            Duration::from_secs(10),
        ),
    })
}

#[cfg(unix)]
#[tokio::test]
async fn connection_classifies_protocol_boundaries_without_exposing_payloads() -> anyhow::Result<()>
{
    let model = json!({"model":"fixture", "displayName":"Fixture model"});
    let cases = [
        (json!({"result":{"account":null}}), json!({}), None),
        (
            json!({"result":{"account":{"type":"chatgpt", "email":"private@example.test"}}}),
            json!({"result":{"data":[model]}}),
            None,
        ),
        (
            json!({"result":{}}),
            json!({}),
            Some(ConnectionError::AccountFailed),
        ),
        (
            json!({"result":{"account":false}}),
            json!({}),
            Some(ConnectionError::AccountFailed),
        ),
        (
            json!({"result":{"account":{}}}),
            json!({}),
            Some(ConnectionError::AccountFailed),
        ),
        (
            json!({"error":{"message":"sensitive-account-token"}}),
            json!({}),
            Some(ConnectionError::AccountFailed),
        ),
        (
            json!({"result":{"account":{"type":"apiKey"}}}),
            json!({"error":{"message":"sensitive-key"}}),
            Some(ConnectionError::ModelsFailed),
        ),
        (
            json!({"result":{"account":{"type":"chatgpt"}}}),
            json!({"result":{"data":[]}}),
            Some(ConnectionError::NoModels),
        ),
    ];
    for (index, (account, models, failure)) in cases.into_iter().enumerate() {
        let mut server = fake_connection(account, models).await?;
        let result = read_connection(&mut server).await;
        server.shutdown().await;
        match failure {
            Some(expected) => {
                let error = result.unwrap_err();
                assert_eq!(error, expected, "case {index}");
                assert!(!error.to_string().contains("sensitive"));
            }
            None => {
                let account = result?;
                assert_eq!(account.connected, index == 1);
                assert_eq!(account.models.len(), usize::from(index == 1));
            }
        }
    }
    let cancel = Cancel::default();
    cancel.stop();
    assert_eq!(
        connect(cancel).await.unwrap_err(),
        ConnectionError::Cancelled
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn startup_distinguishes_missing_unlaunchable_broken_and_cancelled_cli() -> anyhow::Result<()>
{
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("fixture-codex");
    assert!(matches!(
        Server::start_at(path.clone(), Cancel::default()).await,
        Err(ConnectionError::MissingInstallation)
    ));
    std::fs::write(
        &path,
        b"#!/usr/bin/env python3\nprint('sensitive-not-json', flush=True)\n",
    )?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    assert!(matches!(
        Server::start_at(path.clone(), Cancel::default()).await,
        Err(ConnectionError::StartFailed)
    ));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    assert!(matches!(
        Server::start_at(path.clone(), Cancel::default()).await,
        Err(ConnectionError::HandshakeFailed)
    ));
    std::fs::write(
        &path,
        b"#!/usr/bin/env python3\nimport time\ntime.sleep(10)\n",
    )?;
    let cancel = Cancel::default();
    let cancellation = cancel.clone();
    let stopper = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancellation.stop();
    });
    assert!(matches!(
        Server::start_at(path, cancel).await,
        Err(ConnectionError::Cancelled)
    ));
    stopper.await?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn request_preflight_reports_typed_setup_changes_without_inference() -> anyhow::Result<()> {
    for (account, models, expected) in [
        (json!({"result":{"account":null}}), json!({}), None),
        (
            json!({"result":{"account":{"type":"chatgpt"}}}),
            json!({"error":{"message":"sensitive-catalog-error"}}),
            Some(ConnectionError::ModelsFailed),
        ),
    ] {
        let mut server = fake_connection(account, models).await?;
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let error = generation_connection(&mut server, &tx).await.unwrap_err();
        assert!(!error.contains("sensitive"));
        match (expected, rx.try_recv()?) {
            (None, Progress::Catalog(account)) => {
                assert!(!account.connected);
                assert!(account.models.is_empty());
                assert!(error.contains("codex login"));
            }
            (Some(expected), Progress::ConnectionFailed(actual)) => assert_eq!(actual, expected),
            (_, event) => panic!("Unexpected preflight progress: {event:?}"),
        }
        server.shutdown().await;
    }
    let cancel = Cancel::default();
    cancel.stop();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    assert!(
        propose("never sent".into(), Default::default(), cancel, tx, None)
            .await
            .is_err()
    );
    assert!(matches!(
        rx.try_recv()?,
        Progress::ConnectionFailed(ConnectionError::Cancelled)
    ));
    Ok(())
}
