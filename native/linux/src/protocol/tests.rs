use super::*;

fn item(kind: &str, bytes: &[u8]) -> Representation {
    Representation {
        kind: kind.into(),
        data: STANDARD.encode(bytes),
        read_error: None,
        source: None,
    }
}

#[test]
fn requests_are_bounded_and_reads_cannot_publish_data() {
    assert!(parse_request(br#"{"operation":"erase"}"#).is_err());
    assert!(
        parse_request(
            br#"{"operation":"read","representations":[{"type":"public.png","data":"eA=="}]}"#
        )
        .is_err()
    );
    assert!(parse_request(br#"{"operation":"read","extra":true}"#).is_err());
    assert!(parse_request(br#"{"operation":"read_picture"}"#).is_ok());
    assert!(prepare_offer(&vec![item("public.png", b"x"); MAX_FORMATS + 1]).is_err());
}

#[test]
fn read_generations_are_bounded_and_empty_reads_keep_their_identity() {
    let previous = parse_request(br#"{"operation":"read"}"#).unwrap();
    assert!(previous.generation.is_none());
    let retry = parse_request(br#"{"operation":"read","generation":"x11:1:2"}"#).unwrap();
    assert_eq!(retry.generation.as_deref(), Some("x11:1:2"));
    for generation in [
        String::new(),
        "x".repeat(129),
        "bad\nvalue".into(),
        "日本語".into(),
    ] {
        let input =
            serde_json::to_vec(&serde_json::json!({"operation": "read", "generation": generation}))
                .unwrap();
        assert!(parse_request(&input).is_err());
    }
    assert!(parse_request(br#"{"operation":"write","generation":"x11:1:2"}"#).is_err());
    let read = serde_json::to_value(Response::read(None, "x11:0:-".into())).unwrap();
    assert_eq!(read["generation"], "x11:0:-");
    assert!(read["representations"].as_array().unwrap().is_empty());
    let write = serde_json::to_value(Response::empty()).unwrap();
    assert!(write.get("generation").is_none());
}

#[test]
fn native_aliases_and_multiple_formats_share_one_complete_offer() {
    let offer = prepare_offer(&[
        item("dev.reshiki.drawing", b"{}"),
        item("public.png", b"png"),
        item(CDX[2], b"cdx"),
        item(CDX[3], b"cdx"),
    ])
    .unwrap();
    assert_eq!(offer.get(NATIVE[0]).unwrap().as_ref(), b"{}");
    assert!(Arc::ptr_eq(
        offer.get(NATIVE[0]).unwrap(),
        offer.get(NATIVE[1]).unwrap()
    ));
    assert_eq!(offer.get("image/png").unwrap().as_ref(), b"png");
    assert_eq!(offer.get("chemical/x-cdx").unwrap().as_ref(), b"cdx");
    assert_eq!(
        choose_type(
            |mime| offer.contains_key(mime),
            &read_formats(&[], false).unwrap()
        ),
        Some(("dev.reshiki.drawing", NATIVE[0]))
    );
    assert_eq!(
        choose_type(
            |mime| offer.contains_key(mime),
            &read_formats(&[], true).unwrap()
        ),
        Some(("public.png", "image/png"))
    );
}

#[test]
fn invalid_offer_is_rejected_as_a_whole() {
    assert!(prepare_offer(&[]).is_err());
    assert!(prepare_offer(&[item("public.png", b"")]).is_err());
    assert!(prepare_offer(&[item("public.png", b"png"), item("unknown", b"data")]).is_err());
    assert!(prepare_offer(&[item(CDX[2], b"first"), item(CDX[3], b"second")]).is_err());
    assert!(prepare_offer(&[item("public.utf8-plain-text", &[0xff])]).is_err());
    assert!(
        prepare_offer(&[Representation {
            kind: "public.png".into(),
            data: "?".into(),
            read_error: None,
            source: None,
        }])
        .is_err()
    );
}

#[test]
fn supported_chemistry_and_image_formats_map_without_claiming_editability() {
    for (kind, mime) in [
        ("public.svg-image", "image/svg+xml"),
        ("com.adobe.pdf", "application/pdf"),
        ("com.mdli.molfile", "chemical/x-mdl-molfile"),
        ("org.opensmiles.smiles", "chemical/x-daylight-smiles"),
        ("chemical/x-cdxml", "chemical/x-cdxml"),
        ("public.utf8-plain-text", "UTF8_STRING"),
    ] {
        let offer = prepare_offer(&[item(kind, b"data")]).unwrap();
        assert_eq!(offer.get(mime).unwrap().as_ref(), b"data");
        assert!(!offer.contains_key(NATIVE[0]));
    }
    assert!(
        choose_type(
            |mime| mime == "text/plain",
            &read_formats(&[], true).unwrap()
        )
        .is_none()
    );
}

#[test]
fn reads_validate_remaining_formats_and_ignore_embedded_options() {
    let request = parse_request(
        br#"{"operation":"read","formats":["public.utf8-plain-text","public.png"],"skip_embedded":true}"#,
    )
    .unwrap();
    assert_eq!(request.formats, ["public.utf8-plain-text", "public.png"]);
    assert!(request.skip_embedded);
    assert!(parse_request(br#"{"operation":"read","formats":["image/png"]}"#).is_err());
    let oversized = serde_json::json!({
        "operation": "read",
        "formats": vec!["public.png"; priority::READABLE.len() + 1],
    });
    assert!(parse_request(&serde_json::to_vec(&oversized).unwrap()).is_err());
    for input in [
        br#"{"operation":"write","formats":["public.png"]}"#.as_slice(),
        br#"{"operation":"write","skip_embedded":true}"#.as_slice(),
    ] {
        assert!(parse_request(input).is_err());
    }
}

#[test]
fn text_precedes_png_and_excluding_text_selects_png() {
    let available = |mime: &str| matches!(mime, "text/plain" | "image/png");
    let formats = read_formats(&[], false).unwrap();
    assert_eq!(
        choose_type(available, &formats),
        Some(("public.utf8-plain-text", "text/plain"))
    );
    let remaining: Vec<_> = formats
        .into_iter()
        .filter(|kind| *kind != "public.utf8-plain-text")
        .collect();
    assert_eq!(
        choose_type(available, &remaining),
        Some(("public.png", "image/png"))
    );
    let mimes = requested_mime_types(&read_formats(&[], false).unwrap(), &[]);
    assert!(
        mimes.iter().position(|mime| mime == "text/plain").unwrap()
            < mimes.iter().position(|mime| mime == "image/png").unwrap()
    );
    assert_eq!(
        requested_mime_types(&["public.utf8-plain-text", "public.png"], &[]),
        [
            "text/plain;charset=utf-8",
            "text/plain",
            "UTF8_STRING",
            "image/png"
        ]
    );
}

#[test]
fn editable_chemistry_precedes_text_and_picture_fallbacks() {
    for (kind, mime) in [
        ("dev.reshiki.drawing", NATIVE[0]),
        ("com.revvity.chemdraw.cdx-clipboard", CDX[0]),
        ("com.mdli.molfile", "chemical/x-mdl-molfile"),
    ] {
        assert_eq!(
            choose_type(
                |available| [mime, "text/plain", "image/tiff", "application/pdf"]
                    .contains(&available),
                &read_formats(&[], false).unwrap(),
            ),
            Some((kind, mime))
        );
    }
    for (kind, mime) in [
        ("public.tiff", "image/tiff"),
        ("com.adobe.pdf", "application/pdf"),
    ] {
        assert_eq!(
            choose_type(
                |available| available == mime,
                &read_formats(&[], false).unwrap()
            ),
            Some((kind, mime))
        );
    }
    assert!(
        choose_type(
            |available| available == "application/pdf",
            &read_formats(&[], true).unwrap(),
        )
        .is_none()
    );
}

#[test]
fn requested_order_is_preserved_and_all_portable_formats_have_mime_mappings() {
    let requested = vec!["public.png".into(), "public.utf8-plain-text".into()];
    let formats = read_formats(&requested, false).unwrap();
    assert_eq!(
        choose_type(|mime| matches!(mime, "image/png" | "text/plain"), &formats),
        Some(("public.png", "image/png"))
    );
    for kind in priority::READABLE {
        assert!(mime_types(kind).is_some(), "{kind}");
    }
    let types = requested_mime_types(&read_formats(&[], false).unwrap(), &[]);
    let distinct: std::collections::BTreeSet<_> = types.iter().collect();
    assert_eq!(types.len(), distinct.len());
}

#[test]
fn known_read_failures_keep_one_bounded_candidate_and_success_omits_errors() {
    let failure = read_failure("public.png", "image/png", "x".repeat(1200));
    assert_eq!(failure.kind, "public.png");
    assert!(failure.data.is_empty());
    assert_eq!(failure.source.as_deref(), Some("image/png"));
    assert_eq!(failure.read_error.as_ref().unwrap().len(), 1000);
    assert!(prepare_offer(std::slice::from_ref(&failure)).is_err());
    let json = serde_json::to_value(Response::read(Some(failure), "x11:1:2".into())).unwrap();
    assert_eq!(json["representations"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["representations"][0]["read_error"]
            .as_str()
            .unwrap()
            .len(),
        1000
    );
    let success = serde_json::to_value(representation("public.png", b"png").unwrap()).unwrap();
    assert!(success.get("read_error").is_none());
    assert!(success.get("source").is_none());
    let previous_wire_format: Representation =
        serde_json::from_str(r#"{"type":"public.png","data":"cG5n"}"#).unwrap();
    assert!(previous_wire_format.read_error.is_none());
    assert!(previous_wire_format.source.is_none());
    let selected = selected_representation("public.png", "image/png", b"png");
    assert_eq!(selected.source.as_deref(), Some("image/png"));
    assert!(selected.read_error.is_none());
    let empty = selected_representation("public.png", "image/png", b"");
    assert_eq!(empty.source.as_deref(), Some("image/png"));
    assert!(empty.read_error.is_some());
}

#[test]
fn failed_cdx_source_skips_to_the_next_mime_without_losing_the_portable_kind() {
    let formats = read_formats(&[], false).unwrap();
    let offered = ["chemical/x-cdx", "application/x-chemdraw", "image/png"];
    let first = choose_type(|mime| offered.contains(&mime), &formats).unwrap();
    assert_eq!(
        first,
        ("com.revvity.chemdraw.cdx-clipboard", "chemical/x-cdx")
    );
    let failed = read_failure(first.0, first.1, "owner refused first CDX MIME");
    let skip_sources = vec![failed.source.unwrap()];
    let requested = requested_mime_types(&formats, &skip_sources);
    assert!(!requested.iter().any(|mime| mime == "chemical/x-cdx"));
    let second = choose_type(
        |mime| offered.contains(&mime) && requested.iter().any(|requested| requested == mime),
        &formats,
    )
    .unwrap();
    assert_eq!(second, (first.0, "application/x-chemdraw"));
    let selected = selected_representation(second.0, second.1, b"secondary CDX data");
    assert_eq!(selected.source.as_deref(), Some("application/x-chemdraw"));
    assert_eq!(
        STANDARD.decode(selected.data).unwrap(),
        b"secondary CDX data"
    );
}

#[test]
fn skipped_sources_are_bounded_and_cannot_be_write_options() {
    assert!(parse_request(br#"{"operation":"read","skip_sources":["image/png"]}"#).is_ok());
    assert!(parse_request(br#"{"operation":"read","skip_sources":[""]}"#).is_err());
    assert!(parse_request(br#"{"operation":"read","skip_sources":["image/png\u0000"]}"#).is_err());
    assert!(parse_request(br#"{"operation":"write","skip_sources":["image/png"]}"#).is_err());
    for sources in [vec!["image/png".to_owned(); 129], vec!["x".repeat(256)]] {
        let request = serde_json::json!({"operation": "read", "skip_sources": sources});
        assert!(parse_request(&serde_json::to_vec(&request).unwrap()).is_err());
    }
}
