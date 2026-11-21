use super::*;

const ORIGINAL: &str =
    include_str!("../../../../../tests/fixtures/tlc/chemdraw26-mixed-spots.cdxml");

fn plate(spot: &str, extra: &str) -> String {
    format!(
        "<CDXML LineWidth='0.6'><colortable><color r='1' g='1' b='1'/><color/><color r='1'/></colortable><page><tlcplate TopLeft='0 0' TopRight='40 0' BottomRight='40 200' BottomLeft='0 200' OriginFraction='0.1' SolventFrontFraction='0.1' {extra}><tlclane>{spot}</tlclane></tlcplate></page></CDXML>"
    )
}
fn spot(extra: &str) -> String {
    format!("<tlcspot Rf='0.28' Width='325095' Height='325095' {extra}/>")
}
fn read(xml: &str) -> (Vec<Graphic>, Option<String>) {
    let (xml, notice) = super::super::super::approximate::approximate(xml);
    (
        super::super::parse(&xml, SCALE, 1, &BTreeSet::new())
            .unwrap()
            .into_document()
            .unwrap(),
        notice,
    )
}
fn baseline(label: &crate::document::Annotation) -> Point {
    let layout = crate::typography::layout(&label.text, &label.format);
    let ascent = layout
        .fragments
        .first()
        .map_or(0., |f| f.position.y + crate::style::text_ascent(&f.style));
    let factor = match label.format.alignment {
        crate::typography::TextAlign::Center => 0.5,
        crate::typography::TextAlign::Right => 1.,
        _ => 0.,
    };
    label.position.offset(factor * layout.width, ascent).turned(
        label.position.offset(layout.width / 2., layout.height / 2.),
        label.angle,
    )
}

#[test]
fn native_tlc_reader_keeps_reference_spots_and_rich_rf_baseline() {
    let (graphics, notice) = read(ORIGINAL);
    assert_eq!(notice, None);
    let g = graphics.first().unwrap();
    assert_eq!(g.kind, GraphicKind::TlcPlate);
    let tlc = g.tlc.as_ref().unwrap();
    assert_eq!(tlc.lanes.len(), 3);
    assert!(tlc.show_origin && tlc.show_solvent_front && tlc.show_borders);
    for (i, rf) in [0.28, 0.51, 0.77].into_iter().enumerate() {
        let spot = tlc.lanes.get(i).unwrap().spots.first().unwrap();
        assert_eq!(spot.rf, rf);
        assert_eq!(spot.width_pt, 325095. / 65536.);
        assert_eq!(spot.height_pt, 325095. / 65536.);
        assert_eq!(spot.style.fill.is_some(), i != 2);
        let c = tlc.spot_center(g, i, rf);
        assert!((f64::from(c.x) / SCALE - (340. + i as f64 * 10.)).abs() < 0.001);
        assert!((f64::from(c.y) / SCALE - (563. - rf * 160.)).abs() < 0.001);
    }
    let label = tlc.labels(g).into_iter().next().unwrap();
    assert_eq!(label.text, "Rf = 0.28");
    assert_eq!(label.format.style.family, "Arial");
    assert_eq!(label.format.style.size_pt, 7.5);
    assert_eq!(label.format.at(1).script, Script::Subscript);
    let at = baseline(&label);
    assert!((f64::from(at.x) / SCALE - 342.98).abs() < 0.01);
    assert!((f64::from(at.y) / SCALE - 515.22).abs() < 0.01);
}

#[test]
fn tlc_missing_flags_are_false_and_spot_style_inherits_the_plate() {
    let (graphics, notice) = read(&plate(&spot(""), "LineWidth='2' color='4'"));
    assert_eq!(notice, None);
    let tlc = graphics.first().unwrap().tlc.as_ref().unwrap();
    assert!(!tlc.show_origin && !tlc.show_solvent_front && !tlc.show_borders);
    let spot = tlc.lanes.first().unwrap().spots.first().unwrap();
    assert_eq!(spot.style.width_pt, 2.);
    assert_eq!(spot.style.fill, None);
    assert_eq!(
        spot.style.stroke,
        crate::palette::Color::Custom([255, 0, 0])
    );
}

#[test]
fn unsupported_tlc_data_warns_without_unhiding_lanes_or_hollowing_shaded_spots() {
    let (graphics, notice) = read(&plate(&spot("Tail='4' CurveType='256'"), ""));
    assert!(notice.unwrap().contains("TLC plate"));
    assert!(
        graphics
            .first()
            .unwrap()
            .tlc
            .as_ref()
            .unwrap()
            .lanes
            .first()
            .unwrap()
            .spots
            .first()
            .unwrap()
            .style
            .fill
            .is_some()
    );
    let xml = plate(&spot(""), "").replace("<tlclane>", "<tlclane Visible='no'>");
    let (graphics, notice) = read(&xml);
    assert!(notice.unwrap().contains("TLC plate"));
    let tlc = graphics.first().unwrap().tlc.as_ref().unwrap();
    assert_eq!(tlc.lanes.len(), 1);
    assert!(tlc.lanes.first().unwrap().spots.is_empty());
    let xml = plate(
        "<tlcspot Rf='0.3' Width='100000' Height='100000'><customspot/><objecttag Name='rf'><t p='1 1'><s>retained</s><unknown/></t></objecttag></tlcspot>",
        "",
    );
    let (_, notice) = read(&xml);
    assert!(notice.unwrap().contains("TLC plate"));
}

#[test]
fn missing_or_invalid_rf_label_is_generated_with_a_notice() {
    let invalid = |run: &str| {
        format!(
            "<tlcspot Rf='0.28' Width='325095' Height='325095' ShowRf='yes'><objecttag Name='rf'><t p='NaN 1'>{run}</t></objecttag></tlcspot>"
        )
    };
    for body in [
        spot("ShowRf='yes'"),
        invalid("<s>bad</s>"),
        invalid("<s size='3' face='1'>bad</s>"),
    ] {
        let (graphics, notice) = read(&plate(&body, ""));
        assert!(notice.unwrap().contains("TLC plate"));
        let g = graphics.first().unwrap();
        let label = g.tlc.as_ref().unwrap().labels(g).remove(0);
        assert_eq!(label.text, "Rf = 0.28");
        assert_eq!(label.format.style.size_pt, 7.5);
        assert!(!label.format.style.bold);
    }
}

#[test]
fn malformed_tlc_geometry_drops_only_that_plate() {
    let xml = plate(&spot(""), "")
        .replace("BottomRight='40 200'", "BottomRight='-40 -200'")
        .replace(
            "</page>",
            "<graphic GraphicType='Rectangle' BoundingBox='70 0 90 30'/></page>",
        );
    let (graphics, notice) = read(&xml);
    assert_eq!(graphics.len(), 1);
    assert_eq!(graphics.first().unwrap().kind, GraphicKind::Rectangle);
    assert!(notice.unwrap().contains("Left out: 1 TLC plate"));
}

#[test]
fn authored_rf_baseline_survives_alignment_rotation_and_a_skewed_plate() {
    for align in ["Left", "Center", "Right"] {
        for angle in [0., 30., 270.] {
            let body = format!(
                "<tlcspot Rf='0.28' Width='325095' Height='325095' ShowRf='yes'><objecttag Name='rf'><t p='60 80' BoundingBox='10 20 150 200' Justification='{align}' RotationAngle='{}'><s size='9'>Rf = 0.280</s></t></objecttag></tlcspot>",
                angle * 65536.
            );
            let xml = plate(&body, "")
                .replace("TopRight='40 0'", "TopRight='40 30'")
                .replace("BottomLeft='0 200'", "BottomLeft='-30 200'")
                .replace("BottomRight='40 200'", "BottomRight='20 230'");
            let (graphics, notice) = read(&xml);
            assert_eq!(notice, None);
            let g = graphics.first().unwrap();
            let label = g
                .tlc
                .as_ref()
                .unwrap()
                .labels(g)
                .into_iter()
                .next()
                .unwrap();
            assert_eq!(label.text, "Rf = 0.280");
            assert_eq!(label.angle, angle as f32);
            let at = baseline(&label);
            assert!((f64::from(at.x) / SCALE - 60.).abs() < 0.001);
            assert!((f64::from(at.y) / SCALE - 80.).abs() < 0.001);
        }
    }
}

#[test]
fn bbox_only_rf_label_preserves_text_with_an_approximation_notice() {
    let xml = plate(
        "<tlcspot Rf='0.28' Width='325095' Height='325095' ShowRf='yes'><objecttag Name='rf'><t BoundingBox='10 20 30 40'><s>my Rf</s></t></objecttag></tlcspot>",
        "",
    );
    let (graphics, notice) = read(&xml);
    assert!(notice.unwrap().contains("TLC plate"));
    let g = graphics.first().unwrap();
    assert_eq!(
        g.tlc.as_ref().unwrap().labels(g).first().unwrap().text,
        "my Rf"
    );
}

#[test]
fn hidden_custom_data_does_not_hide_a_known_rf_label_and_hidden_rf_keeps_its_record() {
    for hidden in [false, true] {
        let body = format!(
            "<tlcspot Rf='0.28' Width='325095' Height='325095' ShowRf='yes'><customspot Visible='no'/><objecttag Name='rf' Visible='{}'><t p='10 20'><s size='9' face='1'>my Rf</s></t></objecttag></tlcspot>",
            if hidden { "no" } else { "yes" }
        );
        let (graphics, notice) = read(&plate(&body, ""));
        assert!(notice.unwrap().contains("TLC plate"));
        let g = graphics.first().unwrap();
        let tlc = g.tlc.as_ref().unwrap();
        let spot = tlc.lanes.first().unwrap().spots.first().unwrap();
        assert_eq!(spot.show_rf, !hidden);
        let label = spot.label.as_ref().unwrap();
        assert_eq!(label.text, "my Rf");
        assert!(label.format.style.bold);
        assert_eq!(tlc.labels(g).is_empty(), hidden);
    }
}

#[test]
fn orphan_tlc_children_are_left_out_with_a_notice() {
    for orphan in [
        "<tlclane><tlcspot Rf='0.2'/></tlclane>",
        "<tlcspot Rf='0.2'/>",
    ] {
        let xml = format!(
            "<CDXML><page>{orphan}<graphic GraphicType='Rectangle' BoundingBox='0 0 20 30'/></page></CDXML>"
        );
        let (graphics, notice) = read(&xml);
        assert_eq!(graphics.len(), 1);
        assert!(notice.unwrap().contains("Left out:"));
    }
}

#[test]
fn omitted_numeric_fields_use_an_explicitly_noticed_fallback() {
    let xml = plate("<tlcspot Width='65536' Height='65536'/>", "")
        .replace(" OriginFraction='0.1' SolventFrontFraction='0.1'", "");
    let (graphics, notice) = read(&xml);
    assert!(notice.unwrap().contains("TLC plate"));
    let t = graphics[0].tlc.as_deref().unwrap();
    assert_eq!(t.origin_fraction, 0.1);
    assert_eq!(t.solvent_front_fraction, 0.1);
    assert_eq!(t.lanes[0].spots[0].rf, 0.);
}

#[test]
fn rf_rotation_exception_stays_confined_to_a_native_plate_label() {
    let xml = "<CDXML><page><objecttag Name='rf'><t p='10 20' RotationAngle='1966080'><s size='9'>other tag</s></t></objecttag></page></CDXML>";
    let (normalized, notice) = super::super::super::approximate::approximate(xml);
    assert!(!normalized.contains("RotationAngle"));
    assert!(notice.unwrap().contains("rotated caption upright"));
}
