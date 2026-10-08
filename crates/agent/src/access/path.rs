//! Lexical intake of requested paths. Nothing here touches the filesystem.
use super::{AccessError, echo, invalid};
use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

const MAX_BYTES: usize = 4096;

/// A lexically acceptable absolute path and its echo for messages.
#[derive(Debug)]
pub(super) struct RequestPath {
    pub(super) path: PathBuf,
    pub(super) echo: String,
}

/// Accept a plain absolute path, or name the first rule it breaks.
pub(super) fn parse(input: &str) -> Result<RequestPath, AccessError> {
    let rule = |rule: &'static str| invalid(input, rule);
    if input.is_empty() {
        return Err(rule("empty"));
    }
    if input.len() > MAX_BYTES {
        return Err(rule("too_long"));
    }
    if input.contains('\0') {
        return Err(rule("nul"));
    }
    if input
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:"))
    {
        return Err(rule("file_uri"));
    }
    let path = Path::new(input);
    // Windows prefixes first: `C:file` and `\file` are not absolute either,
    // and each deserves its own rule.
    #[cfg(windows)]
    windows::prefix(path).map_err(rule)?;
    if !path.is_absolute() {
        return Err(rule("relative"));
    }
    if path.components().any(|part| part == Component::ParentDir) {
        return Err(rule("dotdot"));
    }
    if input
        .chars()
        .next_back()
        .is_some_and(std::path::is_separator)
    {
        return Err(rule("trailing_separator"));
    }
    #[cfg(windows)]
    windows::components(input, path).map_err(rule)?;
    Ok(RequestPath {
        path: path.to_path_buf(),
        echo: echo(input),
    })
}

/// Extensions match ASCII-case-insensitively and are given without the dot.
pub(super) fn check_extension(
    request: &RequestPath,
    extensions: &[&str],
) -> Result<(), AccessError> {
    let allowed = request
        .path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|found| {
            extensions
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(found))
        });
    if allowed {
        Ok(())
    } else {
        Err(AccessError::ExtensionNotAllowed {
            path: request.echo.clone(),
            allowed: extensions.iter().map(|&allowed| allowed.into()).collect(),
        })
    }
}

#[cfg(windows)]
mod windows {
    use std::path::{Component, Path, Prefix};

    /// Device names Win32 resolves in any folder, with or without an extension.
    const RESERVED: [&str; 32] = [
        "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM0", "COM1", "COM2", "COM3", "COM4",
        "COM5", "COM6", "COM7", "COM8", "COM9", "COM¹", "COM²", "COM³", "LPT0", "LPT1", "LPT2",
        "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³",
    ];

    pub(super) fn prefix(path: &Path) -> Result<(), &'static str> {
        match path.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Verbatim(_) | Prefix::VerbatimUNC(..) | Prefix::VerbatimDisk(_) => {
                    Err("verbatim")
                }
                Prefix::DeviceNS(_) => Err("device"),
                Prefix::UNC(..) => Err("unc"),
                Prefix::Disk(_) if !path.has_root() => Err("drive_relative"),
                Prefix::Disk(_) => Ok(()),
            },
            Some(Component::RootDir) => Err("rooted_without_drive"),
            _ => Ok(()),
        }
    }

    /// Only `X:` plus a root reaches this point.
    pub(super) fn components(input: &str, path: &Path) -> Result<(), &'static str> {
        if input.get(2..).is_some_and(|rest| rest.contains(':')) {
            return Err("ads");
        }
        for component in path.components() {
            if let Component::Normal(name) = component {
                let name = name.to_string_lossy();
                if reserved(&name) {
                    return Err("reserved_name");
                }
                if name.ends_with(['.', ' ']) {
                    return Err("trailing_dot_space");
                }
            }
        }
        Ok(())
    }

    fn reserved(name: &str) -> bool {
        let stem = name.split('.').next().unwrap_or(name);
        let stem = stem.trim_end_matches([' ', '.']);
        RESERVED
            .iter()
            .any(|device| device.eq_ignore_ascii_case(stem))
    }
}
