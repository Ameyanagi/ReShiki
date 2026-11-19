//! The editor's PNG preview of a drawing, shared by the assistant's canvas
//! tools, the MCP `render` and `convert` tools, the CLI and the browser
//! library, so every surface draws the same pixels.
use crate::export;
use reshiki_model::{document::Document, scene};

/// How far [`png`] scales a small drawing up.
pub const MAX_UPSCALE: f32 = 3.;

/// Renders `document` on white, scaled up to [`MAX_UPSCALE`] and down to fit
/// within `max_width` x `max_height` pixels and `max_pixels` in all.
pub fn png(
    document: &Document,
    max_width: u32,
    max_height: u32,
    max_pixels: u64,
) -> Result<Vec<u8>, String> {
    // f32::clamp panics when its minimum exceeds its maximum.
    if max_width == 0 || max_height == 0 {
        return Err("Preview size must be at least 1 × 1 pixels".into());
    }
    let tree = export::parse_svg(scene::svg(document))?;
    let size = tree.size();
    let budget = (max_pixels as f32 / (size.width() * size.height())).sqrt();
    let scale = (max_width as f32 / size.width())
        .min(max_height as f32 / size.height())
        .min(MAX_UPSCALE)
        .min(budget);
    let width = (size.width() * scale).ceil().clamp(1., max_width as f32) as u32;
    let height = (size.height() * scale).ceil().clamp(1., max_height as f32) as u32;
    let mut pixels =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or("Could not create canvas image")?;
    pixels.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixels.as_mut(),
    );
    pixels.encode_png().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki_model::document::Point;

    fn ethanol() -> Document {
        let mut doc = Document::default();
        let atoms = [
            doc.add_atom("C", Point::new(0., 0.)),
            doc.add_atom("C", Point::new(42., 0.)),
            doc.add_atom("O", Point::new(84., 0.)),
        ];
        doc.add_bond(atoms[0], atoms[1], 1, "plain");
        doc.add_bond(atoms[1], atoms[2], 1, "plain");
        doc
    }

    /// The width and height in a PNG's IHDR chunk.
    fn size(png: &[u8]) -> (u32, u32) {
        let word = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
        (word(16), word(20))
    }

    /// A pixel budget that the size limits already keep is no limit: the
    /// MCP render tool, whose sizes fit its budget, draws the same bytes as
    /// before the budget moved into this function.
    #[test]
    fn a_budget_the_size_keeps_changes_nothing() {
        let doc = ethanol();
        let unlimited = png(&doc, 1600, 1000, u64::MAX).unwrap();
        assert_eq!(png(&doc, 1600, 1000, 16_000_000).unwrap(), unlimited);
        assert_eq!(png(&doc, 1600, 1000, 1_600_000).unwrap(), unlimited);
    }

    /// The browser library allows 8192 × 8192 and relies on the budget.
    #[test]
    fn the_pixel_budget_scales_a_large_preview_down() {
        let doc = ethanol();
        let (width, height) = size(&png(&doc, 8192, 8192, 10_000).unwrap());
        assert!(
            u64::from(width) * u64::from(height) <= 10_500,
            "{width} × {height}"
        );
        let (wide, tall) = size(&png(&doc, 8192, 8192, u64::MAX).unwrap());
        assert!(wide > width && tall > height);
    }

    #[test]
    fn an_empty_size_is_refused() {
        let doc = ethanol();
        assert!(png(&doc, 0, 10, u64::MAX).is_err());
        assert!(png(&doc, 10, 0, u64::MAX).is_err());
    }
}
