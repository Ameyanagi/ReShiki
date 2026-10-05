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
