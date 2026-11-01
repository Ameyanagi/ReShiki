#![cfg(target_os = "macos")]

use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[test]
fn print_worker_exits_when_parent_is_killed() {
    let mut parent = Command::new("sh")
        .args([
            "-c",
            "exec 3<&0; \"$0\" --print-worker <&3 & echo $!; wait",
            env!("CARGO_BIN_EXE_reshiki"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // The open stdin keeps the worker waiting for its request, before AppKit
    // or a print panel.
    let input = parent.stdin.take().unwrap();
    let mut output = BufReader::new(parent.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    let pid = line.trim().to_owned();
    // The parent watchdog is the worker's second thread; `ps -M` adds a header.
    let deadline = Instant::now() + Duration::from_secs(30);
    while Command::new("ps")
        .args(["-M", "-p", &pid])
        .output()
        .unwrap()
        .stdout
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        < 3
    {
        assert!(Instant::now() < deadline, "print worker did not start");
        thread::sleep(Duration::from_millis(50));
    }
    // EOF on the shared stdout also detects an exited orphan not yet reaped.
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || sender.send(output.read_line(&mut String::new())));
    parent.kill().unwrap();
    parent.wait().unwrap();
    let exited = matches!(receiver.recv_timeout(Duration::from_secs(1)), Ok(Ok(0)));
    if !exited {
        let _ = Command::new("kill").args(["-KILL", &pid]).status();
    }
    drop(input);
    assert!(exited, "print worker survived its parent's SIGKILL");
}
