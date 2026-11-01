use super::*;
use crate::document::{Annotation, Point};

#[test]
fn journal_presets_use_publisher_settings() {
    let mut ids = std::collections::HashSet::new();
    for preset in Preset::ALL {
        assert!(ids.insert(preset.id()));
        let style: DrawingStyle = serde_json::from_str(preset.json()).unwrap();
        style.validate().unwrap();
        assert_eq!(style.name, preset.to_string());
    }
    assert_eq!(Preset::Jacs.style(), DrawingStyle::default());
    assert_eq!(Preset::Nature.style().font_family, "Helvetica");
    assert!((Preset::Angewandte.style().bold_width_pt - 2.6015625).abs() < 0.000001);
    assert_eq!(Preset::Rsc.style().line_width_pt, 0.5);
}

#[test]
fn stereochemistry_options_survive_native_and_cdxml_style_roundtrips() {
    use crate::chemistry::cdxml::{import_cdxml, presentation};
    let directory = tempfile::tempdir().unwrap();
    for two_wedge in [false, true] {
        for fusion_h in [false, true] {
            for wavy in [false, true] {
                let style = DrawingStyle {
                    two_wedge_stereocentres: two_wedge,
                    ring_fusion_hydrogens: fusion_h,
                    wavy_unknown_alkenes: wavy,
                    ..Default::default()
                };
                let path = directory.path().join("stereo.reshiki-style");
                save(&path, &style).unwrap();
                assert_eq!(load(&path).unwrap(), style);
                let mut doc = Document {
                    drawing_style: style,
                    ..Default::default()
                };
                let a = doc.add_atom("C", Point::new(0., 0.));
                let b = doc.add_atom("C", Point::new(42., 0.));
                doc.add_bond(a, b, 1, "plain");
                let xml = crate::exchange::drawing::write(&doc, Default::default()).unwrap();
                // Binary CDX/CDS have no tag for these and simply omit them.
                crate::exchange::to_cdx(&xml).unwrap();
                crate::exchange::to_cds(&xml).unwrap();
                let tree = presentation::parse(&xml).unwrap();
                let native = presentation::drawing_style(tree.root_element()).unwrap();
                let native_back: presentation::NativeDrawingStyle =
                    serde_json::from_slice(&serde_json::to_vec(&native).unwrap()).unwrap();
                assert_eq!(native_back, native);
                let imported = import_cdxml(&xml).unwrap().document;
                let stationery = from_chemdraw_xml(&xml, "Stereo").unwrap();
                for imported in [
                    native.into_document().unwrap(),
                    imported.drawing_style,
                    stationery,
                ] {
                    assert_eq!(imported.two_wedge_stereocentres, two_wedge);
                    assert_eq!(imported.ring_fusion_hydrogens, fusion_h);
                    assert_eq!(imported.wavy_unknown_alkenes, wavy);
                }
                for (attribute, present) in [
                    ("ReShikiTwoWedgeStereocentres", two_wedge),
                    ("ReShikiRingFusionHydrogens", !fusion_h),
                    ("ReShikiWavyUnknownAlkenes", wavy),
                ] {
                    assert_eq!(tree.root_element().attribute(attribute).is_some(), present);
                }
            }
        }
    }
}

#[test]
fn absent_stereochemistry_style_fields_keep_native_defaults_and_old_assets() {
    use crate::chemistry::cdxml::presentation::{self, NativeDrawingStyle};
    let source = include_str!("../../../../assets/drawing_style.json");
    let defaults = NativeDrawingStyle::defaults().unwrap();
    assert_eq!(
        serde_json::to_string_pretty(&defaults).unwrap(),
        source.trim_end()
    );
    let xml = presentation::parse("<CDXML BondLength=\"14.4\"/>").unwrap();
    assert_eq!(
        presentation::drawing_style(xml.root_element()).unwrap(),
        defaults
    );
    assert!(defaults.into_document().unwrap().is_default());
    for preset in Preset::ALL.into_iter().chain([Preset::Presentation]) {
        let style = preset.style();
        assert!(!style.two_wedge_stereocentres);
        assert!(style.ring_fusion_hydrogens);
        assert!(!style.wavy_unknown_alkenes);
        let encoded = serde_json::to_string(&style).unwrap();
        assert!(!encoded.contains("two_wedge_stereocentres"));
        assert!(!encoded.contains("ring_fusion_hydrogens"));
        assert!(!encoded.contains("wavy_unknown_alkenes"));
        assert_eq!(
            serde_json::from_str::<DrawingStyle>(&encoded).unwrap(),
            style
        );
    }
    for attribute in [
        "ReShikiTwoWedgeStereocentres",
        "ReShikiRingFusionHydrogens",
        "ReShikiWavyUnknownAlkenes",
    ] {
        let source = format!("<CDXML BondLength=\"14.4\" {attribute}=\"perhaps\"/>");
        let xml = presentation::parse(&source).unwrap();
        assert!(presentation::drawing_style(xml.root_element()).is_err());
    }
}

fn stationery() -> Vec<u8> {
    // Deliberately different from ReShiki defaults, with a nonstandard font ID.
    crate::exchange::to_cdx(
        r#"<CDXML BondLength="17" BondSpacing="18" LineWidth="0.75"
            BoldWidth="2.6015625" MarginWidth="2" HashSpacing="2.6015625"
            LabelFont="393" LabelSize="12"><fonttable><font id="393" name="Arial" charset="cp1252"/>
            </fonttable></CDXML>"#,
    )
    .unwrap()
}

#[test]
fn reads_current_and_legacy_stationery_without_importing_artwork() {
    let directory = tempfile::tempdir().unwrap();
    let binary = stationery();
    assert_eq!(&binary[22..24], &[0, 0x80]);
    let padded = [&binary[..22], &[0u8; 6], &binary[22..]].concat();
    let legacy = [&binary[..22], &[0u8; 6], &binary[28..]].concat();
    for bytes in [binary, padded, legacy] {
        let path = directory.path().join("Publisher.CDS");
        std::fs::write(&path, &bytes).unwrap();
        let style = load(&path).unwrap();
        assert_eq!(style.name, "Publisher");
        assert_eq!(style.font_family, "Arial");
        assert_eq!(style.bond_length_pt, 17.);
        assert_eq!(style.bold_width_pt, 2.6015625);
        assert_eq!(style.font_size_pt, 12.);
    }
    let mut artwork = stationery();
    // Unknown object containing a large opaque property; valid framing, no drawing import.
    let end = artwork.len() - 4;
    let mut object = vec![0xfe, 0x8f, 7, 0, 0, 0, 0xfe, 0x7f, 0xff, 0xff];
    object.extend(70_000u32.to_le_bytes());
    object.extend(vec![0; 70_000]);
    object.extend([0, 0]);
    artwork.splice(end..end, object);
    let path = directory.path().join("Artwork.cds");
    std::fs::write(&path, &artwork).unwrap();
    assert_eq!(load(&path).unwrap().font_size_pt, 12.);
    assert!(
        crate::exchange::from_cdx(&artwork).is_err(),
        "Drawing imports remain strict"
    );
    for cut in [0, 12, 27, 35, artwork.len() - 3] {
        std::fs::write(&path, &artwork[..cut]).unwrap();
        assert!(load(&path).is_err(), "Truncated at {cut}");
    }
}

#[test]
fn stationery_requires_explicit_valid_settings() {
    let xml = crate::exchange::from_cdx(&stationery()).unwrap();
    let style = from_chemdraw_xml(&xml, "Reference").unwrap();
    assert_eq!(style.line_width_pt, 0.75);
    for invalid in [
        xml.replace("BondLength=\"17\"", "BondLength=\"NaN\""),
        xml.replace("LabelFont=\"393\"", "LabelFont=\"99\""),
        xml.replace("HashSpacing=\"2.6015625\"", ""),
        xml.replace("LineWidth=\"0.75\"", "LineWidth=\"9\""),
    ] {
        assert!(from_chemdraw_xml(&invalid, "Reference").is_err());
    }
}

#[test]
fn reads_legacy_packed_label_style_and_xml_files() {
    let binary = stationery();
    let mut legacy = [&binary[..22], &[0u8; 6]].concat();
    let mut pos = 28;
    while binary[pos..pos + 2] != [0, 0] {
        let tag = u16::from_le_bytes([binary[pos], binary[pos + 1]]);
        let size = u16::from_le_bytes([binary[pos + 2], binary[pos + 3]]) as usize;
        let end = pos + 4 + size;
        if !matches!(tag, 0x081a | 0x081c) {
            legacy.extend_from_slice(&binary[pos..end]);
        }
        pos = end;
    }
    legacy.extend([0x0a, 0x08, 8, 0]);
    for word in [393u16, 96, 240, 2] {
        legacy.extend(word.to_le_bytes());
    }
    legacy.extend([0, 0, 0, 0]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy.cds");
    std::fs::write(&path, legacy).unwrap();
    let style = load(&path).unwrap();
    assert_eq!(style.font_family, "Arial");
    assert_eq!(style.font_size_pt, 12.);
    let path = path.with_extension("cdxml");
    std::fs::write(&path, crate::exchange::from_cdx(&binary).unwrap()).unwrap();
    assert_eq!(load(&path).unwrap(), style);
}

#[test]
fn exported_percentages_do_not_round_down_in_chemdraw() {
    for (ratio, expected) in [
        (0.12, "12.0"),
        (0.179, "17.9"),
        (0.18, "18.0"),
        (0.2, "20.0"),
    ] {
        let mut doc = Document::default();
        doc.drawing_style.name = "Custom spacing".into();
        doc.drawing_style.bond_spacing_ratio = ratio;
        let xml = crate::exchange::drawing::write(&doc, Default::default()).unwrap();
        let tree = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(tree.root_element().attribute("BondSpacing"), Some(expected));
    }
}

#[test]
fn reusable_style_files_are_validated_and_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Presentation.reshiki-style");
    let style = Preset::Presentation.style();
    crate::storage::write_atomic(&path, &serde_json::to_vec_pretty(&style).unwrap()).unwrap();
    assert_eq!(load(&path).unwrap(), style);
    std::fs::write(&path, vec![b' '; 65537]).unwrap();
    assert!(load(&path).unwrap_err().contains("64 KB"));
    let mut invalid = style;
    invalid.bond_length_world = 42.;
    std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(load(&path).unwrap_err().contains("coordinate units"));
}

#[test]
fn style_roundtrip_preserves_coordinates_and_explicit_overrides()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::default();
    let a = doc.add_atom("N", Point::new(20., 30.));
    // Leave room for presentation-size labels without rescaling geometry.
    let b = doc.add_atom("O", Point::new(104., 30.));
    doc.add_bond(a, b, 1, "plain");
    doc.atom_mut(b).ok_or("Oxygen")?.text_style = Some(TextStyle {
        size_pt: 12.,
        color: crate::palette::Color::Custom([80, 0, 0]),
        ..Default::default()
    });
    doc.annotations.push(Annotation {
        id: 3,
        position: Point::new(40., 90.),
        text: "Scheme 1".into(),
        format: Default::default(),
    });
    let styled = apply(&doc, Preset::Presentation.style(), true, false)?;
    assert_eq!(
        styled.atom(a).ok_or("Nitrogen")?.position,
        doc.atom(a).ok_or("Nitrogen")?.position
    );
    assert_eq!(
        styled.atom(b).ok_or("Oxygen")?.text_style,
        doc.atom(b).ok_or("Oxygen")?.text_style
    );
    assert_eq!(
        styled
            .annotations
            .first()
            .ok_or("Caption")?
            .format
            .style
            .size_pt,
        16.
    );
    assert_eq!(styled.bonds, doc.bonds);
    let reopened: Document = serde_json::from_str(&serde_json::to_string(&styled)?)?;
    assert_eq!(styled, reopened);
    let primitive = crate::scene::primitives(&styled);
    assert!(primitive.iter().any(|p| matches!(p, crate::scene::Primitive::Line(_, _, width) if (*width - styled.drawing_style.world(1.)).abs() < 0.001)));
    assert!(primitive.iter().any(|p| matches!(p, crate::scene::Primitive::Text{text, style, ..} if text == "N" && style.size_pt == 16.)));
    assert!(primitive.iter().any(|p| matches!(p, crate::scene::Primitive::Text{text, style, ..} if text == "O" && style.size_pt == 12.)));
    let mut crowded = styled;
    crowded.atom_mut(b).ok_or("Oxygen")?.position.x = 40.;
    assert!(
        !crate::scene::primitives(&crowded)
            .iter()
            .any(|p| matches!(p, crate::scene::Primitive::Line(..))),
        "A bond completely covered by enlarged labels must not cross their text"
    );
    Ok(())
}

#[test]
fn optional_layout_scaling_is_explicit_and_atomic() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let scaled = apply(&doc, Preset::Presentation.style(), false, true).unwrap();
    assert!((scaled.atoms[0].position.distance(scaled.atoms[1].position) - 70.).abs() < 0.001);
    assert_eq!(
        editing::center(&doc, &[a, b]),
        editing::center(&scaled, &[a, b])
    );
    let mut invalid = Preset::Presentation.style();
    invalid.line_width_pt = f32::NAN;
    assert!(apply(&doc, invalid, true, true).is_err());
    assert_eq!(doc.atoms[0].position, Point::new(0., 0.));
    let mut history = crate::document::History::default();
    assert!(history.commit(doc.clone(), &scaled));
    let mut current = scaled.clone();
    assert!(history.undo(&mut current));
    assert_eq!(current, doc);
    assert!(history.redo(&mut current));
    assert_eq!(current, scaled);
}

#[test]
fn formatting_roundtrips_presets_boundaries_and_small_margins() {
    use crate::style::units::{self, Dimension, Unit};
    let mut values = vec![
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        1e-9,
        0.,
        14.4,
        2.6015625,
    ];
    for dimension in Dimension::ALL {
        let (min, max) = dimension.range();
        values.extend([min, max]);
        for preset in Preset::ALL {
            values.push(dimension.get(&preset.style()));
        }
    }
    for value in values {
        for unit in Unit::ALL {
            let text = units::format(value, unit);
            assert_eq!(
                units::parse(&text, unit).unwrap().points.to_bits(),
                value.to_bits(),
                "{value} pt -> {text} {unit}"
            );
        }
    }
    assert_eq!(units::format(14.4, Unit::Millimetres), "5.08");
}

#[test]
fn exported_style_bytes_are_the_saved_file() {
    // The browser downloads these bytes instead of calling `save`.
    let directory = tempfile::tempdir().unwrap();
    let style = Preset::Presentation.style();
    for name in ["Drawing.reshiki-style", "Drawing.cds"] {
        let path = directory.path().join(name);
        let bytes = to_bytes(&path, &style).unwrap();
        save(&path, &style).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let read = load(&path).unwrap();
        assert_eq!(read.font_family, style.font_family);
        assert_eq!(read.font_size_pt, style.font_size_pt);
    }
    assert_eq!(
        load(&directory.path().join("Drawing.reshiki-style")).unwrap(),
        style
    );
    let mut invalid = style;
    invalid.bond_length_world = 42.;
    assert!(to_bytes(std::path::Path::new("Drawing.cds"), &invalid).is_err());
}
