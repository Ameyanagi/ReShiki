use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::mem::ManuallyDrop;

const CHEMDRAW: &[u8] = include_bytes!("../../../../../tests/fixtures/native-ethyl-clipboard.cdx");
const FOREIGN: GUID = GUID::from_u128(0x920a1869_a774_4e43_8910_22a4105c9615);

#[implement(IEnumFORMATETC)]
struct Formats {
    formats: Vec<FORMATETC>,
    at: Cell<usize>,
}

impl IEnumFORMATETC_Impl for Formats_Impl {
    fn Next(&self, count: u32, output: *mut FORMATETC, fetched: *mut u32) -> HRESULT {
        if output.is_null() || (fetched.is_null() && count != 1) {
            return E_POINTER;
        }
        let at = self.at.get();
        let amount = (count as usize).min(self.formats.len().saturating_sub(at));
        unsafe {
            for index in 0..amount {
                *output.add(index) = self.formats[at + index];
            }
            if !fetched.is_null() {
                *fetched = amount as u32;
            }
        }
        self.at.set(at + amount);
        if amount == count as usize {
            S_OK
        } else {
            S_FALSE
        }
    }
    fn Skip(&self, count: u32) -> CResult<()> {
        self.at
            .set((self.at.get() + count as usize).min(self.formats.len()));
        Ok(())
    }
    fn Reset(&self) -> CResult<()> {
        self.at.set(0);
        Ok(())
    }
    fn Clone(&self) -> CResult<IEnumFORMATETC> {
        Ok(Formats {
            formats: self.formats.clone(),
            at: Cell::new(self.at.get()),
        }
        .into())
    }
}

#[implement(IDataObject)]
struct ClipboardObject {
    storages: BTreeMap<u32, IStorage>,
    globals: BTreeMap<u32, Vec<u8>>,
    retrievals: Rc<RefCell<BTreeMap<u32, usize>>>,
    failed_retrieval: Option<u32>,
}

impl ClipboardObject {
    fn formats(&self) -> Vec<FORMATETC> {
        let mut formats: Vec<_> = self
            .storages
            .keys()
            .map(|&id| storage::format_etc(id, TYMED_ISTORAGE))
            .collect();
        formats.extend(
            self.globals
                .keys()
                .map(|&id| storage::format_etc(id, TYMED_HGLOBAL)),
        );
        formats
    }
}

impl IDataObject_Impl for ClipboardObject_Impl {
    fn GetData(&self, format: *const FORMATETC) -> CResult<STGMEDIUM> {
        self.QueryGetData(format).ok()?;
        let format = unsafe { &*format };
        let id = u32::from(format.cfFormat);
        if let Some(storage) = self.storages.get(&id) {
            *self.retrievals.borrow_mut().entry(id).or_default() += 1;
            if self.failed_retrieval == Some(id) {
                return Err(Error::new(E_FAIL, "Synthetic embedded retrieval failure"));
            }
            Ok(STGMEDIUM {
                tymed: TYMED_ISTORAGE.0 as u32,
                u: STGMEDIUM_0 {
                    pstg: ManuallyDrop::new(Some(storage.clone())),
                },
                pUnkForRelease: ManuallyDrop::new(None),
            })
        } else {
            storage::global(&self.globals[&id])
        }
    }
    fn GetDataHere(&self, format: *const FORMATETC, medium: *mut STGMEDIUM) -> CResult<()> {
        self.QueryGetData(format).ok()?;
        if medium.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe {
            let Some(storage) = self.storages.get(&u32::from((*format).cfFormat)) else {
                return Err(DV_E_TYMED.into());
            };
            if (*medium).tymed != TYMED_ISTORAGE.0 as u32 {
                return Err(DV_E_TYMED.into());
            }
            let destination = (*medium)
                .u
                .pstg
                .as_ref()
                .ok_or_else(|| Error::from(E_POINTER))?;
            storage.CopyTo(None, None, destination)?;
            WriteClassStg(destination, &ReadClassStg(storage)?)
        }
    }
    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        if format.is_null() {
            return E_POINTER;
        }
        let format = unsafe { &*format };
        if format.dwAspect != DVASPECT_CONTENT.0 {
            return DV_E_DVASPECT;
        }
        if format.lindex != -1 {
            return DV_E_LINDEX;
        }
        if self.formats().iter().any(|supported| {
            supported.cfFormat == format.cfFormat && supported.tymed & format.tymed != 0
        }) {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }
    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, _: *mut FORMATETC) -> HRESULT {
        E_NOTIMPL
    }
    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> CResult<()> {
        Err(E_NOTIMPL.into())
    }
    fn EnumFormatEtc(&self, direction: u32) -> CResult<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        Ok(Formats {
            formats: self.formats(),
            at: Cell::new(0),
        }
        .into())
    }
    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Option<&IAdviseSink>) -> CResult<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
    fn DUnadvise(&self, _: u32) -> CResult<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
    fn EnumDAdvise(&self) -> CResult<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[30, 70, 150, 255])
        .unwrap();
    bytes
}

fn foreign_storage(name: &str, bytes: &[u8], size: Option<u64>, nested: bool) -> CResult<IStorage> {
    unsafe {
        let memory = CreateILockBytesOnHGlobal(None, true)?;
        let root = StgCreateDocfileOnILockBytes(
            &memory,
            STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE,
            0,
        )?;
        WriteClassStg(&root, &FOREIGN)?;
        let storage = if nested {
            root.CreateStorage(
                w!("Nested"),
                STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE,
                0,
                0,
            )?
        } else {
            root.clone()
        };
        let name: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
        let stream = storage.CreateStream(
            PCWSTR(name.as_ptr()),
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
        assert_eq!(written as usize, bytes.len());
        if let Some(size) = size {
            stream.SetSize(size)?;
        }
        stream.Commit(STGC_DEFAULT)?;
        root.Commit(STGC_DEFAULT.0 as u32)?;
        Ok(root)
    }
}

#[test]
fn locked_contents_is_unrecognized_until_the_stream_can_be_opened() {
    std::thread::spawn(|| -> CResult<()> {
        let _apartment = Apartment::new()?;
        let storage = foreign_storage("CONTENTS", CHEMDRAW, None, false)?;
        let held = unsafe {
            storage.OpenStream(w!("CONTENTS"), None, STGM_READ | STGM_SHARE_EXCLUSIVE, 0)?
        };
        // A real compound-storage share violation exercises OpenStream's error
        // path before CDX is recognized, without replacing it with a test stub.
        assert!(
            unsafe {
                storage.OpenStream(w!("CONTENTS"), None, STGM_READ | STGM_SHARE_EXCLUSIVE, 0)
            }
            .is_err()
        );
        assert!(read_contents(&storage)?.is_none());
        drop(held);
        assert_eq!(read_contents(&storage)?, Some(CHEMDRAW.to_vec()));
        Ok(())
    })
    .join()
    .unwrap()
    .unwrap();
}

fn check_reader(
    storages: BTreeMap<u32, IStorage>,
    formats: [u32; 2],
    picture_only: bool,
    allow_foreign: bool,
    failed_retrieval: Option<u32>,
    expected: Option<(&str, &[u8])>,
) -> Result<()> {
    let expected_counts = storages
        .keys()
        .map(|&id| (id, 1_usize))
        .collect::<BTreeMap<_, _>>();
    let retrievals = Rc::new(RefCell::new(BTreeMap::new()));
    let object: IDataObject = ClipboardObject {
        storages,
        globals: BTreeMap::new(),
        retrievals: retrievals.clone(),
        failed_retrieval,
    }
    .into();
    let queried = Cell::new(false);
    let result = super::super::read_data_object(&object, formats, picture_only, || {
        queried.set(true);
        Ok(allow_foreign)
    })?;
    assert_eq!(
        result.map(EmbeddedData::into_parts),
        expected.map(|(kind, bytes)| (kind, bytes.to_vec()))
    );
    assert_eq!(&*retrievals.borrow(), &expected_counts);
    let own_result =
        expected.is_some_and(|(kind, _)| kind == "dev.reshiki.drawing" || kind == "public.png");
    assert_eq!(queried.get(), !picture_only && !own_result);
    Ok(())
}

#[test]
fn embedded_reader_retrieves_each_storage_once_and_checks_both_own_classes_first() {
    // Exercise the actual production IDataObject reader directly on an STA;
    // flushing a clipboard caches the media and hides source retrieval counts.
    std::thread::spawn(|| -> Result<()> {
        let _apartment = Apartment::new()?;
        let source = clipboard::format("Embed Source")?;
        let embedded = clipboard::format("Embedded Object")?;
        let formats = [source, embedded];
        check_reader(
            BTreeMap::from([(source, foreign_storage("CONTENTS", CHEMDRAW, None, false)?)]),
            formats,
            false,
            true,
            None,
            Some(("com.revvity.chemdraw.cdx-clipboard", CHEMDRAW)),
        )?;
        check_reader(
            BTreeMap::from([(
                source,
                foreign_storage("CONTENTS", b"unrelated object", None, false)?,
            )]),
            formats,
            false,
            true,
            None,
            None,
        )?;
        for (picture_only, allow_foreign) in [(false, false), (true, true)] {
            check_reader(
                BTreeMap::from([(
                    source,
                    foreign_storage("CONTENTS", CDX_SIGNATURE, Some(CDX_LIMIT as u64 + 1), false)?,
                )]),
                formats,
                picture_only,
                allow_foreign,
                None,
                None,
            )?;
        }
        let document = br#"{"version":15,"atoms":[],"bonds":[]}"#;
        let preview = png();
        for picture_only in [false, true] {
            check_reader(
                BTreeMap::from([
                    (
                        source,
                        foreign_storage(
                            "CONTENTS",
                            CDX_SIGNATURE,
                            Some(CDX_LIMIT as u64 + 1),
                            false,
                        )?,
                    ),
                    (
                        embedded,
                        Drawing::new(document.to_vec(), preview.clone(), None)?.storage()?,
                    ),
                ]),
                formats,
                picture_only,
                true,
                None,
                Some(if picture_only {
                    ("public.png", preview.as_slice())
                } else {
                    ("dev.reshiki.drawing", document.as_slice())
                }),
            )?;
        }
        Ok(())
    })
    .join()
    .unwrap()
    .unwrap();
}

#[test]
fn unavailable_first_embedded_format_preserves_other_own_and_standalone_priority() {
    std::thread::spawn(|| -> Result<()> {
        let _apartment = Apartment::new()?;
        let source = clipboard::format("Embed Source")?;
        let embedded = clipboard::format("Embedded Object")?;
        let formats = [source, embedded];
        let document = br#"{"version":15,"atoms":[],"bonds":[]}"#;
        check_reader(
            BTreeMap::from([
                (source, foreign_storage("CONTENTS", CHEMDRAW, None, false)?),
                (
                    embedded,
                    Drawing::new(document.to_vec(), png(), None)?.storage()?,
                ),
            ]),
            formats,
            false,
            true,
            Some(source),
            Some(("dev.reshiki.drawing", document)),
        )?;
        // Standalone chemistry can proceed when the first OLE acquisition fails.
        check_reader(
            BTreeMap::from([(source, foreign_storage("CONTENTS", CHEMDRAW, None, false)?)]),
            formats,
            false,
            false,
            Some(source),
            None,
        )?;
        // A successfully acquired foreign CDX also wins over an earlier failure.
        check_reader(
            BTreeMap::from([
                (source, foreign_storage("CONTENTS", CHEMDRAW, None, false)?),
                (
                    embedded,
                    foreign_storage("CONTENTS", CHEMDRAW, None, false)?,
                ),
            ]),
            formats,
            false,
            true,
            Some(source),
            Some(("com.revvity.chemdraw.cdx-clipboard", CHEMDRAW)),
        )?;
        // If no usable representation wins, retain the acquisition diagnostic.
        let retrievals = Rc::new(RefCell::new(BTreeMap::new()));
        let object: IDataObject = ClipboardObject {
            storages: BTreeMap::from([(
                source,
                foreign_storage("CONTENTS", CHEMDRAW, None, false)?,
            )]),
            globals: BTreeMap::new(),
            retrievals: retrievals.clone(),
            failed_retrieval: Some(source),
        }
        .into();
        assert!(super::super::read_data_object(&object, formats, false, || Ok(true)).is_err());
        assert_eq!(&*retrievals.borrow(), &BTreeMap::from([(source, 1)]));
        Ok(())
    })
    .join()
    .unwrap()
    .unwrap();
}

fn publish(
    embedded: &'static str,
    create: impl FnOnce() -> CResult<IStorage> + Send + 'static,
    globals: Vec<(u32, Vec<u8>)>,
) {
    // OleFlushClipboard completes all callbacks on this publishing STA and
    // persists its media before the thread exits. The reader's STA therefore
    // does not wait for callbacks on a test thread blocked in join().
    std::thread::spawn(move || -> Result<()> {
        let _apartment = Apartment::new()?;
        let object: IDataObject = ClipboardObject {
            storages: BTreeMap::from([(clipboard::format(embedded)?, create()?)]),
            globals: globals.into_iter().collect(),
            retrievals: Rc::new(RefCell::new(BTreeMap::new())),
            failed_retrieval: None,
        }
        .into();
        unsafe {
            OleSetClipboard(&object)?;
            OleFlushClipboard()?;
        }
        Ok(())
    })
    .join()
    .unwrap()
    .unwrap();
}

fn read(picture_only: bool) -> Result<(String, Vec<u8>)> {
    let operation = if picture_only { "read_picture" } else { "read" };
    let request = serde_json::to_vec(&serde_json::json!({ "operation": operation }))?;
    let packet: serde_json::Value = serde_json::from_slice(&clipboard::invoke(&request)?)?;
    let representations = packet["representations"].as_array().unwrap();
    assert_eq!(representations.len(), 1);
    let item = &representations[0];
    Ok((
        item["type"].as_str().unwrap().into(),
        STANDARD.decode(item["data"].as_str().unwrap())?,
    ))
}

#[test]
fn office_chemdraw_storage_precedes_images_without_changing_image_only_or_native_paste() {
    let _clipboard = clipboard::CLIPBOARD_TEST_LOCK.lock().unwrap();
    assert!(CHEMDRAW.starts_with(CDX_SIGNATURE));
    let preview = png();
    let png_format = clipboard::format("PNG").unwrap();
    let direct_cdx = clipboard::format("ChemDraw Interchange Format").unwrap();
    let cdx_kind = "com.revvity.chemdraw.cdx-clipboard";

    for embedded in ["Embed Source", "Embedded Object"] {
        publish(
            embedded,
            || foreign_storage("CONTENTS", CHEMDRAW, None, false),
            vec![(png_format, preview.clone())],
        );
        assert_eq!(read(false).unwrap(), (cdx_kind.into(), CHEMDRAW.to_vec()));
        assert_eq!(read(true).unwrap(), ("public.png".into(), preview.clone()));
    }

    // Standalone chemistry wins even when a foreign embedded CDX is oversized.
    publish(
        "Embed Source",
        || foreign_storage("CONTENTS", CDX_SIGNATURE, Some(CDX_LIMIT as u64 + 1), false),
        vec![
            (direct_cdx, CHEMDRAW.to_vec()),
            (png_format, preview.clone()),
        ],
    );
    assert_eq!(read(false).unwrap(), (cdx_kind.into(), CHEMDRAW.to_vec()));

    // The application's own embedded drawing still wins over standalone CDX.
    let own_preview = preview.clone();
    let document = br#"{"version":15,"atoms":[],"bonds":[]}"#.to_vec();
    let own_document = document.clone();
    publish(
        "Embedded Object",
        move || Drawing::new(own_document, own_preview, None)?.storage(),
        vec![
            (direct_cdx, CHEMDRAW.to_vec()),
            (png_format, preview.clone()),
        ],
    );
    assert_eq!(
        read(false).unwrap(),
        ("dev.reshiki.drawing".into(), document)
    );
    assert_eq!(read(true).unwrap(), ("public.png".into(), preview.clone()));

    // No arbitrary stream traversal, nested lookup, prefix match or signature scan.
    for (name, bytes, nested) in [
        ("Preview", CHEMDRAW.to_vec(), false),
        ("CONTENTS", CHEMDRAW.to_vec(), true),
        ("CONTENTS", b"unrelated object".to_vec(), false),
        ("CONTENTS", b"VjCD0100".to_vec(), false),
        ("CONTENTS", b"VjCD0100\0\0\0\0".to_vec(), false),
        ("CONTENTS", [b"prefix".as_slice(), CHEMDRAW].concat(), false),
    ] {
        publish(
            "Embed Source",
            move || foreign_storage(name, &bytes, None, nested),
            vec![(png_format, preview.clone())],
        );
        assert_eq!(read(false).unwrap(), ("public.png".into(), preview.clone()));
    }

    // A full signature routes malformed CDX to normal import validation rather
    // than quietly choosing PNG. A recognized oversize stream fails explicitly.
    publish(
        "Embed Source",
        || foreign_storage("CONTENTS", CDX_SIGNATURE, None, false),
        vec![(png_format, preview.clone())],
    );
    assert_eq!(
        read(false).unwrap(),
        (cdx_kind.into(), CDX_SIGNATURE.to_vec())
    );
    publish(
        "Embedded Object",
        || foreign_storage("CONTENTS", CDX_SIGNATURE, Some(CDX_LIMIT as u64 + 1), false),
        vec![(png_format, preview.clone())],
    );
    let error = read(false).unwrap_err().to_string();
    assert!(error.contains("16 MB structure limit"), "{error}");
    assert_eq!(read(true).unwrap(), ("public.png".into(), preview.clone()));

    // Preserve ordinary DIB fallback as well as PNG for unsupported objects.
    let dib = clipboard::to_dib(&preview).unwrap();
    publish(
        "Embedded Object",
        || foreign_storage("CONTENTS", b"unrelated", None, false),
        vec![(8, dib)],
    );
    let (kind, image) = read(false).unwrap();
    assert_eq!(kind, "public.png");
    assert_eq!(
        clipboard::bitmap(&image).unwrap(),
        clipboard::bitmap(&preview).unwrap()
    );
}
