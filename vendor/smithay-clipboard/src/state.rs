use std::collections::HashMap;
use std::io::{Error, ErrorKind, Result};
use std::os::fd::AsFd;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use sctk::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use sctk::data_device_manager::data_offer::{DataOfferError, DataOfferHandler, DragOffer};
use sctk::data_device_manager::data_source::{CopyPasteSource, DataSourceHandler};
use sctk::data_device_manager::{DataDeviceManagerState, WritePipe};
use sctk::primary_selection::PrimarySelectionManagerState;
use sctk::primary_selection::device::{PrimarySelectionDevice, PrimarySelectionDeviceHandler};
use sctk::primary_selection::selection::{PrimarySelectionSource, PrimarySelectionSourceHandler};
use sctk::registry::{ProvidesRegistryState, RegistryState};
use sctk::seat::pointer::{PointerData, PointerEvent, PointerEventKind, PointerHandler};
use sctk::seat::{Capability, SeatHandler, SeatState};
use sctk::{
    delegate_pointer, delegate_primary_selection, delegate_registry, delegate_seat,
    registry_handlers,
};

use sctk::reexports::calloop::LoopHandle;
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::wl_data_device::WlDataDevice;
use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
use sctk::reexports::client::protocol::wl_data_source::WlDataSource;
use sctk::reexports::client::protocol::wl_keyboard::WlKeyboard;
use sctk::reexports::client::protocol::wl_pointer::WlPointer;
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::wp::primary_selection::zv1::client::{
    zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
    zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
};
use wayland_backend::client::ObjectId;

use crate::mime::{ALLOWED_MIME_TYPES, MimeType};
use crate::rich::{Inner, StoredOffer};

mod binary;
mod offers;
mod transfers;

struct ClipboardSource {
    source: CopyPasteSource,
    offer: Arc<StoredOffer>,
}

struct PrimarySource {
    source: PrimarySelectionSource,
    offer: Arc<StoredOffer>,
}

pub struct State {
    pub primary_selection_manager_state: Option<PrimarySelectionManagerState>,
    pub data_device_manager_state: Option<DataDeviceManagerState>,
    pub reply_tx: Sender<Result<String>>,
    pub exit: bool,

    registry_state: RegistryState,
    seat_state: SeatState,

    seats: HashMap<ObjectId, ClipboardSeatState>,
    /// The latest seat which got an event.
    latest_seat: Option<ObjectId>,

    loop_handle: LoopHandle<'static, Self>,
    queue_handle: QueueHandle<Self>,
    client: Arc<Inner>,
    binary: binary::Binary,
    offers: offers::Offers,
    transfers: transfers::Transfers,
    primary_sources: Vec<PrimarySource>,
    data_sources: Vec<ClipboardSource>,
}

impl State {
    #[must_use]
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<Self>,
        loop_handle: LoopHandle<'static, Self>,
        reply_tx: Sender<Result<String>>,
        client: Arc<Inner>,
    ) -> Option<Self> {
        // NOTE: while it's mutable, it's not part of the hash compute.
        #[allow(clippy::mutable_key_type)]
        let mut seats = HashMap::new();

        let data_device_manager_state = DataDeviceManagerState::bind(globals, queue_handle).ok();
        let primary_selection_manager_state =
            PrimarySelectionManagerState::bind(globals, queue_handle).ok();

        // When both globals are not available nothing could be done.
        if data_device_manager_state.is_none() && primary_selection_manager_state.is_none() {
            return None;
        }

        let seat_state = SeatState::new(globals, queue_handle);
        for seat in seat_state.seats() {
            seats.insert(seat.id(), Default::default());
        }

        Some(Self {
            registry_state: RegistryState::new(globals),
            queue_handle: queue_handle.clone(),
            primary_selection_manager_state,
            primary_sources: Vec::new(),
            data_device_manager_state,
            data_sources: Vec::new(),
            latest_seat: None,
            loop_handle,
            exit: false,
            seat_state,
            reply_tx,
            seats,
            client,
            binary: Default::default(),
            offers: Default::default(),
            transfers: Default::default(),
        })
    }

    /// Store selection for the given target.
    ///
    /// Selection source is only created when `Some(())` is returned.
    pub fn store_selection(&mut self, ty: SelectionTarget, contents: String) -> Option<()> {
        let latest = self.latest_seat.as_ref()?;
        let seat = self.seats.get_mut(latest)?;

        if !seat.has_focus || seat.latest_serial == 0 {
            return None;
        }
        let contents: Arc<[u8]> = Arc::from(contents.into_bytes());
        let data =
            ALLOWED_MIME_TYPES.iter().map(|mime| ((*mime).to_owned(), contents.clone())).collect();
        let offer = StoredOffer::new(data, self.client.memory.clone(), false).ok()?;

        match ty {
            SelectionTarget::Clipboard => {
                let mgr = self.data_device_manager_state.as_ref()?;
                if self.data_sources.len() >= binary::MAX_SOURCES {
                    return None;
                }
                let source =
                    mgr.create_copy_paste_source(&self.queue_handle, ALLOWED_MIME_TYPES.iter());
                source.set_selection(seat.data_device.as_ref()?, seat.latest_serial);
                self.data_sources.push(ClipboardSource { source, offer });
            },
            SelectionTarget::Primary => {
                let mgr = self.primary_selection_manager_state.as_ref()?;
                if self.primary_sources.len() >= binary::MAX_SOURCES {
                    return None;
                }
                let source =
                    mgr.create_selection_source(&self.queue_handle, ALLOWED_MIME_TYPES.iter());
                source.set_selection(seat.primary_device.as_ref()?, seat.latest_serial);
                self.primary_sources.push(PrimarySource { source, offer });
            },
        }

        Some(())
    }

    /// Load selection for the given target.
    pub fn load_selection(&mut self, ty: SelectionTarget) -> Result<()> {
        let latest = self
            .latest_seat
            .as_ref()
            .ok_or_else(|| Error::other("no events received on any seat"))?;
        let seat = self.seats.get_mut(latest).ok_or_else(|| Error::other("active seat lost"))?;

        if !seat.has_focus {
            return Err(Error::other("client doesn't have focus"));
        }

        let (read_pipe, mime_type) = match ty {
            SelectionTarget::Clipboard => {
                let selection = seat
                    .data_device
                    .as_ref()
                    .and_then(|data| data.data().selection_offer())
                    .ok_or_else(|| Error::other("selection is empty"))?;

                if !self.offers.valid(selection.inner()) {
                    return Err(Error::other("clipboard offer metadata exceeds its limit"));
                }

                let mime_type =
                    selection.with_mime_types(MimeType::find_allowed).ok_or_else(|| {
                        Error::new(ErrorKind::NotFound, "supported mime-type is not found")
                    })?;

                (
                    selection.receive(mime_type.to_string()).map_err(|err| match err {
                        DataOfferError::InvalidReceive => Error::other("offer is not ready yet"),
                        DataOfferError::Io(err) => err,
                    })?,
                    mime_type,
                )
            },
            SelectionTarget::Primary => {
                let selection = seat
                    .primary_device
                    .as_ref()
                    .and_then(|data| data.data().selection_offer())
                    .ok_or_else(|| Error::other("selection is empty"))?;

                let mime_type =
                    selection.with_mime_types(MimeType::find_allowed).ok_or_else(|| {
                        Error::new(ErrorKind::NotFound, "supported mime-type is not found")
                    })?;

                (selection.receive(mime_type.to_string())?, mime_type)
            },
        };

        self.receive_pipe(
            read_pipe,
            transfers::ReadReply::Text { sender: self.reply_tx.clone(), mime_type },
        );
        Ok(())
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seats.insert(seat.id(), Default::default());
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        let seat_state = self.seats.get_mut(&seat.id()).unwrap();

        match capability {
            Capability::Keyboard => {
                seat_state.keyboard = Some(seat.get_keyboard(qh, seat.id()));

                // Selection sources are tied to the keyboard, so add/remove decives
                // when we gain/loss capability.

                if seat_state.data_device.is_none() && self.data_device_manager_state.is_some() {
                    seat_state.data_device = self
                        .data_device_manager_state
                        .as_ref()
                        .map(|mgr| mgr.get_data_device(qh, &seat));
                }

                if seat_state.primary_device.is_none()
                    && self.primary_selection_manager_state.is_some()
                {
                    seat_state.primary_device = self
                        .primary_selection_manager_state
                        .as_ref()
                        .map(|mgr| mgr.get_selection_device(qh, &seat));
                }
            },
            Capability::Pointer => {
                seat_state.pointer = self.seat_state.get_pointer(qh, &seat).ok();
            },
            _ => (),
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        let seat_state = self.seats.get_mut(&seat.id()).unwrap();
        match capability {
            Capability::Keyboard => {
                seat_state.data_device = None;
                seat_state.primary_device = None;
                seat_state.has_focus = false;
                seat_state.latest_serial = 0;

                if let Some(keyboard) = seat_state.keyboard.take() {
                    if keyboard.version() >= 3 {
                        keyboard.release()
                    }
                }
                self.seat_lost_focus(&seat.id());
            },
            Capability::Pointer => {
                if let Some(pointer) = seat_state.pointer.take() {
                    if pointer.version() >= 3 {
                        pointer.release()
                    }
                }
            },
            _ => (),
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seats.remove(&seat.id());
        self.seat_lost_focus(&seat.id());
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        pointer: &WlPointer,
        events: &[PointerEvent],
    ) {
        let seat = pointer.data::<PointerData>().unwrap().seat();
        let seat_id = seat.id();
        let seat_state = match self.seats.get_mut(&seat_id) {
            Some(seat_state) => seat_state,
            None => return,
        };

        let mut updated_serial = false;
        for event in events {
            match event.kind {
                PointerEventKind::Press { serial, .. }
                | PointerEventKind::Release { serial, .. } => {
                    updated_serial = true;
                    seat_state.latest_serial = serial;
                },
                _ => (),
            }
        }

        // Only update the seat we're using when the serial got updated.
        if updated_serial {
            self.latest_seat = Some(seat_id);
        }
    }
}

impl DataDeviceHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        _: f64,
        _: f64,
        _: &WlSurface,
    ) {
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    // The selection is finished and ready to be used.
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        self.selection_receipt(device);
    }
}

impl DataSourceHandler for State {
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        write_pipe: WritePipe,
    ) {
        // An earlier source can still be serving a pipe while a later Copy is
        // published. Select its own immutable bytes by protocol object identity.
        let offer = self
            .data_sources
            .iter()
            .find(|entry| entry.source.inner() == source)
            .map(|entry| entry.offer.clone());
        if let Some(offer) = offer
            && let Some(bytes) = offer.bytes(&mime)
        {
            self.send_pipe(write_pipe, offer, bytes);
        }
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, deleted: &WlDataSource) {
        self.source_cancelled(&deleted.id());
        self.data_sources.retain(|entry| entry.source.inner() != deleted);
    }

    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
}

impl DataOfferHandler for State {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

impl ProvidesRegistryState for State {
    registry_handlers![SeatState];

    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
}

impl PrimarySelectionDeviceHandler for State {
    fn selection(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ZwpPrimarySelectionDeviceV1,
    ) {
    }
}

impl PrimarySelectionSourceHandler for State {
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &ZwpPrimarySelectionSourceV1,
        mime: String,
        write_pipe: WritePipe,
    ) {
        let offer = self
            .primary_sources
            .iter()
            .find(|entry| entry.source.inner() == source)
            .map(|entry| entry.offer.clone());
        if let Some(offer) = offer
            && let Some(bytes) = offer.bytes(&mime)
        {
            self.send_pipe(write_pipe, offer, bytes);
        }
    }

    fn cancelled(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        deleted: &ZwpPrimarySelectionSourceV1,
    ) {
        self.primary_sources.retain(|entry| entry.source.inner() != deleted);
    }
}

impl Dispatch<WlKeyboard, ObjectId, State> for State {
    fn event(
        state: &mut State,
        _: &WlKeyboard,
        event: <WlKeyboard as sctk::reexports::client::Proxy>::Event,
        data: &ObjectId,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use sctk::reexports::client::protocol::wl_keyboard::Event as WlKeyboardEvent;
        let seat_state = match state.seats.get_mut(data) {
            Some(seat_state) => seat_state,
            None => return,
        };
        match event {
            WlKeyboardEvent::Key { serial, .. } | WlKeyboardEvent::Modifiers { serial, .. } => {
                seat_state.latest_serial = serial;
                state.latest_seat = Some(data.clone());
            },
            // NOTE both selections rely on keyboard focus.
            WlKeyboardEvent::Enter { serial, .. } => {
                seat_state.latest_serial = serial;
                seat_state.has_focus = true;
                state.latest_seat = Some(data.clone());
            },
            WlKeyboardEvent::Leave { .. } => {
                seat_state.latest_serial = 0;
                seat_state.has_focus = false;
                state.seat_lost_focus(data);
            },
            _ => (),
        }
    }
}

delegate_seat!(State);
delegate_pointer!(State);
delegate_primary_selection!(State);
delegate_registry!(State);

#[derive(Debug, Clone, Copy)]
pub enum SelectionTarget {
    /// The target is clipboard selection.
    Clipboard,
    /// The target is primary selection.
    Primary,
}

#[derive(Debug, Default)]
struct ClipboardSeatState {
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,
    data_device: Option<DataDevice>,
    primary_device: Option<PrimarySelectionDevice>,
    has_focus: bool,

    /// The latest serial used to set the selection content.
    latest_serial: u32,
}

impl Drop for ClipboardSeatState {
    fn drop(&mut self) {
        if let Some(keyboard) = self.keyboard.take() {
            if keyboard.version() >= 3 {
                keyboard.release();
            }
        }

        if let Some(pointer) = self.pointer.take() {
            if pointer.version() >= 3 {
                pointer.release();
            }
        }
    }
}

fn set_non_blocking(fd: impl AsFd) -> std::io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(&fd)?;
    rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK)?;
    Ok(())
}
