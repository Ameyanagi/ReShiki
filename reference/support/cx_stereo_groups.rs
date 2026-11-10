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
