use super::*;
#[tokio::test]
async fn response_limit_rejects_extra_byte_and_accepts_exact_boundary() {
    assert_eq!(
        read_limited(b"abcd".as_slice(), 4, "test").await.unwrap(),
        b"abcd"
    );
    assert!(matches!(
        read_limited(b"abcde".as_slice(), 4, "test").await,
        Err(Error::Limit("test"))
    ));
}
#[cfg(unix)]
#[tokio::test]
async fn deadline_and_dropping_future_kill_worker() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("worker");
    let pid_file = dir.path().join("pid");
    let quoted = pid_file.to_string_lossy().replace('\'', "'\\''");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s' \"$$\" > '{quoted}'\nexec /bin/sleep 30\n"),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Signal the same deadline branch only after the child acknowledges
    // startup. OS executable validation can legitimately exceed a short
    // wall-clock deadline on the first run of a newly created script.
    let (trigger, deadline) = tokio::sync::oneshot::channel();
    let deadline_script = script.clone();
    let task = tokio::spawn(async move {
        exchange_with_deadline(deadline_script, b"x", async {
            let _ = deadline.await;
        })
        .await
    });
    let pid = ready_pid(&pid_file).await;
    trigger.send(()).unwrap();
    assert!(matches!(task.await.unwrap(), Err(Error::Timeout)));
    assert_process_stopped(&pid).await;
    std::fs::remove_file(&pid_file).unwrap();
    let task = tokio::spawn(async move { exchange(script, b"x", Duration::from_secs(30)).await });
    let pid = ready_pid(&pid_file).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_process_stopped(&pid).await;
}
#[cfg(unix)]
async fn ready_pid(path: &std::path::Path) -> String {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(pid) = std::fs::read_to_string(path)
                && pid.trim().parse::<u32>().is_ok()
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Geometry stub did not acknowledge startup")
}
#[cfg(unix)]
async fn assert_process_stopped(pid: &str) {
    for _ in 0..100 {
        let status = Command::new("/bin/ps")
            .args(["-p", pid, "-o", "pid="])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .unwrap();
        if !status.success() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Cancelled worker {pid} remained alive");
}
