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
        read_error: None,
        source: None,
    }
}

fn expected_read(mut representation: Representation) -> Representation {
    representation.source = Some(representation.kind.clone());
    representation
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
            vec![expected_read(representation(native_kind, data))]
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
            read_error: None,
            source: None,
        })
        .collect();
        let mut expected = output_types.to_vec();
        if plain_text {
            request.push(Representation {
                kind: "public.utf8-plain-text".into(),
                data: STANDARD.encode(text),
                read_error: None,
                source: None,
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
        (vec![text.clone(), png.clone()], text.clone()),
    ] {
        write(&board, &input)?;
        assert_eq!(read(&board, false)?, vec![expected_read(expected)]);
    }
    write(&board, &[native.clone(), png.clone()])?;
    assert_eq!(read(&board, true)?, vec![expected_read(png.clone())]);
    for &kind in PICTURES {
        let image = representation(kind, kind);
        write(&board, std::slice::from_ref(&image))?;
        assert_eq!(read(&board, false)?, vec![expected_read(image)]);
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
            read_error: None,
            source: None,
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
            read_error: None,
            source: None,
        }],
    ] {
        assert!(write(&board, &invalid).is_err());
        assert_eq!(read(&board, false)?, vec![expected_read(native.clone())]);
    }
    Ok(())
}

#[test]
fn synthetic_reads_use_shared_order_and_fetch_one_candidate_per_request() -> Result<(), String> {
    let text = "public.utf8-plain-text";
    let png = "public.png";
    let cdx = "com.perkinelmer.chemdraw.cdx-clipboard";
    let native = "dev.reshiki.drawing";
    for (available, expected) in [
        (vec![text, png], text),
        (vec![cdx, text, png], cdx),
        (vec![native, cdx, "com.mdli.molfile", text, png], native),
        (vec![text, "public.tiff"], text),
        (vec!["com.adobe.pdf"], "com.adobe.pdf"),
        (vec!["public.jpeg", "public.tiff"], "public.tiff"),
    ] {
        let mut fetched = Vec::new();
        let result = read_next(
            &[],
            &[],
            false,
            |kind| available.contains(&kind),
            |kind| {
                fetched.push(kind.to_owned());
                Ok(kind.as_bytes().to_vec())
            },
        )?;
        assert_eq!(result, [expected_read(representation(expected, expected))]);
        assert_eq!(fetched, [expected]);
    }
    // After the editor rejects nonchemical text, only the picture is fetched.
    let formats: Vec<_> = READABLE
        .iter()
        .filter(|kind| **kind != text)
        .map(|kind| (*kind).to_owned())
        .collect();
    for contents in ["CCO", "hello", "title\nM  END\n"] {
        let available = [
            representation(text, contents),
            representation(png, "picture"),
        ];
        let request = if contents == "hello" {
            formats.as_slice()
        } else {
            &[]
        };
        let result = read_next(
            request,
            &[],
            false,
            |kind| available.iter().any(|item| item.kind == kind),
            |kind| {
                let item = available
                    .iter()
                    .find(|item| item.kind == kind)
                    .ok_or("Missing synthetic data")?;
                STANDARD
                    .decode(&item.data)
                    .map_err(|error| error.to_string())
            },
        )?;
        assert_eq!(
            result,
            [expected_read(
                available
                    .get(usize::from(contents == "hello"))
                    .ok_or("Missing expected representation")?
                    .clone()
            )]
        );
    }
    let explicit = read_next(
        &[],
        &[],
        true,
        |kind| [native, text, png].contains(&kind),
        |kind| Ok(kind.as_bytes().to_vec()),
    )?;
    assert_eq!(explicit, [expected_read(representation(png, png))]);
    Ok(())
}

#[test]
fn failed_native_transfer_is_reported_without_fetching_alternatives() -> Result<(), String> {
    let mut reads = Vec::new();
    let result = read_next(
        &[],
        &[],
        false,
        |kind| ["dev.reshiki.drawing", "public.png"].contains(&kind),
        |kind| {
            reads.push(kind.to_owned());
            Err("Clipboard data exceeds the 64 MB limit".into())
        },
    )?;
    assert_eq!(reads, ["dev.reshiki.drawing"]);
    let item = result.first().ok_or("Missing failure representation")?;
    assert_eq!(item.kind, "dev.reshiki.drawing");
    assert!(item.data.is_empty());
    assert_eq!(
        item.read_error.as_deref(),
        Some("Clipboard data exceeds the 64 MB limit")
    );
    assert!(priority::read_formats(&vec!["public.png".into(); READABLE.len() + 1], false).is_err());
    assert!(priority::read_formats(&["unknown".into()], false).is_err());
    Ok(())
}

#[test]
fn retry_skips_the_failed_pasteboard_alias_without_reading_it_again() -> Result<(), String> {
    let failed = "com.revvity.chemdraw.cdx-clipboard";
    let alternate = "com.perkinelmer.chemdraw.cdx-clipboard";
    let mut reads = Vec::new();
    let result = read_next(
        &[],
        &[failed.into()],
        false,
        |kind| [failed, alternate, "public.png"].contains(&kind),
        |kind| {
            reads.push(kind.to_owned());
            Ok(b"valid alternate".to_vec())
        },
    )?;
    assert_eq!(reads, [alternate]);
    let item = result.first().ok_or("Missing alternate representation")?;
    assert_eq!(item.kind, alternate);
    assert_eq!(item.source.as_deref(), Some(alternate));
    assert!(priority::validate_skip_sources(&vec!["public.png".into(); 129]).is_err());
    assert!(priority::validate_skip_sources(&["bad\0source".into()]).is_err());
    Ok(())
}

#[test]
fn generation_change_stops_retry_before_applying_old_exclusions() -> Result<(), String> {
    let first = read_generation(None, || "7".into(), || Ok(Vec::new()))?;
    assert_eq!(first.generation.as_deref(), Some("7"));
    let read = std::cell::Cell::new(false);
    let retry = read_generation(
        first.generation.as_deref(),
        || "8".into(),
        || {
            read.set(true);
            Ok(Vec::new())
        },
    );
    assert!(matches!(retry, Err(error) if error == priority::CLIPBOARD_CHANGED));
    assert!(
        !read.get(),
        "replacement data must not be read with old exclusions"
    );
    Ok(())
}

#[test]
fn generation_change_during_acquisition_overrides_candidate_errors() {
    use std::cell::Cell;
    for fails in [false, true] {
        let count = Cell::new(7);
        let result = read_generation(
            None,
            || count.get().to_string(),
            || {
                count.set(8);
                if fails {
                    Err("selected representation failed".into())
                } else {
                    Ok(vec![representation("public.png", "picture")])
                }
            },
        );
        assert!(matches!(result, Err(error) if error == priority::CLIPBOARD_CHANGED));
    }
}

#[test]
fn private_pasteboard_replacement_invalidates_retry_generation() -> Result<(), String> {
    let _lock = PASTEBOARD_TEST
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let board = NSPasteboard::pasteboardWithUniqueName();
    let _release = PrivatePasteboard(&board);
    let cdx = "com.revvity.chemdraw.cdx-clipboard";
    write(&board, &[representation(cdx, "damaged")])?;
    let first = read_requested(&board, false, &[], &[], None)?;
    write(
        &board,
        &[
            representation(cdx, "replacement"),
            representation("public.png", "png"),
        ],
    )?;
    let retry = read_requested(
        &board,
        false,
        &[],
        &[cdx.into()],
        first.generation.as_deref(),
    );
    assert!(matches!(retry, Err(error) if error == priority::CLIPBOARD_CHANGED));
    Ok(())
}
