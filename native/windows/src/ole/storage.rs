use super::*;
use std::mem::ManuallyDrop;
use windows::Win32::Graphics::Gdi::*;

#[derive(Clone, Default)]
pub(super) struct Drawing {
    pub document: Vec<u8>,
    pub png: Vec<u8>,
    pub emf: Vec<u8>,
    pub extent: SIZE,
}

pub(super) struct Medium(pub STGMEDIUM);
impl Drop for Medium {
    fn drop(&mut self) {
        unsafe {
            ReleaseStgMedium(&mut self.0);
        }
    }
}

struct PreviewBitmap {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}
impl PreviewBitmap {
    fn new(image: &image::RgbaImage) -> CResult<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: image.width() as i32,
                biHeight: -(image.height() as i32),
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        // Own the memory DC and its selected DIB together, restoring selection
        // before deletion. AlphaBlend needs premultiplied BGRA source pixels.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err(Error::from_win32());
            }
            let mut pixels = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, None, 0) {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    let _ = DeleteDC(dc);
                    return Err(error);
                }
            };
            let previous = SelectObject(dc, bitmap);
            let output = std::slice::from_raw_parts_mut(pixels.cast::<u8>(), image.as_raw().len());
            for (source, target) in image.pixels().zip(output.as_chunks_mut::<4>().0.iter_mut()) {
                for (destination, channel) in [2, 1, 0].into_iter().enumerate() {
                    target[destination] =
                        ((u32::from(source[channel]) * u32::from(source[3]) + 127) / 255) as u8;
                }
                target[3] = source[3];
            }
            Ok(Self {
                dc,
                bitmap,
                previous,
            })
        }
    }
}
impl Drop for PreviewBitmap {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.dc);
        }
    }
}

pub(super) fn format_etc(id: u32, medium: TYMED) -> FORMATETC {
    FORMATETC {
        cfFormat: id as u16,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: medium.0 as u32,
    }
}
pub(super) fn global(bytes: &[u8]) -> CResult<STGMEDIUM> {
    let memory = clipboard::Memory::new(bytes).map_err(error)?;
    let result = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: memory.0 },
        pUnkForRelease: ManuallyDrop::new(None),
    };
    std::mem::forget(memory);
    Ok(result)
}

const OBJECT_DESCRIPTOR_HEADER_SIZE: usize = 52;
// The clipboard header uses the Win32 field layout on both x64 and ARM64.
const _: [(); OBJECT_DESCRIPTOR_HEADER_SIZE] = [(); std::mem::size_of::<OBJECTDESCRIPTOR>()];
const _: [(); 16] = [(); std::mem::size_of::<GUID>()];

fn descriptor_header_bytes(descriptor: &OBJECTDESCRIPTOR) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(OBJECT_DESCRIPTOR_HEADER_SIZE);
    bytes.extend_from_slice(&descriptor.cbSize.to_le_bytes());
    // COM GUIDs store the first three integer fields in little-endian order,
    // followed by data4 unchanged; neither whole-u128 byte order matches this.
    bytes.extend_from_slice(&descriptor.clsid.data1.to_le_bytes());
    bytes.extend_from_slice(&descriptor.clsid.data2.to_le_bytes());
    bytes.extend_from_slice(&descriptor.clsid.data3.to_le_bytes());
    bytes.extend_from_slice(&descriptor.clsid.data4);
    bytes.extend_from_slice(&descriptor.dwDrawAspect.to_le_bytes());
    bytes.extend_from_slice(&descriptor.sizel.cx.to_le_bytes());
    bytes.extend_from_slice(&descriptor.sizel.cy.to_le_bytes());
    bytes.extend_from_slice(&descriptor.pointl.x.to_le_bytes());
    bytes.extend_from_slice(&descriptor.pointl.y.to_le_bytes());
    bytes.extend_from_slice(&descriptor.dwStatus.to_le_bytes());
    bytes.extend_from_slice(&descriptor.dwFullUserTypeName.to_le_bytes());
    bytes.extend_from_slice(&descriptor.dwSrcOfCopy.to_le_bytes());
    bytes
}

fn write_stream(storage: &IStorage, name: PCWSTR, bytes: &[u8]) -> CResult<()> {
    unsafe {
        let stream = storage.CreateStream(
            name,
            STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE,
            0,
            0,
        )?;
        let mut written = 0;
        stream
            .Write(
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                Some(&mut written),
            )
            .ok()?;
        if written as usize != bytes.len() {
            return Err(error("Incomplete Office drawing write"));
        }
        stream.Commit(STGC_DEFAULT)?;
    }
    Ok(())
}
fn read_stream(storage: &IStorage, name: PCWSTR) -> CResult<Vec<u8>> {
    unsafe {
        let stream = storage.OpenStream(name, None, STGM_READ | STGM_SHARE_EXCLUSIVE, 0)?;
        let mut stat = STATSTG::default();
        stream.Stat(&mut stat, STATFLAG_NONAME)?;
        if stat.cbSize == 0 || stat.cbSize > LIMIT as u64 {
            return Err(error("Invalid Office drawing size"));
        }
        let mut bytes = vec![0; stat.cbSize as usize];
        let mut read = 0;
        stream
            .Read(
                bytes.as_mut_ptr().cast(),
                bytes.len() as u32,
                Some(&mut read),
            )
            .ok()?;
        if read as usize != bytes.len() {
            return Err(error("Truncated Office drawing"));
        }
        Ok(bytes)
    }
}
impl Drawing {
    pub fn new(document: Vec<u8>, png: Vec<u8>, metafile: Option<Vec<u8>>) -> CResult<Self> {
        if document.is_empty() || document.len() + png.len() > LIMIT {
            return Err(error("Office drawing exceeds 64 MB"));
        }
        let _: serde_json::Value = serde_json::from_slice(&document).map_err(error)?;
        let image = clipboard::bitmap(&png).map_err(error)?;
        if image.as_raw().len() > LIMIT {
            return Err(error("Office preview exceeds 64 MB"));
        }
        let (width, height) = (image.width() as i32, image.height() as i32);
        let dimensions = png::Decoder::new(std::io::Cursor::new(&png))
            .read_info()
            .map_err(error)?
            .info()
            .pixel_dims;
        let (xppu, yppu) = dimensions
            .filter(|d| d.unit == png::Unit::Meter)
            .map(|d| (d.xppu.max(1), d.yppu.max(1)))
            .unwrap_or((3780, 3780));
        let extent = SIZE {
            cx: (i64::from(width) * 100_000 / i64::from(xppu)).clamp(1, i64::from(i32::MAX)) as i32,
            cy: (i64::from(height) * 100_000 / i64::from(yppu)).clamp(1, i64::from(i32::MAX))
                as i32,
        };
        if let Some(emf) = metafile {
            if emf.len() > LIMIT || emf.len() < 88 {
                return Err(error("Invalid Office vector preview size"));
            }
            let handle = unsafe { SetEnhMetaFileBits(&emf) };
            if handle.is_invalid() {
                return Err(error("Invalid Office vector preview"));
            }
            unsafe {
                let _ = DeleteEnhMetaFile(handle);
            }
            return Ok(Self {
                document,
                png,
                emf,
                extent,
            });
        }
        let source = PreviewBitmap::new(&image)?;
        let frame = RECT {
            left: 0,
            top: 0,
            right: extent.cx,
            bottom: extent.cy,
        };
        let emf = unsafe {
            let dc = CreateEnhMetaFileW(None, PCWSTR::null(), Some(&frame), PCWSTR::null());
            if dc.is_invalid() {
                return Err(Error::from_win32());
            }
            // The source is a 1200-dpi image, while the metafile DC uses screen
            // pixels. Map the whole source into the physical HIMETRIC frame.
            let viewport = SIZE {
                cx: ((i64::from(extent.cx) * i64::from(GetDeviceCaps(dc, HORZRES)))
                    / i64::from(GetDeviceCaps(dc, HORZSIZE).max(1) * 100))
                .max(1) as i32,
                cy: ((i64::from(extent.cy) * i64::from(GetDeviceCaps(dc, VERTRES)))
                    / i64::from(GetDeviceCaps(dc, VERTSIZE).max(1) * 100))
                .max(1) as i32,
            };
            SetMapMode(dc, MM_ANISOTROPIC);
            let _ = SetWindowExtEx(dc, width, height, None);
            let _ = SetViewportExtEx(dc, viewport.cx, viewport.cy, None);
            let painted = AlphaBlend(
                dc,
                0,
                0,
                width,
                height,
                source.dc,
                0,
                0,
                width,
                height,
                BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                },
            )
            .as_bool();
            let emf = CloseEnhMetaFile(dc);
            if emf.is_invalid() {
                return Err(Error::from_win32());
            }
            let size = GetEnhMetaFileBits(emf, None);
            if !painted || size == 0 || size as usize > LIMIT {
                let _ = DeleteEnhMetaFile(emf);
                return Err(error("Could not render Office preview"));
            }
            let mut bytes = vec![0; size as usize];
            let written = GetEnhMetaFileBits(emf, Some(&mut bytes));
            let _ = DeleteEnhMetaFile(emf);
            if written != size {
                return Err(error("Incomplete Office preview"));
            }
            bytes
        };
        Ok(Self {
            document,
            png,
            emf,
            extent,
        })
    }
    pub fn load(storage: &IStorage) -> CResult<Self> {
        let document = read_stream(storage, w!("ReShiki.Drawing"))?;
        let png = read_stream(storage, w!("ReShiki.Preview"))?;
        let emf = match read_stream(storage, w!("ReShiki.Metafile")) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.code() == STG_E_FILENOTFOUND => None,
            Err(error) => return Err(error),
        };
        Self::new(document, png, emf)
    }
    pub fn save(&self, storage: &IStorage) -> CResult<()> {
        trace("Save storage");
        write_stream(storage, w!("ReShiki.Drawing"), &self.document)?;
        write_stream(storage, w!("ReShiki.Preview"), &self.png)?;
        write_stream(storage, w!("ReShiki.Metafile"), &self.emf)?;
        unsafe {
            WriteClassStg(storage, &CLSID)?;
            WriteFmtUserTypeStg(
                storage,
                clipboard::format("dev.reshiki.drawing").map_err(error)? as u16,
                w!("ReShiki drawing"),
            )?;
            let cache: IOleCache = CreateDataCache(None, &CLSID)?;
            let persistence: IPersistStorage = cache.cast()?;
            persistence.InitNew(storage)?;
            let format = format_etc(14, TYMED_ENHMF);
            cache.Cache(&format, ADVF_PRIMEFIRST.0 as u32)?;
            let medium = Medium(self.metafile()?);
            cache.SetData(&format, &medium.0, false)?;
            persistence.Save(storage, true)?;
            persistence.SaveCompleted(None)?;
        }
        Ok(())
    }
    pub fn storage(&self) -> CResult<IStorage> {
        unsafe {
            let bytes = CreateILockBytesOnHGlobal(None, true)?;
            let storage = StgCreateDocfileOnILockBytes(
                &bytes,
                STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE,
                0,
            )?;
            self.save(&storage)?;
            Ok(storage)
        }
    }
    pub fn metafile(&self) -> CResult<STGMEDIUM> {
        let handle = unsafe { SetEnhMetaFileBits(&self.emf) };
        if handle.is_invalid() {
            return Err(Error::from_win32());
        }
        Ok(STGMEDIUM {
            tymed: TYMED_ENHMF.0 as u32,
            u: STGMEDIUM_0 {
                hEnhMetaFile: handle,
            },
            pUnkForRelease: ManuallyDrop::new(None),
        })
    }
    pub fn descriptor(&self) -> CResult<STGMEDIUM> {
        let label: Vec<_> = "ReShiki drawing".encode_utf16().chain(Some(0)).collect();
        let header_size = OBJECT_DESCRIPTOR_HEADER_SIZE;
        let descriptor = OBJECTDESCRIPTOR {
            cbSize: (header_size + label.len() * 2) as u32,
            clsid: CLSID,
            dwDrawAspect: DVASPECT_CONTENT.0,
            sizel: self.extent,
            pointl: POINTL::default(),
            dwStatus: OLEMISC_CANTLINKINSIDE.0 as u32,
            dwFullUserTypeName: header_size as u32,
            dwSrcOfCopy: 0,
        };
        let mut bytes = descriptor_header_bytes(&descriptor);
        bytes.extend(label.into_iter().flat_map(u16::to_le_bytes));
        global(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    #[test]
    fn object_descriptor_header_matches_windows_abi() {
        assert_eq!(size_of::<OBJECTDESCRIPTOR>(), 52);
        assert_eq!(align_of::<OBJECTDESCRIPTOR>(), 4);
        assert_eq!(
            [
                offset_of!(OBJECTDESCRIPTOR, cbSize),
                offset_of!(OBJECTDESCRIPTOR, clsid),
                offset_of!(OBJECTDESCRIPTOR, dwDrawAspect),
                offset_of!(OBJECTDESCRIPTOR, sizel),
                offset_of!(OBJECTDESCRIPTOR, pointl),
                offset_of!(OBJECTDESCRIPTOR, dwStatus),
                offset_of!(OBJECTDESCRIPTOR, dwFullUserTypeName),
                offset_of!(OBJECTDESCRIPTOR, dwSrcOfCopy),
            ],
            [0, 4, 20, 24, 32, 40, 44, 48]
        );
        assert_eq!(size_of::<GUID>(), 16);
        assert_eq!(align_of::<GUID>(), 4);
        assert_eq!(
            [
                offset_of!(GUID, data1),
                offset_of!(GUID, data2),
                offset_of!(GUID, data3),
                offset_of!(GUID, data4),
            ],
            [0, 4, 6, 8]
        );
        assert_eq!((size_of::<SIZE>(), align_of::<SIZE>()), (8, 4));
        assert_eq!((offset_of!(SIZE, cx), offset_of!(SIZE, cy)), (0, 4));
        assert_eq!((size_of::<POINTL>(), align_of::<POINTL>()), (8, 4));
        assert_eq!((offset_of!(POINTL, x), offset_of!(POINTL, y)), (0, 4));
    }

    #[test]
    fn object_descriptor_header_preserves_field_order_and_signed_values() {
        // Distinct field bytes expose swaps and omissions independently of the
        // production metadata; signed geometry must retain its exact bit pattern.
        let descriptor = OBJECTDESCRIPTOR {
            cbSize: 0x04030201,
            clsid: GUID::from_values(0x08070605, 0x0a09, 0x0c0b, [13, 14, 15, 16, 17, 18, 19, 20]),
            dwDrawAspect: 0x18171615,
            sizel: SIZE {
                cx: i32::MIN,
                cy: -1,
            },
            pointl: POINTL { x: -2, y: i32::MAX },
            dwStatus: 0x2c2b2a29,
            dwFullUserTypeName: 0x302f2e2d,
            dwSrcOfCopy: 0x34333231,
        };
        let expected: [u8; 52] = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            0, 0, 0, 128, 255, 255, 255, 255, 254, 255, 255, 255, 255, 255, 255, 127, 41, 42, 43,
            44, 45, 46, 47, 48, 49, 50, 51, 52,
        ];
        assert_eq!(descriptor_header_bytes(&descriptor), expected);
    }

    #[test]
    fn object_descriptor_getdata_returns_expected_clipboard_bytes() {
        let _apartment = Apartment::new().unwrap();
        let data: IDataObject = super::super::object::ClipboardObject {
            drawing: Drawing {
                extent: SIZE { cx: 2540, cy: 1270 },
                ..Drawing::default()
            },
            formats: BTreeMap::new(),
        }
        .into();
        let id = clipboard::format("Object Descriptor").unwrap();
        let format = format_etc(id, TYMED_HGLOBAL);
        // SAFETY: The local IDataObject and complete FORMATETC remain live on
        // this STA. GetData transfers the returned medium to our release guard.
        assert_eq!(unsafe { data.QueryGetData(&format) }, S_OK);
        let medium = Medium(unsafe { data.GetData(&format) }.unwrap());
        assert_eq!(medium.0.tymed, TYMED_HGLOBAL.0 as u32);
        assert!(medium.0.pUnkForRelease.is_none());

        // Independent wire fixture: 52-byte header, our COM GUID, 25.4 × 12.7 mm
        // extent, and 16 UTF-16LE code units including the label's terminal NUL.
        let expected: [u8; 84] = [
            0x54, 0, 0, 0, 0x7e, 0x2b, 0xac, 0x3b, 0xa2, 0x73, 0x3a, 0x4f, 0x9c, 0xe7, 0x5e, 0x9b,
            0x43, 0x8c, 0x59, 0xb4, 1, 0, 0, 0, 0xec, 9, 0, 0, 0xf6, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0x10, 0, 0, 0, 0x34, 0, 0, 0, 0, 0, 0, 0, b'R', 0, b'e', 0, b'S', 0, b'h', 0, b'i',
            0, b'k', 0, b'i', 0, b' ', 0, b'd', 0, b'r', 0, b'a', 0, b'w', 0, b'i', 0, b'n', 0,
            b'g', 0, 0, 0,
        ];
        let bytes = {
            struct Unlock(HGLOBAL);
            impl Drop for Unlock {
                fn drop(&mut self) {
                    // SAFETY: This guard owns one successful lock and drops
                    // before the surrounding Medium releases the allocation.
                    unsafe {
                        let _ = GlobalUnlock(self.0);
                    }
                }
            }

            // SAFETY: The checked medium tag selects hGlobal. Medium retains
            // ownership until after this block has copied and unlocked its data.
            let memory = unsafe { medium.0.u.hGlobal };
            // GlobalSize may include allocation rounding beyond cbSize.
            assert!(unsafe { GlobalSize(memory) } >= expected.len());
            // SAFETY: memory is the live allocation owned by Medium.
            let pointer = unsafe { GlobalLock(memory) };
            assert!(!pointer.is_null());
            let _unlock = Unlock(memory);
            // SAFETY: The allocation is locked, and GlobalSize bounds this
            // byte range. Copying finishes before the lock and owner drop.
            unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), expected.len()) }.to_vec()
        };
        assert_eq!(bytes, expected);
        // SAFETY: The same live object and valid structure are used on this STA;
        // the unsupported medium must be rejected before allocating any output.
        assert_eq!(
            unsafe { data.QueryGetData(&format_etc(id, TYMED_ISTORAGE)) },
            DV_E_FORMATETC
        );
    }

    #[test]
    fn compound_storage_retains_drawing_and_offline_presentation() {
        let _apartment = Apartment::new().unwrap();
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, 40, 20);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: 11811,
            yppu: 11811,
            unit: png::Unit::Meter,
        }));
        let mut pixels = vec![0; 40 * 20 * 4];
        for row in pixels.as_chunks_mut::<160>().0.iter_mut() {
            for pixel in row[10 * 4..20 * 4].as_chunks_mut::<4>().0.iter_mut() {
                pixel.copy_from_slice(&[255, 0, 0, 128]);
            }
            for pixel in row[20 * 4..].as_chunks_mut::<4>().0.iter_mut() {
                pixel.copy_from_slice(&[20, 150, 220, 255]);
            }
        }
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
        let drawing = Drawing::new(
            br#"{"version":15,"atoms":[],"bonds":[]}"#.to_vec(),
            png,
            None,
        )
        .unwrap();
        assert!((drawing.extent.cx - 338).abs() <= 1);
        let storage = drawing.storage().unwrap();
        assert_eq!(unsafe { ReadClassStg(&storage) }.unwrap(), CLSID);
        let restored = Drawing::load(&storage).unwrap();
        assert_eq!(restored.document, drawing.document);
        assert_eq!(restored.png, drawing.png);
        assert_eq!(restored.emf, drawing.emf);
        // Documents made before vector previews did not store this stream.
        // Their original PNG remains a supported fallback.
        unsafe { storage.DestroyElement(w!("ReShiki.Metafile")) }.unwrap();
        let legacy = Drawing::load(&storage).unwrap();
        assert_eq!(legacy.document, drawing.document);
        assert!(!legacy.emf.is_empty());
        // The default handler can display the saved preview without launching
        // ReShiki; this is what a reopened Office document initially uses.
        let cache: IOleCache = unsafe { CreateDataCache(None, &CLSID) }.unwrap();
        unsafe { cache.cast::<IPersistStorage>().unwrap().Load(&storage) }.unwrap();
        let data: IDataObject = cache.cast().unwrap();
        let medium = Medium(unsafe { data.GetData(&format_etc(14, TYMED_ENHMF)) }.unwrap());
        assert_eq!(medium.0.tymed, TYMED_ENHMF.0 as u32);
        assert!(unsafe { GetEnhMetaFileBits(medium.0.u.hEnhMetaFile, None) } > 100);
        // Play the cached image at its physical extent. A missing map-mode
        // transform crops away the colored right half of a high-DPI image.
        unsafe {
            let dc = CreateCompatibleDC(None);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: 160,
                    biHeight: -80,
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = std::ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, None, 0).unwrap();
            let old = SelectObject(dc, bitmap);
            for pixel in std::slice::from_raw_parts_mut(pixels.cast::<u8>(), 160 * 80 * 4)
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
            {
                pixel.copy_from_slice(&[40, 200, 60, 255]);
            }
            assert!(
                PlayEnhMetaFile(
                    dc,
                    medium.0.u.hEnhMetaFile,
                    &RECT {
                        left: 0,
                        top: 0,
                        right: 160,
                        bottom: 80
                    }
                )
                .as_bool()
            );
            let _ = GdiFlush();
            let result = std::slice::from_raw_parts(pixels.cast::<u8>(), 160 * 80 * 4);
            let colored = result
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] > 200 && p[1] > 120 && p[1] < 180 && p[2] < 50)
                .count();
            let at = |x: usize| &result[(40 * 160 + x) * 4..(40 * 160 + x) * 4 + 3];
            let transparent = at(20).to_vec();
            let blended = at(60).to_vec();
            SelectObject(dc, old);
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(dc);
            assert_eq!(
                transparent,
                [40, 200, 60],
                "transparent pixels painted the background"
            );
            assert!(
                blended
                    .iter()
                    .zip([20u8, 100, 158])
                    .all(|(a, b)| a.abs_diff(b) <= 2),
                "semi-transparent color was not blended correctly: {blended:?}"
            );
            assert!(
                colored > 160 * 80 / 4,
                "metafile clipped its high-DPI preview: {colored}"
            );
        }
    }
}
