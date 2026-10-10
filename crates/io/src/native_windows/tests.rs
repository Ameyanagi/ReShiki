use super::*;

#[test]
fn windows_print_keeps_vector_text_page_offsets_and_physical_size() {
    let mut doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    doc.page_layout = Some(crate::pages::Layout {
        columns: 2,
        ..crate::pages::Layout::around(&doc)
    });
    let result: Value = serde_json::from_slice(&print_snapshot(&doc).unwrap()).unwrap();
    assert_eq!(result["version"], 1);
    assert_eq!(result["pages"].as_array().unwrap().len(), 2);
    assert!(result["width_pt"].as_f64().unwrap() > 500.);
    assert!(
        result["primitives"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["fill"].is_array())
    );
    assert!(
        result["primitives"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["kind"] == "path")
    );
    let first = result["pages"][0][0].as_f64().unwrap();
    let second = result["pages"][1][0].as_f64().unwrap();
    assert!(first - second > 500.);
}

#[test]
fn windows_print_keeps_group_alpha_and_emf_reports_partial_alpha_loss() {
    let mut doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk"
    ))
    .unwrap();
    let ids = doc.all_ids();
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.5).unwrap();
    doc.page_layout = Some(crate::pages::Layout::around(&doc));
    let snapshot: Value = serde_json::from_slice(&print_snapshot(&doc).unwrap()).unwrap();
    let primitives = snapshot["primitives"].as_array().unwrap();
    assert!(
        primitives
            .iter()
            .any(|p| p["fill"][3] == 128 || p["stroke"]["color"][3] == 128)
    );
    assert!(
        primitives.iter().any(|p| {
            p["fill"] == json!([0, 0, 0, 255]) || p["stroke"]["color"] == json!([0, 0, 0, 255])
        }),
        "Exposed cage ink must stay opaque; the page background is not sufficient"
    );
    let error = office_metafile(&doc).unwrap_err();
    assert!(error.contains("partial transparency"));
    assert!(error.contains("SVG"));
}

#[test]
fn windows_print_keeps_exposed_open_chain_opaque_and_emf_accepts_it() {
    let mut doc = Document::default();
    let ids: Vec<_> = [(0., 0., -20.), (80., 0., -20.), (120., 80., 20.)]
        .into_iter()
        .map(|(x, y, z)| {
            let id = doc.add_atom("C", crate::document::Point::new(x, y));
            doc.atom_mut(id).unwrap().depth = z;
            id
        })
        .collect();
    for pair in ids.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    doc.page_layout = Some(crate::pages::Layout::around(&doc));
    let opaque = print_snapshot(&doc).unwrap();
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.5).unwrap();
    let actual = print_snapshot(&doc).unwrap();
    assert_eq!(
        actual, opaque,
        "Exposed chain ink must match the opaque reference"
    );
    let snapshot: Value = serde_json::from_slice(&actual).unwrap();
    let colors: Vec<_> = snapshot["primitives"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| [&p["fill"], &p["stroke"]["color"]])
        .filter(|color| color.is_array())
        .collect();
    assert!(
        colors.iter().any(|color| **color == json!([0, 0, 0, 255])),
        "Missing actual chain ink"
    );
    assert!(colors.iter().all(|color| color[3] == 255));

    let emf = office_metafile(&doc).unwrap();
    assert!(emf.len() >= 88, "Missing EMF header");
    let u32_at = |offset| u32::from_le_bytes(emf[offset..offset + 4].try_into().unwrap());
    assert_eq!(u32_at(0), 1, "Missing EMR_HEADER");
    assert_eq!(u32_at(40), 0x464d4520, "Missing EMF signature");
    assert_eq!(u32_at(48) as usize, emf.len(), "EMF byte count");
    // The producer records GdipFillPath/GdipDrawPath as EMF+ dual. Inspect
    // its GDI fallback, requiring painted geometry rather than BEGINPATH.
    let (mut offset, mut records, mut painted_geometry) = (0, 0, 0);
    let (mut path_open, mut path_geometry, mut eof) = (false, false, false);
    while offset < emf.len() {
        assert!(offset + 8 <= emf.len(), "Truncated EMF record");
        let kind = u32_at(offset);
        let size = u32_at(offset + 4) as usize;
        assert!(
            size >= 8 && size.is_multiple_of(4) && size <= emf.len() - offset,
            "Invalid EMF record size"
        );
        let geometry = match kind {
            // EMR_LINETO, or point arrays for Bezier/polygon/polyline records.
            54 => {
                assert!(size >= 16, "Truncated line geometry");
                true
            }
            2..=6 | 85..=89 => {
                assert!(size >= 28, "Truncated point-array geometry");
                let points = u32_at(offset + 24) as usize;
                let point_size = if kind >= 85 { 4 } else { 8 };
                let valid_points = match kind {
                    2 | 85 => points >= 4 && (points - 1).is_multiple_of(3),
                    5 | 88 => points >= 3 && points.is_multiple_of(3),
                    6 | 89 => points >= 1,
                    _ => points >= 2,
                };
                assert!(
                    valid_points && points <= (size - 28) / point_size,
                    "Invalid or truncated geometry"
                );
                true
            }
            // EMR_POLYPOLYLINE/POLYPOLYGON and their 16-bit variants.
            7 | 8 | 90 | 91 => {
                assert!(size >= 32, "Truncated multiple-path geometry");
                let polygons = u32_at(offset + 24) as usize;
                let points = u32_at(offset + 28) as usize;
                assert!(
                    polygons > 0 && polygons <= (size - 32) / 4,
                    "Invalid path count"
                );
                let point_start = 32 + polygons * 4;
                let point_size = if kind >= 90 { 4 } else { 8 };
                assert!(
                    points > 0 && points <= (size - point_start) / point_size,
                    "Empty or truncated geometry"
                );
                let mut total = 0;
                for i in 0..polygons {
                    let count = u32_at(offset + 32 + i * 4) as usize;
                    assert!(
                        count >= 2 && count <= points - total,
                        "Invalid subpath geometry"
                    );
                    total += count;
                }
                assert_eq!(total, points, "Inconsistent geometry point count");
                true
            }
            _ => false,
        };
        if geometry {
            if path_open {
                path_geometry = true;
            } else {
                painted_geometry += 1;
            }
        }
        match kind {
            59 => {
                // EMR_BEGINPATH opens construction but paints nothing.
                assert!(!path_open, "Nested path construction");
                path_open = true;
                path_geometry = false;
            }
            60 => {
                // EMR_ENDPATH selects the completed path.
                assert!(path_open, "Unopened path construction");
                path_open = false;
            }
            62..=64 => {
                // EMR_FILLPATH/STROKEANDFILLPATH/STROKEPATH.
                assert!(size >= 24 && !path_open, "Invalid path paint record");
                if path_geometry {
                    painted_geometry += 1;
                }
                path_geometry = false;
            }
            67 | 68 => {
                // Selecting a clip or aborting consumes unpainted geometry.
                path_geometry = false;
                if kind == 68 {
                    path_open = false;
                }
            }
            14 => {
                // EMR_EOF must be the final complete record.
                assert!(size >= 20 && !path_open, "Invalid EOF record");
                assert_eq!(offset + size, emf.len(), "Records after EOF");
                assert_eq!(u32_at(offset + size - 4) as usize, size, "EOF SizeLast");
                eof = true;
            }
            _ => {}
        }
        records += 1;
        offset += size;
    }
    assert!(eof, "Missing EOF");
    assert_eq!(records, u32_at(52), "EMF record count");
    assert!(painted_geometry > 0, "Missing painted vector geometry");
}
