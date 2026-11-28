//! Collect a child process's output with the same bounded wait in every harness.
use std::{
    io::Read,
    process::{Child, Output},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// The limit also covers pipes held open by a process the child started.
pub fn wait_with_watchdog(mut child: Child, limit: Duration) -> Output {
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + limit;
    loop {
        let status = child.try_wait().expect("poll reshiki");
        if let Some(status) = status.filter(|_| stdout.is_finished() && stderr.is_finished()) {
            return Output {
                status,
                stdout: stdout.join().expect("read stdout"),
                stderr: stderr.join().expect("read stderr"),
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("reshiki did not exit and close its output within {limit:?}");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}
