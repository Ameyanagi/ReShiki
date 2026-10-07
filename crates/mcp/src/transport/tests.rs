use super::*;
use crate::framing::{Admit, Limits};
use serde_json::{Value, json};
use tokio::sync::mpsc;

/// The framing admits it, but rmcp cannot parse it: 1e400 overflows f64.
const INVALID: &[u8] = br#"{"jsonrpc":"2.0","id":7,"method":"tools/list","params":{"n":1e400}}"#;

/// A transport whose outbound queue holds one line and is already full, with
/// one admitted request waiting that rmcp cannot parse.
fn blocked() -> (
    Stdio,
    mpsc::Sender<Inbound>,
    mpsc::Receiver<Outbound>,
    Arc<Tracker>,
) {
    let tracker = Arc::new(Tracker::new(&Limits::default()));
    assert_eq!(tracker.admit(&Key::Int(7), INVALID.len()), Admit::Ok);
    let (requests, inbound) = mpsc::channel(1);
    requests
        .try_send(Inbound::Request {
            key: Key::Int(7),
            line: INVALID.to_vec(),
        })
        .unwrap();
    let (outbound, written) = mpsc::channel(1);
    outbound
        .try_send(Outbound {
            key: None,
            line: b"filler".to_vec(),
        })
        .unwrap();
    let stdio = Stdio {
        inbound,
        outbound,
        tracker: Arc::clone(&tracker),
        status: Arc::new(Status::default()),
        log: Log::silent(),
        max_line: usize::MAX,
        initialize_delivered: false,
        pending: None,
    };
    (stdio, requests, written, tracker)
}

/// Polls `receive` once and drops it, as rmcp does when another event wins
/// its select.
async fn receive_then_drop(stdio: &mut Stdio) {
    tokio::select! {
        biased;
        _ = stdio.receive() => panic!("the outbound queue is full"),
        () = std::future::ready(()) => {}
    }
}

#[tokio::test]
async fn a_dropped_receive_keeps_the_reply_it_owes() {
    let (mut stdio, requests, mut written, tracker) = blocked();
    receive_then_drop(&mut stdio).await;
    // The request was taken and its slot is still held.
    assert_eq!(requests.capacity(), 1);
    assert_eq!(tracker.outstanding(), 1);

    assert_eq!(written.recv().await.unwrap().line, b"filler");
    drop(requests);
    // The next call queues the reply before it reads on.
    assert!(stdio.receive().await.is_none());
    let reply = written.try_recv().unwrap();
    assert_eq!(reply.key, Some(Key::Int(7)));
    assert_eq!(
        serde_json::from_slice::<Value>(&reply.line).unwrap(),
        json!({"jsonrpc": "2.0", "id": 7, "error": {"code": -32600, "message": "Invalid Request"}})
    );
}

#[tokio::test]
async fn dropping_the_transport_settles_a_pending_reply() {
    // No room: the slot is freed, since the writer never sees the reply.
    let (mut stdio, _requests, mut written, tracker) = blocked();
    receive_then_drop(&mut stdio).await;
    drop(stdio);
    assert_eq!(tracker.outstanding(), 0);
    assert_eq!(written.recv().await.unwrap().line, b"filler");
    assert!(written.recv().await.is_none());

    // Room: the reply is queued, and its slot stays for the writer.
    let (mut stdio, _requests, mut written, tracker) = blocked();
    receive_then_drop(&mut stdio).await;
    assert_eq!(written.recv().await.unwrap().line, b"filler");
    drop(stdio);
    assert_eq!(tracker.outstanding(), 1);
    assert_eq!(written.recv().await.unwrap().key, Some(Key::Int(7)));
}
