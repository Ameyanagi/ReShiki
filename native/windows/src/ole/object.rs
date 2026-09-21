use super::storage::{format_etc, global};
use super::*;
use std::{ffi::c_void, mem::ManuallyDrop};
use windows::Win32::Graphics::Gdi::LOGPALETTE;

fn id(name: &str) -> CResult<u32> {
    clipboard::format(name).map_err(error)
}
fn supported(
    drawing: &Drawing,
    formats: &BTreeMap<u32, Vec<u8>>,
    preview: bool,
) -> CResult<Vec<FORMATETC>> {
    let mut result = vec![
        format_etc(id("Embed Source")?, TYMED_ISTORAGE),
        format_etc(id("Object Descriptor")?, TYMED_HGLOBAL),
    ];
    if preview && !drawing.emf.is_empty() {
        result.push(format_etc(14, TYMED_ENHMF));
    }
    result.extend(formats.keys().map(|id| format_etc(*id, TYMED_HGLOBAL)));
    Ok(result)
}
fn query(format: *const FORMATETC, supported: &[FORMATETC]) -> HRESULT {
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
    if supported
        .iter()
        .any(|f| f.cfFormat == format.cfFormat && f.tymed & format.tymed != 0)
    {
        S_OK
    } else {
        DV_E_FORMATETC
    }
}
fn get_data(
    drawing: &Drawing,
    formats: &BTreeMap<u32, Vec<u8>>,
    format: *const FORMATETC,
    preview: bool,
) -> CResult<STGMEDIUM> {
    query(format, &supported(drawing, formats, preview)?).ok()?;
    let format = unsafe { &*format };
    let kind = u32::from(format.cfFormat);
    trace(format!("GetData {kind} medium {}", format.tymed));
    if kind == id("Embed Source")? {
        return Ok(STGMEDIUM {
            tymed: TYMED_ISTORAGE.0 as u32,
            u: STGMEDIUM_0 {
                pstg: ManuallyDrop::new(Some(drawing.storage()?)),
            },
            pUnkForRelease: ManuallyDrop::new(None),
        });
    }
    if kind == id("Object Descriptor")? {
        return drawing.descriptor();
    }
    if kind == 14 {
        return drawing.metafile();
    }
    global(
        formats
            .get(&kind)
            .ok_or_else(|| Error::from(DV_E_FORMATETC))?,
    )
}
fn data_here(drawing: &Drawing, format: *const FORMATETC, medium: *mut STGMEDIUM) -> CResult<()> {
    if format.is_null() || medium.is_null() {
        return Err(E_POINTER.into());
    }
    unsafe {
        if (*format).cfFormat as u32 != id("Embed Source")?
            || (*medium).tymed != TYMED_ISTORAGE.0 as u32
        {
            return Err(DV_E_TYMED.into());
        }
        drawing.save(
            (*medium)
                .u
                .pstg
                .as_ref()
                .ok_or_else(|| Error::from(E_POINTER))?,
        )
    }
}
fn canonical(input: *const FORMATETC, output: *mut FORMATETC) -> HRESULT {
    if input.is_null() || output.is_null() {
        return E_POINTER;
    }
    unsafe {
        *output = *input;
        (*output).ptd = std::ptr::null_mut();
    }
    DATA_S_SAMEFORMATETC
}

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
fn enumeration(direction: u32, formats: Vec<FORMATETC>) -> CResult<IEnumFORMATETC> {
    if direction != DATADIR_GET.0 as u32 {
        return Err(E_NOTIMPL.into());
    }
    Ok(Formats {
        formats,
        at: Cell::new(0),
    }
    .into())
}

#[implement(IDataObject)]
pub(super) struct ClipboardObject {
    pub drawing: Drawing,
    pub formats: BTreeMap<u32, Vec<u8>>,
}
impl IDataObject_Impl for ClipboardObject_Impl {
    fn GetData(&self, format: *const FORMATETC) -> CResult<STGMEDIUM> {
        get_data(&self.drawing, &self.formats, format, true)
    }
    fn GetDataHere(&self, format: *const FORMATETC, medium: *mut STGMEDIUM) -> CResult<()> {
        data_here(&self.drawing, format, medium)
    }
    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        supported(&self.drawing, &self.formats, true)
            .map(|f| query(format, &f))
            .unwrap_or(E_FAIL)
    }
    fn GetCanonicalFormatEtc(&self, input: *const FORMATETC, output: *mut FORMATETC) -> HRESULT {
        canonical(input, output)
    }
    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> CResult<()> {
        Err(E_NOTIMPL.into())
    }
    fn EnumFormatEtc(&self, direction: u32) -> CResult<IEnumFORMATETC> {
        enumeration(direction, supported(&self.drawing, &self.formats, true)?)
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

#[implement(IOleObject, IPersistStorage, IDataObject)]
pub(super) struct DrawingObject {
    pub state: Rc<State>,
}
impl DrawingObject_Impl {
    fn formats(&self) -> CResult<BTreeMap<u32, Vec<u8>>> {
        let drawing = self.state.drawing.borrow();
        Ok(BTreeMap::from([
            (id("dev.reshiki.drawing")?, drawing.document.clone()),
            (id("PNG")?, drawing.png.clone()),
        ]))
    }
}
impl IDataObject_Impl for DrawingObject_Impl {
    fn GetData(&self, format: *const FORMATETC) -> CResult<STGMEDIUM> {
        get_data(&self.state.drawing.borrow(), &self.formats()?, format, true)
    }
    fn GetDataHere(&self, format: *const FORMATETC, medium: *mut STGMEDIUM) -> CResult<()> {
        data_here(&self.state.drawing.borrow(), format, medium)
    }
    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        self.formats()
            .and_then(|f| supported(&self.state.drawing.borrow(), &f, true))
            .map(|f| query(format, &f))
            .unwrap_or(E_FAIL)
    }
    fn GetCanonicalFormatEtc(&self, input: *const FORMATETC, output: *mut FORMATETC) -> HRESULT {
        canonical(input, output)
    }
    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> CResult<()> {
        Err(E_NOTIMPL.into())
    }
    fn EnumFormatEtc(&self, direction: u32) -> CResult<IEnumFORMATETC> {
        enumeration(
            direction,
            supported(&self.state.drawing.borrow(), &self.formats()?, true)?,
        )
    }
    fn DAdvise(
        &self,
        format: *const FORMATETC,
        flags: u32,
        sink: Option<&IAdviseSink>,
    ) -> CResult<u32> {
        let data: IDataObject = self.to_interface();
        unsafe { self.state.data_advise.Advise(&data, format, flags, sink) }
    }
    fn DUnadvise(&self, connection: u32) -> CResult<()> {
        unsafe { self.state.data_advise.Unadvise(connection) }
    }
    fn EnumDAdvise(&self) -> CResult<IEnumSTATDATA> {
        unsafe { self.state.data_advise.EnumAdvise() }
    }
}
impl IPersist_Impl for DrawingObject_Impl {
    fn GetClassID(&self) -> CResult<GUID> {
        Ok(CLSID)
    }
}
impl IPersistStorage_Impl for DrawingObject_Impl {
    fn IsDirty(&self) -> HRESULT {
        if self.state.dirty.get() {
            S_OK
        } else {
            S_FALSE
        }
    }
    fn InitNew(&self, storage: Option<&IStorage>) -> CResult<()> {
        *self.state.storage.borrow_mut() = storage.cloned();
        Ok(())
    }
    fn Load(&self, storage: Option<&IStorage>) -> CResult<()> {
        trace("Load embedded drawing");
        let storage = storage.ok_or_else(|| Error::from(E_POINTER))?;
        let drawing = Drawing::load(storage)?;
        *self.state.drawing.borrow_mut() = drawing;
        *self.state.storage.borrow_mut() = Some(storage.clone());
        self.state.dirty.set(false);
        Ok(())
    }
    fn Save(&self, storage: Option<&IStorage>, _: BOOL) -> CResult<()> {
        self.state
            .drawing
            .borrow()
            .save(storage.ok_or_else(|| Error::from(E_POINTER))?)
    }
    fn SaveCompleted(&self, storage: Option<&IStorage>) -> CResult<()> {
        if let Some(storage) = storage {
            *self.state.storage.borrow_mut() = Some(storage.clone());
        }
        self.state.dirty.set(false);
        Ok(())
    }
    fn HandsOffStorage(&self) -> CResult<()> {
        self.state.storage.borrow_mut().take();
        Ok(())
    }
}
impl IOleObject_Impl for DrawingObject_Impl {
    fn SetClientSite(&self, site: Option<&IOleClientSite>) -> CResult<()> {
        *self.state.site.borrow_mut() = site.cloned();
        Ok(())
    }
    fn GetClientSite(&self) -> CResult<IOleClientSite> {
        self.state
            .site
            .borrow()
            .clone()
            .ok_or_else(|| E_FAIL.into())
    }
    fn SetHostNames(&self, _: &PCWSTR, _: &PCWSTR) -> CResult<()> {
        Ok(())
    }
    fn Close(&self, save: &OLECLOSE) -> CResult<()> {
        trace("Close object");
        if *save != OLECLOSE_NOSAVE && self.state.dirty.get() {
            self.state.save_container()?;
        }
        unsafe {
            let _ = self.state.advise.SendOnClose();
        }
        self.state.site.borrow_mut().take();
        self.state.storage.borrow_mut().take();
        Ok(())
    }
    fn SetMoniker(&self, _: &OLEWHICHMK, _: Option<&IMoniker>) -> CResult<()> {
        Err(E_NOTIMPL.into())
    }
    fn GetMoniker(&self, _: &OLEGETMONIKER, _: &OLEWHICHMK) -> CResult<IMoniker> {
        Err(OLE_E_CANT_BINDTOSOURCE.into())
    }
    fn InitFromData(&self, _: Option<&IDataObject>, _: BOOL, _: u32) -> CResult<()> {
        Err(E_NOTIMPL.into())
    }
    fn GetClipboardData(&self, _: u32) -> CResult<IDataObject> {
        Ok(self.to_interface())
    }
    fn DoVerb(
        &self,
        verb: i32,
        _: *const MSG,
        site: Option<&IOleClientSite>,
        _: i32,
        _: HWND,
        _: *const RECT,
    ) -> CResult<()> {
        trace(format!("DoVerb {verb}"));
        if let Some(site) = site {
            *self.state.site.borrow_mut() = Some(site.clone());
        }
        match verb {
            0 | 1 | -1 | -2 | -4 | -5 => self.state.start(),
            -3 => Ok(()),
            _ => Err(OLEOBJ_E_NOVERBS.into()),
        }
    }
    fn EnumVerbs(&self) -> CResult<IEnumOLEVERB> {
        unsafe { OleRegEnumVerbs(&CLSID) }
    }
    fn Update(&self) -> CResult<()> {
        Ok(())
    }
    fn IsUpToDate(&self) -> CResult<()> {
        Ok(())
    }
    fn GetUserClassID(&self) -> CResult<GUID> {
        Ok(CLSID)
    }
    fn GetUserType(&self, _: &USERCLASSTYPE) -> CResult<PWSTR> {
        let name: Vec<_> = "ReShiki drawing".encode_utf16().chain(Some(0)).collect();
        unsafe {
            let pointer = CoTaskMemAlloc(name.len() * 2).cast::<u16>();
            if pointer.is_null() {
                return Err(E_OUTOFMEMORY.into());
            }
            std::ptr::copy_nonoverlapping(name.as_ptr(), pointer, name.len());
            Ok(PWSTR(pointer))
        }
    }
    fn SetExtent(&self, aspect: DVASPECT, size: *const SIZE) -> CResult<()> {
        if aspect != DVASPECT_CONTENT {
            return Err(DV_E_DVASPECT.into());
        }
        if size.is_null() {
            return Err(E_POINTER.into());
        }
        let size = unsafe { *size };
        if size.cx <= 0 || size.cy <= 0 {
            return Err(E_INVALIDARG.into());
        }
        self.state.drawing.borrow_mut().extent = size;
        Ok(())
    }
    fn GetExtent(&self, aspect: DVASPECT) -> CResult<SIZE> {
        if aspect == DVASPECT_CONTENT {
            Ok(self.state.drawing.borrow().extent)
        } else {
            Err(DV_E_DVASPECT.into())
        }
    }
    fn Advise(&self, sink: Option<&IAdviseSink>) -> CResult<u32> {
        unsafe { self.state.advise.Advise(sink) }
    }
    fn Unadvise(&self, connection: u32) -> CResult<()> {
        unsafe { self.state.advise.Unadvise(connection) }
    }
    fn EnumAdvise(&self) -> CResult<IEnumSTATDATA> {
        unsafe { self.state.advise.EnumAdvise() }
    }
    fn GetMiscStatus(&self, _: DVASPECT) -> CResult<OLEMISC> {
        Ok(OLEMISC_CANTLINKINSIDE)
    }
    fn SetColorScheme(&self, _: *const LOGPALETTE) -> CResult<()> {
        Ok(())
    }
}

#[implement(IClassFactory)]
pub(super) struct Factory;
impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Option<&IUnknown>,
        iid: *const GUID,
        output: *mut *mut c_void,
    ) -> CResult<()> {
        if output.is_null() || iid.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe {
            *output = std::ptr::null_mut();
        }
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        trace("Create embedded object");
        let object: IUnknown = DrawingObject {
            state: State::new(Drawing::default())?,
        }
        .into();
        unsafe { object.query(iid, output).ok() }
    }
    fn LockServer(&self, lock: BOOL) -> CResult<()> {
        LOCKS.with(|locks| {
            locks.set(if lock.as_bool() {
                locks.get().saturating_add(1)
            } else {
                locks.get().saturating_sub(1)
            })
        });
        Ok(())
    }
}
