//! A generic password in the login Keychain.
use super::{ACCOUNT, FAILED, SERVICE};
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

/// `errSecItemNotFound`.
const NOT_FOUND: i32 = -25300;

fn store_result<T>(result: security_framework::base::Result<T>, missing: T) -> Result<T, String> {
    match result {
        Ok(value) => Ok(value),
        Err(error) if error.code() == NOT_FOUND => Ok(missing),
        Err(_) => Err(FAILED.into()),
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> security_framework::base::Result<T> + Send + 'static,
    missing: T,
) -> Result<T, String> {
    store_result(
        tokio::task::spawn_blocking(work)
            .await
            .map_err(|_| FAILED)?,
        missing,
    )
}

async fn read_in_background(
    work: impl FnOnce() -> security_framework::base::Result<Option<Vec<u8>>> + Send + 'static,
) -> Result<Option<Vec<u8>>, String> {
    let (send, receive) = tokio::sync::oneshot::channel();
    // A Keychain prompt can leave the read blocked indefinitely. A detached
    // read lets the editor's async runtime shut down while macOS asks.
    let reader = std::thread::Builder::new()
        .name("reshiki-keychain-read".into())
        .spawn(move || {
            let _ = send.send(work());
        })
        .map_err(|_| FAILED)?;
    drop(reader);
    store_result(receive.await.map_err(|_| FAILED)?, None)
}

pub async fn read() -> Result<Option<Vec<u8>>, String> {
    read_in_background(|| get_generic_password(SERVICE, ACCOUNT).map(Some)).await
}

pub async fn write(bytes: Vec<u8>) -> Result<(), String> {
    blocking(move || set_generic_password(SERVICE, ACCOUNT, &bytes), ()).await
}

pub async fn delete() -> Result<(), String> {
    blocking(|| delete_generic_password(SERVICE, ACCOUNT), ()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread, time::Duration};

    #[tokio::test]
    async fn startup_read_reports_a_value_and_a_missing_entry() {
        let data = vec![1, 2, 3];
        assert_eq!(
            read_in_background(|| Ok(Some(vec![1, 2, 3]))).await,
            Ok(Some(data))
        );
        assert_eq!(
            read_in_background(|| Err(security_framework::base::Error::from_code(NOT_FOUND))).await,
            Ok(None)
        );
    }

    #[test]
    fn shutdown_does_not_wait_for_a_pending_keychain_read() {
        let (release, blocked) = mpsc::channel();
        let (ready, about_to_stop) = mpsc::channel();
        let (stopped, shutdown) = mpsc::channel();
        let app = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let (reading, started) = tokio::sync::oneshot::channel();
            runtime.block_on(async {
                tokio::spawn(read_in_background(move || {
                    let _ = reading.send(());
                    let _ = blocked.recv();
                    Ok(None)
                }));
                started.await.unwrap();
            });
            ready.send(()).unwrap();
            drop(runtime);
            stopped.send(()).unwrap();
        });
        let pending = about_to_stop.recv_timeout(Duration::from_secs(5)).is_ok();
        let stopped_before_release =
            pending && shutdown.recv_timeout(Duration::from_secs(5)).is_ok();
        release.send(()).unwrap();
        app.join().unwrap();
        assert!(stopped_before_release);
    }
}
