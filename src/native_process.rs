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
mod tests;
