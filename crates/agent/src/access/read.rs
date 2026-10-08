//! Bounded reads of regular files inside read roots.
use super::{
    AccessError, Grants, io_error,
    path::{check_extension, parse},
    resolution_error,
    root::Kind,
};
use cap_std::fs::OpenOptions;
use std::io::{self, Read};

impl Grants {
    /// Read a regular file of at most `limit` bytes whose extension is one of
    /// `extensions` (without the dot, ASCII-case-insensitive).
    pub fn read(
        &self,
        path: &str,
        limit: usize,
        extensions: &[&str],
    ) -> Result<Vec<u8>, AccessError> {
        let request = parse(path)?;
        check_extension(&request, extensions)?;
        let (root, rel) = self.locate(Kind::Read, &request)?;
        let echo = request.echo.as_str();
        let mut options = OpenOptions::new();
        options.read(true);
        // A FIFO opens without blocking and is then refused as not a regular
        // file; a terminal never becomes the controlling one.
        #[cfg(unix)]
        cap_std::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NONBLOCK | libc::O_NOCTTY);
        let file = root.dir.open_with(&rel, &options).map_err(|error| {
            // Windows refuses to open a directory as a file with an OS denial.
            let denied =
                error.kind() == io::ErrorKind::PermissionDenied && error.raw_os_error().is_some();
            if denied && root.dir.metadata(&rel).is_ok_and(|found| found.is_dir()) {
                AccessError::NotARegularFile { path: echo.into() }
            } else {
                resolution_error(error, echo)
            }
        })?;
        let metadata = file.metadata().map_err(|error| io_error(error, echo))?;
        if !metadata.is_file() {
            return Err(AccessError::NotARegularFile { path: echo.into() });
        }
        let too_large = || AccessError::FileTooLarge {
            path: echo.into(),
            limit,
        };
        let cap = u64::try_from(limit).unwrap_or(u64::MAX);
        if metadata.len() > cap {
            return Err(too_large());
        }
        // The file may grow after the check: read one byte past the limit.
        let mut bytes = Vec::new();
        file.take(cap.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| io_error(error, echo))?;
        if bytes.len() > limit {
            return Err(too_large());
        }
        Ok(bytes)
    }
}
