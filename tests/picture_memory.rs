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
