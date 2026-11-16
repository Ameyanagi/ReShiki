//! A generic password in the login Keychain.
use super::{ACCOUNT, FAILED, SERVICE};
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

/// `errSecItemNotFound`.
const NOT_FOUND: i32 = -25300;

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> security_framework::base::Result<T> + Send + 'static,
    missing: T,
) -> Result<T, String> {
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) if error.code() == NOT_FOUND => Ok(missing),
        _ => Err(FAILED.into()),
    }
}

pub async fn read() -> Result<Option<Vec<u8>>, String> {
    blocking(|| get_generic_password(SERVICE, ACCOUNT).map(Some), None).await
}

pub async fn write(bytes: Vec<u8>) -> Result<(), String> {
    blocking(move || set_generic_password(SERVICE, ACCOUNT, &bytes), ()).await
}

pub async fn delete() -> Result<(), String> {
    blocking(|| delete_generic_password(SERVICE, ACCOUNT), ()).await
}
