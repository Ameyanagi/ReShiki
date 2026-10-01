//! Run AppKit on the real main thread. Save to private PDFs, never a printer.
#[cfg(target_os = "macos")]
#[path = "../src/printing.rs"]
mod printing;

#[cfg(target_os = "macos")]
mod check {
    use super::printing::{Request, load_snapshot, print_operation};
    use objc2::{AnyThread, rc::Retained};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSPrintAllPages, NSPrintFirstPage,
        NSPrintJobSavingURL, NSPrintLastPage, NSPrintSaveJob,
    };
    use objc2_core_graphics::{CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo};
    use objc2_foundation::{MainThreadMarker, NSNumber, NSPoint, NSRect, NSSize, NSString, NSURL};
    use objc2_pdf_kit::{PDFDisplayBox, PDFDocument, PDFPage};
    use pdf_writer::{Content, Pdf, Rect, Ref};
    use std::{error::Error, path::Path};

    type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

    fn make_pdf(path: &Path, size: NSSize, marks: &[NSRect]) -> Result {
        let mut pdf = Pdf::new();
        let catalog = Ref::new(1);
        let pages = Ref::new(2);
        let ids: Vec<_> = (0..marks.len())
            .map(|index| Ref::new(3 + 2 * index as i32))
            .collect();
        pdf.catalog(catalog).pages(pages);
        pdf.pages(pages)
            .kids(ids.iter().copied())
            .count(marks.len() as i32);
        for (page, mark) in ids.into_iter().zip(marks) {
            let content_id = Ref::new(page.get() + 1);
            pdf.page(page)
                .parent(pages)
                .media_box(Rect::new(0., 0., size.width as f32, size.height as f32))
                .contents(content_id);
            let mut content = Content::new();
            content
                .set_fill_rgb(0., 0., 0.)
                .rect(
                    mark.origin.x as f32,
                    mark.origin.y as f32,
                    mark.size.width as f32,
                    mark.size.height as f32,
                )
                .fill_nonzero();
            pdf.stream(content_id, &content.finish());
        }
        std::fs::write(path, pdf.finish())?;
        Ok(())
    }

    fn open(path: &Path) -> Result<Retained<PDFDocument>> {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str().ok_or("Invalid path")?));
        // SAFETY: A retained file URL and fresh PDFDocument are used on the main thread.
        unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }
            .ok_or_else(|| "Missing PDF".into())
    }

    fn ink_bounds(page: &PDFPage) -> Result<NSRect> {
        // SAFETY: The caller retains the page and its parent on the main thread.
        let size = unsafe { page.boundsForBox(PDFDisplayBox::MediaBox) }.size;
        let (width, height) = (
            (size.width * 2.).ceil() as usize,
            (size.height * 2.).ceil() as usize,
        );
        let mut bytes = vec![255_u8; width * height * 4];
        let space = CGColorSpace::new_device_rgb().ok_or("No RGB color space")?;
        // SAFETY: The stable Vec owns width*height*4 bytes throughout the context's lifetime.
        let context = unsafe {
            CGBitmapContextCreate(
                bytes.as_mut_ptr().cast(),
                width,
                height,
                8,
                width * 4,
                Some(&space),
                CGImageAlphaInfo::PremultipliedLast.0,
            )
        }
        .ok_or("No bitmap context")?;
        CGContext::set_rgb_fill_color(Some(&context), 1., 1., 1., 1.);
        CGContext::fill_rect(
            Some(&context),
            NSRect::new(NSPoint::ZERO, NSSize::new(width as f64, height as f64)),
        );
        CGContext::scale_ctm(Some(&context), 2., 2.);
        // SAFETY: Both objects are live, and drawing finishes before inspecting the buffer.
        unsafe {
            page.drawWithBox_toContext(PDFDisplayBox::MediaBox, &context);
        }
        drop(context);
        let (mut left, mut right, mut top, mut bottom) = (width, 0, height, 0);
        for (index, pixel) in bytes.as_chunks::<4>().0.iter().enumerate() {
            if pixel[0] < 128 {
                let (x, y) = (index % width, index / width);
                left = left.min(x);
                right = right.max(x + 1);
                top = top.min(y);
                bottom = bottom.max(y + 1);
            }
        }
        assert!(left < right && top < bottom, "Printed page is blank");
        Ok(NSRect::new(
            NSPoint::new(left as f64, top as f64),
            NSSize::new((right - left) as f64, (bottom - top) as f64),
        ))
    }

    pub fn run() -> Result {
        let main = MainThreadMarker::new().ok_or("Print tests must run on the main thread")?;
        let app = NSApplication::sharedApplication(main);
        app.setActivationPolicy(NSApplicationActivationPolicy::Prohibited);
        app.finishLaunching();
        let directory = tempfile::tempdir()?;
        let marks = [
            NSRect::new(NSPoint::new(60., 100.), NSSize::new(28.8, 14.4)),
            NSRect::new(NSPoint::new(90., 140.), NSSize::new(43.2, 28.8)),
        ];
        for size in [
            NSSize::new(595.2756, 841.8898),
            NSSize::new(792., 612.),
            NSSize::new(360., 480.),
        ] {
            let source = directory.path().join("source.pdf");
            make_pdf(&source, size, &marks)?;
            for scale in [1., 0.5] {
                for range_only in [false, true] {
                    let output = directory.path().join("printed.pdf");
                    let expected = directory.path().join("expected.pdf");
                    let selected = if range_only { &marks[1..] } else { &marks[..] };
                    let scaled: Vec<_> = selected
                        .iter()
                        .map(|mark| {
                            NSRect::new(
                                NSPoint::new(
                                    mark.origin.x * scale,
                                    size.height * (1. - scale) + mark.origin.y * scale,
                                ),
                                NSSize::new(mark.size.width * scale, mark.size.height * scale),
                            )
                        })
                        .collect();
                    make_pdf(&expected, size, &scaled)?;
                    let snapshot = load_snapshot(
                        &Request {
                            path: source.to_string_lossy().into_owned(),
                            title: "Regression".into(),
                        },
                        main,
                    )?;
                    let operation = print_operation(snapshot, "Print regression", main);
                    let info = operation.printInfo();
                    assert_eq!(info.scalingFactor(), 1.);
                    operation.setShowsPrintPanel(false);
                    operation.setShowsProgressPanel(false);
                    // SAFETY: Use AppKit's documented key/value types. Save-only disposition
                    // prevents these tests from submitting a physical printer job.
                    unsafe {
                        info.setJobDisposition(NSPrintSaveJob);
                        let values = info.dictionary();
                        let url = NSURL::fileURLWithPath(&NSString::from_str(
                            output.to_str().ok_or("Invalid path")?,
                        ));
                        values.insert(NSPrintJobSavingURL, &*url);
                        if range_only {
                            values.insert(NSPrintAllPages, &*NSNumber::new_bool(false));
                            values.insert(NSPrintFirstPage, &*NSNumber::new_usize(2));
                            values.insert(NSPrintLastPage, &*NSNumber::new_usize(2));
                        }
                    }
                    info.setScalingFactor(scale);
                    assert!(operation.runOperation(), "Save-to-PDF failed");
                    let result = open(&output)?;
                    let reference = open(&expected)?;
                    // SAFETY: All PDF objects remain retained on this main thread.
                    unsafe {
                        assert_eq!(result.pageCount(), selected.len());
                        for index in 0..result.pageCount() {
                            let page = result.pageAtIndex(index).ok_or("Missing printed page")?;
                            let original = reference
                                .pageAtIndex(index)
                                .ok_or("Missing expected page")?;
                            let bounds = page.boundsForBox(PDFDisplayBox::MediaBox);
                            assert!(
                                (bounds.size.width - size.width).abs() <= 0.5
                                    && (bounds.size.height - size.height).abs() <= 0.5,
                                "Paper dimensions changed: expected={size:?} actual={bounds:?}"
                            );
                            let actual = ink_bounds(&page)?;
                            let desired = ink_bounds(&original)?;
                            assert!(
                                (actual.origin.x - desired.origin.x).abs() <= 1.
                                    && (actual.origin.y - desired.origin.y).abs() <= 1.
                                    && (actual.size.width - desired.size.width).abs() <= 1.
                                    && (actual.size.height - desired.size.height).abs() <= 1.,
                                "Artwork shifted/scaled: paper={size:?} scale={scale} page={index} actual={actual:?} expected={desired:?}"
                            );
                        }
                    }
                }
            }
        }
        println!(
            "Rust native printing: portrait/landscape/custom paper, 100%/50%, order and page ranges passed"
        );
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{Read, Write};
    if std::env::args().any(|arg| arg == "--worker") {
        let mut input = Vec::new();
        std::io::stdin().take(65537).read_to_end(&mut input)?;
        match printing::execute(&input) {
            Ok(output) => std::io::stdout().write_all(&output)?,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    check::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {}
