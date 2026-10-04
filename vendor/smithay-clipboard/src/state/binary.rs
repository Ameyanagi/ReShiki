//! Requests use real input state and a positive, generation-specific receipt.

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use sctk::data_device_manager::data_offer::DataOfferError;
use sctk::reexports::client::protocol::wl_callback::{self, WlCallback};
use sctk::reexports::client::protocol::wl_data_device::WlDataDevice;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_backend::client::ObjectId;

use super::transfers::ReadReply;
use super::{ClipboardSource, State};
use crate::rich::{Completion, MAX_TRANSFERS, Operation, unavailable};

pub(super) const MAX_SOURCES: usize = 32;

#[derive(Default)]
pub(super) struct Binary {
    pending: HashMap<u64, Operation>,
    // Keep outstanding sync identifiers even after cancellation/timeout. The
    // callback has no client-side destroy request; retaining this bounded set
    // prevents repeated cancelled requests from growing protocol objects.
    synchronizing: HashSet<u64>,
    writes: HashMap<ObjectId, WriteReceipt>,
}

struct WriteReceipt {
    seat: ObjectId,
    device: WlDataDevice,
    marker: String,
    completion: Completion<()>,
}

impl Binary {
    pub(super) fn timeout(&self, now: Instant) -> Option<Duration> {
        self.pending
            .values()
            .map(|operation| operation.control().deadline)
            .chain(
                self.writes
                    .values()
                    .map(|write| write.completion.control.deadline),
            )
            .map(|deadline| deadline.saturating_duration_since(now))
            .min()
    }
}

impl State {
    pub fn request(&mut self, connection: &Connection, operation: Operation) {
        #[cfg(feature = "wayland-qa")]
        if matches!(&operation, Operation::Write { .. }) {
            crate::qa::hold("request", "");
        }
        if self.client.stopped.load(Ordering::Acquire) {
            operation.fail(unavailable());
            return;
        }
        if let Some(error) = operation.control().error(Instant::now()) {
            operation.fail(error);
            return;
        }
        if self.binary.synchronizing.len() >= MAX_TRANSFERS {
            operation.fail(io::Error::other(
                "clipboard display requests are still pending",
            ));
            return;
        }
        let id = operation.id();
        self.binary.synchronizing.insert(id);
        self.binary.pending.insert(id, operation);
        // Iced and this worker dispatch separate queues on the same display.
        // This asynchronous barrier lets our queue observe input/focus events
        // preceding the user's action. Its reply is never a write receipt.
        connection.display().sync(&self.queue_handle, id);
    }

    fn finish_sync(&mut self, id: u64) {
        self.binary.synchronizing.remove(&id);
        let Some(operation) = self.binary.pending.remove(&id) else {
            return;
        };
        if self.client.stopped.load(Ordering::Acquire) {
            operation.fail(unavailable());
            return;
        }
        if let Some(error) = operation.control().error(Instant::now()) {
            operation.fail(error);
            return;
        }
        let Some(seat_id) = self.latest_seat.clone() else {
            operation.fail(io::Error::other("no clipboard input seat is available"));
            return;
        };
        let Some(seat) = self.seats.get(&seat_id).filter(|seat| seat.has_focus) else {
            operation.fail(io::Error::other(
                "focus ReShiki and try the clipboard command again",
            ));
            return;
        };
        let Some(device) = seat.data_device.as_ref() else {
            operation.fail(io::Error::other(
                "the seat has no standard clipboard data device",
            ));
            return;
        };

        match operation {
            Operation::Write { offer, completion } => {
                if seat.latest_serial == 0 {
                    completion.finish(Err(io::Error::other("no valid clipboard input serial")));
                    return;
                }
                if self.data_sources.len() >= MAX_SOURCES {
                    completion.finish(Err(io::Error::other("clipboard sources are still pending")));
                    return;
                }
                let Some(manager) = &self.data_device_manager_state else {
                    completion.finish(Err(io::Error::other(
                        "standard Wayland clipboard unavailable",
                    )));
                    return;
                };
                let Some(marker) = offer.marker.clone() else {
                    completion.finish(Err(io::Error::other("clipboard receipt marker is missing")));
                    return;
                };
                let source =
                    manager.create_copy_paste_source(&self.queue_handle, offer.mime_types());
                let source_id = source.inner().id();
                self.binary.writes.insert(
                    source_id,
                    WriteReceipt {
                        seat: seat_id,
                        device: device.inner().clone(),
                        marker,
                        completion,
                    },
                );
                source.set_selection(device, seat.latest_serial);
                self.data_sources.push(ClipboardSource { source, offer });
            }
            Operation::Read {
                mime_types,
                completion,
            } => {
                let Some(selection) = device.data().selection_offer() else {
                    completion.finish(Ok(None));
                    return;
                };
                if !self.offers.valid(selection.inner()) {
                    completion.finish(Err(io::Error::other(
                        "clipboard offer metadata exceeds its limit",
                    )));
                    return;
                }
                let chosen = selection.with_mime_types(|available| {
                    mime_types
                        .iter()
                        .find(|mime| available.contains(mime))
                        .cloned()
                });
                let Some(mime_type) = chosen else {
                    completion.finish(Ok(None));
                    return;
                };
                match selection.receive(mime_type.clone()) {
                    Ok(pipe) => {
                        self.receive_pipe(
                            pipe,
                            ReadReply::Binary {
                                completion,
                                mime_type,
                            },
                        );
                    }
                    Err(error) => {
                        let error = match error {
                            DataOfferError::Io(error) => error,
                            DataOfferError::InvalidReceive => {
                                io::Error::other("clipboard offer is not ready")
                            }
                        };
                        completion.finish(Err(error));
                    }
                }
            }
        }
    }

    pub(super) fn selection_receipt(&mut self, device: &WlDataDevice) {
        let Some((seat_id, seat)) = self.seats.iter().find(|(_, seat)| {
            seat.has_focus
                && seat
                    .data_device
                    .as_ref()
                    .is_some_and(|data| data.inner() == device)
        }) else {
            return;
        };
        let Some(selection) = seat
            .data_device
            .as_ref()
            .and_then(|data| data.data().selection_offer())
        else {
            return;
        };
        if !self.offers.valid(selection.inner()) {
            return;
        }
        let ready: Vec<_> = selection.with_mime_types(|mime_types| {
            self.binary
                .writes
                .iter()
                .filter_map(|(source, write)| {
                    (&write.seat == seat_id
                        && &write.device == device
                        && mime_types.contains(&write.marker))
                    .then_some(source.clone())
                })
                .collect()
        });
        for source in ready {
            #[cfg(feature = "wayland-qa")]
            if let Some(write) = self.binary.writes.get(&source) {
                // This offer and generation marker came from the compositor.
                // Only delivery is delayed; no sync or queued-write substitute.
                crate::qa::hold("receipt", &write.marker);
            }
            if let Some(write) = self.binary.writes.remove(&source) {
                let result = write
                    .completion
                    .control
                    .error(Instant::now())
                    .map_or(Ok(()), Err);
                write.completion.finish(result);
            }
        }
    }

    pub(super) fn source_cancelled(&mut self, source: &ObjectId) {
        if let Some(write) = self.binary.writes.remove(source) {
            write.completion.finish(Err(io::Error::other(
                "clipboard ownership was rejected or replaced",
            )));
        }
    }

    pub(super) fn seat_lost_focus(&mut self, seat: &ObjectId) {
        let lost: Vec<_> = self
            .binary
            .writes
            .iter()
            .filter_map(|(source, write)| (&write.seat == seat).then_some(source.clone()))
            .collect();
        for source in lost {
            if let Some(write) = self.binary.writes.remove(&source) {
                write.completion.finish(Err(io::Error::other(
                    "clipboard focus changed before publication was confirmed; try again",
                )));
            }
        }
    }

    pub fn maintain(&mut self) {
        let now = Instant::now();
        let expired: Vec<_> = self
            .binary
            .pending
            .iter()
            .filter_map(|(id, operation)| operation.control().error(now).map(|error| (*id, error)))
            .collect();
        for (id, error) in expired {
            if let Some(operation) = self.binary.pending.remove(&id) {
                operation.fail(error);
            }
        }
        let expired: Vec<_> = self
            .binary
            .writes
            .iter()
            .filter_map(|(source, write)| {
                write
                    .completion
                    .control
                    .error(now)
                    .map(|error| (source.clone(), error))
            })
            .collect();
        for (source, error) in expired {
            if let Some(write) = self.binary.writes.remove(&source) {
                write.completion.finish(Err(error));
            }
        }
        // A timed-out or cancelled publication never unsets the selection or
        // drops a sent source. It can still be serving another application.
        self.expire_transfers(now);
        self.offers.prune();
    }

    pub fn timeout(&self) -> Option<Duration> {
        let now = Instant::now();
        self.binary
            .timeout(now)
            .into_iter()
            .chain(self.transfers.timeout(now))
            .min()
    }
}

impl Dispatch<WlCallback, u64, State> for State {
    fn event(
        state: &mut State,
        _: &WlCallback,
        event: wl_callback::Event,
        id: &u64,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.finish_sync(*id);
        }
    }
}
