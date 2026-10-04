//! Mechanical pipe operations; native worker policies remain in their callers.
use tokio::{io::AsyncRead, io::AsyncReadExt, io::AsyncWriteExt, process::ChildStdin};

pub(crate) async fn write(mut input: ChildStdin, request: &[u8]) -> Result<(), String> {
    input.write_all(request).await.map_err(|e| e.to_string())?;
    input.shutdown().await.map_err(|e| e.to_string())?;
    drop(input);
    Ok(())
}

pub(crate) async fn capture(
    input: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    input
        .take(limit as u64)
        .read_to_end(&mut output)
        .await
        .map_err(|e| e.to_string())?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn capture_keeps_exact_prefix_and_accepts_short_or_empty_streams() {
        let bytes = vec![0xff; 65537];
        assert_eq!(
            capture(bytes.as_slice(), 65536).await.unwrap(),
            bytes[..65536]
        );
        assert_eq!(capture(b"short".as_slice(), 65536).await.unwrap(), b"short");
        assert!(capture(b"ignored".as_slice(), 0).await.unwrap().is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn writing_closes_stdin_for_eof_dependent_worker_and_preserves_output() {
        use std::process::Stdio;
        let mut child = tokio::process::Command::new("/bin/sh")
            .args(["-c", "cat; printf ' diagnostic ' >&2; exit 7"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let errors = child.stderr.take().unwrap();
        let request = b"exact\0binary\xffrequest";
        let exchange = async {
            tokio::try_join!(
                write(input, request),
                capture(output, 65537),
                capture(errors, 65536),
                async { child.wait().await.map_err(|e| e.to_string()) },
            )
        };
        let (_, output, errors, status) =
            tokio::time::timeout(std::time::Duration::from_secs(5), exchange)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(output, request);
        assert_eq!(errors, b" diagnostic ");
        assert_eq!(status.code(), Some(7));
    }
}
