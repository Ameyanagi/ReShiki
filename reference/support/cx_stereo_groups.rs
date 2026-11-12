//! #1006: the Python oracle marks only SMARTSQ fields with distinct CX group
//! pairs colliding under RDKit's wrapping key, whose renumbering changes native
//! acceptance to an editable-query rejection. Both original and renumbered
//! inputs must produce exactly ReShiki's unsupported-query rejection.
use reshiki::chemistry::molfile::ReadError;

pub fn query_rejection<T>(
    text: &str,
    original_success: bool,
    actual: &Result<T, ReadError>,
    read: impl FnOnce(&str) -> Result<T, ReadError>,
) -> Option<String> {
    let unsupported = |result: &Result<T, ReadError>| {
        matches!(result, Err(ReadError::Unsupported("substance-group query")))
    };
    (!original_success || !unsupported(actual) || !unsupported(&read(text))).then(|| {
        "CX stereo-group wrap exception requires original native acceptance and identical unsupported substance-group-query rejections".to_owned()
    })
}

/// Prints the collision counts and reports whether the corpus covered any.
///
/// Expected difference, RDKit 2026.09.1 on Windows x64 only: its CX reader
/// takes group ids from `std::atoi` (`read_int`, CXSmilesOps.cpp:127, since
/// rdkit#9273), and the MSVC runtime saturates out-of-range ids at INT_MAX. So
/// `o2147483649` never wraps onto `o1`'s key `10 * id + type` (line 1395) and
/// the oracle marks those cases `unwrapped` instead of renumbering them; they
/// are compared as read. glibc and macOS keep the wrap, so elsewhere every
/// collision must still be a renumbering.
pub fn collision_coverage(target: &str, renumberings: usize, unwrapped: usize) -> bool {
    eprintln!(
        "{target}: {renumberings} stereo-group renumberings, {unwrapped} collisions read without the key wrap (expected difference: RDKit 2026.09.1 on Windows x64)"
    );
    assert!(
        unwrapped == 0 || cfg!(windows),
        "Only RDKit 2026.09.1 on Windows reads CX stereo-group collisions without the key wrap"
    );
    renumberings + unwrapped > 0
}
