use super::*;

fn item(kind: &str, bytes: &[u8]) -> Representation {
    Representation {
        kind: kind.into(),
        data: STANDARD.encode(bytes),
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
        choose_type(|mime| offer.contains_key(mime), false),
        Some(("dev.reshiki.drawing", NATIVE[0]))
    );
    assert_eq!(
        choose_type(|mime| offer.contains_key(mime), true),
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
            data: "?".into()
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
    assert!(choose_type(|mime| mime == "text/plain", true).is_none());
}
