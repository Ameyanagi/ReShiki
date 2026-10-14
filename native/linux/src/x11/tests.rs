use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::time::Duration;

#[test]
fn owner_and_advertised_timestamp_changes_abort_before_retry_selection() {
    let original = SelectionGeneration {
        owner: 7,
        timestamp: Some(11),
    };
    let token = original.check(None).unwrap();
    assert!(SelectionGeneration::check_owner(original.owner, Some(&token)).is_ok());
    assert_eq!(
        SelectionGeneration::check_owner(8, Some(&token)).unwrap_err(),
        protocol::CLIPBOARD_CHANGED
    );
    assert_eq!(original.check(Some(&token)).unwrap(), token);
    for replacement in [
        SelectionGeneration {
            owner: 8,
            timestamp: Some(11),
        },
        SelectionGeneration {
            owner: 7,
            timestamp: Some(12),
        },
        SelectionGeneration {
            owner: NONE,
            timestamp: None,
        },
    ] {
        assert_eq!(
            replacement.check(Some(&token)).unwrap_err(),
            protocol::CLIPBOARD_CHANGED
        );
    }
    let unsupported_timestamp = SelectionGeneration {
        owner: 7,
        timestamp: None,
    };
    assert_eq!(unsupported_timestamp.token(), "x11:7:-");
    let empty = SelectionGeneration {
        owner: NONE,
        timestamp: None,
    };
    assert_eq!(empty.check(None).unwrap(), "x11:0:-");
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn replacement_between_candidate_reads_aborts_before_source_exclusions() {
    let mut first = Clipboard::connect().unwrap();
    first
        .publish(
            protocol::prepare_offer(&[
                protocol::representation("public.utf8-plain-text", b"hello").unwrap(),
                protocol::representation("public.png", b"first png").unwrap(),
            ])
            .unwrap(),
        )
        .unwrap();
    let first_worker = std::thread::spawn(move || first.serve().unwrap());
    let formats = protocol::read_formats(&[], false).unwrap();
    let (candidate, generation) = Clipboard::connect()
        .unwrap()
        .read(&formats, &[], None)
        .unwrap();
    let candidate = candidate.unwrap();
    assert_eq!(candidate.kind, "public.utf8-plain-text");
    let mut replacement = Clipboard::connect().unwrap();
    replacement
        .publish(
            protocol::prepare_offer(&[
                protocol::representation("public.utf8-plain-text", b"CCO").unwrap(),
                protocol::representation("public.png", b"replacement png").unwrap(),
            ])
            .unwrap(),
        )
        .unwrap();
    first_worker.join().unwrap();
    let replacement_worker = std::thread::spawn(move || replacement.serve().unwrap());
    let result = Clipboard::connect().unwrap().read(
        &formats,
        &[candidate.source.unwrap()],
        Some(&generation),
    );
    assert_eq!(result.unwrap_err(), protocol::CLIPBOARD_CHANGED);
    let _cleanup = image_owner(1);
    replacement_worker.join().unwrap();
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn replacement_during_a_candidate_read_is_never_a_candidate_failure() {
    for refuse in [false, true] {
        let mut owner = image_owner(100);
        let target = *owner.data.keys().next().unwrap();
        let worker = std::thread::spawn(move || {
            loop {
                if let Event::SelectionRequest(request) =
                    owner.wait_event(Instant::now() + TRANSFER_TIMEOUT).unwrap()
                {
                    if request.target == target {
                        let _replacement = image_owner(1);
                        if refuse {
                            owner.notify(&request, NONE).unwrap();
                        } else {
                            owner.request(request).unwrap();
                        }
                        owner.connection.flush().unwrap();
                        return;
                    }
                    owner.request(request).unwrap();
                    owner.connection.flush().unwrap();
                }
            }
        });
        let result = Clipboard::connect().unwrap().read(
            &protocol::read_formats(&[], true).unwrap(),
            &[],
            None,
        );
        assert_eq!(result.unwrap_err(), protocol::CLIPBOARD_CHANGED);
        worker.join().unwrap();
    }
}

#[test]
fn incremental_progress_refreshes_only_the_idle_deadline() {
    let start = Instant::now();
    let mut timing = TransferTiming::new(start);
    timing.progress(start + Duration::from_secs(4)).unwrap();
    assert_eq!(timing.deadline(), start + Duration::from_secs(9));
    timing.progress(start + Duration::from_secs(8)).unwrap();
    assert_eq!(timing.deadline(), start + Duration::from_secs(13));
    for seconds in (12..60).step_by(4) {
        timing
            .progress(start + Duration::from_secs(seconds))
            .unwrap();
    }
    assert_eq!(timing.deadline(), start + TRANSFER_TOTAL_TIMEOUT);
    assert!(timing.progress(start + TRANSFER_TOTAL_TIMEOUT).is_err());
    assert_eq!(timing.deadline(), start + TRANSFER_TOTAL_TIMEOUT);
}

#[test]
fn expired_idle_deadline_cannot_be_revived() {
    let start = Instant::now();
    let mut timing = TransferTiming::new(start);
    let deadline = timing.deadline();
    assert_eq!(deadline, start + TRANSFER_TIMEOUT);
    assert!(timing.progress(deadline).is_err());
    assert_eq!(timing.deadline(), deadline);
    assert!(timing.progress(deadline + Duration::from_secs(1)).is_err());
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn buffered_events_beyond_dispatch_budget_do_not_wait_for_socket_activity() {
    let mut owner = image_owner(1);
    let window = owner.window;
    let event = ClientMessageEvent::new(32, window, owner.atoms.property, [0_u32; 5]);
    for _ in 0..4096 {
        owner
            .connection
            .send_event(false, window, EventMask::NO_EVENT, event)
            .unwrap()
            .check()
            .unwrap();
    }
    let replacement = image_owner(1);
    // A round trip buffers the real ownership-loss event behind the noise.
    owner.connection.get_input_focus().unwrap().reply().unwrap();
    let (finished, completion) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || finished.send(owner.serve()).unwrap());
    let result = completion.recv_timeout(Duration::from_secs(1));
    if result.is_err() {
        // A regressed batch loop can need another wake for every 256 events.
        // Join only after completion; a broken loop must not hang the test.
        for _ in 0..32 {
            replacement
                .connection
                .send_event(false, window, EventMask::NO_EVENT, event)
                .unwrap()
                .check()
                .unwrap();
            if completion.recv_timeout(Duration::from_millis(25)).is_ok() {
                worker.join().unwrap();
                panic!("buffered event dispatch exceeded one second");
            }
        }
        drop(worker);
        panic!("buffered event dispatch did not recover after bounded wakeups");
    }
    worker.join().unwrap();
    result.unwrap().unwrap();
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn event_wait_honors_deadline_without_an_incoming_event() {
    let clipboard = Clipboard::connect().unwrap();
    let started = Instant::now();
    let deadline = started + Duration::from_millis(30);
    let result = clipboard.wait_event(deadline).unwrap_err();
    assert_eq!(result, "X11 clipboard transfer timed out");
    assert!(Instant::now() >= deadline);
    assert!(started.elapsed() < Duration::from_secs(1));
}

fn image_owner(size: usize) -> Clipboard {
    let mut owner = Clipboard::connect().unwrap();
    owner
        .publish(
            protocol::prepare_offer(&[Representation {
                kind: "public.png".into(),
                data: STANDARD.encode(vec![37_u8; size]),
                read_error: None,
                source: None,
            }])
            .unwrap(),
        )
        .unwrap();
    owner
}

fn read_offered_targets(count: usize) -> Result<Option<Representation>, String> {
    let mut owner = image_owner(100);
    let target = *owner.data.keys().next().unwrap();
    let mut targets = Vec::new();
    for index in 1..count {
        targets.push(
            owner
                .connection
                .intern_atom(false, format!("_RESHIKI_TEST_TARGET_{index}").as_bytes())
                .unwrap()
                .reply()
                .unwrap()
                .atom,
        );
    }
    // The supported format must be found beyond the outgoing offer limit.
    targets.push(target);
    let (validation_done, validation_completion) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || -> Result<(), String> {
        loop {
            if let Event::SelectionRequest(request) =
                owner.wait_event(Instant::now() + TRANSFER_TIMEOUT)?
            {
                if request.target == owner.atoms.targets {
                    owner
                        .connection
                        .change_property32(
                            PropMode::REPLACE,
                            request.requestor,
                            request.property,
                            AtomEnum::ATOM,
                            &targets,
                        )
                        .map_err(error)?
                        .check()
                        .map_err(error)?;
                    owner.notify(&request, request.property)?;
                } else {
                    owner.request(request)?;
                }
                owner.connection.flush().map_err(error)?;
                if request.target == target || count > MAX_TARGETS {
                    finish_read_validation(&mut owner, &validation_completion);
                    return Ok(());
                }
            }
        }
    });
    let result = Clipboard::connect()
        .unwrap()
        .read(&protocol::read_formats(&[], true).unwrap(), &[], None)
        .map(|(candidate, _)| candidate);
    validation_done.send(()).unwrap();
    let served = worker.join().unwrap();
    if result.is_ok() {
        served.unwrap();
    }
    result
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn supported_target_after_many_unknown_targets_is_read() {
    let result = read_offered_targets(MAX_TARGETS).unwrap().unwrap();
    assert_eq!(result.kind, "public.png");
    assert_eq!(STANDARD.decode(result.data).unwrap(), vec![37_u8; 100]);
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn oversized_target_list_is_rejected() {
    let result = read_offered_targets(MAX_TARGETS + 1).unwrap_err();
    assert!(result.contains("transfer limit"), "{result}");
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn incremental_read_accepts_progress_beyond_the_idle_timeout() {
    let mut owner = image_owner(128);
    owner.chunk = 16;
    let (validation_done, validation_completion) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match owner.wait_event(deadline).unwrap() {
                Event::SelectionRequest(request) => owner.request(request).unwrap(),
                Event::PropertyNotify(event)
                    if event.state == Property::DELETE
                        && owner.transfers.iter().any(|transfer| {
                            transfer.window == event.window && transfer.property == event.atom
                        }) =>
                {
                    // Nine handshakes exceed five seconds; none is idle that long.
                    std::thread::sleep(Duration::from_millis(750));
                    owner.advance(event.window, event.atom);
                    owner.connection.flush().unwrap();
                    if owner.transfers.is_empty() {
                        break;
                    }
                }
                Event::DestroyNotify(_) => break,
                _ => {}
            }
            owner.connection.flush().unwrap();
        }
        finish_read_validation(&mut owner, &validation_completion);
    });
    let result = Clipboard::connect()
        .unwrap()
        .read(&protocol::read_formats(&[], true).unwrap(), &[], None)
        .map(|(candidate, _)| candidate);
    validation_done.send(()).unwrap();
    worker.join().unwrap();
    assert_eq!(
        STANDARD.decode(result.unwrap().unwrap().data).unwrap(),
        vec![37_u8; 128]
    );
}

fn read_incremental_slowly(reader: &Clipboard, target: Atom) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let header = reader.request_property(target, LIMIT, deadline)?;
    assert_eq!(header.type_, reader.atoms.incr);
    let mut bytes = Vec::new();
    loop {
        // The real owner's serve loop must extend its deadline on each ACK.
        std::thread::sleep(Duration::from_millis(750));
        reader
            .connection
            .delete_property(reader.window, reader.atoms.property)
            .map_err(error)?
            .check()
            .map_err(error)?;
        reader.connection.flush().map_err(error)?;
        loop {
            if let Event::PropertyNotify(event) = reader.wait_event(deadline)?
                && event.window == reader.window
                && event.atom == reader.atoms.property
                && event.state == Property::NEW_VALUE
            {
                let chunk = reader.property(128_usize.saturating_sub(bytes.len()))?;
                if chunk.type_ == NONE {
                    continue;
                }
                assert_eq!(chunk.type_, target);
                assert_eq!(chunk.format, 8);
                if chunk.value.is_empty() {
                    return Ok(bytes);
                }
                bytes.extend_from_slice(&chunk.value);
                break;
            }
        }
    }
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn incremental_write_accepts_progress_beyond_the_idle_timeout() {
    let mut owner = image_owner(128);
    owner.chunk = 16;
    let target = *owner.data.keys().next().unwrap();
    let worker = std::thread::spawn(move || owner.serve().unwrap());
    let reader = Clipboard::connect().unwrap();
    let result = read_incremental_slowly(&reader, target);
    drop(reader);
    let _replacement = image_owner(1);
    worker.join().unwrap();
    assert_eq!(result.unwrap(), vec![37_u8; 128]);
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn one_shot_owner_exit_aborts_with_a_top_level_generation_error() {
    let mut owner = image_owner(100);
    let target = *owner.data.keys().next().unwrap();
    let worker = std::thread::spawn(move || {
        loop {
            if let Event::SelectionRequest(request) =
                owner.wait_event(Instant::now() + TRANSFER_TIMEOUT).unwrap()
            {
                owner.request(request).unwrap();
                owner.connection.flush().unwrap();
                if request.target == target {
                    break;
                }
            }
        }
    });
    let result = Clipboard::connect()
        .unwrap()
        .read(&protocol::read_formats(&[], true).unwrap(), &[], None)
        .unwrap_err();
    assert_eq!(result, protocol::CLIPBOARD_CHANGED);
    worker.join().unwrap();
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn oversized_incremental_header_is_rejected_before_allocation() {
    let mut owner = image_owner(100);
    let target = *owner.data.keys().next().unwrap();
    let (validation_done, validation_completion) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        loop {
            if let Event::SelectionRequest(request) =
                owner.wait_event(Instant::now() + TRANSFER_TIMEOUT).unwrap()
            {
                if request.target == target {
                    owner
                        .connection
                        .change_property32(
                            PropMode::REPLACE,
                            request.requestor,
                            request.property,
                            owner.atoms.incr,
                            &[(LIMIT + 1) as u32],
                        )
                        .unwrap()
                        .check()
                        .unwrap();
                    owner.notify(&request, request.property).unwrap();
                    owner.connection.flush().unwrap();
                    break;
                }
                owner.request(request).unwrap();
                owner.connection.flush().unwrap();
            }
        }
        finish_read_validation(&mut owner, &validation_completion);
    });
    let result = Clipboard::connect()
        .unwrap()
        .read(&protocol::read_formats(&[], true).unwrap(), &[], None)
        .map(|(candidate, _)| candidate)
        .unwrap()
        .unwrap();
    assert_eq!(result.kind, "public.png");
    assert!(result.data.is_empty());
    let error = result.read_error.unwrap();
    assert!(error.contains("oversized"), "{error}");
    validation_done.send(()).unwrap();
    worker.join().unwrap();
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn failed_text_transfer_is_returned_before_a_remaining_png_request() {
    let mut owner = Clipboard::connect().unwrap();
    owner
        .publish(
            protocol::prepare_offer(&[
                protocol::representation("public.utf8-plain-text", b"text").unwrap(),
                protocol::representation("public.png", b"png").unwrap(),
            ])
            .unwrap(),
        )
        .unwrap();
    let atom = |mime: &[u8]| {
        owner
            .connection
            .intern_atom(true, mime)
            .unwrap()
            .reply()
            .unwrap()
            .atom
    };
    let text = atom(b"text/plain;charset=utf-8");
    let png = atom(b"image/png");
    let (validation_done, validation_completion) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut requested = Vec::new();
        loop {
            if let Event::SelectionRequest(request) =
                owner.wait_event(Instant::now() + TRANSFER_TIMEOUT).unwrap()
            {
                if request.target == text {
                    requested.push(text);
                    owner.notify(&request, NONE).unwrap();
                } else {
                    owner.request(request).unwrap();
                    if request.target == png {
                        requested.push(png);
                    }
                }
                owner.connection.flush().unwrap();
                if request.target == png {
                    break;
                }
            }
        }
        finish_read_validation(&mut owner, &validation_completion);
        requested
    });
    let formats = protocol::read_formats(&[], false).unwrap();
    let failed = Clipboard::connect()
        .unwrap()
        .read(&formats, &[], None)
        .map(|(candidate, _)| candidate)
        .unwrap()
        .unwrap();
    assert_eq!(failed.kind, "public.utf8-plain-text");
    assert!(failed.data.is_empty());
    assert!(failed.read_error.unwrap().contains("refused"));
    let remaining: Vec<_> = formats
        .into_iter()
        .filter(|kind| *kind != "public.utf8-plain-text")
        .collect();
    let image = Clipboard::connect()
        .unwrap()
        .read(&remaining, &[], None)
        .map(|(candidate, _)| candidate)
        .unwrap()
        .unwrap();
    assert_eq!(image.kind, "public.png");
    assert!(image.read_error.is_none());
    assert_eq!(STANDARD.decode(image.data).unwrap(), b"png");
    validation_done.send(()).unwrap();
    assert_eq!(worker.join().unwrap(), [text, png]);
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn failed_cdx_source_can_fall_through_to_a_second_mime_for_the_same_kind() {
    let mut owner = Clipboard::connect().unwrap();
    owner
        .publish(
            protocol::prepare_offer(&[
                protocol::representation("chemical/x-cdx", b"cdx").unwrap(),
                protocol::representation("public.png", b"png").unwrap(),
            ])
            .unwrap(),
        )
        .unwrap();
    let atom = |mime: &[u8]| {
        owner
            .connection
            .intern_atom(true, mime)
            .unwrap()
            .reply()
            .unwrap()
            .atom
    };
    let first = atom(b"chemical/x-cdx");
    let second = atom(b"application/x-chemdraw");
    let (validation_done, validation_completion) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut requested = Vec::new();
        loop {
            if let Event::SelectionRequest(request) =
                owner.wait_event(Instant::now() + TRANSFER_TIMEOUT).unwrap()
            {
                if request.target == first {
                    requested.push(first);
                    owner.notify(&request, NONE).unwrap();
                } else {
                    owner.request(request).unwrap();
                    if request.target == second {
                        requested.push(second);
                    }
                }
                owner.connection.flush().unwrap();
                if request.target == second {
                    break;
                }
            }
        }
        finish_read_validation(&mut owner, &validation_completion);
        requested
    });
    let formats = protocol::read_formats(&[], false).unwrap();
    let failed = Clipboard::connect()
        .unwrap()
        .read(&formats, &[], None)
        .map(|(candidate, _)| candidate)
        .unwrap()
        .unwrap();
    assert_eq!(failed.kind, "com.revvity.chemdraw.cdx-clipboard");
    assert_eq!(failed.source.as_deref(), Some("chemical/x-cdx"));
    assert!(failed.read_error.unwrap().contains("refused"));
    let skip_sources = vec![failed.source.unwrap()];
    let success = Clipboard::connect()
        .unwrap()
        .read(&formats, &skip_sources, None)
        .map(|(candidate, _)| candidate)
        .unwrap()
        .unwrap();
    assert_eq!(success.kind, failed.kind);
    assert_eq!(success.source.as_deref(), Some("application/x-chemdraw"));
    assert!(success.read_error.is_none());
    assert_eq!(STANDARD.decode(success.data).unwrap(), b"cdx");
    validation_done.send(()).unwrap();
    assert_eq!(worker.join().unwrap(), [first, second]);
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn stalled_incremental_reader_does_not_block_other_readers_or_exit() {
    let mut owner = image_owner(2 * 1024 * 1024);
    let target = *owner.data.keys().next().unwrap();
    let (finished, completion) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = owner.serve();
        finished.send(result).unwrap();
    });
    let stalled = Clipboard::connect().unwrap();
    let header = stalled
        .request_property(target, LIMIT, Instant::now() + TRANSFER_TIMEOUT)
        .unwrap();
    assert_eq!(header.type_, stalled.atoms.incr);
    // Leave INCR unacknowledged, keeping this requestor alive.
    assert!(
        Clipboard::connect()
            .unwrap()
            .read(&protocol::read_formats(&[], true).unwrap(), &[], None)
            .map(|(candidate, _)| candidate)
            .unwrap()
            .is_some()
    );
    let _replacement = image_owner(1);
    completion
        .recv_timeout(TRANSFER_TIMEOUT + std::time::Duration::from_secs(1))
        .unwrap()
        .unwrap();
    worker.join().unwrap();
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn native_and_incremental_image_roundtrip_and_replacement() {
    let png = vec![37_u8; 2 * 1024 * 1024];
    let offer = protocol::prepare_offer(&[
        Representation {
            kind: "dev.reshiki.drawing".into(),
            data: STANDARD.encode(b"{\"version\":1}"),
            read_error: None,
            source: None,
        },
        Representation {
            kind: "public.png".into(),
            data: STANDARD.encode(&png),
            read_error: None,
            source: None,
        },
    ])
    .unwrap();
    let mut owner = Clipboard::connect().unwrap();
    owner.publish(offer).unwrap();
    let worker = std::thread::spawn(move || owner.serve().unwrap());
    let native = Clipboard::connect()
        .unwrap()
        .read(&protocol::read_formats(&[], false).unwrap(), &[], None)
        .map(|(candidate, _)| candidate)
        .unwrap()
        .unwrap();
    assert_eq!(native.kind, "dev.reshiki.drawing");
    assert_eq!(STANDARD.decode(native.data).unwrap(), b"{\"version\":1}");
    let image = Clipboard::connect()
        .unwrap()
        .read(&protocol::read_formats(&[], true).unwrap(), &[], None)
        .map(|(candidate, _)| candidate)
        .unwrap()
        .unwrap();
    assert_eq!(image.kind, "public.png");
    assert_eq!(STANDARD.decode(image.data).unwrap(), png);
    let mut replacement = Clipboard::connect().unwrap();
    replacement
        .publish(
            protocol::prepare_offer(&[Representation {
                kind: "public.utf8-plain-text".into(),
                data: STANDARD.encode(b"new"),
                read_error: None,
                source: None,
            }])
            .unwrap(),
        )
        .unwrap();
    worker.join().unwrap();
}

// A synthetic owner must keep serving metadata until the reader has validated
// its completed transfer. The completion signal also prevents shutdown races.
fn finish_read_validation(owner: &mut Clipboard, finished: &std::sync::mpsc::Receiver<()>) {
    loop {
        match finished.recv_timeout(Duration::from_millis(10)) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        while let Some(event) = owner.connection.poll_for_event().unwrap() {
            if let Event::SelectionRequest(request) = event {
                owner.request(request).unwrap();
            }
        }
        owner.connection.flush().unwrap();
    }
}
