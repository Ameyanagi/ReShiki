//! Opt-in public picture workloads and exact pre-change output captures.
//! The allocator belongs only to this integration-test binary. Measurements
//! count requested Rust heap bytes, not allocator slack, RSS, or GPU storage.
use iced::widget::image::Handle;
use image::{ImageFormat, Rgba, RgbaImage};
use reshiki::pictures::{Picture, exchange};
use reshiki_process_heap::allocation_metrics;
use std::{
    alloc::System,
    io::{Cursor, Write},
    path::PathBuf,
    time::Instant,
};

#[global_allocator]
static ALLOCATOR: allocation_metrics::MeasuredAllocator<System> =
    allocation_metrics::MeasuredAllocator::new(System);

fn observe<T>(name: &str, operation: impl FnOnce() -> T) -> T {
    let before = allocation_metrics::reset();
    let result = operation();
    let snapshot = allocation_metrics::snapshot();
    let peak = snapshot.peak_bytes.saturating_sub(before);
    let retained = snapshot.live_bytes.saturating_sub(before);
    let allocated = snapshot.allocated_bytes;
    let allocations = snapshot.allocation_count;
    println!("PICTURE_MEMORY,{name},{peak},{retained},{allocated},{allocations}");
    result
}

fn raster(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        Rgba([
            (x * 19 + y * 7) as u8,
            (x * 11 + y * 31) as u8,
            (x * 41 + y * 13) as u8,
            [0, 1, 127, 128, 254, 255][((x + y * 3) % 6) as usize],
        ])
    })
}

fn png(pixels: &RgbaImage) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    pixels.write_to(&mut output, ImageFormat::Png).unwrap();
    output.into_inner()
}

fn associated_tiff(width: u32, height: u32, orientation: u16) -> Vec<u8> {
    let mut pixels = raster(width, height);
    for pixel in pixels.pixels_mut() {
        let alpha = u16::from(pixel.0[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    let mut output = Cursor::new(Vec::new());
    {
        let mut encoder = tiff::encoder::TiffEncoder::new(&mut output).unwrap();
        let mut image = encoder
            .new_image::<tiff::encoder::colortype::RGBA8>(width, height)
            .unwrap();
        image
            .encoder()
            .write_tag(tiff::tags::Tag::ExtraSamples, &[1u16][..])
            .unwrap();
        image
            .encoder()
            .write_tag(tiff::tags::Tag::Orientation, orientation)
            .unwrap();
        image.write_data(pixels.as_raw()).unwrap();
    }
    output.into_inner()
}

fn png16_sources() -> [(String, Vec<u8>); 4] {
    let value = |x: u32, y: u32, channel: u32| {
        [0, 1, 255, 256, 511, 32768, 65535][((x + y + channel * 3) % 7) as usize]
    };
    [
        (
            "gray16",
            image::DynamicImage::ImageLuma16(image::ImageBuffer::from_fn(7, 3, |x, y| {
                image::Luma([value(x, y, 0)])
            })),
        ),
        (
            "gray_alpha16",
            image::DynamicImage::ImageLumaA16(image::ImageBuffer::from_fn(7, 3, |x, y| {
                image::LumaA([value(x, y, 0), value(x, y, 1)])
            })),
        ),
        (
            "rgb16",
            image::DynamicImage::ImageRgb16(image::ImageBuffer::from_fn(7, 3, |x, y| {
                image::Rgb([value(x, y, 0), value(x, y, 1), value(x, y, 2)])
            })),
        ),
        (
            "rgba16",
            image::DynamicImage::ImageRgba16(image::ImageBuffer::from_fn(7, 3, |x, y| {
                image::Rgba([
                    value(x, y, 0),
                    value(x, y, 1),
                    value(x, y, 2),
                    value(x, y, 3),
                ])
            })),
        ),
    ]
    .map(|(name, image)| {
        let mut output = Cursor::new(Vec::new());
        image.write_to(&mut output, ImageFormat::Png).unwrap();
        (name.to_owned(), output.into_inner())
    })
}

fn assert_reflected_handle(picture: &Picture, original: &RgbaImage) -> Handle {
    let handle = picture.handle(true).expect("Reflected handle");
    let Handle::Rgba {
        width,
        height,
        pixels,
        ..
    } = &handle
    else {
        panic!("Reflected handle must contain decoded RGBA pixels");
    };
    assert_eq!((*width, *height), original.dimensions());
    for y in 0..*height {
        for x in 0..*width {
            let offset = ((y * width + x) * 4) as usize;
            assert_eq!(
                &pixels[offset..offset + 4],
                &original.get_pixel(x, height - 1 - y).0,
                "Wrong RGBA sample at ({x}, {y})"
            );
        }
    }
    let reused = picture.handle(true).expect("Cached reflected handle");
    assert_eq!(handle.id(), reused.id());
    assert_eq!(handle, reused);
    let shared = picture.clone();
    assert_eq!(handle.id(), shared.handle(true).unwrap().id());
    handle
}

#[test]
fn reflection_preserves_rows_alpha_dimensions_and_shared_handle_identity() {
    for (width, height) in [(1, 1), (1, 5), (7, 5), (8, 6)] {
        let original = raster(width, height);
        let picture = Picture::import(&png(&original)).unwrap();
        let normal = picture.handle(false).expect("Normal handle");
        assert_eq!(normal.id(), picture.handle(false).unwrap().id());
        assert_reflected_handle(&picture, &original);
        assert_eq!(normal.id(), picture.handle(false).unwrap().id());
        assert_eq!(
            image::load_from_memory(picture.png()).unwrap().into_rgba8(),
            original
        );
    }
}

#[test]
#[ignore = "Pre-change byte capture/verification; set mode and baseline directory explicitly"]
fn picture_output_matches_prechange_baseline() {
    let mode = std::env::var("RESHIKI_PICTURE_MEMORY_MODE")
        .expect("Set RESHIKI_PICTURE_MEMORY_MODE=capture or verify");
    assert!(matches!(mode.as_str(), "capture" | "verify"));
    let directory = PathBuf::from(
        std::env::var_os("RESHIKI_PICTURE_MEMORY_BASELINE_DIR")
            .expect("Set RESHIKI_PICTURE_MEMORY_BASELINE_DIR"),
    );
    if mode == "capture" {
        std::fs::create_dir_all(&directory).unwrap();
    }
    let check = |name: &str, bytes: &[u8]| {
        let path = directory.join(name);
        if mode == "capture" {
            // Refuse to replace the original output during a later run.
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            output.write_all(bytes).unwrap();
        } else {
            assert_eq!(bytes, std::fs::read(&path).unwrap(), "{}", path.display());
        }
    };
    for (width, height) in [(1, 1), (1, 5), (7, 5), (8, 6)] {
        let original = raster(width, height);
        let source = png(&original);
        let picture = Picture::import(&source).unwrap();
        let stem = format!("{width}x{height}");
        check(&format!("{stem}-source.png"), &source);
        check(&format!("{stem}-stored.png"), picture.png());
        let exchange =
            exchange::import(&source, "PNG", 0.37, &mut exchange::Budget::default()).unwrap();
        check(&format!("{stem}-opacity.png"), exchange.png());
        check(
            &format!("{stem}-flipped.png"),
            &exchange::export(&picture, true).unwrap(),
        );
        let handle = assert_reflected_handle(&picture, &original);
        let Handle::Rgba { pixels, .. } = handle else {
            unreachable!()
        };
        check(&format!("{stem}-handle.rgba"), &pixels);
    }
    for (name, source) in png16_sources() {
        check(&format!("{name}-source.png"), &source);
        let picture =
            exchange::import(&source, "PNG", 0.37, &mut exchange::Budget::default()).unwrap();
        check(&format!("{name}-stored.png"), picture.png());
    }
    for orientation in 1..=8 {
        let source = associated_tiff(7, 5, orientation);
        check(
            &format!("associated-exif{orientation}-source.tiff"),
            &source,
        );
        let picture =
            exchange::import(&source, "TIFF", 0.37, &mut exchange::Budget::default()).unwrap();
        check(
            &format!("associated-exif{orientation}-stored.png"),
            picture.png(),
        );
    }
    println!("PICTURE_BASELINE,{mode},{}", directory.display());
}

#[test]
#[ignore = "Opt-in allocation observations; run this binary with --test-threads=1"]
fn picture_public_operations_memory() {
    println!(
        "PICTURE_MEMORY,operation,peak_additional_bytes,retained_additional_bytes,allocated_bytes,allocations"
    );
    let stress = png(&raster(2048, 1536));
    for (name, source) in [
        (
            "transparent_fixture",
            include_bytes!("fixtures/assistant-images/rhodium-dimer-transparent.png").as_slice(),
        ),
        ("rgba_3m", stress.as_slice()),
    ] {
        let picture = observe(&format!("{name}/ordinary_import"), || {
            Picture::import(source).unwrap()
        });
        let normalized = observe(&format!("{name}/exchange_opacity"), || {
            exchange::import(source, "PNG", 0.37, &mut exchange::Budget::default()).unwrap()
        });
        let exported = observe(&format!("{name}/reflected_export"), || {
            exchange::export(&picture, true).unwrap()
        });
        let handle = observe(&format!("{name}/first_reflected_handle"), || {
            picture.handle(true).unwrap()
        });
        let reused = observe(&format!("{name}/reused_reflected_handle"), || {
            picture.handle(true).unwrap()
        });
        assert_eq!(handle.id(), reused.id());
        assert_eq!(handle, reused);
        assert_eq!(normalized.width(), picture.width());
        assert_eq!(normalized.height(), picture.height());
        assert!(!exported.is_empty());
    }
    let associated = associated_tiff(1024, 768, 6);
    let picture = observe("associated_tiff_768k/exchange_opacity", || {
        exchange::import(&associated, "TIFF", 0.37, &mut exchange::Budget::default()).unwrap()
    });
    assert_eq!((picture.width(), picture.height()), (768, 1024));
}

fn storage_capacity(picture: Picture) -> (usize, usize) {
    let Handle::Bytes(_, bytes) = picture.handle(false).unwrap() else {
        panic!("Normal picture handle must contain its encoded PNG");
    };
    drop(picture);
    let storage = bytes
        .try_into_mut()
        .expect("The retained handle must be the sole PNG owner");
    (storage.len(), storage.capacity())
}

fn timings<T>(name: &str, iterations: usize, mut operation: impl FnMut() -> T) {
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let result = std::hint::black_box(operation());
        samples.push(start.elapsed().as_nanos());
        drop(result);
    }
    samples.sort_unstable();
    println!(
        "PICTURE_TIMING,{name},{iterations},{},{},{}",
        samples[0],
        samples[iterations / 2],
        samples[iterations - 1],
    );
}

fn noise(width: u32, height: u32) -> RgbaImage {
    let mut state = 0x9e37_79b9u32;
    RgbaImage::from_fn(width, height, |_, _| {
        Rgba(std::array::from_fn(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        }))
    })
}

#[test]
#[ignore = "Opt-in storage capacity and timings; run this binary with --test-threads=1"]
fn picture_storage_capacity_and_timing() {
    let stress = png(&raster(2048, 1536));
    let solid = png(&RgbaImage::from_pixel(2048, 1536, Rgba([180, 50, 55, 127])));
    let near_limit = png(&noise(2000, 2000));
    assert!(near_limit.len() <= reshiki::pictures::MAX_BYTES);
    println!("PICTURE_STORAGE,case,png_len,storage_capacity");
    println!("PICTURE_TIMING,case/operation,iterations,min_ns,median_ns,max_ns");
    for (name, source) in [
        (
            "transparent_fixture",
            include_bytes!("fixtures/assistant-images/rhodium-dimer-transparent.png").as_slice(),
        ),
        ("rgba_3m", stress.as_slice()),
        ("solid_3m", solid.as_slice()),
        ("rgba_near_limit", near_limit.as_slice()),
    ] {
        drop(Picture::import(source).unwrap());
        let picture = observe(&format!("{name}/storage_import"), || {
            Picture::import(source).unwrap()
        });
        let (len, capacity) = storage_capacity(picture);
        println!("PICTURE_STORAGE,{name},{len},{capacity}");
        timings(&format!("{name}/import"), 9, || {
            Picture::import(source).unwrap()
        });
        let picture = Picture::import(source).unwrap();
        let clones: Vec<_> = (0..9).map(|_| picture.clone()).collect();
        assert!(clones.iter().all(|clone| {
            clone.handle(false).unwrap().id() == picture.handle(false).unwrap().id()
        }));
        drop(clones);
        timings(&format!("{name}/reflected_export"), 9, || {
            exchange::export(&picture, true).unwrap()
        });
        // Each first reflection needs its own cache; import and destruction are
        // outside the measured handle operation.
        let mut first_handle_times = Vec::with_capacity(9);
        for _ in 0..9 {
            let picture = Picture::import(source).unwrap();
            let start = Instant::now();
            let handle = std::hint::black_box(picture.handle(true).unwrap());
            first_handle_times.push(start.elapsed().as_nanos());
            assert_eq!(handle.id(), picture.handle(true).unwrap().id());
        }
        first_handle_times.sort_unstable();
        println!(
            "PICTURE_TIMING,{name}/first_reflected_handle,9,{},{},{}",
            first_handle_times[0], first_handle_times[4], first_handle_times[8],
        );
    }
}

fn figure_document() -> reshiki::document::Document {
    use reshiki::{
        document::{Annotation, Document, Point},
        editing::{self, Transform},
        palette::Color,
        typography::TextFormat,
    };
    let mut document: Document =
        serde_json::from_str(include_str!("fixtures/ui-drawn-ethanol.reshiki")).unwrap();
    let picture = Picture::import(&png(&raster(7, 5))).unwrap();
    document
        .graphics
        .push(picture.graphic(10_000, Point::new(150., 70.)));
    editing::transform(&mut document, &[10_000], Transform::Rotate(37.));
    editing::transform(&mut document, &[10_000], Transform::FlipHorizontal);
    for (index, bold) in [false, true].into_iter().enumerate() {
        let mut format = TextFormat::default();
        format.style.bold = bold;
        format.style.italic = true;
        format.style.color = Color::Custom([180, 50, 55]);
        document.annotations.push(Annotation {
            id: 10_001 + index as u64,
            position: Point::new(0., 110. + index as f32 * 40.),
            text: "HNO 東京".into(),
            format,
        });
    }
    document
}

#[test]
#[ignore = "Pre-change figure bytes; set mode and baseline directory explicitly"]
fn figure_output_matches_prechange_baseline() {
    let mode = std::env::var("RESHIKI_PICTURE_MEMORY_MODE").unwrap();
    assert!(matches!(mode.as_str(), "capture" | "verify"));
    let directory = PathBuf::from(std::env::var_os("RESHIKI_PICTURE_MEMORY_BASELINE_DIR").unwrap());
    if mode == "capture" {
        std::fs::create_dir_all(&directory).unwrap();
    }
    let check = |name: &str, bytes: &[u8]| {
        let path = directory.join(name);
        if mode == "capture" {
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            output.write_all(bytes).unwrap();
        } else {
            assert_eq!(bytes, std::fs::read(&path).unwrap(), "{}", path.display());
        }
    };
    let mut document = figure_document();
    for theme in reshiki::canvas_theme::CanvasTheme::ALL {
        document.canvas_theme = theme;
        for clipboard in [false, true] {
            for format in ["svg", "png"] {
                let figure = if clipboard {
                    reshiki::export::clipboard_figure(&document, format)
                } else {
                    reshiki::export::figure(&document, format)
                }
                .unwrap();
                let name = format!("figure-{theme}-{clipboard}.{format}");
                check(&name, &figure.bytes);
                check(
                    &format!("{name}.receipt"),
                    figure.detail.unwrap_or_default().as_bytes(),
                );
                if format == "png" {
                    let decoded = image::load_from_memory(&figure.bytes).unwrap();
                    assert!(decoded.width() > 0 && decoded.height() > 0);
                }
            }
        }
        check(
            &format!("figure-preview-{theme}.png"),
            &reshiki::assistant::canvas_tools::image(&document).unwrap(),
        );
    }
    let gallery: reshiki::document::Document =
        serde_json::from_str(include_str!("../assets/examples/shortcut-examples.rsk")).unwrap();
    let figure = reshiki::export::figure(&gallery, "png").unwrap();
    check("figure-gallery.png", &figure.bytes);
    check("figure-gallery.receipt", figure.detail.unwrap().as_bytes());
    println!("FIGURE_BASELINE,{mode},{}", directory.display());
}

#[test]
#[ignore = "Opt-in figure memory/timings; run this binary with --test-threads=1"]
fn figure_public_operations_memory_and_timing() {
    use reshiki::document::{Document, Point};
    let ordinary = figure_document();
    let gallery: Document =
        serde_json::from_str(include_str!("../assets/examples/shortcut-examples.rsk")).unwrap();
    let picture = Picture::import(&png(&noise(2048, 1536))).unwrap();
    let mut picture_heavy = Document::default();
    picture_heavy
        .graphics
        .push(picture.graphic(1, Point::default()));
    for (name, document) in [
        ("ordinary", ordinary),
        ("gallery", gallery),
        ("picture_heavy", picture_heavy),
    ] {
        drop(reshiki::export::figure(&document, "png").unwrap());
        let figure = observe(&format!("figure_{name}/png"), || {
            reshiki::export::figure(&document, "png").unwrap()
        });
        println!("FIGURE_MEMORY_DETAIL,{name},{}", figure.detail.unwrap());
        timings(&format!("figure_{name}/png"), 5, || {
            reshiki::export::figure(&document, "png").unwrap()
        });
    }
}
