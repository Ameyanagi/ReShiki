//! Stream the shared xTB job's progress to the Calculate card.

use super::state::Failure;
use iced::futures::{Stream, channel::mpsc};
use reshiki_xtb_jobs::run as driver;
pub(crate) use reshiki_xtb_jobs::run::{Job, Report};
#[cfg(test)]
pub(crate) use reshiki_xtb_jobs::run::{example_computed, example_engine};
use reshiki_xtb_worker::client::Cancel;
use std::{sync::Arc, time::Duration};

#[derive(Debug, Clone)]
pub(crate) enum Event {
    /// Accepted steps since the previous event, starting at `first`.
    /// Step 0 is the input. Positions are in Å for the newest step.
    Steps {
        first: usize,
        energies: Vec<f64>,
        positions: Vec<[f64; 3]>,
    },
    /// The final Hessian evaluation is starting.
    Hessian,
    /// Elapsed time goes on while the worker computes: every 250 ms.
    Tick,
    /// The last event of a job.
    Done(Result<Arc<Report>, Failure>),
}

/// Start `job` on a new thread. The stream ends after `Event::Done`.
pub(crate) fn start(job: Job, cancel: Cancel) -> impl Stream<Item = Event> + Send + 'static {
    let (tx, rx) = mpsc::unbounded();
    let ticks = tx.clone();
    // Ends when the receiver is dropped or the channel is closed after Done.
    let _ = std::thread::Builder::new()
        .name("xtb-tick".into())
        .spawn(move || {
            while !ticks.is_closed() {
                std::thread::sleep(Duration::from_millis(250));
                if ticks.unbounded_send(Event::Tick).is_err() {
                    break;
                }
            }
        });
    let worker_tx = tx.clone();
    let thread = std::thread::Builder::new()
        .name("xtb-run".into())
        .spawn(move || {
            let result = std::env::current_exe()
                .map_err(|_| Failure::Crashed("could not start the xTB worker".into()))
                .and_then(|executable| {
                    driver::run(&executable, job, &cancel, |event| {
                        let event = match event {
                            driver::Event::Steps {
                                first,
                                energies,
                                positions,
                            } => Event::Steps {
                                first,
                                energies,
                                positions,
                            },
                            driver::Event::Hessian => Event::Hessian,
                        };
                        if worker_tx.unbounded_send(event).is_err() {
                            cancel.cancel();
                        }
                    })
                });
            let done = Event::Done(result.map(Arc::new));
            let _ = worker_tx.unbounded_send(done);
            worker_tx.close_channel();
        });
    if let Err(error) = thread {
        let _ = tx.unbounded_send(Event::Done(Err(Failure::Other(error.to_string()))));
        tx.close_channel();
    }
    rx
}
