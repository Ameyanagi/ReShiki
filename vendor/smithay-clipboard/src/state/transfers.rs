//! Nonblocking pipes with bounded memory, concurrency, work and lifetime.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use sctk::data_device_manager::{ReadPipe, WritePipe};
use sctk::reexports::calloop::{PostAction, RegistrationToken};

use super::{State, set_non_blocking};
use crate::mime::{MimeType, normalize_to_lf};
use crate::rich::{
    Completion, Control, Counter, Data, MAX_BYTES, MAX_RETAINED_BYTES, MAX_TRANSFERS, StoredOffer,
    TIMEOUT, oversized,
};

const DISPATCH_BYTES: usize = 64 * 1024;

#[derive(Default)]
pub(super) struct Transfers {
    next_id: u64,
    active: HashMap<u64, Transfer>,
}

struct Transfer {
    token: Option<RegistrationToken>,
    deadline: Instant,
    control: Option<Arc<Control>>,
    body: Body,
}

enum Body {
    Read { bytes: Vec<u8>, memory: Counter, reply: ReadReply },
    Write { _offer: Arc<StoredOffer>, bytes: Arc<[u8]>, written: usize },
}

pub(super) enum ReadReply {
    Text { sender: Sender<io::Result<String>>, mime_type: MimeType },
    Binary { completion: Completion<Option<Data>>, mime_type: String },
}

impl ReadReply {
    fn finish(self, result: io::Result<Vec<u8>>) {
        match self {
            Self::Text { sender, mime_type } => {
                let result = result.map(|bytes| {
                    let text = match String::from_utf8(bytes) {
                        Ok(text) => text,
                        Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
                    };
                    match mime_type {
                        MimeType::TextPlain | MimeType::TextPlainUtf8 => normalize_to_lf(text),
                        MimeType::Utf8String => text,
                    }
                });
                let _ = sender.send(result);
            },
            Self::Binary { completion, mime_type } => {
                completion.finish(result.map(|bytes| Some(Data { mime_type, bytes })));
            },
        }
    }

    fn control(&self) -> Option<Arc<Control>> {
        match self {
            Self::Text { .. } => None,
            Self::Binary { completion, .. } => Some(completion.control.clone()),
        }
    }
}

impl Transfer {
    fn error(&self, now: Instant) -> Option<io::Error> {
        self.control.as_ref().and_then(|control| control.error(now)).or_else(|| {
            (now >= self.deadline)
                .then(|| io::Error::new(io::ErrorKind::TimedOut, "clipboard transfer timed out"))
        })
    }

    fn finish(self, result: io::Result<()>) {
        if let Body::Read { bytes, reply, .. } = self.body {
            reply.finish(result.map(|()| bytes));
        }
    }
}

impl Transfers {
    fn next_id(&mut self) -> io::Result<u64> {
        if self.active.len() >= MAX_TRANSFERS {
            return Err(io::Error::other("too many clipboard pipe transfers"));
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| io::Error::other("clipboard transfer identifiers exhausted"))?;
        Ok(self.next_id)
    }

    pub(super) fn timeout(&self, now: Instant) -> Option<Duration> {
        self.active.values().map(|transfer| transfer.deadline.saturating_duration_since(now)).min()
    }
}

impl State {
    pub(super) fn receive_pipe(&mut self, pipe: ReadPipe, reply: ReadReply) {
        let id = match self.transfers.next_id().and_then(|id| {
            set_non_blocking(&pipe)?;
            Ok(id)
        }) {
            Ok(id) => id,
            Err(error) => {
                reply.finish(Err(error));
                return;
            },
        };
        let memory = match Counter::reserve(self.client.memory.clone(), 0, MAX_RETAINED_BYTES) {
            Ok(memory) => memory,
            Err(error) => {
                reply.finish(Err(error));
                return;
            },
        };
        let control = reply.control();
        let deadline = control.as_ref().map_or_else(|| Instant::now() + TIMEOUT, |c| c.deadline);
        self.transfers.active.insert(
            id,
            Transfer {
                token: None,
                deadline,
                control,
                body: Body::Read { bytes: Vec::new(), memory, reply },
            },
        );
        match self
            .loop_handle
            .insert_source(pipe, move |_, file, state| state.read_ready(id, file.as_ref()))
        {
            Ok(token) => {
                if let Some(transfer) = self.transfers.active.get_mut(&id) {
                    transfer.token = Some(token);
                }
            },
            Err(error) => {
                if let Some(transfer) = self.transfers.active.remove(&id) {
                    transfer.finish(Err(io::Error::other(error.to_string())));
                }
            },
        }
    }

    pub(super) fn send_pipe(&mut self, pipe: WritePipe, offer: Arc<StoredOffer>, bytes: Arc<[u8]>) {
        // An empty private receipt representation completes by closing its fd.
        if bytes.is_empty() {
            return;
        }
        let Ok(id) = self.transfers.next_id() else { return };
        if set_non_blocking(&pipe).is_err() {
            return;
        }
        self.transfers.active.insert(
            id,
            Transfer {
                token: None,
                deadline: Instant::now() + TIMEOUT,
                control: None,
                body: Body::Write { _offer: offer, bytes, written: 0 },
            },
        );
        match self
            .loop_handle
            .insert_source(pipe, move |_, file, state| state.write_ready(id, file.as_ref()))
        {
            Ok(token) => {
                if let Some(transfer) = self.transfers.active.get_mut(&id) {
                    transfer.token = Some(token);
                }
            },
            Err(_) => {
                self.transfers.active.remove(&id);
            },
        }
    }

    fn read_ready(&mut self, id: u64, mut file: &File) -> PostAction {
        let Some(transfer) = self.transfers.active.get_mut(&id) else {
            return PostAction::Remove;
        };
        let result = if let Some(error) = transfer.error(Instant::now()) {
            Some(Err(error))
        } else if let Body::Read { bytes, memory, .. } = &mut transfer.body {
            read_available(&mut file, bytes, memory)
        } else {
            Some(Err(io::Error::other("invalid clipboard read transfer")))
        };
        if let Some(result) = result {
            if let Some(transfer) = self.transfers.active.remove(&id) {
                transfer.finish(result);
            }
            PostAction::Remove
        } else {
            PostAction::Continue
        }
    }

    fn write_ready(&mut self, id: u64, mut file: &File) -> PostAction {
        let Some(transfer) = self.transfers.active.get_mut(&id) else {
            return PostAction::Remove;
        };
        let result = if let Some(error) = transfer.error(Instant::now()) {
            Some(Err(error))
        } else if let Body::Write { bytes, written, .. } = &mut transfer.body {
            write_available(&mut file, bytes, written)
        } else {
            Some(Err(io::Error::other("invalid clipboard write transfer")))
        };
        if let Some(result) = result {
            if let Some(transfer) = self.transfers.active.remove(&id) {
                transfer.finish(result);
            }
            PostAction::Remove
        } else {
            PostAction::Continue
        }
    }

    pub(super) fn expire_transfers(&mut self, now: Instant) {
        let expired: Vec<_> = self
            .transfers
            .active
            .iter()
            .filter_map(|(id, transfer)| transfer.error(now).map(|error| (*id, error)))
            .collect();
        for (id, error) in expired {
            if let Some(transfer) = self.transfers.active.remove(&id) {
                if let Some(token) = transfer.token {
                    self.loop_handle.remove(token);
                }
                transfer.finish(Err(error));
            }
        }
    }
}

fn read_available(
    reader: &mut impl Read,
    bytes: &mut Vec<u8>,
    memory: &mut Counter,
) -> Option<io::Result<()>> {
    let mut buffer = [0_u8; 4096];
    let mut progressed = 0;
    // Bound both bytes and syscall retries, including repeated EINTR.
    for _ in 0..128 {
        match reader.read(&mut buffer) {
            Ok(0) => return Some(Ok(())),
            Ok(count) => {
                if bytes.len().saturating_add(count) > MAX_BYTES {
                    return Some(Err(oversized()));
                }
                if let Err(error) = memory.grow(count, MAX_RETAINED_BYTES) {
                    return Some(Err(error));
                }
                if let Err(error) = bytes.try_reserve_exact(count) {
                    return Some(Err(io::Error::other(error.to_string())));
                }
                bytes.extend_from_slice(&buffer[..count]);
                progressed += count;
                if progressed >= DISPATCH_BYTES {
                    return None;
                }
            },
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return None,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {},
            Err(error) => return Some(Err(error)),
        }
    }
    None
}

fn write_available(
    writer: &mut impl Write,
    bytes: &[u8],
    written: &mut usize,
) -> Option<io::Result<()>> {
    let mut progressed = 0;
    for _ in 0..128 {
        let end = bytes.len().min(written.saturating_add(DISPATCH_BYTES - progressed));
        match writer.write(&bytes[*written..end]) {
            Ok(0) => {
                return Some(Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "clipboard pipe closed",
                )));
            },
            Ok(count) => {
                *written += count;
                progressed += count;
                if *written == bytes.len() {
                    return Some(Ok(()));
                }
                if progressed >= DISPATCH_BYTES {
                    return None;
                }
            },
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return None,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {},
            Err(error) => return Some(Err(error)),
        }
    }
    None
}
