//! The panic hook logs where a panic happened, never its payload. It
//! replaces the process-wide hook, so it runs in its own test binary.
use reshiki_mcp::{
    install_panic_hook,
    log::{Level, Log},
};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// An in-memory writer shared with the test.
#[derive(Clone, Default)]
struct Memory(Arc<Mutex<Vec<u8>>>);

impl Write for Memory {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_panic_logs_its_location_and_never_its_payload() {
    let memory = Memory::default();
    let writer = memory.clone();
    let (log, done) = Log::start(move || writer, Level::Error);
    install_panic_hook(log);
    let panicked = thread::spawn(|| panic!("RESHIKI-CANARY-panic")).join();
    assert!(panicked.is_err());
    assert!(done.finish(Duration::from_secs(10)));
    let output = String::from_utf8(memory.0.lock().unwrap().clone()).unwrap();
    assert!(output.contains("panicked at"), "{output}");
    assert!(output.contains("panic_hook.rs:"), "{output}");
    assert!(!output.contains("RESHIKI-CANARY"), "{output}");
}
