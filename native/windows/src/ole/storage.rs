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
mod tests;
