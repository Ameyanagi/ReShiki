//! One linear scan of the XML bytes before any parser runs. It refuses DTD
//! declarations, nesting deep enough to overflow the parser's stack, and
//! elements whose attribute count makes the parser's duplicate check quadratic.

/// Levels below the root element; the root is level 0.
const MAX_DEPTH: usize = 64;
const MAX_ATTRIBUTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Refused {
    #[error("DTD declarations are not supported")]
    Dtd,
    #[error("XML elements are nested more than 64 levels deep")]
    Depth,
    #[error("an XML element has more than 256 attributes")]
    Attributes,
}

fn after(bytes: &[u8], start: usize, marker: &[u8]) -> Option<usize> {
    let offset = bytes
        .get(start..)?
        .windows(marker.len())
        .position(|part| part == marker)?;
    start.checked_add(offset)?.checked_add(marker.len())
}

enum Markup {
    /// A comment, CDATA section or processing instruction ending here.
    Skipped(usize),
    Unclosed,
    Other,
}

fn markup(bytes: &[u8], position: usize, cdata: bool) -> Markup {
    let rest = bytes.get(position..).unwrap_or_default();
    let (prefix, marker): (_, &[u8]) = if rest.starts_with(b"<!--") {
        (4, b"-->")
    } else if cdata && rest.starts_with(b"<![CDATA[") {
        (9, b"]]>")
    } else if rest.starts_with(b"<?") {
        (2, b"?>")
    } else {
        return Markup::Other;
    };
    after(bytes, position + prefix, marker).map_or(Markup::Unclosed, Markup::Skipped)
}

/// Every CDXML and stationery reader calls this before it parses. Callers
/// impose their byte bound first. Malformed or unclosed constructs are left to
/// the XML parser's syntax errors.
///
/// A document type may name an external DTD, which is never fetched. Its
/// internal subset may hold only comments and processing instructions: the
/// parser reads other declarations more loosely than XML defines them, so a
/// scan cannot tell where one ends, and an entity could hide behind it.
pub(crate) fn check(text: &str) -> Result<(), Refused> {
    let bytes = text.as_bytes();
    let mut position = 0usize;
    let mut depth = 0usize;
    // Quoted values in the start tag or document type being read.
    let mut quoted = None;
    let mut doctype = false;
    let mut subset = false;
    while let Some(&byte) = bytes.get(position) {
        if subset {
            match byte {
                b'<' => match markup(bytes, position, false) {
                    Markup::Skipped(end) => {
                        position = end;
                        continue;
                    }
                    Markup::Unclosed => return Ok(()),
                    Markup::Other => return Err(Refused::Dtd),
                },
                b']' => subset = false,
                byte if byte.is_ascii_whitespace() => {}
                _ => return Err(Refused::Dtd),
            }
        } else if let Some(count) = quoted.as_mut() {
            match byte {
                b'\'' | b'"' => {
                    *count += 1;
                    if *count > MAX_ATTRIBUTES && !doctype {
                        return Err(Refused::Attributes);
                    }
                    let Some(end) = after(bytes, position + 1, &[byte]) else {
                        return Ok(());
                    };
                    position = end;
                    continue;
                }
                b'[' if doctype => subset = true,
                b'>' => {
                    if !doctype && bytes.get(position.wrapping_sub(1)) == Some(&b'/') {
                        depth = depth.saturating_sub(1);
                    }
                    quoted = None;
                    doctype = false;
                }
                _ => {}
            }
        } else if byte == b'<' {
            match markup(bytes, position, true) {
                Markup::Skipped(end) => {
                    position = end;
                    continue;
                }
                Markup::Unclosed => return Ok(()),
                Markup::Other => {}
            }
            match bytes.get(position + 1) {
                Some(b'/') => depth = depth.saturating_sub(1),
                Some(b'!') => {
                    doctype = true;
                    quoted = Some(0usize);
                }
                _ => {
                    if depth > MAX_DEPTH {
                        return Err(Refused::Depth);
                    }
                    depth += 1;
                    quoted = Some(0);
                }
            }
        }
        position += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
