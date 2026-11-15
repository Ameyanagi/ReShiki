//! An item in the Secret Service keyring (the portal's keyring in a sandbox).
//! With no keyring the sign-in lasts for this run only; it never goes to a file
//! of our own.
use super::{ACCOUNT, FAILED, SERVICE};

const ATTRIBUTES: [(&str, &str); 2] = [("service", SERVICE), ("account", ACCOUNT)];

fn failed(_: oo7::Error) -> String {
    FAILED.into()
}

/// A dismissed unlock prompt fails here, like a missing keyring.
async fn keyring() -> Result<oo7::Keyring, String> {
    let keyring = oo7::Keyring::new().await.map_err(failed)?;
    keyring.unlock().await.map_err(failed)?;
    Ok(keyring)
}

pub async fn read() -> Result<Option<Vec<u8>>, String> {
    let items = keyring().await?.search_items(&ATTRIBUTES).await;
    match items.map_err(failed)?.first() {
        Some(item) => Ok(Some(item.secret().await.map_err(failed)?.to_vec())),
        None => Ok(None),
    }
}

pub async fn write(bytes: Vec<u8>) -> Result<(), String> {
    let keyring = keyring().await?;
    let created = keyring.create_item("ReShiki GitHub sign-in", &ATTRIBUTES, bytes, true);
    created.await.map_err(failed)
}

pub async fn delete() -> Result<(), String> {
    keyring().await?.delete(&ATTRIBUTES).await.map_err(failed)
}
