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
