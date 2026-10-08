use super::*;
use std::time::Duration;

/// A sink that records every forwarded report.
fn recorder() -> (Sink, Arc<Mutex<Vec<Progress>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink: Sink = {
        let seen = seen.clone();
        Arc::new(move |progress| seen.lock().unwrap().push(progress))
    };
    (sink, seen)
}

fn at(completed: f64) -> Progress {
    Progress {
        completed,
        total: Some(10.),
        message: None,
    }
}

fn completed(seen: &Mutex<Vec<Progress>>) -> Vec<f64> {
    seen.lock().unwrap().iter().map(|p| p.completed).collect()
}

#[test]
fn monotonic_forwards_only_strictly_increasing_progress() {
    let (sink, seen) = recorder();
    let monotonic = Monotonic::new(sink);
    for value in [1., 1., 0.5, 2., f64::NAN, f64::INFINITY, 3.] {
        monotonic.report(at(value));
    }
    assert_eq!(completed(&seen), [1., 2., 3.]);
    assert_eq!(seen.lock().unwrap()[0], at(1.));
}

#[test]
fn monotonic_drops_every_report_after_finish() {
    let (sink, seen) = recorder();
    let monotonic = Monotonic::new(sink);
    monotonic.report(at(1.));
    monotonic.finish();
    monotonic.report(at(2.));
    monotonic.finish();
    monotonic.report(at(3.));
    assert_eq!(completed(&seen), [1.]);
}

#[test]
fn a_first_report_of_zero_is_forwarded() {
    let (sink, seen) = recorder();
    let monotonic = Monotonic::new(sink);
    monotonic.report(at(0.));
    monotonic.report(at(-1.));
    assert_eq!(completed(&seen), [0.]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forward_maps_structure_counts_and_ignores_proposals_and_previews() {
    let (sink, seen) = recorder();
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let forwarder = tokio::spawn(forward(rx, Arc::new(Monotonic::new(sink))));
    let proposal = crate::Proposal {
        explanation: String::new(),
        replace_ids: Vec::new(),
        molecules: Vec::new(),
        reactions: Vec::new(),
        composition: Default::default(),
        sketch: None,
    };
    for event in [
        Event::Proposal(Box::new(proposal)),
        Event::Structures {
            completed: 1,
            total: 3,
        },
        Event::Preview(Box::default()),
        Event::Structures {
            completed: 1,
            total: 3,
        },
        Event::Structures {
            completed: 2,
            total: 3,
        },
    ] {
        tx.send(event).await.unwrap();
    }
    drop(tx);
    tokio::time::timeout(Duration::from_secs(30), forwarder)
        .await
        .unwrap()
        .unwrap();
    let structures = |completed| Progress {
        completed,
        total: Some(3.),
        message: None,
    };
    assert_eq!(*seen.lock().unwrap(), [structures(1.), structures(2.)]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forward_stops_reporting_once_the_call_finishes() {
    let (sink, seen) = recorder();
    let monotonic = Arc::new(Monotonic::new(sink));
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let forwarder = tokio::spawn(forward(rx, monotonic.clone()));
    tx.send(Event::Structures {
        completed: 1,
        total: 2,
    })
    .await
    .unwrap();
    // Finish only once the first report has been forwarded.
    tokio::time::timeout(Duration::from_secs(30), async {
        while seen.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    monotonic.finish();
    tx.send(Event::Structures {
        completed: 2,
        total: 2,
    })
    .await
    .unwrap();
    drop(tx);
    tokio::time::timeout(Duration::from_secs(30), forwarder)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed(&seen), [1.]);
}
