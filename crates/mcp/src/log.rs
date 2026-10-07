//! The bounded, non-blocking stderr log.
//!
//! One thread, `reshiki-mcp-err`, owns the writer and writes each message as
//! `reshiki-mcp: <level>: <text>` followed by a flush. Callers only ever
//! `try_send` into a queue of [`QUEUE`] messages: when it is full, or the
//! thread is gone, the message is counted in [`Log::dropped`] and discarded,
//! so a client that never reads stderr can never block the server.
//!
//! Messages are static text, numbers and source locations only. Client ids,
//! tool names, arguments and document text are never logged, and the API
//! enforces this by construction: [`Log::event`] takes a `&'static str` and
//! numeric fields, [`Log::panicked`] a source location, and [`Log::notice`]
//! exists for the startup banner's version string.
use std::{
    fmt,
    io::Write,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::Duration,
};

#[cfg(test)]
mod tests;

/// Messages the log thread may hold before new ones are dropped.
pub const QUEUE: usize = 256;

/// Verbosity, from least to most verbose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }
}

/// A level other than `error`, `warn`, `info` or `debug`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownLevel;

impl fmt::Display for UnknownLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected one of error, warn, info or debug")
    }
}

impl std::error::Error for UnknownLevel {}

impl FromStr for Level {
    type Err = UnknownLevel;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "error" => Ok(Self::Error),
            "warn" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            _ => Err(UnknownLevel),
        }
    }
}

enum Msg {
    Line(String),
    Stop,
}

/// A cheap, cloneable handle to one log thread.
#[derive(Clone)]
pub struct Log {
    sender: Option<SyncSender<Msg>>,
    level: Level,
    dropped: Arc<AtomicU64>,
}

/// Stops the log thread; see [`LogDone::finish`].
pub struct LogDone {
    sender: Option<SyncSender<Msg>>,
    done: Receiver<()>,
}

impl Log {
    /// Starts the `reshiki-mcp-err` thread. `make` runs on that thread, so
    /// the writer it returns (a stderr lock, say) never crosses threads.
    /// Events more verbose than `level` are discarded.
    pub fn start<W, F>(make: F, level: Level) -> (Self, LogDone)
    where
        W: Write,
        F: FnOnce() -> W + Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(QUEUE);
        let (done_sender, done) = mpsc::sync_channel(1);
        let spawned = thread::Builder::new()
            .name("reshiki-mcp-err".into())
            .spawn(move || {
                let mut out = make();
                while let Ok(Msg::Line(line)) = receiver.recv() {
                    // A failing stderr must never affect the server.
                    let _ = out.write_all(line.as_bytes()).and_then(|()| out.flush());
                }
                // Later events now count as dropped instead of queueing.
                drop(receiver);
                let _ = out.flush();
                let _ = done_sender.send(());
            });
        // Without a thread both handles stay silent and `finish` fails.
        let sender = spawned.is_ok().then_some(sender);
        let log = Self {
            sender: sender.clone(),
            level,
            dropped: Arc::default(),
        };
        (log, LogDone { sender, done })
    }

    /// A log that discards everything.
    pub fn silent() -> Self {
        Self {
            sender: None,
            level: Level::Error,
            dropped: Arc::default(),
        }
    }

    /// Logs `what k=v ...` when `level` is enabled. Never blocks.
    pub fn event(&self, level: Level, what: &'static str, fields: &[(&'static str, u64)]) {
        if level > self.level {
            return;
        }
        let mut text = String::from(what);
        for (name, value) in fields {
            text.push(' ');
            text.push_str(name);
            text.push('=');
            text.push_str(&value.to_string());
        }
        self.send(level, &text);
    }

    /// Logs `what value` whatever the level. Its only caller is the startup
    /// banner, whose value is the version string.
    pub fn notice(&self, what: &'static str, value: &str) {
        self.send(Level::Info, &format!("{what} {value}"));
    }

    /// Logs a panic's location. `file` is a source path, never a payload.
    pub fn panicked(&self, file: &str, line: u32) {
        self.send(Level::Error, &format!("worker panicked at {file}:{line}"));
    }

    /// Messages discarded because the queue was full or the thread gone.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    fn send(&self, level: Level, text: &str) {
        let Some(sender) = &self.sender else {
            return;
        };
        let line = format!("reshiki-mcp: {}: {text}\n", level.as_str());
        // Full or disconnected: count it and move on, never wait.
        if sender.try_send(Msg::Line(line)).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl LogDone {
    /// Asks the thread to stop after the queued messages and waits up to
    /// `timeout`. Returns true only if the thread flushed and stopped in
    /// time. Never blocks longer than `timeout`, even on a stuck writer.
    pub fn finish(mut self, timeout: Duration) -> bool {
        if let Some(sender) = self.sender.take() {
            // A full queue means a stuck or slow writer; the wait below
            // then times out instead of blocking on the queue.
            let _ = sender.try_send(Msg::Stop);
        }
        self.done.recv_timeout(timeout).is_ok()
    }
}
