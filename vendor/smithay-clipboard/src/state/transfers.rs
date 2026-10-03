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
    Read {
        bytes: Vec<u8>,
        memory: Counter,
        reply: ReadReply,
    },
    Write {
        _offer: Arc<StoredOffer>,
        bytes: Arc<[u8]>,
        written: usize,
    },
}

pub(super) enum ReadReply {
    Text {
        sender: Sender<io::Result<String>>,
        mime_type: MimeType,
    },
    Binary {
        completion: Completion<Option<Data>>,
        mime_type: String,
    },
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
            }
            Self::Binary {
                completion,
                mime_type,
            } => {
                completion.finish(result.map(|bytes| Some(Data { mime_type, bytes })));
            }
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
        self.control
            .as_ref()
            .and_then(|control| control.error(now))
            .or_else(|| {
                (now >= self.deadline).then(|| {
                    io::Error::new(io::ErrorKind::TimedOut, "clipboard transfer timed out")
                })
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
        self.active
            .values()
            .map(|transfer| transfer.deadline.saturating_duration_since(now))
            .min()
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
            }
        };
        let memory = match Counter::reserve(self.client.memory.clone(), 0, MAX_RETAINED_BYTES) {
            Ok(memory) => memory,
            Err(error) => {
                reply.finish(Err(error));
                return;
            }
        };
        let control = reply.control();
        let deadline = control
            .as_ref()
            .map_or_else(|| Instant::now() + TIMEOUT, |c| c.deadline);
        self.transfers.active.insert(
            id,
            Transfer {
                token: None,
                deadline,
                control,
                body: Body::Read {
                    bytes: Vec::new(),
                    memory,
                    reply,
                },
            },
        );
        match self.loop_handle.insert_source(pipe, move |_, file, state| {
            state.read_ready(id, file.as_ref())
        }) {
            Ok(token) => {
                if let Some(transfer) = self.transfers.active.get_mut(&id) {
                    transfer.token = Some(token);
                }
            }
            Err(error) => {
                if let Some(transfer) = self.transfers.active.remove(&id) {
                    transfer.finish(Err(io::Error::other(error.to_string())));
                }
            }
        }
    }

    pub(super) fn send_pipe(&mut self, pipe: WritePipe, offer: Arc<StoredOffer>, bytes: Arc<[u8]>) {
        // An empty private receipt representation completes by closing its fd.
        if bytes.is_empty() {
            return;
        }
        let Ok(id) = self.transfers.next_id() else {
            return;
        };
        if set_non_blocking(&pipe).is_err() {
            return;
        }
        self.transfers.active.insert(
            id,
            Transfer {
                token: None,
                deadline: Instant::now() + TIMEOUT,
                control: None,
                body: Body::Write {
                    _offer: offer,
                    bytes,
                    written: 0,
                },
            },
        );
        match self.loop_handle.insert_source(pipe, move |_, file, state| {
            state.write_ready(id, file.as_ref())
        }) {
            Ok(token) => {
                if let Some(transfer) = self.transfers.active.get_mut(&id) {
                    transfer.token = Some(token);
                }
            }
            Err(_) => {
                self.transfers.active.remove(&id);
            }
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
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return None,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
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
        let end = bytes
            .len()
            .min(written.saturating_add(DISPATCH_BYTES - progressed));
        match writer.write(&bytes[*written..end]) {
            Ok(0) => {
                return Some(Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "clipboard pipe closed",
                )));
            }
            Ok(count) => {
                *written += count;
                progressed += count;
                if *written == bytes.len() {
                    return Some(Ok(()));
                }
                if progressed >= DISPATCH_BYTES {
                    return None;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return None,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Some(Err(error)),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich::{Client, MAX_RETAINED_BYTES};
    use crate::worker::Command;
    use sctk::reexports::calloop::channel;
    use std::future::Future;
    use std::io::Cursor;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll, Waker};

    struct ChunkReader {
        data: Cursor<Vec<u8>>,
        calls: usize,
    }

    impl Read for ChunkReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.calls % 3 == 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let end = buffer.len().min(127);
            self.data.read(&mut buffer[..end])
        }
    }

    #[derive(Default)]
    struct ChunkWriter {
        bytes: Vec<u8>,
        calls: usize,
    }

    impl Write for ChunkWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.calls % 5 == 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let count = bytes.len().min(31);
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn memory() -> (Arc<AtomicUsize>, Counter) {
        let used = Arc::new(AtomicUsize::new(0));
        let counter = Counter::reserve(used.clone(), 0, MAX_RETAINED_BYTES).unwrap();
        (used, counter)
    }

    #[test]
    fn short_binary_reads_and_would_block_preserve_every_byte_until_eof() {
        let expected: Vec<_> = (0..70_003).map(|index| (index % 256) as u8).collect();
        let mut reader = ChunkReader {
            data: Cursor::new(expected.clone()),
            calls: 0,
        };
        let mut received = Vec::new();
        let (used, mut memory) = memory();
        let mut done = false;
        for _ in 0..10_000 {
            let previous_calls = reader.calls;
            if let Some(result) = read_available(&mut reader, &mut received, &mut memory) {
                result.unwrap();
                done = true;
                break;
            }
            assert!(reader.calls - previous_calls <= 128);
        }
        assert!(done, "the pipe must reach EOF");
        assert_eq!(received, expected);
        assert_eq!(used.load(Ordering::Acquire), received.len());
        drop(memory);
        assert_eq!(used.load(Ordering::Acquire), 0);
    }

    #[test]
    fn short_binary_writes_resume_from_the_exact_offset() {
        let expected: Vec<_> = (0..70_003).map(|index| (index % 256) as u8).collect();
        let mut writer = ChunkWriter::default();
        let mut written = 0;
        let mut done = false;
        for _ in 0..10_000 {
            let previous_calls = writer.calls;
            if let Some(result) = write_available(&mut writer, &expected, &mut written) {
                result.unwrap();
                done = true;
                break;
            }
            assert!(writer.calls - previous_calls <= 128);
        }
        assert!(done);
        assert_eq!(written, expected.len());
        assert_eq!(writer.bytes, expected);
    }

    #[test]
    fn a_zero_write_terminates_instead_of_spinning() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Ok(0)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut written = 0;
        let error = write_available(&mut Closed, b"payload", &mut written)
            .unwrap()
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert_eq!(written, 0);
    }

    #[test]
    fn repeated_interrupts_are_bounded_per_dispatch() {
        struct Interrupted(usize);
        impl Read for Interrupted {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                self.0 += 1;
                Err(io::ErrorKind::Interrupted.into())
            }
        }
        impl Write for Interrupted {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                self.0 += 1;
                Err(io::ErrorKind::Interrupted.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let (_, mut memory) = memory();
        let mut read = Interrupted(0);
        assert!(read_available(&mut read, &mut Vec::new(), &mut memory).is_none());
        assert_eq!(read.0, 128);
        let mut write = Interrupted(0);
        assert!(write_available(&mut write, b"payload", &mut 0).is_none());
        assert_eq!(write.0, 128);
    }

    #[test]
    fn a_read_cannot_exceed_its_representation_or_shared_retention_budget() {
        let used = Arc::new(AtomicUsize::new(0));
        let occupied =
            Counter::reserve(used.clone(), MAX_RETAINED_BYTES - 3, MAX_RETAINED_BYTES).unwrap();
        let mut memory = Counter::reserve(used.clone(), 0, MAX_RETAINED_BYTES).unwrap();
        let mut bytes = Vec::new();
        let mut reader = Cursor::new(b"four");
        assert!(
            read_available(&mut reader, &mut bytes, &mut memory)
                .unwrap()
                .is_err()
        );
        assert!(bytes.is_empty());
        assert_eq!(used.load(Ordering::Acquire), MAX_RETAINED_BYTES - 3);
        drop(occupied);

        let mut full = vec![0; MAX_BYTES];
        let mut one_more = Cursor::new([1]);
        assert!(
            read_available(&mut one_more, &mut full, &mut memory)
                .unwrap()
                .is_err()
        );
        assert_eq!(full.len(), MAX_BYTES);
    }

    #[test]
    fn expiry_returns_an_error_instead_of_delivering_a_partial_read() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let (_, memory) = memory();
        let transfer = Transfer {
            token: None,
            deadline: Instant::now(),
            control: None,
            body: Body::Read {
                bytes: b"partial".to_vec(),
                memory,
                reply: ReadReply::Text {
                    sender,
                    mime_type: MimeType::TextPlain,
                },
            },
        };
        let error = transfer.error(Instant::now()).unwrap();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        transfer.finish(Err(error));
        assert_eq!(
            receiver.try_recv().unwrap().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }

    #[test]
    fn binary_completion_does_not_apply_the_legacy_text_normalizer() {
        let (sender, commands) = channel::channel();
        let client = Client::new(100_000, sender);
        client.inner.ready.store(true, Ordering::Release);
        let mut request = client.read(vec!["image/png".into()]);
        let Command::Rich(crate::rich::Operation::Read { completion, .. }) =
            commands.try_recv().unwrap()
        else {
            panic!()
        };
        let bytes = vec![0, 255, b'\r', b'\n', 128];
        ReadReply::Binary {
            completion,
            mime_type: "image/png".into(),
        }
        .finish(Ok(bytes.clone()));
        let result = Pin::new(&mut request).poll(&mut Context::from_waker(Waker::noop()));
        assert!(
            matches!(result, Poll::Ready(Ok(Some(data))) if data.bytes == bytes && data.mime_type == "image/png")
        );
        client.stop(100_000);

        let (sender, receiver) = std::sync::mpsc::channel();
        ReadReply::Text {
            sender,
            mime_type: MimeType::TextPlain,
        }
        .finish(Ok(b"a\r\nb\rc".to_vec()));
        assert_eq!(receiver.try_recv().unwrap().unwrap(), "a\nb\nc");
    }
}
