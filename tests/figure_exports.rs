use anyhow::Context;
use reshiki::{
    document::{Document, Point},
    export,
};

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
    // Clipboard sizing still uses 1200 dpi; it must not quietly change scale.
    assert!(export::clipboard_png(&doc).is_err());
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
