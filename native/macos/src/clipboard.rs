//! Explicit, bounded access to one pasteboard item. Runs only in a worker.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const LIMIT: usize = 64 * 1024 * 1024;
const PICTURES: &[&str] = &[
    "public.png",
    "public.tiff",
    "public.jpeg",
    "org.webmproject.webp",
    "public.webp",
];
const CDXML_TYPES: &[&str] = &[
    "com.revvity.cdxml",
    "com.perkinelmer.cdxml",
    "com.cambridgesoft.cdxml",
];
const READABLE: &[&str] = &[
    "dev.reshiki.drawing",
    "dev.moruno.drawing",
    "com.revvity.chemdraw.cdx-clipboard",
    "com.perkinelmer.chemdraw.cdx-clipboard",
    "com.cambridgesoft.cdx",
    "com.revvity.cdx",
    "com.perkinelmer.cdx",
    "com.revvity.cdxml",
    "com.perkinelmer.cdxml",
    "com.cambridgesoft.cdxml",
    "chemical/x-cdxml",
    "com.mdli.molfile",
    "org.opensmiles.smiles",
    "public.png",
    "public.tiff",
    "public.jpeg",
    "org.webmproject.webp",
    "public.webp",
    "public.utf8-plain-text",
    "com.adobe.pdf",
    "public.svg-image",
];

fn pasteboard_types(kind: &str) -> Option<&'static [&'static str]> {
    // NSPasteboardItem accepts UTI identifiers, not MIME strings. These aliases
    // are ChemDraw's declared CDXML types; the worker protocol stays portable.
    if kind == "chemical/x-cdxml" {
        Some(CDXML_TYPES)
    } else {
        READABLE
            .iter()
            .find(|&&name| name == kind)
            .map(std::slice::from_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct Representation {
    #[serde(rename = "type")]
    kind: String,
    data: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    operation: String,
    #[serde(default)]
    representations: Vec<Representation>,
}

#[derive(Serialize)]
struct Response {
    representations: Vec<Representation>,
}

fn read(board: &NSPasteboard, image_only: bool) -> Result<Vec<Representation>, String> {
    let Some(item) = board
        .pasteboardItems()
        .and_then(|items| items.firstObject())
    else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    let mut size = 0;
    let available = item.types();
    for &name in if image_only { PICTURES } else { READABLE } {
        let kind = NSString::from_str(name);
        if !available.containsObject(&kind) {
            continue;
        }
        let Some(data) = item.dataForType(&kind) else {
            continue;
        };
        if data.len() > LIMIT - size {
            return Err("Clipboard data exceeds the 64 MB limit".into());
        }
        size += data.len();
        result.push(Representation {
            kind: name.into(),
            data: STANDARD.encode(data.to_vec()),
        });
        // Fetch only the best editable/image representation. Lazy providers may
        // have very large alternate representations that we do not need.
        if matches!(
            name,
            "dev.reshiki.drawing"
                | "dev.moruno.drawing"
                | "com.mdli.molfile"
                | "org.opensmiles.smiles"
                | "public.utf8-plain-text"
        ) || name.contains("cdx")
            || PICTURES.contains(&name)
        {
            break;
        }
    }
    Ok(result)
}

fn write(board: &NSPasteboard, representations: &[Representation]) -> Result<(), String> {
    if representations.is_empty() || representations.len() > 20 {
        return Err("No supported clipboard representations".into());
    }
    let item = NSPasteboardItem::new();
    let mut size = 0;
    let mut names = HashSet::new();
    for representation in representations {
        let kinds = pasteboard_types(&representation.kind)
            .ok_or("Invalid or oversized clipboard representation")?;
        // Count actual native aliases as well as input representations. An
        // alias collision must not silently overwrite previously prepared data.
        let remaining = (LIMIT - size) / kinds.len();
        if kinds.iter().any(|&kind| !names.insert(kind))
            || representation.data.len() > remaining.div_ceil(3) * 4
        {
            return Err("Invalid or oversized clipboard representation".into());
        }
        let bytes = STANDARD
            .decode(&representation.data)
            .map_err(|_| "Invalid clipboard encoding")?;
        if bytes.is_empty() || bytes.len() > remaining {
            return Err("Invalid or oversized clipboard representation".into());
        }
        size += bytes.len() * kinds.len();
        let data = NSData::with_bytes(&bytes);
        for &kind in kinds {
            if !item.setData_forType(&data, &NSString::from_str(kind)) {
                return Err(format!("Could not prepare clipboard data for {kind}"));
            }
        }
    }
    // Prepare every representation before replacing any existing data. Cut is
    // committed by the editor only after the worker acknowledges this write.
    let object: &ProtocolObject<dyn NSPasteboardWriting> = ProtocolObject::from_ref(&*item);
    let objects = NSArray::from_slice(&[object]);
    board.clearContents();
    if !board.writeObjects(&objects) {
        return Err("Could not write clipboard data".into());
    }
    Ok(())
}

pub fn execute(input: &[u8]) -> Result<Vec<u8>, String> {
    if input.len() > LIMIT * 2 {
        return Err("Clipboard request is too large".into());
    }
    let request: Request =
        serde_json::from_slice(input).map_err(|_| "Invalid clipboard request or data")?;
    let board = NSPasteboard::generalPasteboard();
    let representations = match request.operation.as_str() {
        "read" => read(&board, false)?,
        "read_picture" => read(&board, true)?,
        "write" => {
            write(&board, &request.representations)?;
            Vec::new()
        }
        _ => return Err("Unsupported clipboard operation".into()),
    };
    serde_json::to_vec(&Response { representations }).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Production handles one request per worker process. Keep these tests from
    // concurrently entering AppKit's process-wide pasteboard type caches.
    static PASTEBOARD_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct PrivatePasteboard<'a>(&'a NSPasteboard);

    impl Drop for PrivatePasteboard<'_> {
        fn drop(&mut self) {
            // SAFETY: This is our uniquely named, still-retained pasteboard.
            // AppKit's releaseGlobally removes its server registration and
            // returns void. The binding omits this oneway Objective-C method.
            unsafe {
                let _: () = objc2::msg_send![self.0, releaseGlobally];
            }
        }
    }

    fn representation(kind: &str, data: &str) -> Representation {
        Representation {
            kind: kind.into(),
            data: STANDARD.encode(data),
        }
    }

    #[test]
    fn explicit_structure_copies_offer_native_data_and_plain_text() -> Result<(), String> {
        let _lock = PASTEBOARD_TEST
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let board = NSPasteboard::pasteboardWithUniqueName();
        let _release = PrivatePasteboard(&board);
        for (kind, native_kind, data) in [
            ("com.mdli.molfile", "com.mdli.molfile", "MOL\nM  END\n"),
            ("org.opensmiles.smiles", "org.opensmiles.smiles", "CO"),
            (
                "chemical/x-cdxml",
                "com.revvity.cdxml",
                "<CDXML>日本語</CDXML>",
            ),
        ] {
            let native = representation(kind, data);
            write(
                &board,
                &[native, representation("public.utf8-plain-text", data)],
            )?;
            assert_eq!(
                read(&board, false)?,
                vec![representation(native_kind, data)]
            );
            let text = board
                .dataForType(&NSString::from_str("public.utf8-plain-text"))
                .ok_or("Missing clipboard text")?;
            assert_eq!(text.to_vec(), data.as_bytes());
        }
        Ok(())
    }

    #[test]
    fn every_copy_as_format_publishes_exact_native_types_and_bytes() -> Result<(), String> {
        let _lock = PASTEBOARD_TEST
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let board = NSPasteboard::pasteboardWithUniqueName();
        let _release = PrivatePasteboard(&board);
        // Mirror the portable Copy As packets, including binary bytes and text
        // fallbacks. Assert the actual server-side types/data, not only read().
        let binary = b"exact\0binary\xffpayload".as_slice();
        let text = "exact UTF-8 日本語\n".as_bytes();
        let cases: [(&str, &[&str], &[&str], bool); 11] = [
            ("png", &[], &["public.png"], false),
            ("svg", &[], &["public.svg-image"], true),
            ("pdf", &[], &["com.adobe.pdf"], false),
            ("mol", &[], &["com.mdli.molfile"], true),
            ("smiles", &[], &["org.opensmiles.smiles"], true),
            ("inchi", &[], &[], true),
            ("cdxml", &["chemical/x-cdxml"], CDXML_TYPES, true),
            (
                "cdx",
                &[],
                &[
                    "com.revvity.chemdraw.cdx-clipboard",
                    "com.perkinelmer.chemdraw.cdx-clipboard",
                    "com.cambridgesoft.cdx",
                ],
                false,
            ),
            ("rxn", &[], &[], true),
            ("reaction-smiles", &[], &[], true),
            ("chemdoodle-reaction", &[], &[], true),
        ];
        for (format, input_types, output_types, plain_text) in cases {
            let data = if plain_text { text } else { binary };
            let mut request: Vec<_> = if input_types.is_empty() {
                output_types
            } else {
                input_types
            }
            .iter()
            .map(|kind| Representation {
                kind: (*kind).into(),
                data: STANDARD.encode(data),
            })
            .collect();
            let mut expected = output_types.to_vec();
            if plain_text {
                request.push(Representation {
                    kind: "public.utf8-plain-text".into(),
                    data: STANDARD.encode(text),
                });
                expected.push("public.utf8-plain-text");
            }
            write(&board, &request).map_err(|error| format!("{format}: {error}"))?;
            let items = board.pasteboardItems().ok_or("Missing pasteboard items")?;
            assert_eq!(items.len(), 1, "{format}");
            let item = items.firstObject().ok_or("Missing pasteboard item")?;
            let types = item.types();
            let actual: HashSet<_> = (0..types.len())
                .map(|index| types.objectAtIndex(index).to_string())
                .collect();
            assert_eq!(
                actual,
                expected.iter().map(|kind| (*kind).to_owned()).collect(),
                "{format} must publish only the requested representations"
            );
            for kind in expected {
                let copied = item
                    .dataForType(&NSString::from_str(kind))
                    .ok_or_else(|| format!("{format}: missing {kind}"))?;
                assert_eq!(copied.to_vec(), data, "{format}: {kind}");
            }
        }
        Ok(())
    }

    #[test]
    fn private_pasteboard_priorities_formats_and_invalid_writes() -> Result<(), String> {
        let _lock = PASTEBOARD_TEST
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let board = NSPasteboard::pasteboardWithUniqueName();
        let _release = PrivatePasteboard(&board);
        let native = representation("dev.reshiki.drawing", "native");
        let legacy = representation("dev.moruno.drawing", "legacy");
        let png = representation("public.png", "png");
        let text = representation("public.utf8-plain-text", "https://example.invalid/picture");
        for (input, expected) in [
            (
                vec![text.clone(), png.clone(), native.clone()],
                native.clone(),
            ),
            (vec![legacy.clone(), native.clone()], native.clone()),
            (
                vec![legacy, png.clone()],
                representation("dev.moruno.drawing", "legacy"),
            ),
            (vec![text, png.clone()], png.clone()),
        ] {
            write(&board, &input)?;
            assert_eq!(read(&board, false)?, vec![expected]);
        }
        write(&board, &[native.clone(), png.clone()])?;
        assert_eq!(read(&board, true)?, vec![png.clone()]);
        for &kind in PICTURES {
            let image = representation(kind, kind);
            write(&board, std::slice::from_ref(&image))?;
            assert_eq!(read(&board, false)?, vec![image]);
        }
        write(&board, std::slice::from_ref(&native))?;
        assert!(read(&board, true)?.is_empty());
        for invalid in [
            vec![png.clone(), png],
            Vec::new(),
            vec![representation("unsupported/type", "data")],
            vec![Representation {
                kind: native.kind.clone(),
                data: "!".into(),
            }],
            vec![representation("public.png", "")],
            vec![
                representation("chemical/x-cdxml", "<CDXML/>"),
                representation("com.revvity.cdxml", "different XML"),
            ],
            vec![
                representation("com.perkinelmer.cdxml", "different XML"),
                representation("chemical/x-cdxml", "<CDXML/>"),
            ],
            // One input expands to three native aliases. Reject it before
            // decoding/publishing when the aliases exceed the combined budget.
            vec![Representation {
                kind: "chemical/x-cdxml".into(),
                data: "A".repeat((LIMIT / 3).div_ceil(3) * 4 + 4),
            }],
        ] {
            assert!(write(&board, &invalid).is_err());
            assert_eq!(read(&board, false)?, vec![native.clone()]);
        }
        Ok(())
    }
}
