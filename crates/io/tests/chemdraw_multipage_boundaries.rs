use reshiki_io::{chemistry::cdxml, engine::LocalEngine, exchange, formats};
use reshiki_model::{document::Document, scene};
use std::path::Path;

#[test]
fn binary_coordinate_xml_bridge_keeps_every_signed_fixed_point_tick() {
    for ticks in [
        [i32::MIN, i32::MAX, 1, -1],
        [-65535, 65535, -1, 1],
        [0, 1, 2, 3],
    ] {
        let coordinates = ticks.map(|tick| (f64::from(tick) / 65536.).to_string());
        let xml = format!(
            "<CDXML><page id='1'><fragment id='2'><n id='3' Element='6' p='{} {}' xyz='{} {} {}'/></fragment>\
             <graphic id='4' GraphicType='Rectangle' BoundingBox='{}'/></page></CDXML>",
            coordinates[0],
            coordinates[1],
            coordinates[0],
            coordinates[1],
            coordinates[2],
            coordinates.join(" "),
        );
        let decoded = exchange::from_cdx(&exchange::to_cdx(&xml).unwrap()).unwrap();
        let tree = roxmltree::Document::parse(&decoded).unwrap();
        for (tag, attribute, count) in [
            ("n", "p", 2),
            ("n", "xyz", 3),
            ("graphic", "BoundingBox", 4),
        ] {
            let actual = tree
                .descendants()
                .find(|node| node.has_tag_name(tag))
                .unwrap()
                .attribute(attribute)
                .unwrap()
                .split_whitespace()
                .map(|value| (value.parse::<f64>().unwrap() * 65536.).trunc() as i32)
                .collect::<Vec<_>>();
            assert_eq!(actual, ticks[..count], "{attribute}: {decoded}");
        }
    }
}

async fn fixture(name: &str, format: &str) -> (Document, String) {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/chemdraw-page-boundaries");
    let path = root.join(format!(
        "{name}.{}",
        if format == "cdxml" {
            "cdxml"
        } else {
            "drawing.cds"
        }
    ));
    let bytes = std::fs::read(&path).unwrap();
    let xml = if format == "cdxml" {
        String::from_utf8(bytes.clone()).unwrap()
    } else {
        exchange::from_cdx(&bytes).unwrap()
    };
    let (document, warnings) =
        formats::read_with_warnings(bytes, format, &LocalEngine::in_process())
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert_eq!(
        warnings
            .iter()
            .filter(|w| w.contains("ChemDraw pages"))
            .count(),
        usize::from(name.ends_with("two-pages"))
    );
    (document, xml)
}

fn page(document: &Document, number: usize, atoms: usize, captions: usize) -> Document {
    let mut page = document.clone();
    page.atoms = document
        .atoms
        .iter()
        .skip(number * atoms)
        .take(atoms)
        .cloned()
        .collect();
    page.annotations = document
        .annotations
        .iter()
        .skip(number * captions)
        .take(captions)
        .cloned()
        .collect();
    page.bonds.retain(|bond| {
        page.atoms.iter().any(|a| a.id == bond.a) && page.atoms.iter().any(|a| a.id == bond.b)
    });
    page.groups.clear();
    page.validate().unwrap();
    page
}

fn check_pages(single: &Document, combined: &Document) {
    let atoms = single.atoms.len();
    let captions = single.annotations.len();
    assert_eq!(combined.atoms.len(), 2 * atoms);
    assert_eq!(combined.annotations.len(), 2 * captions);
    let first = page(combined, 0, atoms, captions);
    let second = page(combined, 1, atoms, captions);
    let (_, first_hi) = scene::bounds(&scene::primitives(&first));
    let (second_lo, _) = scene::bounds(&scene::primitives(&second));
    assert!(
        second_lo.y > first_hi.y,
        "page contents overlap: {first_hi:?} / {second_lo:?}"
    );
    let origin = |page: &Document| {
        page.atoms
            .first()
            .map(|a| a.position)
            .or_else(|| page.annotations.first().map(|a| a.position))
            .unwrap()
    };
    let expected = origin(single);
    for page in [first, second] {
        let actual = origin(&page);
        for (atom, source) in page.atoms.iter().zip(&single.atoms) {
            assert_eq!(atom.element, source.element);
            let actual_style = atom
                .text_style
                .clone()
                .unwrap_or_else(|| page.drawing_style.text_style());
            let expected_style = source
                .text_style
                .clone()
                .unwrap_or_else(|| single.drawing_style.text_style());
            assert_eq!(actual_style.family, expected_style.family);
            assert_eq!(actual_style.size_pt, expected_style.size_pt);
            assert_eq!(actual_style.bold, expected_style.bold);
            assert_eq!(actual_style.italic, expected_style.italic);
            assert!(
                atom.position
                    .offset(-actual.x, -actual.y)
                    .distance(source.position.offset(-expected.x, -expected.y))
                    < 0.002
            );
        }
        for (caption, source) in page.annotations.iter().zip(&single.annotations) {
            assert_eq!(caption.format, source.format);
            assert!(
                caption
                    .position
                    .offset(-actual.x, -actual.y)
                    .distance(source.position.offset(-expected.x, -expected.y))
                    < 0.002
            );
        }
    }
}

async fn roundtrip_pages(single: &Document, combined: &Document) {
    let engine = LocalEngine::in_process();
    for format in ["cdxml", "cdx", "cds"] {
        let bytes = if format == "cds" {
            let xml = formats::write_with_engine(combined.clone(), "cdxml", &engine)
                .await
                .unwrap();
            exchange::to_cds(std::str::from_utf8(&xml).unwrap()).unwrap()
        } else {
            formats::write_with_engine(combined.clone(), format, &engine)
                .await
                .unwrap()
        };
        let mut restored =
            formats::read_with_engine(bytes, if format == "cds" { "cdx" } else { format }, &engine)
                .await
                .unwrap();
        // External fragment/group order may change. Match these fixtures by
        // their geometric page order rather than imposing stable external IDs.
        restored.atoms.sort_by(|a, b| {
            a.position
                .y
                .total_cmp(&b.position.y)
                .then(a.position.x.total_cmp(&b.position.x))
        });
        restored
            .annotations
            .sort_by(|a, b| a.position.y.total_cmp(&b.position.y));
        check_pages(single, &restored);
    }
}

#[tokio::test]
async fn boxless_captions_use_effective_line_height_and_wrapping_before_page_placement() {
    for name in ["line-height", "wrapped-caption"] {
        for format in ["cdxml", "cdx"] {
            let (single, _) = fixture(&format!("{name}-single"), format).await;
            let (combined, _) = fixture(&format!("{name}-two-pages"), format).await;
            assert_eq!(single.annotations[0].format.line_spacing, 3.);
            check_pages(&single, &combined);
            roundtrip_pages(&single, &combined).await;
        }
    }
}

#[tokio::test]
async fn large_default_and_explicit_atom_labels_do_not_cross_page_boundaries() {
    for label in ["default", "explicit", "above", "below"] {
        for format in ["cdxml", "cdx"] {
            let (single, _) = fixture(&format!("large-label-{label}-single"), format).await;
            let (combined, _) = fixture(&format!("large-label-{label}-two-pages"), format).await;
            let atom = single.atoms.first().unwrap();
            assert_eq!(
                atom.text_style
                    .as_ref()
                    .map_or(single.drawing_style.font_size_pt, |style| style.size_pt),
                72.
            );
            check_pages(&single, &combined);
            roundtrip_pages(&single, &combined).await;
        }
    }
}

#[tokio::test]
async fn native_xyz_pages_retain_single_page_geometry_at_default_and_nondefault_scales() {
    for name in [
        "xyz-origin",
        "fractional-xyz",
        "mixed-xyz",
        "hybrid-fragments",
    ] {
        for format in ["cdxml", "cdx"] {
            let (single, xml) = fixture(&format!("{name}-single"), format).await;
            let (combined, arranged) = fixture(&format!("{name}-two-pages"), format).await;
            check_pages(&single, &combined);
            roundtrip_pages(&single, &combined).await;
            let before = cdxml::prepare_cdxml(&xml).unwrap();
            let after = cdxml::prepare_cdxml(&arranged).unwrap();
            assert_eq!(after.conformer_3d, before.conformer_3d);
            let original = &before.molecule.positions;
            for positions in after.molecule.positions.chunks_exact(original.len()) {
                for (point, expected) in positions.iter().zip(original) {
                    assert_eq!(point.z, expected.z);
                    assert!(
                        ((point.x - positions[0].x) - (expected.x - original[0].x)).abs() < 1e-9
                    );
                    assert!(
                        ((point.y - positions[0].y) - (expected.y - original[0].y)).abs() < 1e-9
                    );
                }
            }
        }
    }
}
