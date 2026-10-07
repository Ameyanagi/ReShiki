use anyhow::Context;
use reshiki::{
    document::{Document, Point},
    engine::LocalEngine,
    export,
};

/// A test-local replica of the app's figure export (src/app/figure_export.rs)
/// up to the save dialog: the prepared bytes and the status details.
async fn sequence(
    engine: &LocalEngine,
    doc: Document,
    format: &'static str,
    pages: bool,
) -> Result<(Vec<u8>, Vec<String>), String> {
    let (doc, notice) = export::figure_document(engine, doc).await?;
    let figure = tokio::task::spawn_blocking(move || {
        if pages {
            export::pages_pdf(&doc).map(|bytes| export::Figure {
                bytes,
                detail: None,
            })
        } else {
            export::figure(&doc, format)
        }
    })
    .await
    .map_err(|error| error.to_string())??;
    let details = figure.detail.into_iter().chain(notice).collect();
    Ok((figure.bytes, details))
}

/// Publication inputs as (case, drawing, format, pages). The aromatic 5-ring
/// stays unresolved, so its analysis fails and exports carry the review notice.
fn publication_cases() -> anyhow::Result<Vec<(&'static str, Document, &'static str, bool)>> {
    let mut ring = Document::default();
    reshiki::editing::ring(&mut ring, Point::default(), 5, true, 42.);
    let mut invalid = ring.clone();
    invalid.bonds.first_mut().context("ring bond")?.b = u64::MAX;
    let mut cases = vec![
        ("png", ring.clone(), "png", false),
        ("svg", ring.clone(), "svg", false),
        ("pages without layout", ring.clone(), "pdf", true),
        ("invalid png", invalid.clone(), "png", false),
        ("invalid pages", invalid, "pdf", true),
    ];
    if cfg!(not(windows)) {
        cases.push(("emf", ring, "emf", false));
    }
    Ok(cases)
}

#[tokio::test]
async fn publication_sequence_characterization() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    for (case, doc, format, pages) in publication_cases()? {
        let (_, notice) = export::figure_document(&engine, doc.clone())
            .await
            .unwrap_or_default();
        let result = sequence(&engine, doc, format, pages).await;
        match case {
            "png" => {
                let (bytes, details) = result.map_err(anyhow::Error::msg)?;
                assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
                let [size, review] = details.as_slice() else {
                    anyhow::bail!("PNG details: {details:?}");
                };
                assert!(size.starts_with("PNG: "), "{size}");
                assert!(
                    review.starts_with("Drawing preserved; chemistry needs review: "),
                    "{review}"
                );
            }
            "svg" => {
                let (_, details) = result.map_err(anyhow::Error::msg)?;
                assert_eq!(details, [notice.context("review notice")?]);
            }
            "pages without layout" => assert_eq!(
                result,
                Err("Set up publication pages before exporting a page PDF.".into())
            ),
            "invalid png" | "invalid pages" => assert!(result.is_err(), "{case}"),
            "emf" => assert_eq!(result, Err("Unsupported drawing export".into())),
            _ => anyhow::bail!("Uncharacterized publication case {case}"),
        }
    }
    Ok(())
}

#[tokio::test]
async fn unresolved_aromatic_drawing_exports_without_assigning_chemistry() -> anyhow::Result<()> {
    let mut doc = Document::default();
    reshiki::editing::ring(&mut doc, Point::default(), 5, true, 42.);
    let engine = Default::default();
    assert!(
        export::checked_document(&engine, doc.clone())
            .await
            .is_err()
    );
    let (prepared, notice) = export::figure_document(&engine, doc.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        prepared, doc,
        "Do not invent a charge or discard aromatic bonds"
    );
    assert!(
        notice
            .context("review notice")?
            .contains("chemistry needs review")
    );
    for format in ["svg", "pdf", "png"] {
        let bytes = export::drawing(&prepared, format).map_err(anyhow::Error::msg)?;
        assert!(!bytes.is_empty(), "{format}");
        if format == "svg" {
            assert_eq!(
                String::from_utf8(bytes)?,
                reshiki::scene::svg_with_background(&doc)
            );
        } else if format == "pdf" {
            assert!(bytes.starts_with(b"%PDF-"));
            assert!(String::from_utf8_lossy(&bytes).contains("/MediaBox"));
        } else {
            let image = image::load_from_memory(&bytes)?.into_rgba8();
            assert!(image.pixels().any(|p| p.0 == [0, 0, 0, 255]));
            assert!(image.pixels().all(|p| p.0.last() == Some(&255)));
        }
    }
    let mut pages = prepared;
    pages.page_layout = Some(reshiki::pages::Layout::default());
    let (pages, notice) = export::figure_document(&engine, pages)
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(notice.is_some());
    let bytes = export::pages_pdf(&pages).map_err(anyhow::Error::msg)?;
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(
        String::from_utf8_lossy(&bytes).matches("/MediaBox").count(),
        1
    );
    // Invalid graph references remain hard failures, even for a figure.
    doc.bonds.first_mut().context("ring bond")?.b = u64::MAX;
    assert!(export::figure_document(&engine, doc).await.is_err());
    Ok(())
}

#[test]
fn whole_gallery_png_is_bounded_and_preserves_publication_size() -> anyhow::Result<()> {
    let doc: Document =
        serde_json::from_str(include_str!("../assets/examples/shortcut-examples.rsk"))?;
    let before = doc.clone();
    let figure = export::figure(&doc, "png").map_err(anyhow::Error::msg)?;
    let reader = png::Decoder::new(std::io::Cursor::new(&figure.bytes)).read_info()?;
    let info = reader.info();
    assert!(u64::from(info.width) * u64::from(info.height) <= 80_000_000);
    let density = info.pixel_dims.context("physical density")?;
    assert_eq!(density.unit, png::Unit::Meter);
    assert_eq!(density.xppu, density.yppu);
    let dpi = f64::from(density.xppu) * 0.0254;
    assert!((dpi - 300.).abs() < 0.02);
    assert!(
        figure
            .detail
            .context("resolution receipt")?
            .contains("300 dpi")
    );
    let svg = reshiki::scene::svg_with_background(&doc);
    let xml = roxmltree::Document::parse(&svg)?;
    for (attr, pixels) in [("width", info.width), ("height", info.height)] {
        let pt = xml
            .root_element()
            .attribute(attr)
            .context("SVG physical size")?
            .trim_end_matches("pt")
            .parse::<f64>()?;
        assert!((f64::from(pixels) / dpi - pt / 72.).abs() < 0.005, "{attr}");
    }
    // Decode all rows, rather than accepting a PNG signature or header alone.
    let image = image::load_from_memory(&figure.bytes)?.into_rgba8();
    assert!(image.pixels().any(|p| p.0 == [0, 0, 0, 255]));
    assert_eq!(doc, before);
    if cfg!(windows) {
        // Windows bounds its preview for DIB/Office/native picture limits,
        // while retaining the gallery's publication size and transparency.
        let bytes = export::clipboard_png(&doc).map_err(anyhow::Error::msg)?;
        let reader = png::Decoder::new(std::io::Cursor::new(&bytes)).read_info()?;
        let info = reader.info();
        assert!(u64::from(info.width) * u64::from(info.height) <= reshiki::pictures::MAX_PIXELS);
        assert!(info.width <= 8192 && info.height <= 8192);
        let density = info.pixel_dims.context("clipboard physical density")?;
        assert_eq!(density.unit, png::Unit::Meter);
        assert_eq!(density.xppu, density.yppu);
        let dpi = f64::from(density.xppu) * 0.0254;
        assert!(dpi > 0. && dpi < 1200.);
        let svg = reshiki::scene::svg(&doc);
        let xml = roxmltree::Document::parse(&svg)?;
        for (attr, pixels) in [("width", info.width), ("height", info.height)] {
            let pt = xml
                .root_element()
                .attribute(attr)
                .context("clipboard SVG physical size")?
                .trim_end_matches("pt")
                .parse::<f64>()?;
            assert!((f64::from(pixels) / dpi - pt / 72.).abs() < 0.02, "{attr}");
        }
        let image = image::load_from_memory(&bytes)?.into_rgba8();
        assert!(image.pixels().any(|p| p.0 == [0, 0, 0, 255]));
        assert!(image.pixels().any(|p| p.0[3] == 0));
    } else {
        // macOS retains fixed 1200 dpi clipboard sizing and its prior limit.
        assert!(export::clipboard_png(&doc).is_err());
    }
    assert_eq!(doc, before);
    Ok(())
}

#[tokio::test]
async fn ordinary_figures_still_refresh_labels_and_export_at_1200_dpi() -> anyhow::Result<()> {
    let doc: Document = serde_json::from_str(include_str!("fixtures/ui-drawn-ethanol.reshiki"))?;
    let engine = Default::default();
    let checked = export::checked_document(&engine, doc.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let (prepared, notice) = export::figure_document(&engine, doc)
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(prepared, checked);
    assert_eq!(notice, None);
    let figure = export::figure(&prepared, "png").map_err(anyhow::Error::msg)?;
    let reader = png::Decoder::new(std::io::Cursor::new(figure.bytes)).read_info()?;
    assert_eq!(reader.info().pixel_dims.context("density")?.xppu, 47244);
    assert!(figure.detail.context("receipt")?.contains("1200 dpi"));
    Ok(())
}
