//! Stream the shared conformer search's progress to the Calculate card.

use super::{super::state::Failure, Phase};
use iced::futures::{Stream, channel::mpsc};
use reshiki_xtb_jobs::conformers as driver;
pub(crate) use reshiki_xtb_jobs::conformers::{Found, Search};
use reshiki_xtb_worker::client::Cancel;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc as sync_channel,
    thread,
    time::Duration,
};

#[derive(Clone, Debug)]
pub(crate) enum Event {
    Phase(Phase),
    Tick,
    /// Always the last event.
    Done(Result<Box<Found>, Failure>),
}

/// Start `search` on a new thread. The stream ends after `Event::Done`.
pub(crate) fn start(search: Search, cancel: Cancel) -> impl Stream<Item = Event> + Send + 'static {
    let (tx, rx) = mpsc::unbounded();
    let worker_tx = tx.clone();
    let worker = thread::Builder::new()
        .name("conformer-search".into())
        .spawn(move || {
            let (stop_ticks, tick_wait) = sync_channel::channel::<()>();
            let tick_tx = worker_tx.clone();
            let tick_cancel = cancel.clone();
            let ticker = thread::Builder::new()
                .name("conformer-tick".into())
                .spawn(move || {
                    while let Err(sync_channel::RecvTimeoutError::Timeout) =
                        tick_wait.recv_timeout(Duration::from_millis(250))
                    {
                        if tick_tx.unbounded_send(Event::Tick).is_err() {
                            tick_cancel.cancel();
                            break;
                        }
                    }
                });
            let result = match ticker {
                Ok(ticker) => {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        driver::search_conformers(search, &cancel, |phase| {
                            let _ = worker_tx.unbounded_send(Event::Phase(phase));
                        })
                    }))
                    .unwrap_or_else(|_| Err(Failure::crashed("Conformer search thread panicked")));
                    drop(stop_ticks);
                    let _ = ticker.join();
                    result
                }
                Err(error) => Err(Failure::Other(error.to_string())),
            };
            let _ = worker_tx.unbounded_send(Event::Done(result.map(Box::new)));
            worker_tx.close_channel();
        });
    if let Err(error) = worker {
        let _ = tx.unbounded_send(Event::Done(Err(Failure::Other(error.to_string()))));
        tx.close_channel();
    }
    rx
}
