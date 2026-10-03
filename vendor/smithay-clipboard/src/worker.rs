use std::io::{Error, Result};
use std::sync::mpsc::Sender;
use std::sync::{Arc, atomic::Ordering};

use sctk::reexports::calloop::channel::Channel;
use sctk::reexports::calloop::{EventLoop, channel};
use sctk::reexports::calloop_wayland_source::WaylandSource;
use sctk::reexports::client::Connection;
use sctk::reexports::client::globals::registry_queue_init;

use crate::rich::{Inner, Operation};
use crate::state::{SelectionTarget, State};

/// Spawn a clipboard worker, which dispatches its own `EventQueue` and handles
/// clipboard requests.
pub fn spawn(
    name: String,
    display: Connection,
    rx_chan: Channel<Command>,
    worker_replier: Sender<Result<String>>,
    client: Arc<Inner>,
) -> Option<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name(name)
        .spawn(move || {
            worker_impl(display, rx_chan, worker_replier, client);
        })
        .ok()
}

/// Clipboard worker thread command.
pub enum Command {
    /// Store data to a clipboard.
    Store(String),
    /// Store data to a primary selection.
    StorePrimary(String),
    /// Load data from a clipboard.
    Load,
    /// Load primary selection.
    LoadPrimary,
    /// A bounded binary request from a command-only client.
    Rich(Operation),
    /// Wake maintenance after a request future is cancelled.
    Wake,
    /// Shutdown the worker.
    Exit,
}

/// Handle clipboard requests.
fn worker_impl(
    connection: Connection,
    rx_chan: Channel<Command>,
    reply_tx: Sender<Result<String>>,
    client: Arc<Inner>,
) {
    // Cover initialization failure, connection loss and ordinary owner shutdown.
    struct StopOnExit(Arc<Inner>);
    impl Drop for StopOnExit {
        fn drop(&mut self) {
            self.0.ready.store(false, Ordering::Release);
            self.0.stopped.store(true, Ordering::Release);
        }
    }
    let _stop = StopOnExit(client.clone());
    let (globals, event_queue) = match registry_queue_init(&connection) {
        Ok(data) => data,
        Err(_) => return,
    };

    let Ok(mut event_loop) = EventLoop::<State>::try_new() else { return };
    let loop_handle = event_loop.handle();

    let mut state = match State::new(
        &globals,
        &event_queue.handle(),
        loop_handle.clone(),
        reply_tx,
        client.clone(),
    ) {
        Some(state) => state,
        None => return,
    };

    let command_connection = connection.clone();
    if loop_handle
        .insert_source(rx_chan, move |event, _, state| {
            if let channel::Event::Msg(event) = event {
                match event {
                    Command::StorePrimary(contents) => {
                        state.store_selection(SelectionTarget::Primary, contents);
                    },
                    Command::Store(contents) => {
                        state.store_selection(SelectionTarget::Clipboard, contents);
                    },
                    Command::Load if state.data_device_manager_state.is_some() => {
                        if let Err(err) = state.load_selection(SelectionTarget::Clipboard) {
                            let _ = state.reply_tx.send(Err(err));
                        }
                    },
                    Command::LoadPrimary if state.data_device_manager_state.is_some() => {
                        if let Err(err) = state.load_selection(SelectionTarget::Primary) {
                            let _ = state.reply_tx.send(Err(err));
                        }
                    },
                    Command::Load | Command::LoadPrimary => {
                        let _ = state
                            .reply_tx
                            .send(Err(Error::other("requested selection is not supported")));
                    },
                    Command::Rich(operation) => state.request(&command_connection, operation),
                    Command::Wake => {},
                    Command::Exit => state.exit = true,
                }
            } else {
                state.exit = true;
            }
        })
        .is_err()
    {
        return;
    }

    if WaylandSource::new(connection, event_queue).insert(loop_handle).is_err() {
        return;
    }
    client.ready.store(true, Ordering::Release);

    loop {
        state.maintain();
        if event_loop.dispatch(state.timeout(), &mut state).is_err() || state.exit {
            break;
        }
    }
}
