use super::*;
use crate::{
    document::Point,
    graphics::{
        Graphic, GraphicKind, GraphicStyle,
        table::{Cell, Frame, Table},
    },
    palette::Color,
};
use std::io::Read as _;

fn source() -> (Document, Frame) {
    let mut doc = Document::default();
    doc.graphics.push(Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::new(-200., -200.),
        Point::new(400., 400.),
        GraphicStyle {
            fill: Some(Color::Custom([200, 30, 60])),
            ..Default::default()
        },
        Default::default(),
        false,
    ));
    let mut g = Graphic::dragged(
        2,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(100., 80.),
        Default::default(),
        Default::default(),
        false,
    );
    g.kind = GraphicKind::Table;
    g.origin = Point::new(40., 20.);
    g.axis_x = Point::new(120., 30.);
    g.axis_y = Point::new(-20., 90.);
    let cell = Cell {
        id: 1,
        lo: Point::default(),
        hi: Point::new(1., 1.),
        members: vec![1],
        borders: vec![],
    };
    let frame = cell.frame(&g).unwrap();
    g.table = Some(Box::new(Table { cells: vec![cell] }));
    doc.graphics.push(g);
    doc.validate().unwrap();
    (doc, frame)
}

#[test]
fn table_png_and_office_svg_clip_affine_contents_and_keep_native_payload_whole() {
    let (doc, frame) = source();
    let drawing = serde_json::to_vec(&doc.current()).unwrap();
    let (lo, hi) = scene::bounds(&scene::primitives(&doc));
    for format in ["png", "svg-office"] {
        let exported = clipboard_figure_with_drawing(
            &doc,
            format,
            Background::Transparent,
            300,
            Some(&drawing),
        )
        .unwrap();
        let decoded = crate::payload::read(&exported.bytes).unwrap().unwrap();
        assert_eq!(
            decoded,
            crate::payload::visible(&doc.current()),
            "Native geometry must remain complete in {format}"
        );
        let (width, height, pixels) = if format == "png" {
            let mut reader = png::Decoder::new(std::io::Cursor::new(&exported.bytes))
                .read_info()
                .unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut pixels).unwrap();
            assert_eq!(info.color_type, png::ColorType::Rgba);
            pixels.truncate(info.buffer_size());
            (info.width, info.height, pixels)
        } else {
            let svg = std::str::from_utf8(&exported.bytes).unwrap();
            assert!(
                svg.contains("<clipPath"),
                "Office SVG must retain vector clipping after outlining"
            );
            let tree = parse_svg(svg.into()).unwrap();
            let mut pix = resvg::tiny_skia::Pixmap::new(
                tree.size().width().ceil() as u32,
                tree.size().height().ceil() as u32,
            )
            .unwrap();
            resvg::render(&tree, Default::default(), &mut pix.as_mut());
            (pix.width(), pix.height(), pix.data().to_vec())
        };
        let mut ink = 0;
        for (i, pixel) in pixels.chunks_exact(4).enumerate() {
            if pixel[3] < 100 {
                continue;
            }
            ink += 1;
            let p = Point::new(
                lo.x + (i as u32 % width) as f32 / width as f32 * (hi.x - lo.x),
                lo.y + (i as u32 / width) as f32 / height as f32 * (hi.y - lo.y),
            );
            let p = frame.local(p).unwrap();
            assert!(
                (-0.04..=1.04).contains(&p.x) && (-0.04..=1.04).contains(&p.y),
                "{format} exposed affine content outside the cell: {p:?}"
            );
        }
        assert!(ink > 1000, "{format} lost the cell ink");
    }
}

#[test]
fn table_pdf_keeps_a_vector_clipping_operator_for_file_and_clipboard_exports() {
    let (doc, _) = source();
    for pdf in [
        drawing(&doc, "pdf").unwrap(),
        clipboard_drawing(&doc, "pdf").unwrap(),
    ] {
        assert!(
            !pdf.windows(b"/Subtype /Image".len())
                .any(|w| w == b"/Subtype /Image"),
            "Native table clipping must stay vector in PDF"
        );
        let mut rest = pdf.as_slice();
        let mut clipped = false;
        while let Some(start) = rest.windows(7).position(|w| w == b"stream\n") {
            rest = &rest[start + 7..];
            let Some(end) = rest.windows(10).position(|w| w == b"\nendstream") else {
                break;
            };
            let stream = &rest[..end];
            let mut decoded = Vec::new();
            let text = if flate2::read::ZlibDecoder::new(stream)
                .read_to_end(&mut decoded)
                .is_ok()
            {
                decoded.as_slice()
            } else {
                stream
            };
            clipped |= String::from_utf8_lossy(text)
                .split_whitespace()
                .any(|word| matches!(word, "W" | "W*"));
            rest = &rest[end + 10..];
        }
        assert!(
            clipped,
            "PDF dropped the native table's affine content clip"
        );
    }
}
