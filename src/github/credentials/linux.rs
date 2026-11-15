//! An item in the Secret Service keyring (the portal's keyring in a sandbox).
//! With no keyring the sign-in lasts for this run only; it never goes to a file
//! of our own.
use super::{ACCOUNT, FAILED, SERVICE};

const ATTRIBUTES: [(&str, &str); 2] = [("service", SERVICE), ("account", ACCOUNT)];

/// A dismissed unlock prompt fails here, like a missing keyring.
async fn keyring() -> oo7::Result<oo7::Keyring> {
    let keyring = oo7::Keyring::new().await?;
    keyring.unlock().await?;
    Ok(keyring)
}

async fn stored() -> oo7::Result<Option<Vec<u8>>> {
    match keyring().await?.search_items(&ATTRIBUTES).await?.first() {
        Some(item) => Ok(Some(item.secret().await?.to_vec())),
        None => Ok(None),
    }
}

pub async fn read() -> Result<Option<Vec<u8>>, String> {
    stored().await.map_err(|_| FAILED.into())
}

pub async fn write(bytes: Vec<u8>) -> Result<(), String> {
    let write = async {
        keyring()
            .await?
            .create_item("ReShiki GitHub sign-in", &ATTRIBUTES, bytes, true)
            .await
    };
    write.await.map_err(|_| FAILED.into())
}

pub async fn delete() -> Result<(), String> {
    let delete = async { keyring().await?.delete(&ATTRIBUTES).await };
    delete.await.map_err(|_| FAILED.into())
}
