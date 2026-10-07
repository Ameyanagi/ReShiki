use super::*;
use std::{
    io,
    sync::{Mutex, mpsc::Sender},
    time::Instant,
};

/// An in-memory writer shared with the test.
#[derive(Clone, Default)]
struct Memory(Arc<Mutex<Vec<u8>>>);

impl Memory {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for Memory {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A writer whose first write blocks until the test drops its sender.
struct Stuck(Receiver<()>);

impl Write for Stuck {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        let _ = self.0.recv();
        Err(io::Error::other("released"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn stuck() -> (Sender<()>, impl FnOnce() -> Stuck + Send + 'static) {
    let (release, blocked) = mpsc::channel();
    (release, move || Stuck(blocked))
}

#[test]
fn levels_parse_from_their_names_only() {
    assert_eq!("error".parse(), Ok(Level::Error));
    assert_eq!("warn".parse(), Ok(Level::Warn));
    assert_eq!("info".parse(), Ok(Level::Info));
    assert_eq!("debug".parse(), Ok(Level::Debug));
    for text in ["", "loud", "WARN", "warning", " info"] {
        assert_eq!(text.parse::<Level>(), Err(UnknownLevel), "{text:?}");
    }
}

#[test]
fn lines_carry_the_prefix_level_and_fields() {
    let memory = Memory::default();
    let sink = memory.clone();
    let (log, done) = Log::start(move || sink, Level::Info);
    log.notice("ReShiki agent API (experimental)", "1.2.3");
    log.event(
        Level::Info,
        "admitted",
        &[("outstanding", 3), ("bytes", 42)],
    );
    log.event(Level::Warn, "slow", &[]);
    log.event(Level::Debug, "hidden", &[("n", 1)]);
    log.panicked("src/main.rs", 7);
    assert!(done.finish(Duration::from_secs(5)));
    assert_eq!(
        memory.text(),
        "reshiki-mcp: info: ReShiki agent API (experimental) 1.2.3\n\
         reshiki-mcp: info: admitted outstanding=3 bytes=42\n\
         reshiki-mcp: warn: slow\n\
         reshiki-mcp: error: worker panicked at src/main.rs:7\n"
    );
    assert_eq!(log.dropped(), 0);
}

#[test]
fn notices_and_panics_ignore_the_level() {
    let memory = Memory::default();
    let sink = memory.clone();
    let (log, done) = Log::start(move || sink, Level::Error);
    log.event(Level::Warn, "hidden", &[]);
    log.notice("banner", "v");
    log.panicked("lib.rs", 1);
    assert!(done.finish(Duration::from_secs(5)));
    assert_eq!(
        memory.text(),
        "reshiki-mcp: info: banner v\nreshiki-mcp: error: worker panicked at lib.rs:1\n"
    );
}

#[test]
fn a_stuck_writer_never_blocks_events_or_finish() {
    let (release, make) = stuck();
    let (log, done) = Log::start(make, Level::Debug);
    let started = Instant::now();
    for n in 0..10_000 {
        log.event(Level::Debug, "flood", &[("n", n)]);
    }
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(log.dropped() > 0);

    let started = Instant::now();
    assert!(!done.finish(Duration::from_millis(100)));
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(release);
}

#[test]
fn events_after_finish_are_dropped_without_blocking() {
    let (log, done) = Log::start(Memory::default, Level::Debug);
    assert!(done.finish(Duration::from_secs(5)));
    log.event(Level::Error, "late", &[]);
    assert_eq!(log.dropped(), 1);
}

#[test]
fn the_silent_log_discards_everything() {
    let log = Log::silent();
    log.event(Level::Error, "nothing", &[("n", 1)]);
    log.notice("banner", "v");
    log.panicked("lib.rs", 1);
    assert_eq!(log.dropped(), 0);
}
