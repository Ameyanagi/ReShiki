use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::time::Duration;

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
                    return Ok(());
                }
            }
        }
    });
    let result = Clipboard::connect().unwrap().read(true);
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
    });
    let result = Clipboard::connect().unwrap().read(true);
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
fn completed_data_survives_one_shot_owner_exit() {
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
    let result = Clipboard::connect().unwrap().read(true).unwrap().unwrap();
    assert_eq!(STANDARD.decode(result.data).unwrap(), vec![37_u8; 100]);
    worker.join().unwrap();
}

#[test]
#[ignore = "requires a dedicated X11 server (run under Xvfb)"]
fn oversized_incremental_header_is_rejected_before_allocation() {
    let mut owner = image_owner(100);
    let target = *owner.data.keys().next().unwrap();
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
    });
    let result = Clipboard::connect().unwrap().read(true).unwrap_err();
    assert!(result.contains("oversized"), "{result}");
    worker.join().unwrap();
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
    assert!(Clipboard::connect().unwrap().read(true).unwrap().is_some());
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
        },
        Representation {
            kind: "public.png".into(),
            data: STANDARD.encode(&png),
        },
    ])
    .unwrap();
    let mut owner = Clipboard::connect().unwrap();
    owner.publish(offer).unwrap();
    let worker = std::thread::spawn(move || owner.serve().unwrap());
    let native = Clipboard::connect().unwrap().read(false).unwrap().unwrap();
    assert_eq!(native.kind, "dev.reshiki.drawing");
    assert_eq!(STANDARD.decode(native.data).unwrap(), b"{\"version\":1}");
    let image = Clipboard::connect().unwrap().read(true).unwrap().unwrap();
    assert_eq!(image.kind, "public.png");
    assert_eq!(STANDARD.decode(image.data).unwrap(), png);
    let mut replacement = Clipboard::connect().unwrap();
    replacement
        .publish(
            protocol::prepare_offer(&[Representation {
                kind: "public.utf8-plain-text".into(),
                data: STANDARD.encode(b"new"),
            }])
            .unwrap(),
        )
        .unwrap();
    worker.join().unwrap();
}
