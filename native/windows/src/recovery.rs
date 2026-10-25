//! Recovery copies must be recycled, never silently permanently deleted.
use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::E_ABORT,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        UI::Shell::{
            FILEOPERATION_FLAGS, FOF_NO_UI, FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE, FileOperation,
            IFileOperation, IShellItem, SHCreateItemFromParsingName,
        },
    },
    core::PCWSTR,
};

const FLAGS: FILEOPERATION_FLAGS =
    FILEOPERATION_FLAGS(FOF_NO_UI.0 | FOFX_EARLYFAILURE.0 | FOFX_RECYCLEONDELETE.0);

pub(super) fn discard(path: &Path) -> Result<(), String> {
    let path = std::path::absolute(path).map_err(|error| error.to_string())?;
    // A fresh apartment avoids inheriting a worker thread's COM mode.
    std::thread::Builder::new()
        .name("reshiki-recovery-recycle".into())
        .spawn(move || recycle(path))
        .map_err(|error| error.to_string())?
        .join()
        .map_err(|_| "The recovery recycling worker stopped".to_owned())?
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: the successful initialization on this thread owns this call.
        unsafe {
            CoUninitialize();
        }
    }
}

fn recycle(path: PathBuf) -> Result<(), String> {
    let name = shell_name(&path)?;
    // SAFETY: one owned COM apartment on this fresh thread, with no shared
    // interfaces or borrowed pointers escaping the operation.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|error| error.to_string())?;
    }
    let _apartment = Apartment;
    let operation = || -> windows::core::Result<()> {
        // SAFETY: the apartment is live, `name` is NUL-terminated and remains
        // allocated through PerformOperations; all interfaces are owned here.
        unsafe {
            let operation: IFileOperation =
                CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
            // RECYCLEONDELETE explicitly requests recycling instead of
            // permanent deletion; ALLOWUNDO alone does not make that promise.
            // https://learn.microsoft.com/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags
            operation.SetOperationFlags(FLAGS)?;
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None)?;
            operation.DeleteItem(&item, None)?;
            operation.PerformOperations()?;
            if operation.GetAnyOperationsAborted()?.as_bool() {
                return Err(E_ABORT.into());
            }
        }
        Ok(())
    };
    operation().map_err(|error| error.to_string())?;
    if path.exists() {
        return Err("The recovery copy was not moved to the Recycle Bin".into());
    }
    Ok(())
}

fn shell_name(path: &Path) -> Result<Vec<u16>, String> {
    let wide: Vec<_> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err("A recovery path contains a NUL character".into());
    }
    // The shell consumes DOS/UNC parsing names rather than the filesystem's
    // extended-length namespace. Preserve all UTF-16 units in the actual name.
    let name: Vec<_> = if let Some(rest) = wide.strip_prefix(&[92, 92, 63, 92, 85, 78, 67, 92]) {
        [92, 92].into_iter().chain(rest.iter().copied()).collect()
    } else if let Some(rest) = wide.strip_prefix(&[92, 92, 63, 92]) {
        if rest.get(1) != Some(&58) {
            return Err("This recovery path is not a filesystem parsing name".into());
        }
        rest.to_vec()
    } else {
        wide
    };
    Ok(name.into_iter().chain([0]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recycling_flags_forbid_a_permanent_delete_fallback() {
        assert_ne!(FLAGS.0 & FOFX_RECYCLEONDELETE.0, 0);
        assert_ne!(FLAGS.0 & FOFX_EARLYFAILURE.0, 0);
    }
    #[test]
    fn shell_paths_preserve_drive_unc_and_unicode_names() {
        for (input, expected) in [
            (r"C:\recovery\draft.json", r"C:\recovery\draft.json"),
            (r"\\?\C:\回復\draft.json", r"C:\回復\draft.json"),
            (
                r"\\?\UNC\server\share\draft.json",
                r"\\server\share\draft.json",
            ),
        ] {
            let name = shell_name(Path::new(input)).unwrap();
            assert_eq!(
                String::from_utf16(&name[..name.len() - 1]).unwrap(),
                expected
            );
        }
    }
}
