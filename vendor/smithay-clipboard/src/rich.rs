//! Bounded binary clipboard requests through an existing GUI-owned connection.
//!
//! A [`Client`] contains only a command sender. The original [`crate::Clipboard`]
//! remains the sole owner of the Wayland worker, and always stops and joins it
//! before releasing its borrowed display. Keeping a client cannot extend that
//! lifetime. This ReShiki extension does not create another data device.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::io::{self, Read};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures_channel::oneshot;
use sctk::reexports::calloop::channel::Sender;

use crate::worker::Command;

/// Maximum bytes in one published offer or received representation.
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
/// Includes the private ownership-receipt MIME added to a binary offer.
pub const MAX_FORMATS: usize = 128;
/// Maximum simultaneously queued requests or pipe transfers.
pub const MAX_TRANSFERS: usize = 16;
/// Published snapshots and in-progress read buffers share this retention cap.
pub const MAX_RETAINED_BYTES: usize = 2 * MAX_BYTES;
/// An operation or a consumer's pipe may not wait indefinitely.
pub const TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) const MARKER_PREFIX: &str = "application/x-reshiki-clipboard-receipt-";
const MAX_MIME_BYTES: usize = 255;

/// MIME aliases can share the same immutable allocation.
pub type Offer = BTreeMap<String, Arc<[u8]>>;

/// One selected representation, with its exact bytes and MIME name.
#[derive(Debug, Eq, PartialEq)]
pub struct Data {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

/// A cancellable reply. Dropping it stops pending preparation or pipe reads.
/// A source already sent to the compositor remains available until replacement
/// or shutdown; failure cleanup never clears another application's selection.
pub struct Request<T> {
    receiver: oneshot::Receiver<io::Result<T>>,
    control: Arc<Control>,
    wake: Sender<Command>,
    finished: bool,
}

impl<T> Future for Request<T> {
    type Output = io::Result<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.receiver).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(reply) => {
                self.finished = true;
                Poll::Ready(reply.unwrap_or_else(|_| Err(unavailable())))
            },
        }
    }
}

impl<T> Drop for Request<T> {
    fn drop(&mut self) {
        if !self.finished {
            self.control.cancelled.store(true, Ordering::Release);
            let _ = self.wake.send(Command::Wake);
        }
    }
}

/// A command-only handle to a clipboard owned by the GUI toolkit.
#[derive(Clone)]
pub struct Client {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub sender: Sender<Command>,
    pub stopped: AtomicBool,
    pub ready: AtomicBool,
    sequence: AtomicU64,
    pending: Arc<AtomicUsize>,
    pub memory: Arc<AtomicUsize>,
}

type Registry = Mutex<HashMap<usize, Weak<Inner>>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

impl Client {
    /// Finds the already-created clipboard for an observed Wayland display.
    ///
    /// `display_key` is only an opaque lookup key (the display pointer cast to
    /// `usize`); this function never dereferences it. An unknown, starting, or
    /// stopped owner returns `None`. This cannot connect to or keep a display
    /// alive. Obtain the key from the actual GUI window's display handle.
    pub fn for_display(display_key: usize) -> Option<Self> {
        let mut entries = registry().lock().unwrap_or_else(|error| error.into_inner());
        entries.retain(|_, entry| entry.strong_count() != 0);
        let inner = entries.get(&display_key)?.upgrade()?;
        (!inner.stopped.load(Ordering::Acquire) && inner.ready.load(Ordering::Acquire))
            .then_some(Self { inner })
    }

    pub(crate) fn new(display_key: usize, sender: Sender<Command>) -> Self {
        let inner = Arc::new(Inner {
            sender,
            stopped: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            sequence: AtomicU64::new(1),
            pending: Arc::new(AtomicUsize::new(0)),
            memory: Arc::new(AtomicUsize::new(0)),
        });
        registry()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(display_key, Arc::downgrade(&inner));
        Self { inner }
    }

    pub(crate) fn stop(&self, display_key: usize) {
        self.inner.stopped.store(true, Ordering::Release);
        let mut entries = registry().lock().unwrap_or_else(|error| error.into_inner());
        if entries
            .get(&display_key)
            .is_some_and(|entry| Weak::ptr_eq(entry, &Arc::downgrade(&self.inner)))
        {
            entries.remove(&display_key);
        }
    }

    /// Publishes a complete immutable offer. Success requires seeing this
    /// generation in the focused seat's resulting standard selection offer.
    /// Queueing a request or completing a display roundtrip is not success.
    pub fn write(&self, offer: Offer) -> Request<()> {
        self.submit(|completion| match StoredOffer::new(offer, self.inner.memory.clone(), true) {
            Ok(offer) => Some(Operation::Write { offer, completion }),
            Err(error) => {
                completion.finish(Err(error));
                None
            },
        })
    }

    /// Receives the first supported MIME in caller priority order. `None`
    /// means the selection is empty or has no requested representation.
    /// Binary data is not decoded as text or normalized.
    pub fn read(&self, mime_types: Vec<String>) -> Request<Option<Data>> {
        self.submit(|completion| match validate_types(&mime_types) {
            Ok(()) => Some(Operation::Read { mime_types, completion }),
            Err(error) => {
                completion.finish(Err(error));
                None
            },
        })
    }

    fn submit<T>(&self, build: impl FnOnce(Completion<T>) -> Option<Operation>) -> Request<T> {
        let (sender, receiver) = oneshot::channel();
        let control = Arc::new(Control {
            cancelled: AtomicBool::new(false),
            deadline: Instant::now() + TIMEOUT,
        });
        let request = Request {
            receiver,
            control: control.clone(),
            wake: self.inner.sender.clone(),
            finished: false,
        };
        let permit = Counter::reserve(self.inner.pending.clone(), 1, MAX_TRANSFERS);
        let id = self
            .inner
            .sequence
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value.checked_add(1));
        let mut completion = Completion { sender: Some(sender), control, permit: None, id: 0 };
        if self.inner.stopped.load(Ordering::Acquire) || !self.inner.ready.load(Ordering::Acquire) {
            completion.finish(Err(unavailable()));
        } else if let (Ok(permit), Ok(id)) = (permit, id) {
            completion.permit = Some(permit);
            completion.id = id;
            if let Some(operation) = build(completion) {
                let _ = self.inner.sender.send(Command::Rich(operation));
            }
        } else {
            completion.finish(Err(io::Error::other("too many pending clipboard requests")));
        }
        request
    }
}

pub(crate) struct Control {
    pub cancelled: AtomicBool,
    pub deadline: Instant,
}

impl Control {
    pub fn error(&self, now: Instant) -> Option<io::Error> {
        if self.cancelled.load(Ordering::Acquire) {
            Some(io::Error::new(io::ErrorKind::Interrupted, "clipboard request cancelled"))
        } else if now >= self.deadline {
            Some(io::Error::new(io::ErrorKind::TimedOut, "clipboard request timed out"))
        } else {
            None
        }
    }
}

pub(crate) struct Completion<T> {
    sender: Option<oneshot::Sender<io::Result<T>>>,
    pub control: Arc<Control>,
    permit: Option<Counter>,
    pub id: u64,
}

impl<T> Completion<T> {
    pub fn finish(mut self, result: io::Result<T>) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(result);
        }
    }
}

pub(crate) enum Operation {
    Write { offer: Arc<StoredOffer>, completion: Completion<()> },
    Read { mime_types: Vec<String>, completion: Completion<Option<Data>> },
}

impl Operation {
    pub fn id(&self) -> u64 {
        match self {
            Self::Write { completion, .. } => completion.id,
            Self::Read { completion, .. } => completion.id,
        }
    }

    pub fn control(&self) -> &Control {
        match self {
            Self::Write { completion, .. } => &completion.control,
            Self::Read { completion, .. } => &completion.control,
        }
    }

    pub fn fail(self, error: io::Error) {
        match self {
            Self::Write { completion, .. } => completion.finish(Err(error)),
            Self::Read { completion, .. } => completion.finish(Err(error)),
        }
    }
}

pub(crate) struct StoredOffer {
    pub data: Offer,
    pub marker: Option<String>,
    _memory: Counter,
}

impl StoredOffer {
    pub fn new(data: Offer, memory: Arc<AtomicUsize>, receipt: bool) -> io::Result<Arc<Self>> {
        if data.is_empty() || data.len() + usize::from(receipt) > MAX_FORMATS {
            return Err(io::Error::other("too many clipboard MIME representations"));
        }
        let mut allocations = HashSet::new();
        let mut size = 0_usize;
        for (mime, bytes) in &data {
            validate_type(mime)?;
            if allocations.insert((bytes.as_ptr() as usize, bytes.len())) {
                size = size.checked_add(bytes.len()).ok_or_else(oversized)?;
            }
            if size > MAX_BYTES {
                return Err(oversized());
            }
        }
        let marker = if receipt { Some(marker()?) } else { None };
        let memory = Counter::reserve(memory, size, MAX_RETAINED_BYTES)?;
        Ok(Arc::new(Self { data, marker, _memory: memory }))
    }

    pub fn mime_types(&self) -> impl Iterator<Item = &str> {
        self.data.keys().map(String::as_str).chain(self.marker.as_deref())
    }

    pub fn bytes(&self, mime: &str) -> Option<Arc<[u8]>> {
        if self.marker.as_deref() == Some(mime) {
            Some(Arc::from([]))
        } else {
            self.data.get(mime).cloned()
        }
    }
}

pub(crate) struct Counter {
    used: Arc<AtomicUsize>,
    amount: usize,
}

impl Counter {
    pub fn reserve(used: Arc<AtomicUsize>, amount: usize, limit: usize) -> io::Result<Self> {
        let mut counter = Self { used, amount: 0 };
        counter.grow(amount, limit)?;
        Ok(counter)
    }

    pub fn grow(&mut self, amount: usize, limit: usize) -> io::Result<()> {
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(amount).filter(|total| *total <= limit)
            })
            .map_err(|_| io::Error::other("clipboard retention budget is exhausted"))?;
        self.amount += amount;
        Ok(())
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.used.fetch_sub(self.amount, Ordering::AcqRel);
    }
}

fn validate_types(types: &[String]) -> io::Result<()> {
    if types.is_empty() || types.len() > MAX_FORMATS {
        return Err(io::Error::other("invalid clipboard MIME preference count"));
    }
    for mime in types {
        validate_type(mime)?;
    }
    Ok(())
}

fn validate_type(mime: &str) -> io::Result<()> {
    if mime.is_empty()
        || mime.len() > MAX_MIME_BYTES
        || !mime.is_ascii()
        || mime.bytes().any(|byte| byte <= b' ' || byte == 0x7f)
        || mime.starts_with(MARKER_PREFIX)
    {
        Err(io::Error::other("invalid or reserved clipboard MIME name"))
    } else {
        Ok(())
    }
}

fn marker() -> io::Result<String> {
    let mut bytes = [0_u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let mut marker = MARKER_PREFIX.to_owned();
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(marker, "{byte:02x}");
    }
    Ok(marker)
}

pub(crate) fn oversized() -> io::Error {
    io::Error::other("clipboard data exceeds 64 MiB")
}

pub(crate) fn unavailable() -> io::Error {
    io::Error::other("the GUI clipboard owner is unavailable")
}
