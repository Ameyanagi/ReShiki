//! Checked copies of the pinned platform's C++ comparison-sort policies.
//! Used where equal keys affect chemical output, including traversal and rings.
#[cfg(target_os = "macos")]
mod apple_sort;
#[cfg(target_os = "linux")]
mod linux_sort;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod sort_common;
#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
mod sort_tests;
#[cfg(target_os = "windows")]
mod windows_sort;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid comparison-sort index")]
    Invalid,
    #[error("Comparison-sort resource limit exceeded")]
    Limit,
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    #[error("No chemistry comparison-sort policy for this platform")]
    UnsupportedPlatform,
}
fn at<T>(items: &[T], i: usize) -> Result<&T, Error> {
    items.get(i).ok_or(Error::Invalid)
}
fn at_mut<T>(items: &mut [T], i: usize) -> Result<&mut T, Error> {
    items.get_mut(i).ok_or(Error::Invalid)
}

pub fn indices(keys: &[i32]) -> Result<Vec<usize>, Error> {
    if keys.len() > 2_000_000 {
        return Err(Error::Limit);
    }
    let mut entries: Vec<_> = keys.iter().enumerate().map(|(i, &key)| (key, i)).collect();
    #[cfg(target_os = "linux")]
    linux_sort::sort(&mut entries)?;
    #[cfg(target_os = "macos")]
    apple_sort::sort(&mut entries)?;
    #[cfg(target_os = "windows")]
    windows_sort::sort(&mut entries)?;
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return Err(Error::UnsupportedPlatform);
    Ok(entries.into_iter().map(|(_, i)| i).collect())
}
