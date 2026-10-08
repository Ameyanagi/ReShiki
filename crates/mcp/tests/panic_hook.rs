//! The panic hook logs where a panic happened, never its payload. It
//! replaces the process-wide hook, so it runs in its own test binary, and
//! each test holds [`HOOK`] while its hook is installed.
#[path = "common/fake_host.rs"]
mod fake_host;

use fake_host::FakeHost;
use reshiki_agent::ops::wire::Principal;
use reshiki_mcp::{
    framing::Limits,
    install_panic_hook,
    log::{Level, Log},
    server::{self, Identity, Quit},
};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, BufReader, Write},
    sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc},
    thread,
    time::Duration,
};

/// Serializes the tests: each replaces the process-wide hook.
static HOOK: Mutex<()> = Mutex::new(());

fn hook() -> MutexGuard<'static, ()> {
    HOOK.lock().unwrap_or_else(PoisonError::into_inner)
}

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
    let _hook = hook();
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

/// A tool call that panics is answered -32603, the next call is answered,
/// and the log names the panic's location but not its payload or the
/// arguments.
#[test]
fn a_panicking_tool_call_is_an_internal_error_and_serving_goes_on() {
    let _hook = hook();
    let memory = Memory::default();
    let writer = memory.clone();
    let (log, done) = Log::start(move || writer, Level::Error);
    install_panic_hook(log.clone());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (reader, mut input) = io::pipe().unwrap();
    let (output_reader, output_writer) = io::pipe().unwrap();
    let (lines, output) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(output_reader).lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let served = runtime.spawn(server::serve(
        Arc::new(FakeHost::default()),
        Principal::local(),
        reader,
        move || output_writer,
        Identity {
            app_version: fake_host::APP_VERSION.into(),
        },
        Limits::default(),
        log,
    ));
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    let mut receive = |id: i64, name: &str| -> Value {
        let call = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": {"secret": "RESHIKI-CANARY-args"}, "_meta": meta},
        });
        writeln!(input, "{call}").unwrap();
        let line = output.recv_timeout(Duration::from_secs(10)).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    assert_eq!(
        receive(1, "panic"),
        json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32603, "message": "Internal error"}})
    );
    let next = receive(2, "echo");
    assert_eq!(next["result"]["isError"], false, "{next}");
    drop(receive);
    drop(input);
    let finished = runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(10), served).await })
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(finished.reason, Quit::Eof);
    assert!(finished.writer.wait(Duration::from_secs(10)));
    assert_eq!(finished.tracker.unanswered(), 0);
    assert!(done.finish(Duration::from_secs(10)));
    let logged = String::from_utf8(memory.0.lock().unwrap().clone()).unwrap();
    assert!(logged.contains("panicked at"), "{logged}");
    assert!(logged.contains("fake_host.rs:"), "{logged}");
    assert!(
        logged.contains("reshiki-mcp: error: tool task failed\n"),
        "{logged}"
    );
    assert!(!logged.contains("RESHIKI-CANARY"), "{logged}");
}
