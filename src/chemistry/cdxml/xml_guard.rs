//! Distinguish actual entity declarations from inert XML text before parsing.

fn after(bytes: &[u8], start: usize, marker: &[u8]) -> Option<usize> {
    let offset = bytes
        .get(start..)?
        .windows(marker.len())
        .position(|part| part == marker)?;
    start.checked_add(offset)?.checked_add(marker.len())
}

/// Linear lexical guard; callers impose their XML byte bound before scanning.
/// Malformed/unclosed constructs are left to the XML parser's syntax errors.
pub(super) fn has_entity_declaration(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut position = 0usize;
    let mut markup = false;
    while let Some(&byte) = bytes.get(position) {
        if byte == b'<' {
            let Some(rest) = bytes.get(position..) else {
                return false;
            };
            let skipped = if rest.starts_with(b"<!--") {
                Some((4, &b"-->"[..]))
            } else if rest.starts_with(b"<![CDATA[") {
                Some((9, &b"]]>"[..]))
            } else if rest.starts_with(b"<?") {
                Some((2, &b"?>"[..]))
            } else {
                None
            };
            if let Some((prefix, marker)) = skipped {
                let Some(end) = after(bytes, position + prefix, marker) else {
                    return false;
                };
                position = end;
                continue;
            }
            if rest.starts_with(b"<!ENTITY") {
                return true;
            }
            markup = true;
        } else if markup && matches!(byte, b'\'' | b'"') {
            let Some(end) = after(bytes, position + 1, &[byte]) else {
                return false;
            };
            position = end;
            continue;
        } else if byte == b'>' {
            markup = false;
        }
        position += 1;
    }
    false
}
