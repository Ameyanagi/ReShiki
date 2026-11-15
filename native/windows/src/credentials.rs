//! One generic credential in the user's Credential Manager, by target name.
use std::{os::windows::ffi::OsStrExt, ptr};
use windows::{
    Win32::{
        Foundation::ERROR_NOT_FOUND,
        Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
            CredReadW, CredWriteW,
        },
    },
    core::{PCWSTR, PWSTR},
};

fn wide(text: &str) -> Vec<u16> {
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

fn missing(error: &windows::core::Error) -> bool {
    error.code() == ERROR_NOT_FOUND.to_hresult()
}

/// The stored bytes, or `None` when nothing is stored under `target`.
pub fn read(target: &str) -> Result<Option<Vec<u8>>, String> {
    let target = wide(target);
    let mut credential: *mut CREDENTIALW = ptr::null_mut();
    // SAFETY: `target` is NUL-terminated and outlives the call; on success the
    // system sets `credential` to one allocation that this function frees.
    match unsafe {
        CredReadW(
            PCWSTR(target.as_ptr()),
            CRED_TYPE_GENERIC,
            0,
            &mut credential,
        )
    } {
        Ok(()) => {}
        Err(error) if missing(&error) => return Ok(None),
        Err(error) => return Err(error.to_string()),
    }
    // SAFETY: a successful read returns a valid credential whose blob pointer
    // covers `CredentialBlobSize` bytes; the bytes are copied before the free.
    let bytes = unsafe {
        let stored = &*credential;
        let bytes = if stored.CredentialBlob.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(stored.CredentialBlob, stored.CredentialBlobSize as usize)
                .to_vec()
        };
        CredFree(credential.cast());
        bytes
    };
    Ok(Some(bytes))
}

/// Store `bytes` under `target`, replacing what was there.
pub fn write(target: &str, bytes: &[u8]) -> Result<(), String> {
    let mut target = wide(target);
    let mut blob = bytes.to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: u32::try_from(blob.len()).map_err(|error| error.to_string())?,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: every pointer in `credential` refers to a local buffer that
    // outlives the call, and the system copies what it keeps.
    unsafe { CredWriteW(&credential, 0) }.map_err(|error| error.to_string())
}

/// Remove what is stored under `target`; nothing stored is not an error.
pub fn delete(target: &str) -> Result<(), String> {
    let target = wide(target);
    // SAFETY: `target` is NUL-terminated and outlives the call.
    match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0) } {
        Err(error) if !missing(&error) => Err(error.to_string()),
        _ => Ok(()),
    }
}
