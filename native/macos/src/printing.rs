//! Snapshot printing through AppKit/PDFKit, on the worker's main thread.
use objc2::{
    AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSGraphicsContext, NSPaperOrientation,
    NSPrintAllPages, NSPrintFirstPage, NSPrintInfo, NSPrintLastPage, NSPrintOperation,
    NSPrintPanelOptions, NSPrintingPaginationMode, NSView,
};
use objc2_core_graphics::CGContext;
use objc2_foundation::{
    MainThreadMarker, NSNumber, NSPoint, NSRange, NSRect, NSSize, NSString, NSURL,
};
use objc2_pdf_kit::{PDFDisplayBox, PDFDocument, PDFPage};
use serde::{Deserialize, Serialize};
use std::io::Read;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub path: String,
    pub title: String,
}

#[derive(Serialize)]
struct Response {
    completed: bool,
}

pub(crate) struct Snapshot {
    // Keep the document alive for the complete operation and its page objects.
    _document: Retained<PDFDocument>,
    pages: Vec<Retained<PDFPage>>,
    page_size: NSSize,
}

define_class!(
    // SAFETY: All ivars are initialized before NSView's initializer. AppKit
    // invokes these standard NSView selectors only on the main thread.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Snapshot]
    struct PublicationPrintView;

    impl PublicationPrintView {
        #[unsafe(method(knowsPageRange:))]
        unsafe fn knows_page_range(&self, range: *mut NSRange) -> bool {
            if range.is_null() {
                false
            } else {
                // SAFETY: AppKit supplies a writable NSRange for this selector.
                unsafe { range.write(NSRange::new(1, self.ivars().pages.len())); }
                true
            }
        }

        #[unsafe(method(rectForPage:))]
        fn rect_for_page(&self, page: isize) -> NSRect {
            if page < 1 || page as usize > self.ivars().pages.len() {
                return NSRect::ZERO;
            }
            NSRect::new(NSPoint::new(0., (page - 1) as f64 * self.ivars().page_size.height), self.ivars().page_size)
        }

        #[unsafe(method(locationOfPrintRect:))]
        fn location_of_print_rect(&self, rect: NSRect) -> NSPoint {
            let Some(operation) = NSPrintOperation::currentOperation(self.mtm()) else {
                return NSPoint::ZERO;
            };
            // Keep artwork at the physical top-left at the chosen scale;
            // hardware margins may clip ink but must not shift the drawing.
            NSPoint::new(0., operation.printInfo().paperSize().height - rect.size.height * self.print_scale())
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, dirty: NSRect) {
            let Some(graphics) = NSGraphicsContext::currentContext() else { return };
            let context = graphics.CGContext();
            let scale = self.print_scale();
            for (index, page) in self.ivars().pages.iter().enumerate() {
                let rect = self.rectForPage(index as isize + 1);
                if !intersects(rect, dirty) { continue; }
                CGContext::save_g_state(Some(&context));
                CGContext::translate_ctm(Some(&context), rect.origin.x, rect.origin.y);
                CGContext::scale_ctm(Some(&context), scale, scale);
                // SAFETY: The page and graphics context are retained for this
                // synchronous main-thread call; MediaBox is a valid PDF box.
                unsafe { page.drawWithBox_toContext(PDFDisplayBox::MediaBox, &context); }
                CGContext::restore_g_state(Some(&context));
            }
        }
    }
);

fn intersects(a: NSRect, b: NSRect) -> bool {
    a.origin.x < b.origin.x + b.size.width
        && b.origin.x < a.origin.x + a.size.width
        && a.origin.y < b.origin.y + b.size.height
        && b.origin.y < a.origin.y + a.size.height
}

impl PublicationPrintView {
    fn print_scale(&self) -> f64 {
        NSPrintOperation::currentOperation(self.mtm())
            .map(|operation| operation.printInfo().scalingFactor())
            .filter(|scale| scale.is_finite() && *scale > 0.)
            .unwrap_or(1.)
    }

    fn new(snapshot: Snapshot, main: MainThreadMarker) -> Retained<Self> {
        let frame = NSRect::new(
            NSPoint::ZERO,
            NSSize::new(
                snapshot.page_size.width,
                snapshot.page_size.height * snapshot.pages.len() as f64,
            ),
        );
        let allocated = Self::alloc(main).set_ivars(snapshot);
        // SAFETY: ivars are initialized and NSView's designated initializer
        // accepts a finite frame, returning this instance retained.
        unsafe { msg_send![super(allocated), initWithFrame: frame] }
    }
}

fn has_pdf_header(reader: impl Read) -> std::io::Result<bool> {
    // Permit a header starting anywhere in the first 1024 bytes, including
    // a marker that straddles that boundary, without reading the whole file.
    let mut prefix = Vec::with_capacity(1028);
    reader.take(1028).read_to_end(&mut prefix)?;
    Ok(prefix.windows(5).any(|bytes| bytes == b"%PDF-"))
}

pub(crate) fn load_snapshot(
    request: &Request,
    _main: MainThreadMarker,
) -> Result<Snapshot, String> {
    if request.path.is_empty() || request.path.len() > 8192 || request.title.len() > 1024 {
        return Err("Invalid print request".into());
    }
    let metadata =
        std::fs::metadata(&request.path).map_err(|_| "Could not read the print snapshot")?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 128 * 1024 * 1024 {
        return Err("Could not read the print snapshot".into());
    }
    // Obvious non-PDF input does not need to initialize PDFKit. Files with a
    // header still pass through its existing document and page validation.
    let file =
        std::fs::File::open(&request.path).map_err(|_| "Could not read the print snapshot")?;
    if !has_pdf_header(file).map_err(|_| "Could not read the print snapshot")? {
        return Err("Could not read the print snapshot".into());
    }
    let url = NSURL::fileURLWithPath(&NSString::from_str(&request.path));
    // SAFETY: The file URL is retained during initialization. This document is
    // used synchronously on the main thread and retained throughout printing.
    let document = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }
        .ok_or("Could not read the print snapshot")?;
    // SAFETY: All calls access this live PDFDocument on the main thread.
    let count = unsafe {
        if document.isLocked() || !document.allowsPrinting() {
            return Err("Could not read the print snapshot".into());
        }
        document.pageCount()
    };
    if !(1..=100).contains(&count) {
        return Err("Could not read the print snapshot".into());
    }
    let mut pages = Vec::with_capacity(count);
    let mut page_size: Option<NSSize> = None;
    for index in 0..count {
        // SAFETY: index is within the document's reported page count, and both
        // the page and its document stay alive until the operation is released.
        let page = unsafe { document.pageAtIndex(index) }.ok_or("Missing print page")?;
        // SAFETY: MediaBox is valid and page is retained on the main thread.
        let size = unsafe { page.boundsForBox(PDFDisplayBox::MediaBox) }.size;
        if !size.width.is_finite()
            || !size.height.is_finite()
            || !(36. ..=2880.).contains(&size.width)
            || !(36. ..=2880.).contains(&size.height)
        {
            return Err("Unsupported print page dimensions".into());
        }
        if let Some(first) = page_size {
            if (first.width - size.width).abs() >= 0.01
                || (first.height - size.height).abs() >= 0.01
            {
                return Err("Print pages must have a uniform paper size".into());
            }
        } else {
            page_size = Some(size);
        }
        pages.push(page);
    }
    Ok(Snapshot {
        _document: document,
        pages,
        page_size: page_size.ok_or("Missing print page")?,
    })
}

pub(crate) fn print_operation(
    snapshot: Snapshot,
    title: &str,
    main: MainThreadMarker,
) -> Retained<NSPrintOperation> {
    let size = snapshot.page_size;
    let count = snapshot.pages.len();
    let info = NSPrintInfo::new();
    info.setOrientation(NSPaperOrientation::Portrait);
    info.setPaperSize(NSSize::new(
        size.width.min(size.height),
        size.width.max(size.height),
    ));
    info.setOrientation(if size.width > size.height {
        NSPaperOrientation::Landscape
    } else {
        NSPaperOrientation::Portrait
    });
    info.setScalingFactor(1.);
    info.setLeftMargin(0.);
    info.setRightMargin(0.);
    info.setTopMargin(0.);
    info.setBottomMargin(0.);
    info.setHorizontallyCentered(false);
    info.setVerticallyCentered(false);
    info.setHorizontalPagination(NSPrintingPaginationMode::Clip);
    info.setVerticalPagination(NSPrintingPaginationMode::Clip);
    // SAFETY: AppKit's exported print-info keys receive their documented
    // NSNumber values; the dictionary retains its keys and objects.
    unsafe {
        let values = info.dictionary();
        values.setObject_forKey(
            &NSNumber::new_bool(true),
            ProtocolObject::from_ref(NSPrintAllPages),
        );
        values.setObject_forKey(
            &NSNumber::new_usize(1),
            ProtocolObject::from_ref(NSPrintFirstPage),
        );
        values.setObject_forKey(
            &NSNumber::new_usize(count),
            ProtocolObject::from_ref(NSPrintLastPage),
        );
    }
    let view = PublicationPrintView::new(snapshot, main);
    let operation = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    operation.setJobTitle(Some(&NSString::from_str(if title.is_empty() {
        "ReShiki drawing"
    } else {
        title
    })));
    operation.setShowsPrintPanel(true);
    operation.setShowsProgressPanel(true);
    operation.setCanSpawnSeparateThread(false);
    operation.printPanel().setOptions(
        NSPrintPanelOptions::ShowsCopies
            | NSPrintPanelOptions::ShowsPageRange
            | NSPrintPanelOptions::ShowsPaperSize
            | NSPrintPanelOptions::ShowsOrientation
            | NSPrintPanelOptions::ShowsScaling
            | NSPrintPanelOptions::ShowsPreview,
    );
    operation
}

pub fn execute(input: &[u8]) -> Result<Vec<u8>, String> {
    if input.len() > 65536 {
        return Err("Print request is too large".into());
    }
    let request: Request = serde_json::from_slice(input).map_err(|_| "Invalid print request")?;
    let main = MainThreadMarker::new().ok_or("Printing must run on the main thread")?;
    let snapshot = load_snapshot(&request, main)?;
    let app = NSApplication::sharedApplication(main);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();
    let operation = print_operation(snapshot, &request.title, main);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    serde_json::to_vec(&Response {
        completed: operation.runOperation(),
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn snapshot_header_preflight_is_bounded_and_accepts_a_leading_prefix() {
        use super::has_pdf_header;
        use std::io::Cursor;

        for offset in [0, 16, 1023, 1024] {
            let mut bytes = vec![b' '; offset];
            bytes.extend_from_slice(b"%PDF-1.7\n");
            bytes.extend_from_slice(&[0; 4096]);
            let mut reader = Cursor::new(bytes);
            assert_eq!(has_pdf_header(&mut reader).unwrap(), offset < 1024);
            assert_eq!(reader.position(), 1028, "only the bounded prefix is read");
        }
        for bytes in [b"This is not a PDF".as_slice(), b"%PDF", b""] {
            assert!(!has_pdf_header(bytes).unwrap());
        }
    }
}
