//! H.264 export through the system AVFoundation encoder.
//!
//! The bindings follow AVAssetWriter.h, AVAssetWriterInput.h,
//! AVVideoSettings.h, CMTime.h and CVPixelBuffer.h from the macOS SDK.

use block2::RcBlock;
use objc2::encode::{Encode, Encoding, RefEncode};
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, NSObject};
use objc2::{class, msg_send};
use objc2_foundation::{NSDictionary, NSError, NSNumber, NSString, NSURL};
use std::ffi::c_void;
use std::path::Path;
use std::ptr::{NonNull, null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::thread;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const WRITING: isize = 1;
const COMPLETED: isize = 2;
const PIXEL_FORMAT_420V: u32 = u32::from_be_bytes(*b"420v");

// The rasterizer supplies sRGB, while the output declares BT.709 transfer.
// Decode sRGB to linear light, then apply the BT.709 OETF before the matrix.
// Cache the byte conversion so encoding needs no per-pixel power functions.
static SRGB_TO_BT709: LazyLock<[i32; 256]> = LazyLock::new(|| {
    std::array::from_fn(|value| {
        let srgb = value as f64 / 255.0;
        let linear = if srgb <= 0.04045 {
            srgb / 12.92
        } else {
            ((srgb + 0.055) / 1.055).powf(2.4)
        };
        let bt709 = if linear < 0.018 {
            4.5 * linear
        } else {
            1.099 * linear.powf(0.45) - 0.099
        };
        (bt709 * 255.0).round() as i32
    })
});

/// Square, fixed-rate video. Per-frame readouts disable held-frame reuse.
#[derive(Clone, Copy)]
pub struct Settings {
    pub size: u16,
    pub fps: u16,
    pub per_frame: bool,
}

/// A framework error, including the precise unavailable-codec classification.
#[derive(Debug)]
pub struct Error {
    message: String,
    codec_unavailable: bool,
}

impl Error {
    /// AVFoundation explicitly reported AVErrorEncoderNotFound (-11834).
    pub fn is_codec_unavailable(&self) -> bool {
        self.codec_unavailable
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}
impl From<String> for Error {
    fn from(message: String) -> Self {
        Self {
            message,
            codec_unavailable: false,
        }
    }
}
impl From<&str> for Error {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

/// Encode to a previously nonexistent file. Call on a worker thread. The
/// renderer supplies straight-alpha RGBA, composited onto white for MP4.
/// Cancellation is polled during backpressure and asynchronous finalization.
/// The caller owns this temporary path, discards it on cancellation/failure,
/// and controls final destination replacement after success.
pub fn encode(
    path: &Path,
    settings: Settings,
    sequence: &[usize],
    mut canceled: impl FnMut() -> bool,
    mut progress: impl FnMut(usize),
    mut render: impl FnMut(usize, usize) -> Result<Vec<u8>, String>,
) -> Result<bool, Error> {
    if canceled() {
        return Ok(false);
    }
    if sequence.is_empty() {
        return Err("There are no frames to export.".into());
    }
    if settings.size == 0 || !settings.size.is_multiple_of(2) || settings.fps == 0 {
        return Err(
            "MP4 export requires a positive, even frame size and a positive frame rate.".into(),
        );
    }
    autoreleasepool(|_| {
        let mut encoder = None;
        let mut previous: Option<(usize, PixelBuffer)> = None;
        for (frame, &index) in sequence.iter().enumerate() {
            if canceled() {
                return Ok(false);
            }
            if settings.per_frame || previous.as_ref().is_none_or(|(last, _)| *last != index) {
                let rgba = render(frame, index)?;
                if canceled() {
                    return Ok(false);
                }
                previous = Some((index, PixelBuffer::new(settings.size, &rgba)?));
            }
            // A render failure/cancel needs no encoder or native codec service.
            if encoder.is_none() {
                encoder = Some(Encoder::new(path, settings.size, settings.fps)?);
            }
            let writer = encoder.as_ref().ok_or("Missing MP4 encoder.")?;
            if !writer.wait_for_input(&mut canceled)? {
                return Ok(false);
            }
            if let Some((_, pixels)) = &previous {
                writer.append(pixels, frame)?;
            }
            progress(frame + 1);
        }
        encoder
            .ok_or("Missing MP4 encoder.")?
            .finish(sequence.len(), &mut canceled)
    })
}

struct Encoder {
    writer: Retained<AnyObject>,
    input: Retained<AnyObject>,
    adaptor: Retained<AnyObject>,
    finished: bool,
    fps: u16,
}

impl Encoder {
    fn new(path: &Path, side: u16, fps: u16) -> Result<Self, Error> {
        let url = NSURL::from_file_path(path).ok_or("The MP4 path is invalid.")?;
        let side = NSNumber::new_u16(side);
        let frame_rate = NSNumber::new_u16(fps);
        let bitrate = NSNumber::new_u64((side.as_u64().pow(2) * 8).max(1_000_000));
        let reorder = NSNumber::new_bool(false);
        // SAFETY: Every selector and constant below has the SDK signature
        // named in this module's header. The dictionaries contain the required
        // types, and configuration is completed before startWriting.
        unsafe {
            let compression = NSDictionary::<NSString, NSObject>::from_slices(
                &[
                    AVVideoProfileLevelKey,
                    AVVideoAverageBitRateKey,
                    AVVideoExpectedSourceFrameRateKey,
                    AVVideoAllowFrameReorderingKey,
                ],
                &[
                    AVVideoProfileLevelH264MainAutoLevel,
                    &bitrate,
                    &frame_rate,
                    &reorder,
                ],
            );
            let colors = NSDictionary::<NSString, NSObject>::from_slices(
                &[
                    AVVideoColorPrimariesKey,
                    AVVideoTransferFunctionKey,
                    AVVideoYCbCrMatrixKey,
                ],
                &[
                    AVVideoColorPrimaries_ITU_R_709_2,
                    AVVideoTransferFunction_ITU_R_709_2,
                    AVVideoYCbCrMatrix_ITU_R_709_2,
                ],
            );
            let settings = NSDictionary::<NSString, NSObject>::from_slices(
                &[
                    AVVideoCodecKey,
                    AVVideoWidthKey,
                    AVVideoHeightKey,
                    AVVideoCompressionPropertiesKey,
                    AVVideoColorPropertiesKey,
                ],
                &[AVVideoCodecTypeH264, &side, &side, &compression, &colors],
            );
            let mut error: *mut NSError = null_mut();
            let writer: Option<Retained<AnyObject>> = msg_send![
                class!(AVAssetWriter), assetWriterWithURL: &*url,
                fileType: AVFileTypeMPEG4, error: &mut error
            ];
            let writer =
                writer.ok_or_else(|| error_message(error, "Could not create the MP4 writer."))?;
            let supported: bool = msg_send![&writer, canApplyOutputSettings: &*settings, forMediaType: AVMediaTypeVideo];
            if !supported {
                return Err("The system H.264 encoder does not support these MP4 settings.".into());
            }
            let input: Retained<AnyObject> = msg_send![
                class!(AVAssetWriterInput), assetWriterInputWithMediaType: AVMediaTypeVideo,
                outputSettings: &*settings
            ];
            let _: () = msg_send![&input, setExpectsMediaDataInRealTime: false];
            let can_add: bool = msg_send![&writer, canAddInput: &*input];
            if !can_add {
                return Err("Could not add the H.264 video track to the MP4 file.".into());
            }
            let _: () = msg_send![&writer, addInput: &*input];
            // We own immutable 420v buffers; no adaptor pool is necessary.
            let adaptor: Retained<AnyObject> = msg_send![
                class!(AVAssetWriterInputPixelBufferAdaptor),
                assetWriterInputPixelBufferAdaptorWithAssetWriterInput: &*input,
                sourcePixelBufferAttributes: None::<&NSDictionary<NSString, NSObject>>
            ];
            let encoder = Self {
                writer,
                input,
                adaptor,
                finished: false,
                fps,
            };
            let started: bool = msg_send![&encoder.writer, startWriting];
            if !started {
                return Err(encoder.error("Could not start MP4 encoding."));
            }
            let _: () = msg_send![&encoder.writer, startSessionAtSourceTime: CMTime::frame(0, fps)];
            Ok(encoder)
        }
    }

    fn status(&self) -> isize {
        // SAFETY: AVAssetWriter.status is a thread-safe NSInteger property.
        unsafe { msg_send![&self.writer, status] }
    }

    fn error(&self, fallback: &str) -> Error {
        // SAFETY: AVAssetWriter.error returns a nullable NSError. Its retained
        // value stays alive while localizedDescription is read.
        let error: Option<Retained<NSError>> = unsafe { msg_send![&self.writer, error] };
        describe_error(error.as_deref(), fallback)
    }

    fn wait_for_input(&self, canceled: &mut impl FnMut() -> bool) -> Result<bool, Error> {
        loop {
            if canceled() {
                return Ok(false);
            }
            if self.status() != WRITING {
                return Err(self.error("MP4 encoding stopped before all frames were written."));
            }
            // SAFETY: This is the input attached to our started asset writer.
            let ready: bool = unsafe { msg_send![&self.input, isReadyForMoreMediaData] };
            if ready {
                return Ok(!canceled());
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn append(&self, pixels: &PixelBuffer, frame: usize) -> Result<(), Error> {
        // SAFETY: The input is ready, the immutable buffer stays alive through
        // the call (the adaptor retains it), and timestamps increase by 1/FPS.
        let appended: bool = unsafe {
            msg_send![&self.adaptor, appendPixelBuffer: pixels.0, withPresentationTime: CMTime::frame(frame, self.fps)]
        };
        if appended {
            Ok(())
        } else {
            Err(self.error("Could not encode an MP4 frame."))
        }
    }

    fn finish(
        &mut self,
        frames: usize,
        canceled: &mut impl FnMut() -> bool,
    ) -> Result<bool, Error> {
        if canceled() {
            return Ok(false);
        }
        let complete = Arc::new(AtomicBool::new(false));
        let finished = Arc::clone(&complete);
        let callback = RcBlock::new(move || finished.store(true, Ordering::Release));
        // SAFETY: No appends are pending. The owned atomic is safe for the
        // framework's completion thread and the copied block is 'static.
        unsafe {
            let _: () = msg_send![&self.input, markAsFinished];
            let _: () =
                msg_send![&self.writer, endSessionAtSourceTime: CMTime::frame(frames, self.fps)];
            let _: () = msg_send![&self.writer, finishWritingWithCompletionHandler: &*callback];
        }
        while !complete.load(Ordering::Acquire) {
            if canceled() {
                return Ok(false);
            }
            thread::sleep(POLL_INTERVAL);
        }
        if canceled() {
            return Ok(false);
        }
        if self.status() != COMPLETED {
            return Err(self.error("Could not finish the MP4 file."));
        }
        self.finished = true;
        Ok(true)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        if !self.finished {
            // SAFETY: All appends occur synchronously on this worker, so none
            // can race cancellation. cancelWriting waits for file cleanup.
            unsafe {
                let _: () = msg_send![&self.writer, cancelWriting];
            }
        }
    }
}

fn error_message(error: *mut NSError, fallback: &str) -> Error {
    // SAFETY: The out pointer is null or a live autoreleased NSError within
    // encode's autorelease pool.
    describe_error(unsafe { error.as_ref() }, fallback)
}

fn describe_error(error: Option<&NSError>, fallback: &str) -> Error {
    let Some(error) = error else {
        return fallback.into();
    };
    let domain = error.domain().to_string();
    let code = error.code();
    let description = error.localizedDescription();
    let reason = error
        .localizedFailureReason()
        .map(|s| s.to_string())
        .unwrap_or_default();
    Error {
        message: format!("{fallback} {description}. {reason} ({domain} {code})"),
        codec_unavailable: domain == "AVFoundationErrorDomain" && code == -11834,
    }
}

struct PixelBuffer(NonNull<CVBuffer>);

impl PixelBuffer {
    fn new(side: u16, rgba: &[u8]) -> Result<Self, Error> {
        let side = usize::from(side);
        if rgba.len() != side.pow(2) * 4 {
            return Err("The renderer returned an incomplete frame.".into());
        }
        let mut buffer = null_mut();
        // SAFETY: CoreVideo allocates and owns both planes; the out pointer is
        // writable and nil attributes request the default allocator/layout.
        let result = unsafe {
            CVPixelBufferCreate(null(), side, side, PIXEL_FORMAT_420V, null(), &mut buffer)
        };
        if result != 0 {
            return Err(format!("Could not allocate an MP4 frame (CoreVideo {result}).").into());
        }
        let pixels = Self(NonNull::new(buffer).ok_or("CoreVideo returned an empty MP4 frame.")?);
        // SAFETY: pixels owns the new buffer, which has not been submitted.
        let result = unsafe { CVPixelBufferLockBaseAddress(pixels.0, 0) };
        if result != 0 {
            return Err(format!("Could not lock an MP4 frame (CoreVideo {result}).").into());
        }
        let filled = pixels.fill(side, rgba);
        // SAFETY: The successful lock above is paired once, including errors.
        let result = unsafe { CVPixelBufferUnlockBaseAddress(pixels.0, 0) };
        filled?;
        if result != 0 {
            return Err(format!("Could not unlock an MP4 frame (CoreVideo {result}).").into());
        }
        Ok(pixels)
    }

    fn fill(&self, side: usize, rgba: &[u8]) -> Result<(), Error> {
        // SAFETY: The caller holds a successful write lock on this newly
        // allocated, unshared 420v buffer. Each plane has its own row stride.
        unsafe {
            let y = CVPixelBufferGetBaseAddressOfPlane(self.0, 0).cast::<u8>();
            let uv = CVPixelBufferGetBaseAddressOfPlane(self.0, 1).cast::<u8>();
            let y_stride = CVPixelBufferGetBytesPerRowOfPlane(self.0, 0);
            let uv_stride = CVPixelBufferGetBytesPerRowOfPlane(self.0, 1);
            if y.is_null() || uv.is_null() || y_stride < side || uv_stride < side {
                return Err("CoreVideo returned an invalid MP4 frame layout.".into());
            }
            let y = std::slice::from_raw_parts_mut(y, y_stride * side);
            let uv = std::slice::from_raw_parts_mut(uv, uv_stride * (side / 2));
            fill_yuv(rgba, side, y, y_stride, uv, uv_stride);
        }
        Ok(())
    }
}

impl Drop for PixelBuffer {
    fn drop(&mut self) {
        // SAFETY: This releases the +1 reference returned by Create.
        unsafe { CVPixelBufferRelease(self.0) }
    }
}

/// BT.709 video-range NV12. Chroma averages each 2×2 block after compositing
/// straight-alpha sRGB renderer output onto white and converting to BT.709
/// transfer; MP4 does not preserve alpha.
#[allow(clippy::indexing_slicing)] // Bounds follow the validated even-sized plane layout.
fn fill_yuv(
    rgba: &[u8],
    side: usize,
    y: &mut [u8],
    y_stride: usize,
    uv: &mut [u8],
    uv_stride: usize,
) {
    let transfer = &*SRGB_TO_BT709;
    for row in (0..side).step_by(2) {
        for col in (0..side).step_by(2) {
            let mut rgb = [0_i32; 3];
            for dy in 0..2 {
                for dx in 0..2 {
                    let offset = ((row + dy) * side + col + dx) * 4;
                    let pixel = &rgba[offset..offset + 4];
                    let alpha = i32::from(pixel[3]);
                    let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(|c| {
                        let srgb = (i32::from(c) * alpha + 255 * (255 - alpha) + 127) / 255;
                        transfer[srgb as usize]
                    });
                    rgb[0] += r;
                    rgb[1] += g;
                    rgb[2] += b;
                    y[(row + dy) * y_stride + col + dx] =
                        (16 + ((11966 * r + 40254 * g + 4064 * b + 32768) >> 16)) as u8;
                }
            }
            let [r, g, b] = rgb;
            let offset = row / 2 * uv_stride + col;
            uv[offset] = (128 + ((-6596 * r - 22189 * g + 28784 * b + 131072) >> 18)) as u8;
            uv[offset + 1] = (128 + ((28784 * r - 26145 * g - 2639 * b + 131072) >> 18)) as u8;
        }
    }
}

// CMTime.h uses four-byte packing, including on 64-bit macOS.
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

impl CMTime {
    fn frame(frame: usize, fps: u16) -> Self {
        Self {
            value: frame as i64,
            timescale: i32::from(fps),
            flags: 1,
            epoch: 0,
        }
    }
}

// SAFETY: This exactly matches CMTime's C layout and Objective-C encoding.
unsafe impl Encode for CMTime {
    const ENCODING: Encoding = Encoding::Struct(
        "?",
        &[i64::ENCODING, i32::ENCODING, u32::ENCODING, i64::ENCODING],
    );
}

#[repr(C)]
struct CVBuffer {
    _private: [u8; 0],
}

// SAFETY: CVPixelBufferRef is a pointer to the opaque __CVBuffer structure.
unsafe impl RefEncode for CVBuffer {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("__CVBuffer", &[]));
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVFileTypeMPEG4: &'static NSString;
    static AVMediaTypeVideo: &'static NSString;
    static AVVideoCodecKey: &'static NSString;
    static AVVideoCodecTypeH264: &'static NSString;
    static AVVideoWidthKey: &'static NSString;
    static AVVideoHeightKey: &'static NSString;
    static AVVideoCompressionPropertiesKey: &'static NSString;
    static AVVideoProfileLevelKey: &'static NSString;
    static AVVideoProfileLevelH264MainAutoLevel: &'static NSString;
    static AVVideoAverageBitRateKey: &'static NSString;
    static AVVideoExpectedSourceFrameRateKey: &'static NSString;
    static AVVideoAllowFrameReorderingKey: &'static NSString;
    static AVVideoColorPropertiesKey: &'static NSString;
    static AVVideoColorPrimariesKey: &'static NSString;
    static AVVideoTransferFunctionKey: &'static NSString;
    static AVVideoYCbCrMatrixKey: &'static NSString;
    static AVVideoColorPrimaries_ITU_R_709_2: &'static NSString;
    static AVVideoTransferFunction_ITU_R_709_2: &'static NSString;
    static AVVideoYCbCrMatrix_ITU_R_709_2: &'static NSString;
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVPixelBufferCreate(
        allocator: *const c_void,
        width: usize,
        height: usize,
        format: u32,
        attributes: *const c_void,
        out: *mut *mut CVBuffer,
    ) -> i32;
    fn CVPixelBufferRelease(buffer: NonNull<CVBuffer>);
    fn CVPixelBufferLockBaseAddress(buffer: NonNull<CVBuffer>, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(buffer: NonNull<CVBuffer>, flags: u64) -> i32;
    fn CVPixelBufferGetBaseAddressOfPlane(buffer: NonNull<CVBuffer>, plane: usize) -> *mut c_void;
    fn CVPixelBufferGetBytesPerRowOfPlane(buffer: NonNull<CVBuffer>, plane: usize) -> usize;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn tiny_mp4_is_readable_and_reuses_held_frames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("animation.mp4");
        let mut rendered = Vec::new();
        let mut done = 0;
        let result = encode(
            &path,
            Settings {
                size: 32,
                fps: 20,
                per_frame: false,
            },
            &[0, 0, 1, 1],
            || false,
            |n| done = n,
            |frame, index| {
                rendered.push((frame, index));
                Ok([if index == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }; 32 * 32]
                    .concat())
            },
        );
        match result {
            Err(error) if error.is_codec_unavailable() => {
                eprintln!("SKIPPED tiny MP4 encode: {error}");
                return;
            }
            result => assert!(result.unwrap()),
        }
        assert_eq!(rendered, [(0, 0), (2, 1)]);
        assert_eq!(done, 4);
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 100);
        assert!(bytes.windows(4).any(|x| x == b"ftyp"));
        assert!(bytes.windows(4).any(|x| x == b"avc1"));
        assert!(bytes.windows(14).any(|x| {
            &x[..4] == b"colr"
                && (&x[4..8] == b"nclc" || &x[4..8] == b"nclx")
                && x[8..] == [0, 1, 0, 1, 0, 1] // BT.709 primaries, transfer, matrix.
        }));
        autoreleasepool(|_| unsafe {
            let url = NSURL::from_file_path(&path).unwrap();
            let asset: Retained<AnyObject> = msg_send![class!(AVURLAsset), URLAssetWithURL: &*url, options: None::<&NSDictionary<NSString, NSObject>>];
            let readable: bool = msg_send![&asset, isReadable];
            assert!(readable);
            let duration: CMTime = msg_send![&asset, duration];
            let seconds = duration.value as f64 / f64::from(duration.timescale);
            assert!((seconds - 0.2).abs() < 0.001, "duration {seconds}");
        });
    }

    #[test]
    fn yuv_has_video_range_white_black_and_averaged_chroma() {
        let mut y = [0; 8];
        let mut uv = [0; 4];
        fill_yuv(&[255; 16], 2, &mut y, 4, &mut uv, 4);
        assert_eq!(y, [235, 235, 0, 0, 235, 235, 0, 0]);
        assert_eq!(uv, [128, 128, 0, 0]);
        fill_yuv(&[[0, 0, 0, 255]; 4].concat(), 2, &mut y, 4, &mut uv, 4);
        assert_eq!(y[0], 16);
        fill_yuv(&[[255, 0, 0, 0]; 4].concat(), 2, &mut y, 4, &mut uv, 4);
        assert_eq!(y[0], 235);
        assert_eq!(uv[..2], [128, 128]);
        fill_yuv(&[[255, 0, 0, 255]; 4].concat(), 2, &mut y, 4, &mut uv, 4);
        assert_eq!(y[0], 63);
        assert_eq!(uv[..2], [102, 240]);
        fill_yuv(
            &[
                [255, 0, 0, 255],
                [255, 0, 0, 255],
                [0, 0, 255, 255],
                [0, 0, 255, 255],
            ]
            .concat(),
            2,
            &mut y,
            4,
            &mut uv,
            4,
        );
        assert_eq!(y, [63, 63, 0, 0, 32, 32, 0, 0]);
        assert_eq!(uv[..2], [171, 179]);
    }

    #[test]
    fn yuv_converts_srgb_midtones_to_the_declared_bt709_transfer() {
        let mut y = [0; 4];
        let mut uv = [0; 2];
        for (srgb, luma) in [(32, 31), (128, 115), (192, 175)] {
            fill_yuv(
                &[[srgb, srgb, srgb, 255]; 4].concat(),
                2,
                &mut y,
                2,
                &mut uv,
                2,
            );
            assert_eq!(y, [luma; 4]);
            assert_eq!(uv, [128, 128]);
        }
        // Composite in the renderer's sRGB space before transfer conversion:
        // black at this alpha over white has the same samples as sRGB 128.
        fill_yuv(&[[0, 0, 0, 127]; 4].concat(), 2, &mut y, 2, &mut uv, 2);
        assert_eq!(y, [115; 4]);
        assert_eq!(uv, [128, 128]);
    }
}
