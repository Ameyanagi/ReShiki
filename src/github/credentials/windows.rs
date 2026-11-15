//! A generic credential in the user's Credential Manager.
use super::{FAILED, SERVICE};
use reshiki_windows::credentials;

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(value)) => Ok(value),
        _ => Err(FAILED.into()),
    }
}

pub async fn read() -> Result<Option<Vec<u8>>, String> {
    blocking(|| credentials::read(SERVICE)).await
}

pub async fn write(bytes: Vec<u8>) -> Result<(), String> {
    blocking(move || credentials::write(SERVICE, &bytes)).await
}

pub async fn delete() -> Result<(), String> {
    blocking(|| credentials::delete(SERVICE)).await
}
