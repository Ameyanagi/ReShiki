use super::{Owner, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use std::ptr::null_mut;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Gdi::*, GdiPlus as gp},
        Storage::Xps::*,
        System::{Com::*, Memory::*},
        UI::Controls::Dialogs::*,
    },
    core::PCWSTR,
};

#[derive(Deserialize)]
struct Snapshot {
    version: u32,
    width_pt: f32,
    height_pt: f32,
    pages: Vec<[f32; 2]>,
    primitives: Vec<Item>,
}
#[derive(Deserialize)]
struct Item {
    kind: String,
    transform: [f32; 6],
    #[serde(default)]
    commands: Vec<Vec<f32>>,
    fill: Option<[u8; 4]>,
    stroke: Option<Stroke>,
    #[serde(default)]
    even_odd: bool,
    data: Option<String>,
    width: Option<f32>,
    height: Option<f32>,
}
#[derive(Deserialize)]
struct Stroke {
    color: [u8; 4],
    width: f32,
    dashes: Option<Vec<f32>>,
    dash_offset: f32,
    cap: String,
    join: String,
    miter: f32,
}
fn parse(data: &[u8]) -> Result<Snapshot> {
    if data.len() > 128 * 1024 * 1024 {
        return Err("Print snapshot exceeds 128 MB".into());
    }
    let s: Snapshot = serde_json::from_slice(data)?;
    if s.version != 1
        || ![s.width_pt, s.height_pt]
            .iter()
            .all(|v| v.is_finite() && (36. ..=2880.).contains(v))
        || s.pages.is_empty()
        || s.pages.len() > 100
        || !s.pages.iter().flatten().all(|v| v.is_finite())
        || s.primitives.len() > 1_000_000
    {
        return Err("Invalid print snapshot".into());
    }
    for item in &s.primitives {
        if !item.transform.iter().all(|v| v.is_finite()) {
            return Err("Invalid print transform".into());
        }
        match item.kind.as_str() {
            "path" => {
                if item.commands.is_empty() || item.commands[0].first() != Some(&0.) {
                    return Err("Invalid print path".into());
                }
                for command in &item.commands {
                    if !command.iter().all(|v| v.is_finite())
                        || !matches!(
                            command.as_slice(),
                            [0., _, _]
                                | [1., _, _]
                                | [2., _, _, _, _]
                                | [3., _, _, _, _, _, _]
                                | [4.]
                        )
                    {
                        return Err("Invalid print path command".into());
                    }
                }
                if let Some(stroke) = &item.stroke
                    && (!stroke.width.is_finite()
                        || stroke.width <= 0.
                        || !stroke.miter.is_finite()
                        || stroke.miter < 1.
                        || !stroke.dash_offset.is_finite()
                        || stroke.dashes.as_ref().is_some_and(|d| {
                            d.len() > 256 || d.iter().any(|v| !v.is_finite() || *v <= 0.)
                        }))
                {
                    return Err("Invalid print stroke".into());
                }
            }
            "image" => {
                if ![item.width, item.height]
                    .iter()
                    .all(|v| v.is_some_and(|v| v.is_finite() && v > 0.))
                {
                    return Err("Invalid print image dimensions".into());
                }
                let bytes = STANDARD.decode(item.data.as_deref().ok_or("Missing print image")?)?;
                super::clipboard::bitmap(&bytes)?;
            }
            _ => return Err("Unknown print primitive".into()),
        }
    }
    Ok(s)
}
fn check(status: gp::Status) -> Result<()> {
    if status == gp::Ok {
        Ok(())
    } else {
        Err(format!("Windows drawing failed (GDI+ {})", status.0).into())
    }
}
struct GdiPlus(usize);
impl GdiPlus {
    fn new() -> Result<Self> {
        let mut token = 0;
        let input = gp::GdiplusStartupInput {
            GdiplusVersion: 1,
            SuppressExternalCodecs: BOOL(1),
            ..Default::default()
        };
        // SAFETY: initialized input and valid output storage; token owns startup.
        check(unsafe { gp::GdiplusStartup(&mut token, &input, null_mut()) })?;
        Ok(Self(token))
    }
}
impl Drop for GdiPlus {
    fn drop(&mut self) {
        unsafe { gp::GdiplusShutdown(self.0) }
    }
}
macro_rules! owned {
    ($name:ident,$raw:ty,$delete:path) => {
        struct $name(*mut $raw);
        impl Default for $name {
            fn default() -> Self {
                Self(null_mut())
            }
        }
        impl Drop for $name {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe {
                        let _ = $delete(self.0.cast());
                    }
                }
            }
        }
    };
}
owned!(Graphics, gp::GpGraphics, gp::GdipDeleteGraphics);
owned!(Path, gp::GpPath, gp::GdipDeletePath);
owned!(Pen, gp::GpPen, gp::GdipDeletePen);
owned!(Brush, gp::GpSolidFill, gp::GdipDeleteBrush);
owned!(Matrix, gp::Matrix, gp::GdipDeleteMatrix);
owned!(Bitmap, gp::GpBitmap, gp::GdipDisposeImage);
struct Saved<'a>(&'a Graphics, u32);
impl<'a> Saved<'a> {
    fn new(graphics: &'a Graphics) -> Result<Self> {
        let mut id = 0;
        check(unsafe { gp::GdipSaveGraphics(graphics.0, &mut id) })?;
        Ok(Self(graphics, id))
    }
}
impl Drop for Saved<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = gp::GdipRestoreGraphics(self.0.0, self.1);
        }
    }
}
fn color([r, g, b, a]: [u8; 4]) -> u32 {
    u32::from_be_bytes([a, r, g, b])
}
fn path(item: &Item) -> Result<Path> {
    let mut path = Path::default();
    // SAFETY: path stays owned for all calls. Commands were validated by parse;
    // pointers are returned by GDI+ and destroyed by their corresponding API.
    unsafe {
        check(gp::GdipCreatePath(
            if item.even_odd {
                gp::FillModeAlternate
            } else {
                gp::FillModeWinding
            },
            &mut path.0,
        ))?;
        let mut last = [0., 0.];
        let mut first = last;
        for command in &item.commands {
            match command.as_slice() {
                [0., x, y] => {
                    check(gp::GdipStartPathFigure(path.0))?;
                    last = [*x, *y];
                    first = last;
                }
                [1., x, y] => {
                    check(gp::GdipAddPathLine(path.0, last[0], last[1], *x, *y))?;
                    last = [*x, *y];
                }
                [2., x, y, a, b] => {
                    check(gp::GdipAddPathBezier(
                        path.0,
                        last[0],
                        last[1],
                        last[0] + (*x - last[0]) * 2. / 3.,
                        last[1] + (*y - last[1]) * 2. / 3.,
                        *a + (*x - *a) * 2. / 3.,
                        *b + (*y - *b) * 2. / 3.,
                        *a,
                        *b,
                    ))?;
                    last = [*a, *b];
                }
                [3., x, y, a, b, c, d] => {
                    check(gp::GdipAddPathBezier(
                        path.0, last[0], last[1], *x, *y, *a, *b, *c, *d,
                    ))?;
                    last = [*c, *d];
                }
                [4.] => {
                    check(gp::GdipClosePathFigure(path.0))?;
                    last = first;
                }
                _ => return Err("Invalid print path command".into()),
            }
        }
    }
    Ok(path)
}
const ARGB32: i32 = 0x26200a;
fn draw(graphics: &Graphics, s: &Snapshot, page: usize) -> Result<()> {
    let _sheet = Saved::new(graphics)?;
    let offset = s.pages.get(page).ok_or("Invalid print page")?;
    // SAFETY: graphics is owned and valid; validated finite geometry is passed
    // by value. All temporary GDI+ objects outlive drawing and are RAII-owned.
    unsafe {
        check(gp::GdipSetSmoothingMode(
            graphics.0,
            gp::SmoothingModeAntiAlias,
        ))?;
        check(gp::GdipSetInterpolationMode(
            graphics.0,
            gp::InterpolationModeHighQualityBicubic,
        ))?;
        check(gp::GdipSetClipRect(
            graphics.0,
            0.,
            0.,
            s.width_pt,
            s.height_pt,
            gp::CombineModeIntersect,
        ))?;
        check(gp::GdipTranslateWorldTransform(
            graphics.0,
            offset[0],
            offset[1],
            gp::MatrixOrderPrepend,
        ))?;
        check(gp::GdipScaleWorldTransform(
            graphics.0,
            0.75,
            0.75,
            gp::MatrixOrderPrepend,
        ))?;
        for item in &s.primitives {
            let _saved = Saved::new(graphics)?;
            let [a, b, c, d, x, y] = item.transform;
            let mut matrix = Matrix::default();
            check(gp::GdipCreateMatrix2(a, b, c, d, x, y, &mut matrix.0))?;
            check(gp::GdipMultiplyWorldTransform(
                graphics.0,
                matrix.0,
                gp::MatrixOrderPrepend,
            ))?;
            if item.kind == "image" {
                let bytes = STANDARD.decode(item.data.as_deref().ok_or("Missing print image")?)?;
                let image = super::clipboard::bitmap(&bytes)?;
                let (width, height) = image.dimensions();
                let mut bgra = image.into_raw();
                for pixel in bgra.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                let mut bitmap = Bitmap::default();
                // The backing pixel vector lives until after the bitmap drops.
                check(gp::GdipCreateBitmapFromScan0(
                    width as i32,
                    height as i32,
                    width as i32 * 4,
                    ARGB32,
                    Some(bgra.as_ptr()),
                    &mut bitmap.0,
                ))?;
                check(gp::GdipDrawImageRect(
                    graphics.0,
                    bitmap.0.cast(),
                    0.,
                    0.,
                    item.width.ok_or("Missing image width")?,
                    item.height.ok_or("Missing image height")?,
                ))?;
            } else {
                let path = path(item)?;
                if let Some(fill) = item.fill {
                    let mut brush = Brush::default();
                    check(gp::GdipCreateSolidFill(color(fill), &mut brush.0))?;
                    check(gp::GdipFillPath(graphics.0, brush.0.cast(), path.0))?;
                }
                if let Some(s) = &item.stroke {
                    let mut pen = Pen::default();
                    check(gp::GdipCreatePen1(
                        color(s.color),
                        s.width,
                        gp::UnitWorld,
                        &mut pen.0,
                    ))?;
                    let cap = match s.cap.as_str() {
                        "round" => gp::LineCapRound,
                        "square" => gp::LineCapSquare,
                        _ => gp::LineCapFlat,
                    };
                    let dash_cap = if s.cap == "round" {
                        gp::DashCapRound
                    } else {
                        gp::DashCapFlat
                    };
                    check(gp::GdipSetPenLineCap197819(pen.0, cap, cap, dash_cap))?;
                    check(gp::GdipSetPenLineJoin(
                        pen.0,
                        match s.join.as_str() {
                            "round" => gp::LineJoinRound,
                            "bevel" => gp::LineJoinBevel,
                            _ => gp::LineJoinMiter,
                        },
                    ))?;
                    check(gp::GdipSetPenMiterLimit(pen.0, s.miter))?;
                    if let Some(dashes) = &s.dashes
                        && !dashes.is_empty()
                    {
                        let dashes: Vec<_> = dashes.iter().map(|v| v / s.width).collect();
                        check(gp::GdipSetPenDashArray(
                            pen.0,
                            dashes.as_ptr(),
                            dashes.len() as i32,
                        ))?;
                        check(gp::GdipSetPenDashOffset(pen.0, s.dash_offset / s.width))?;
                    }
                    check(gp::GdipDrawPath(graphics.0, pen.0, path.0))?;
                }
            }
        }
    }
    Ok(())
}
pub(super) fn render(data: &[u8], dpi: f32) -> Result<Vec<u8>> {
    let s = parse(data)?;
    if !dpi.is_finite() || !(36. ..=600.).contains(&dpi) {
        return Err("Invalid render resolution".into());
    }
    let (width, height) = (
        (s.width_pt * dpi / 72.).ceil() as u32,
        (s.height_pt * dpi / 72.).ceil() as u32,
    );
    if u64::from(width) * u64::from(height) > 80_000_000 {
        return Err("Print preview is too large".into());
    }
    let _runtime = GdiPlus::new()?;
    let mut pixels = vec![255u8; width as usize * height as usize * 4];
    {
        let mut bitmap = Bitmap::default();
        let mut graphics = Graphics::default();
        // SAFETY: pixel buffer is correctly sized and stays alive until both
        // native wrappers drop. Stride fits i32 under the pixel/dimension limit.
        unsafe {
            check(gp::GdipCreateBitmapFromScan0(
                width as i32,
                height as i32,
                width as i32 * 4,
                ARGB32,
                Some(pixels.as_mut_ptr().cast_const()),
                &mut bitmap.0,
            ))?;
            check(gp::GdipBitmapSetResolution(bitmap.0, dpi, dpi))?;
            check(gp::GdipGetImageGraphicsContext(
                bitmap.0.cast(),
                &mut graphics.0,
            ))?;
            check(gp::GdipSetPageUnit(graphics.0, gp::UnitPoint))?;
        }
        draw(&graphics, &s, 0)?;
    }
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(output)
}
struct Dialog(PRINTDLGEXW);
impl Drop for Dialog {
    fn drop(&mut self) {
        // SAFETY: these three resources are returned with caller ownership by
        // the common dialog. No aliases are used after this guard drops.
        unsafe {
            if !self.0.hDC.is_invalid() {
                let _ = DeleteDC(self.0.hDC);
            }
            if !self.0.hDevMode.is_invalid() {
                let _ = GlobalFree(self.0.hDevMode);
            }
            if !self.0.hDevNames.is_invalid() {
                let _ = GlobalFree(self.0.hDevNames);
            }
        }
    }
}
fn set_paper(memory: HGLOBAL, s: &Snapshot) -> Result<()> {
    // SAFETY: memory comes from PrintDlg; validate its size, lock while reading
    // or writing, preserve the driver's trailing private configuration bytes.
    unsafe {
        if GlobalSize(memory) < std::mem::size_of::<DEVMODEW>() {
            return Err("Invalid printer settings".into());
        }
        let pointer = GlobalLock(memory).cast::<DEVMODEW>();
        if pointer.is_null() {
            return Err(windows::core::Error::from_win32().into());
        }
        let mut mode = pointer.read_unaligned();
        let width = s.width_pt.min(s.height_pt);
        let height = s.width_pt.max(s.height_pt);
        let paper = [
            (595.276, 841.89, 9),
            (419.528, 595.276, 11),
            (612., 792., 1),
            (612., 1008., 5),
        ]
        .into_iter()
        .find(|(w, h, _)| (width - w).abs() < 1. && (height - h).abs() < 1.)
        .map(|(_, _, id)| id)
        .unwrap_or(256);
        mode.dmFields |= DM_ORIENTATION | DM_PAPERSIZE;
        mode.Anonymous1.Anonymous1.dmOrientation = if s.width_pt > s.height_pt { 2 } else { 1 };
        mode.Anonymous1.Anonymous1.dmPaperSize = paper;
        if paper == 256 {
            mode.dmFields |= DM_PAPERWIDTH | DM_PAPERLENGTH;
            mode.Anonymous1.Anonymous1.dmPaperWidth = (width * 254. / 72.).round() as i16;
            mode.Anonymous1.Anonymous1.dmPaperLength = (height * 254. / 72.).round() as i16;
        } else {
            mode.dmFields &= !(DM_PAPERWIDTH | DM_PAPERLENGTH);
        }
        pointer.write_unaligned(mode);
        let _ = GlobalUnlock(memory);
    }
    Ok(())
}
struct Com;
impl Com {
    fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
struct Job(HDC, bool);
impl Drop for Job {
    fn drop(&mut self) {
        if self.1 {
            unsafe {
                AbortDoc(self.0);
            }
        }
    }
}
fn spool(
    dc: HDC,
    s: &Snapshot,
    title: &str,
    pages: &[usize],
    output: Option<&str>,
) -> Result<bool> {
    let _runtime = GdiPlus::new()?;
    let title: Vec<_> = title
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect::<String>()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let output = output.map(|s| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>());
    let info = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: PCWSTR(title.as_ptr()),
        lpszOutput: output
            .as_ref()
            .map(|v| PCWSTR(v.as_ptr()))
            .unwrap_or(PCWSTR::null()),
        ..Default::default()
    };
    // SAFETY: the caller owns a live printer DC for this job. DOCINFO strings
    // and each graphics object live until their synchronous calls complete.
    unsafe {
        if StartDocW(dc, &info) <= 0 {
            return Err("The print job was cancelled or could not start".into());
        }
        let mut job = Job(dc, true);
        let dpi_x = GetDeviceCaps(dc, LOGPIXELSX);
        let dpi_y = GetDeviceCaps(dc, LOGPIXELSY);
        if dpi_x <= 0 || dpi_y <= 0 {
            return Err("Invalid printer resolution".into());
        }
        for page in pages {
            if StartPage(dc) <= 0 {
                return Err("Could not start the print page".into());
            }
            {
                let mut graphics = Graphics::default();
                check(gp::GdipCreateFromHDC(dc, &mut graphics.0))?;
                check(gp::GdipSetPageUnit(graphics.0, gp::UnitPoint))?;
                check(gp::GdipTranslateWorldTransform(
                    graphics.0,
                    -GetDeviceCaps(dc, PHYSICALOFFSETX) as f32 * 72. / dpi_x as f32,
                    -GetDeviceCaps(dc, PHYSICALOFFSETY) as f32 * 72. / dpi_y as f32,
                    gp::MatrixOrderPrepend,
                ))?;
                draw(&graphics, s, *page)?;
            }
            if EndPage(dc) <= 0 {
                return Err("Could not finish the print page".into());
            }
        }
        if EndDoc(dc) <= 0 {
            return Err("The print job was cancelled or could not finish".into());
        }
        job.1 = false;
    }
    Ok(true)
}
pub(super) fn show(data: &[u8], title: &str) -> Result<bool> {
    let snapshot = parse(data)?;
    let _com = Com::new()?;
    let owner = Owner::new()?;
    let mut ranges = vec![
        PRINTPAGERANGE {
            nFromPage: 1,
            nToPage: snapshot.pages.len() as u32
        };
        100
    ];
    let mut dialog = Dialog(PRINTDLGEXW {
        lStructSize: std::mem::size_of::<PRINTDLGEXW>() as u32,
        hwndOwner: owner.0,
        Flags: PD_RETURNDC | PD_USEDEVMODECOPIESANDCOLLATE | PD_NOSELECTION | PD_NOCURRENTPAGE,
        nMinPage: 1,
        nMaxPage: snapshot.pages.len() as u32,
        nMaxPageRanges: ranges.len() as u32,
        lpPageRanges: ranges.as_mut_ptr(),
        nCopies: 1,
        nStartPage: START_PAGE_GENERAL,
        ..Default::default()
    });
    // SAFETY: default printer settings are obtained without showing a dialog.
    // Returned allocations transfer to Dialog; ranges stay alive throughout.
    unsafe {
        let mut defaults = PRINTDLGW {
            lStructSize: std::mem::size_of::<PRINTDLGW>() as u32,
            Flags: PD_RETURNDEFAULT,
            ..Default::default()
        };
        let found = PrintDlgW(&mut defaults).as_bool();
        dialog.0.hDevMode = defaults.hDevMode;
        dialog.0.hDevNames = defaults.hDevNames;
        if !defaults.hDC.is_invalid() {
            let _ = DeleteDC(defaults.hDC);
        }
        if found && !dialog.0.hDevMode.is_invalid() {
            set_paper(dialog.0.hDevMode, &snapshot)?;
        }
        PrintDlgExW(&mut dialog.0)?;
    }
    if dialog.0.dwResultAction != PD_RESULT_PRINT {
        return Ok(false);
    }
    if dialog.0.hDC.is_invalid() {
        return Err("The printer did not return a drawing surface".into());
    }
    let pages: Vec<_> = if dialog.0.Flags.contains(PD_PAGENUMS) {
        let mut pages = Vec::new();
        for range in ranges.iter().take(dialog.0.nPageRanges.min(100) as usize) {
            if range.nFromPage == 0
                || range.nToPage < range.nFromPage
                || range.nToPage > snapshot.pages.len() as u32
            {
                return Err("Invalid print page range".into());
            }
            pages.extend((range.nFromPage - 1..range.nToPage).map(|v| v as usize));
        }
        pages
    } else {
        (0..snapshot.pages.len()).collect()
    };
    if pages.is_empty() {
        return Err("No pages selected to print".into());
    }
    spool(
        dialog.0.hDC,
        &snapshot,
        title,
        &pages,
        dialog.0.Flags.contains(PD_PRINTTOFILE).then_some("FILE:"),
    )
}
