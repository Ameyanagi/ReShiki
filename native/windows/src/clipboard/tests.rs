use super::*;

fn candidate(source: &str, bytes: &[u8]) -> ReadCandidate {
    ReadCandidate {
        source: source.into(),
        bytes: Ok(bytes.to_vec()),
    }
}

#[test]
fn html_clipboard_header_uses_utf8_byte_offsets_and_fragment_markers() {
    for fragment in [
        "<span><i>N</i>-ethyl</span>",
        "<meta charset=\"utf-8\"><span>日本語 β 🧪</span>",
    ] {
        let bytes = cf_html(fragment);
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.starts_with("Version:0.9\r\n"));
        let offset = |key: &str| {
            let value = text
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .unwrap();
            assert_eq!(value.len(), 10);
            value.parse::<usize>().unwrap()
        };
        let start_html = offset("StartHTML:");
        let end_html = offset("EndHTML:");
        let start_fragment = offset("StartFragment:");
        let end_fragment = offset("EndFragment:");
        assert_eq!(
            &bytes[start_html..start_fragment],
            b"<html><body><!--StartFragment-->"
        );
        assert_eq!(&bytes[start_fragment..end_fragment], fragment.as_bytes());
        assert_eq!(end_fragment - start_fragment, fragment.len());
        assert_eq!(
            &bytes[end_fragment..end_html],
            b"<!--EndFragment--></body></html>"
        );
        assert_eq!(&bytes[end_html..], &[0]);
        assert!(text[..start_html].ends_with("\r\n"));
    }
}

#[test]
fn rich_name_formats_map_to_registered_office_formats_without_entering_paste() {
    let html = "<span><i>N</i>-日本語</span>";
    let rtf = br"{\rtf1\ansi {\i N}-ethyl}";
    let representations = [
        ("public.html", html.as_bytes()),
        ("public.rtf", rtf.as_slice()),
    ]
    .into_iter()
    .map(|(kind, bytes)| Representation {
        kind: kind.into(),
        data: STANDARD.encode(bytes),
        read_error: None,
        embedded: None,
        source: None,
    })
    .collect();
    let formats = prepare_formats(representations, false).unwrap();
    assert_eq!(formats.len(), 2);
    assert_eq!(mapped("public.html"), "HTML Format");
    assert_eq!(mapped("public.rtf"), "Rich Text Format");
    assert_eq!(formats[&format("HTML Format").unwrap()], cf_html(html));
    assert_eq!(formats[&format("Rich Text Format").unwrap()], rtf);
    for kind in ["public.html", "public.rtf"] {
        assert!(!priority::READABLE.contains(&kind));
        assert!(priority::read_formats(&[kind.into()], false).is_err());
    }
}

#[test]
fn explicit_structure_formats_keep_native_bytes_and_unicode_text() {
    for (kind, name, data) in [
        (
            "com.mdli.molfile",
            "chemical/x-mdl-molfile",
            "MOL\nM  END\n",
        ),
        ("org.opensmiles.smiles", "SMILES", "CO"),
        (
            "chemical/x-cdxml",
            "chemical/x-cdxml",
            "<CDXML>日本語</CDXML>",
        ),
    ] {
        let representations = [kind, "public.utf8-plain-text"]
            .into_iter()
            .map(|kind| Representation {
                kind: kind.into(),
                data: STANDARD.encode(data),
                read_error: None,
                embedded: None,
                source: None,
            })
            .collect();
        let formats = prepare_formats(representations, false).unwrap();
        assert_eq!(formats.len(), 2);
        assert_eq!(formats[&format(name).unwrap()], data.as_bytes());
        let unicode: Vec<_> = data
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(formats[&UNICODE], unicode);
    }
}

#[test]
fn editable_office_copy_omits_standalone_bitmap_but_copy_image_keeps_it() {
    let image = image::RgbaImage::from_pixel(20, 10, image::Rgba([30, 70, 150, 255]));
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 20, 10);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(image.as_raw())
        .unwrap();
    let reps = || {
        vec![
            Representation {
                kind: "public.png".into(),
                data: STANDARD.encode(&png),
                read_error: None,
                embedded: None,
                source: None,
            },
            Representation {
                kind: "dev.reshiki.drawing".into(),
                data: STANDARD.encode(b"{\"version\":15}"),
                read_error: None,
                embedded: None,
                source: None,
            },
        ]
    };
    let office = prepare_formats(reps(), true).unwrap();
    assert!(!office.contains_key(&DIB));
    assert_eq!(office[&format("PNG").unwrap()], png);
    assert!(office.contains_key(&format("dev.reshiki.drawing").unwrap()));
    let picture = prepare_formats(reps(), false).unwrap();
    assert_eq!(bitmap(&from_dib(&picture[&DIB]).unwrap()).unwrap(), image);
}
#[test]
fn real_clipboard_unicode_native_priority_and_invalid_write() {
    let _clipboard = CLIPBOARD_TEST_LOCK.lock().unwrap();
    let rep = |kind: &str, bytes: &[u8]| Representation {
        kind: kind.into(),
        data: STANDARD.encode(bytes),
        read_error: None,
        embedded: None,
        source: None,
    };
    let text = "日本語 CCO".as_bytes();
    write(
        vec![
            rep("public.utf8-plain-text", text),
            rep("dev.reshiki.drawing", b"{\"version\":15}"),
        ],
        false,
    )
    .unwrap();
    let packet = read_packet(false, &[], false, &[], None).unwrap();
    assert_eq!(packet.representations[0].kind, "dev.reshiki.drawing");
    write(vec![rep("public.utf8-plain-text", text)], false).unwrap();
    assert_eq!(
        STANDARD
            .decode(
                &read_packet(false, &[], false, &[], None)
                    .unwrap()
                    .representations[0]
                    .data
            )
            .unwrap(),
        text
    );
    assert!(
        write(
            vec![Representation {
                kind: "public.png".into(),
                data: "!".into(),
                read_error: None,
                embedded: None,
                source: None,
            }],
            false
        )
        .is_err()
    );
    assert_eq!(
        STANDARD
            .decode(
                &read_packet(false, &[], false, &[], None)
                    .unwrap()
                    .representations[0]
                    .data
            )
            .unwrap(),
        text
    );
    write(vec![rep("com.cambridgesoft.cdx", b"VjCD0100\0")], false).unwrap();
    assert_eq!(
        read_packet(false, &[], false, &[], None)
            .unwrap()
            .representations[0]
            .kind,
        "com.revvity.chemdraw.cdx-clipboard"
    );
}
#[test]
fn dib_roundtrip_keeps_pixels_and_resolution() {
    let image = image::RgbaImage::from_pixel(20, 10, image::Rgba([30, 70, 150, 255]));
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 20, 10);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_pixel_dims(Some(png::PixelDimensions {
        xppu: 11811,
        yppu: 11811,
        unit: png::Unit::Meter,
    }));
    encoder
        .write_header()
        .unwrap()
        .write_image_data(image.as_raw())
        .unwrap();
    let result = from_dib(&to_dib(&png).unwrap()).unwrap();
    assert_eq!(bitmap(&result).unwrap(), image);
    assert_eq!(
        png::Decoder::new(Cursor::new(result))
            .read_info()
            .unwrap()
            .info()
            .pixel_dims
            .unwrap()
            .xppu,
        11811
    );
    assert!(from_dib(&[0; 40]).is_err());
    for length in 0..40 {
        assert!(from_dib(&vec![0; length]).is_err());
    }
}

#[test]
fn ordinary_read_fetches_text_before_picture_and_retries_only_remaining_formats() {
    let formats = priority::read_formats(&[], false).unwrap();
    let mut attempted = Vec::new();
    let packet = read_requested(&formats, |kind| {
        attempted.push(kind.to_owned());
        Ok(match kind {
            "public.utf8-plain-text" => Some(candidate("CF_UNICODETEXT", b"CCO")),
            "public.png" => Some(candidate("PNG", b"picture")),
            _ => None,
        })
    });
    assert_eq!(packet.representations[0].kind, "public.utf8-plain-text");
    assert_eq!(
        STANDARD.decode(&packet.representations[0].data).unwrap(),
        b"CCO"
    );
    assert!(!attempted.iter().any(|kind| kind == "public.png"));

    let requested: Vec<_> = formats
        .into_iter()
        .filter(|kind| *kind != "public.utf8-plain-text")
        .map(str::to_owned)
        .collect();
    let retry = priority::read_formats(&requested, false).unwrap();
    let packet = read_requested(&retry, |kind| {
        assert_ne!(kind, "public.utf8-plain-text");
        Ok((kind == "public.png").then(|| candidate(kind, b"picture")))
    });
    assert_eq!(packet.representations[0].kind, "public.png");
}

#[test]
fn chemistry_precedes_text_and_tiff_precedes_jpeg_pdf() {
    for chemical in [
        "dev.reshiki.drawing",
        "com.revvity.chemdraw.cdx-clipboard",
        "com.mdli.molfile",
    ] {
        let formats = priority::read_formats(&[], false).unwrap();
        let mut attempted = Vec::new();
        let packet = read_requested(&formats, |kind| {
            attempted.push(kind.to_owned());
            Ok((kind == chemical
                || matches!(
                    kind,
                    "public.utf8-plain-text" | "public.tiff" | "com.adobe.pdf"
                ))
            .then(|| candidate(kind, b"data")))
        });
        assert_eq!(packet.representations[0].kind, chemical);
        assert!(
            !attempted
                .iter()
                .any(|kind| kind == "public.utf8-plain-text")
        );
    }
    let formats = priority::read_formats(&[], true).unwrap();
    let packet = read_requested(&formats, |kind| {
        assert_ne!(kind, "public.utf8-plain-text");
        assert_ne!(kind, "com.adobe.pdf");
        Ok(matches!(kind, "public.tiff" | "public.jpeg").then(|| candidate(kind, b"picture")))
    });
    assert_eq!(packet.representations[0].kind, "public.tiff");
}

#[test]
fn typed_aliases_and_vector_formats_keep_requested_kinds() {
    for kind in CDX_KINDS {
        assert!(registered_formats(kind).contains(&"ChemDraw Interchange Format"));
        assert_eq!(mapped(kind), "ChemDraw Interchange Format");
    }
    for kind in CDXML_KINDS {
        assert!(registered_formats(kind).contains(&"ChemDraw XML"));
    }
    for kind in CDX_KINDS.iter().chain(CDXML_KINDS) {
        let requested = vec![(*kind).to_owned()];
        let formats = priority::read_formats(&requested, false).unwrap();
        let packet = read_requested(&formats, |_| Ok(Some(candidate(kind, b"chemistry"))));
        assert_eq!(packet.representations[0].kind, *kind);
        assert!(packet.representations[0].read_error.is_none());
        assert!(packet.representations[0].embedded.is_none());
    }
    assert!(registered_formats("public.png").contains(&"PNG"));
    assert!(registered_formats("public.tiff").contains(&"TIFF"));
    assert!(registered_formats("com.mdli.molfile").contains(&"MDLCT"));
    for (kind, name) in [
        ("com.adobe.pdf", "PDF"),
        ("public.svg-image", "image/svg+xml"),
    ] {
        assert!(registered_formats(kind).contains(&name));
        let requested = vec![kind.to_owned()];
        let formats = priority::read_formats(&requested, false).unwrap();
        let packet = read_requested(&formats, |_| Ok(Some(candidate(kind, b"vector"))));
        assert_eq!(packet.representations[0].kind, kind);
        assert!(priority::read_formats(&requested, true).unwrap().is_empty());
    }
}

#[test]
fn failed_native_read_is_bounded_and_does_not_fetch_lower_priority_data() {
    let mut attempted = Vec::new();
    let packet = read_requested(&["dev.reshiki.drawing", "public.png"], |kind| {
        attempted.push(kind.to_owned());
        Err(anyhow::anyhow!("x".repeat(2000)))
    });
    assert_eq!(attempted, ["dev.reshiki.drawing"]);
    let representation = &packet.representations[0];
    assert_eq!(representation.kind, "dev.reshiki.drawing");
    assert!(representation.data.is_empty());
    assert_eq!(representation.read_error.as_ref().unwrap().len(), 1000);
    let serialized = serde_json::to_value(&packet).unwrap();
    assert!(!serialized["representations"][0]["read_error"].is_null());
}

#[test]
fn physical_sources_are_skipped_before_transfer_across_portable_aliases() {
    let formats = priority::read_formats(&[], false).unwrap();
    let mut transfers = Vec::new();
    let common_cdx = "ChemDraw Interchange Format";
    let mol = "chemical/x-mdl-molfile";
    let packet = read_requested(&formats, |kind| {
        read_kind_with(
            kind,
            &[],
            |source| Ok(source == common_cdx || source == mol),
            |source| {
                transfers.push(source.to_owned());
                Ok(b"data".to_vec())
            },
        )
    });
    assert_eq!(
        packet.representations[0].source.as_deref(),
        Some(common_cdx)
    );
    let skipped = vec![common_cdx.to_owned()];
    let packet = read_requested(&formats, |kind| {
        read_kind_with(
            kind,
            &skipped,
            |source| Ok(source == common_cdx || source == mol),
            |source| {
                transfers.push(source.to_owned());
                Ok(b"data".to_vec())
            },
        )
    });
    assert_eq!(packet.representations[0].kind, "com.mdli.molfile");
    assert_eq!(packet.representations[0].source.as_deref(), Some(mol));
    assert_eq!(transfers, [common_cdx, mol]);
}

#[test]
fn failed_png_transfer_keeps_secondary_png_and_dib_available_for_retry() {
    let original = image::RgbaImage::from_pixel(1, 1, image::Rgba([30, 70, 150, 255]));
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(original.as_raw())
        .unwrap();
    let dib = to_dib(&png).unwrap();
    let mut transfers = Vec::new();
    let available = ["PNG", "image/png", "CF_DIBV5", "CF_DIB"];
    let mut skipped = Vec::new();
    for expected in available {
        let packet = read_requested(&["public.png"], |kind| {
            read_kind_with(
                kind,
                &skipped,
                |source| Ok(available.contains(&source)),
                |source| {
                    transfers.push(source.to_owned());
                    match source {
                        "image/png" => Ok(png.clone()),
                        "CF_DIB" => Ok(dib.clone()),
                        _ => Err(anyhow::anyhow!("Synthetic unavailable clipboard memory")),
                    }
                },
            )
        });
        let representation = &packet.representations[0];
        assert_eq!(representation.kind, "public.png");
        assert_eq!(representation.source.as_deref(), Some(expected));
        assert_eq!(
            representation.read_error.is_some(),
            matches!(expected, "PNG" | "CF_DIBV5")
        );
        if representation.read_error.is_none() {
            assert_eq!(
                bitmap(&STANDARD.decode(&representation.data).unwrap()).unwrap(),
                original
            );
        }
        skipped.push(expected.to_owned());
    }
    assert_eq!(transfers, available);
    let packet = read_requested(&["public.png"], |kind| {
        read_kind_with(
            kind,
            &skipped,
            |source| {
                assert!(!available.contains(&source));
                Ok(false)
            },
            |_| panic!("Skipped sources should not transfer data"),
        )
    });
    assert!(packet.representations.is_empty());
}

#[test]
fn embedded_failure_retry_skips_ole_and_preserves_standalone_native_candidate() {
    let formats = [
        "dev.reshiki.drawing",
        "public.utf8-plain-text",
        "public.png",
    ];
    let mut ole_reads = 0;
    let packet = read_packet_with(
        &formats,
        false,
        false,
        |_| {
            ole_reads += 1;
            Err(anyhow::anyhow!("Damaged embedded storage"))
        },
        |_| panic!("Ordinary clipboard read before reporting embedded failure"),
    )
    .unwrap();
    assert_eq!(packet.representations[0].kind, "dev.reshiki.drawing");
    assert_eq!(packet.representations[0].embedded, Some(true));
    assert!(packet.representations[0].read_error.is_some());
    let packet = read_packet_with(
        &formats,
        false,
        true,
        |_| {
            ole_reads += 1;
            Err(anyhow::anyhow!("Must not revisit OLE"))
        },
        |formats| {
            Ok(read_requested(formats, |kind| {
                Ok((kind == "dev.reshiki.drawing").then(|| candidate(kind, b"valid drawing")))
            }))
        },
    )
    .unwrap();
    assert_eq!(ole_reads, 1);
    assert_eq!(packet.representations[0].kind, "dev.reshiki.drawing");
    assert!(packet.representations[0].embedded.is_none());
    assert!(packet.representations[0].read_error.is_none());
}

#[test]
fn embedded_success_returns_one_marked_candidate_without_reading_standalone_data() {
    let packet = read_packet_with(
        &[
            "dev.reshiki.drawing",
            "com.revvity.chemdraw.cdx-clipboard",
            "public.png",
        ],
        false,
        false,
        |_| {
            Ok(Some(super::super::ole::EmbeddedData::ChemDraw(
                b"cdx".to_vec(),
            )))
        },
        |_| panic!("Standalone clipboard read after embedded success"),
    )
    .unwrap();
    assert_eq!(packet.representations.len(), 1);
    assert_eq!(
        packet.representations[0].kind,
        "com.revvity.chemdraw.cdx-clipboard"
    );
    assert_eq!(packet.representations[0].embedded, Some(true));
}

#[test]
fn excluded_embedded_kind_does_not_block_requested_standalone_kind() {
    for (requested, picture_only, embedded) in [
        (
            "public.tiff",
            true,
            super::super::ole::EmbeddedData::Picture(b"preview".to_vec()),
        ),
        (
            "dev.reshiki.drawing",
            false,
            super::super::ole::EmbeddedData::ChemDraw(b"cdx".to_vec()),
        ),
    ] {
        let mut ordinary_reads = 0;
        let packet = read_packet_with(
            &[requested],
            picture_only,
            false,
            |_| Ok(Some(embedded)),
            |formats| {
                ordinary_reads += 1;
                assert_eq!(formats, &[requested]);
                Ok(read_requested(formats, |kind| {
                    Ok(Some(candidate(kind, b"requested data")))
                }))
            },
        )
        .unwrap();
        assert_eq!(ordinary_reads, 1);
        assert_eq!(packet.representations[0].kind, requested);
        assert!(packet.representations[0].embedded.is_none());
        assert_eq!(
            STANDARD.decode(&packet.representations[0].data).unwrap(),
            b"requested data"
        );
    }
}

#[test]
fn replacement_owner_aborts_retry_before_applying_source_exclusions() {
    let generation = std::cell::Cell::new(ClipboardGeneration {
        owner: 41,
        formats: 3,
    });
    let source = "ChemDraw Interchange Format";
    let kind = "com.revvity.chemdraw.cdx-clipboard";
    let initial = read_generation_with(
        None,
        || generation.get(),
        || {
            read_packet_with(
                &[kind],
                false,
                false,
                |_| Ok(None),
                |formats| {
                    Ok(read_requested(formats, |kind| {
                        read_kind_with(
                            kind,
                            &[],
                            |candidate| Ok(candidate == source),
                            |_| Err(anyhow::anyhow!("Damaged original clipboard data")),
                        )
                    }))
                },
            )
        },
    )
    .unwrap();
    assert_eq!(initial.generation.as_deref(), Some("windows:41:3"));
    assert!(initial.representations[0].read_error.is_some());
    assert_eq!(initial.representations[0].source.as_deref(), Some(source));

    generation.set(ClipboardGeneration {
        owner: 42,
        formats: 3,
    });
    let skipped = vec![source.to_owned()];
    for skip_embedded in [false, true] {
        let result = read_generation_with(
            initial.generation.as_deref(),
            || generation.get(),
            || {
                read_packet_with(
                    &[kind],
                    false,
                    skip_embedded,
                    |_| panic!("Changed generation must abort before OLE acquisition"),
                    |formats| {
                        Ok(read_requested(formats, |kind| {
                            read_kind_with(
                                kind,
                                &skipped,
                                |_| panic!("Changed generation must abort before source selection"),
                                |_| panic!("Changed generation must abort before source transfer"),
                            )
                        }))
                    },
                )
            },
        );
        assert_eq!(
            result.err().unwrap().to_string(),
            priority::CLIPBOARD_CHANGED
        );
    }

    let fresh = read_generation_with(
        None,
        || generation.get(),
        || {
            Ok(read_requested(&[kind], |kind| {
                read_kind_with(
                    kind,
                    &[],
                    |candidate| Ok(candidate == source),
                    |_| Ok(b"valid replacement".to_vec()),
                )
            }))
        },
    )
    .unwrap();
    assert_eq!(fresh.generation.as_deref(), Some("windows:42:3"));
    assert!(fresh.representations[0].read_error.is_none());
    assert_eq!(
        STANDARD.decode(&fresh.representations[0].data).unwrap(),
        b"valid replacement"
    );
}

#[test]
fn owner_or_format_changes_during_acquisition_override_candidate_errors() {
    for (embedded, replacement) in [false, true].into_iter().flat_map(|embedded| {
        [
            ClipboardGeneration {
                owner: 74,
                formats: 3,
            },
            ClipboardGeneration {
                owner: 0,
                formats: 0,
            },
            ClipboardGeneration {
                owner: 73,
                formats: 0,
            },
        ]
        .into_iter()
        .map(move |replacement| (embedded, replacement))
    }) {
        let generation = std::cell::Cell::new(ClipboardGeneration {
            owner: 73,
            formats: 3,
        });
        let result = read_generation_with(
            Some("windows:73:3"),
            || generation.get(),
            || {
                read_packet_with(
                    &["dev.reshiki.drawing"],
                    false,
                    !embedded,
                    |_| {
                        generation.set(replacement);
                        Err(anyhow::anyhow!("Damaged embedded storage"))
                    },
                    |formats| {
                        Ok(read_requested(formats, |_| {
                            generation.set(replacement);
                            Err(anyhow::anyhow!("Unavailable native clipboard memory"))
                        }))
                    },
                )
            },
        );
        assert_eq!(
            result.err().unwrap().to_string(),
            priority::CLIPBOARD_CHANGED
        );
    }

    let generation = std::cell::Cell::new(ClipboardGeneration {
        owner: 73,
        formats: 3,
    });
    let result = read_generation_with(
        Some("windows:73:3"),
        || generation.get(),
        || {
            generation.set(ClipboardGeneration {
                owner: 74,
                formats: 3,
            });
            Err(anyhow::anyhow!("Clipboard acquisition failed"))
        },
    );
    assert_eq!(
        result.err().unwrap().to_string(),
        priority::CLIPBOARD_CHANGED
    );
}

#[test]
fn owner_loss_or_format_change_aborts_before_acquisition() {
    for replacement in [
        ClipboardGeneration {
            owner: 0,
            formats: 0,
        },
        ClipboardGeneration {
            owner: 0,
            formats: 3,
        },
        ClipboardGeneration {
            owner: 73,
            formats: 4,
        },
    ] {
        let result = read_generation_with(
            Some("windows:73:3"),
            || replacement,
            || panic!("Changed owner or format count must abort before acquisition"),
        );
        assert_eq!(
            result.err().unwrap().to_string(),
            priority::CLIPBOARD_CHANGED
        );
    }
}

#[test]
fn delayed_rendering_sequence_changes_with_same_owner_allow_paste_and_retry() {
    for embedded in [false, true] {
        let generation = ClipboardGeneration {
            owner: 73,
            formats: 3,
        };
        let sequence = std::cell::Cell::new(100);
        let mut expected = None;
        for _ in 0..2 {
            let packet = read_generation_with(
                expected.as_deref(),
                || generation,
                || {
                    read_packet_with(
                        &["com.revvity.chemdraw.cdx-clipboard"],
                        false,
                        !embedded,
                        |_| {
                            sequence.set(sequence.get() + 1);
                            Ok(Some(super::super::ole::EmbeddedData::ChemDraw(
                                b"rendered data".to_vec(),
                            )))
                        },
                        |formats| {
                            Ok(read_requested(formats, |kind| {
                                sequence.set(sequence.get() + 1);
                                Ok(Some(candidate(kind, b"rendered data")))
                            }))
                        },
                    )
                },
            )
            .unwrap();
            assert_eq!(packet.generation.as_deref(), Some("windows:73:3"));
            assert_eq!(packet.representations[0].read_error, None);
            assert_eq!(packet.representations[0].embedded, embedded.then_some(true));
            assert_eq!(
                STANDARD.decode(&packet.representations[0].data).unwrap(),
                b"rendered data"
            );
            expected = packet.generation;
        }
        assert_eq!(sequence.get(), 102);
    }
}

#[test]
fn stable_empty_read_has_generation_and_write_response_omits_it() {
    let empty = read_generation_with(
        Some("windows:0:0"),
        || ClipboardGeneration {
            owner: 0,
            formats: 0,
        },
        || {
            read_packet_with(
                &[],
                false,
                false,
                |_| panic!("Empty format list must not acquire OLE data"),
                |_| panic!("Empty format list must not acquire ordinary data"),
            )
        },
    )
    .unwrap();
    assert!(empty.representations.is_empty());
    assert_eq!(
        serde_json::to_value(&empty).unwrap()["generation"],
        "windows:0:0"
    );
    let write_response = Packet {
        representations: vec![],
        generation: None,
    };
    assert!(
        serde_json::to_value(&write_response)
            .unwrap()
            .get("generation")
            .is_none()
    );
}

#[test]
fn read_metadata_and_options_cannot_enter_write_requests() {
    let request: Request = serde_json::from_value(serde_json::json!({"operation":"read"})).unwrap();
    assert!(request.formats.is_empty());
    assert!(!request.skip_embedded);
    assert!(request.skip_sources.is_empty());
    assert!(request.generation.is_none());
    let representation: Representation =
        serde_json::from_value(serde_json::json!({"type":"public.png", "data":"aQ=="})).unwrap();
    assert!(representation.read_error.is_none());
    assert!(representation.embedded.is_none());
    assert!(representation.source.is_none());
    for request in [
        serde_json::json!({"operation":"read", "formats":["unknown"]}),
        serde_json::json!({"operation":"write", "formats":["public.png"]}),
        serde_json::json!({"operation":"write", "skip_embedded":true}),
        serde_json::json!({"operation":"write", "skip_sources":["PNG"]}),
        serde_json::json!({"operation":"write", "generation":"91"}),
        serde_json::json!({"operation":"read", "generation":""}),
        serde_json::json!({"operation":"read", "skip_sources":[""]}),
        serde_json::json!({"operation":"read", "skip_sources":["a".repeat(256)]}),
        serde_json::json!({"operation":"read", "representations":[{"type":"public.png", "data":""}]}),
        serde_json::json!({"operation":"write", "representations":[{"type":"public.png", "data":"aQ==", "read_error":"fake"}]}),
        serde_json::json!({"operation":"write", "representations":[{"type":"public.png", "data":"aQ==", "embedded":false}]}),
        serde_json::json!({"operation":"write", "representations":[{"type":"public.png", "data":"aQ==", "source":"PNG"}]}),
    ] {
        assert!(invoke(&serde_json::to_vec(&request).unwrap()).is_err());
    }
    assert!(unicode_text(&[b'C', 0, b'O', 0, 0, 0]).is_ok());
    assert!(unicode_text(&[0]).is_err());
    assert!(unicode_text(&[0, 0xd8]).is_err());
}
