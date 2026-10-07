//! Runs the reshiki binary for headless tests and kills it if it hangs.
use std::{
    io::Read,
    process::{Child, Command, Output, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Starts the reshiki binary with `args`, piping stdout and stderr.
pub fn spawn(args: &[&str], stdin: Stdio) -> Child {
    Command::new(env!("CARGO_BIN_EXE_reshiki"))
        .args(args)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start reshiki")
}

/// Collects `child`'s output; kills it and panics if it runs past `limit`.
pub fn wait_with_watchdog(mut child: Child, limit: Duration) -> Output {
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll reshiki") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("reshiki did not exit within {limit:?}");
        }
        thread::sleep(Duration::from_millis(20));
    };
    Output {
        status,
        stdout: stdout.join().expect("read stdout"),
        stderr: stderr.join().expect("read stderr"),
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
