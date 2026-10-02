use crate::protocol::{
    self, LIMIT, MAX_FORMATS, MAX_TRANSFERS, Offer, Representation, TRANSFER_TIMEOUT,
};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    pipe::{PipeFlags, pipe_with},
};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Write},
    os::fd::{AsFd, OwnedFd},
    sync::Arc,
    time::Instant,
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    backend::ObjectId,
    delegate_noop, event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry, wl_seat},
};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1 as ext_device, ext_data_control_manager_v1 as ext_manager,
    ext_data_control_offer_v1 as ext_offer, ext_data_control_source_v1 as ext_source,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1 as wlr_device, zwlr_data_control_manager_v1 as wlr_manager,
    zwlr_data_control_offer_v1 as wlr_offer, zwlr_data_control_source_v1 as wlr_source,
};

fn error(error: impl std::fmt::Display) -> String {
    format!("Wayland clipboard: {error}")
}

enum Manager {
    Ext(ext_manager::ExtDataControlManagerV1),
    Wlr(wlr_manager::ZwlrDataControlManagerV1),
}
enum Device {
    Ext(ext_device::ExtDataControlDeviceV1),
    Wlr(wlr_device::ZwlrDataControlDeviceV1),
}
enum Incoming {
    Ext(ext_offer::ExtDataControlOfferV1),
    Wlr(wlr_offer::ZwlrDataControlOfferV1),
}
impl Incoming {
    fn id(&self) -> ObjectId {
        match self {
            Self::Ext(offer) => offer.id(),
            Self::Wlr(offer) => offer.id(),
        }
    }
    fn destroy(&self) {
        match self {
            Self::Ext(offer) => offer.destroy(),
            Self::Wlr(offer) => offer.destroy(),
        }
    }
    fn receive(&self, mime: &str, fd: &OwnedFd) {
        match self {
            Self::Ext(offer) => offer.receive(mime.into(), fd.as_fd()),
            Self::Wlr(offer) => offer.receive(mime.into(), fd.as_fd()),
        }
    }
}
struct Advertised {
    offer: Incoming,
    mimes: BTreeSet<String>,
}
struct Transfer {
    file: File,
    data: Arc<[u8]>,
    offset: usize,
    deadline: Instant,
}

#[derive(Default)]
struct State {
    offers: Vec<Advertised>,
    selection: Option<ObjectId>,
    data: Offer,
    active: bool,
    failure: Option<String>,
    transfers: Vec<Transfer>,
}

impl State {
    fn add_offer(&mut self, offer: Incoming) {
        if self.offers.len() >= 4 {
            offer.destroy();
            self.failure = Some("Wayland compositor supplied too many simultaneous offers".into());
        } else {
            self.offers.push(Advertised {
                offer,
                mimes: BTreeSet::new(),
            });
        }
    }
    fn mime(&mut self, id: ObjectId, mime: String) {
        if let Some(offer) = self.offers.iter_mut().find(|offer| offer.offer.id() == id) {
            if offer.mimes.len() >= MAX_FORMATS || mime.len() > 256 {
                self.failure = Some("Wayland clipboard offered too many formats".into());
            } else {
                offer.mimes.insert(mime);
            }
        }
    }
    fn select(&mut self, id: Option<ObjectId>) {
        self.selection = id.clone();
        self.offers.retain(|offer| {
            let keep = Some(offer.offer.id()) == id;
            if !keep {
                offer.offer.destroy();
            }
            keep
        });
    }
    fn discard(&mut self, id: ObjectId) {
        self.offers.retain(|offer| {
            let keep = offer.offer.id() != id;
            if !keep {
                offer.offer.destroy();
            }
            keep
        });
    }
    fn send(&mut self, mime: &str, fd: OwnedFd) {
        if !self.active || self.transfers.len() >= MAX_TRANSFERS {
            return;
        }
        let Some(data) = self.data.get(mime).cloned() else {
            return;
        };
        // A receiver can stop draining its pipe. Never block the dispatch thread.
        let result = fcntl_getfl(&fd).and_then(|flags| fcntl_setfl(&fd, flags | OFlags::NONBLOCK));
        if result.is_err() {
            return;
        }
        self.transfers.push(Transfer {
            file: File::from(fd),
            data,
            offset: 0,
            deadline: Instant::now() + TRANSFER_TIMEOUT,
        });
    }
    fn send_pending(&mut self) {
        let now = Instant::now();
        self.transfers.retain_mut(|transfer| {
            if now >= transfer.deadline {
                return false;
            }
            let end = transfer
                .offset
                .saturating_add(64 * 1024)
                .min(transfer.data.len());
            let bytes = transfer.data.get(transfer.offset..end).unwrap_or_default();
            match transfer.file.write(bytes) {
                Ok(0) => false,
                Ok(count) => {
                    transfer.offset += count;
                    transfer.offset < transfer.data.len()
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    true
                }
                Err(_) => false,
            }
        });
    }
    fn check(&self) -> Result<(), String> {
        self.failure
            .as_ref()
            .map_or(Ok(()), |error| Err(error.clone()))
    }
}

pub struct Clipboard {
    connection: Connection,
    queue: EventQueue<State>,
    manager: Manager,
    device: Device,
    state: State,
}

impl Clipboard {
    pub fn connect() -> Result<Self, String> {
        let connection = Connection::connect_to_env().map_err(error)?;
        let (globals, mut queue) = registry_queue_init::<State>(&connection).map_err(error)?;
        let handle = queue.handle();
        let seat: wl_seat::WlSeat = globals
            .bind(&handle, 1..=2, ())
            .map_err(|_| "Wayland clipboard has no available seat")?;
        let (manager, device) = if let Ok(manager) =
            globals.bind::<ext_manager::ExtDataControlManagerV1, _, _>(&handle, 1..=1, ())
        {
            let device = manager.get_data_device(&seat, &handle, ());
            (Manager::Ext(manager), Device::Ext(device))
        } else if let Ok(manager) =
            globals.bind::<wlr_manager::ZwlrDataControlManagerV1, _, _>(&handle, 1..=2, ())
        {
            let device = manager.get_data_device(&seat, &handle, ());
            (Manager::Wlr(manager), Device::Wlr(device))
        } else {
            return Err("This Wayland compositor does not expose ext-data-control-v1 or wlr-data-control. Use an X11 desktop session or export the drawing to a file.".into());
        };
        let mut state = State::default();
        queue.roundtrip(&mut state).map_err(error)?;
        state.check()?;
        Ok(Self {
            connection,
            queue,
            manager,
            device,
            state,
        })
    }

    pub fn publish(&mut self, offer: Offer) -> Result<(), String> {
        self.state.data = offer;
        self.state.active = true;
        let handle = self.queue.handle();
        match (&self.manager, &self.device) {
            (Manager::Ext(manager), Device::Ext(device)) => {
                let source = manager.create_data_source(&handle, ());
                for mime in self.state.data.keys() {
                    source.offer(mime.clone());
                }
                device.set_selection(Some(&source));
            }
            (Manager::Wlr(manager), Device::Wlr(device)) => {
                let source = manager.create_data_source(&handle, ());
                for mime in self.state.data.keys() {
                    source.offer(mime.clone());
                }
                device.set_selection(Some(&source));
            }
            _ => return Err("Mismatched Wayland clipboard protocol".into()),
        }
        // The sync response follows set_selection. A disconnected or cancelled
        // source is never acknowledged to the application as a successful copy.
        self.queue.roundtrip(&mut self.state).map_err(error)?;
        self.state.check()?;
        if !self.state.active {
            return Err("Wayland clipboard ownership changed before publication completed".into());
        }
        Ok(())
    }

    fn dispatch(&mut self, reader: Option<&File>) -> Result<(), String> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(error)?;
        self.state.check()?;
        let flush_blocked = match self.connection.flush() {
            Ok(()) => false,
            Err(wayland_client::backend::WaylandError::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                true
            }
            Err(e) => return Err(error(e)),
        };
        let Some(guard) = self.queue.prepare_read() else {
            return Ok(());
        };
        let mut fds = vec![PollFd::new(
            &self.connection,
            if flush_blocked {
                PollFlags::IN | PollFlags::OUT
            } else {
                PollFlags::IN
            },
        )];
        if let Some(reader) = reader {
            fds.push(PollFd::new(reader, PollFlags::IN));
        }
        for transfer in &self.state.transfers {
            fds.push(PollFd::new(&transfer.file, PollFlags::OUT));
        }
        let timeout = Timespec {
            tv_sec: 0,
            tv_nsec: 100_000_000,
        };
        match poll(&mut fds, Some(&timeout)) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(error(e)),
        }
        let ready = fds.first().is_some_and(|fd| {
            fd.revents()
                .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR)
        });
        drop(fds);
        if ready {
            guard.read().map_err(error)?;
        } else {
            drop(guard);
        }
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(error)?;
        self.state.check()
    }

    pub fn serve(&mut self) -> Result<(), String> {
        while self.state.active || !self.state.transfers.is_empty() {
            self.state.send_pending();
            self.dispatch(None)?;
        }
        Ok(())
    }

    pub fn read(&mut self, picture_only: bool) -> Result<Option<Representation>, String> {
        let Some(selection) = self.state.selection.clone() else {
            return Ok(None);
        };
        let Some(offer) = self
            .state
            .offers
            .iter()
            .find(|offer| offer.offer.id() == selection)
        else {
            return Err("Wayland clipboard offer is missing".into());
        };
        let Some((kind, mime)) =
            protocol::choose_type(|mime| offer.mimes.contains(mime), picture_only)
        else {
            return Ok(None);
        };
        let (read, write) = pipe_with(PipeFlags::CLOEXEC).map_err(error)?;
        // Only our receiving end is nonblocking. External selection owners may
        // use ordinary blocking write_all/io::copy on the FD we send them.
        fcntl_setfl(&read, fcntl_getfl(&read).map_err(error)? | OFlags::NONBLOCK).map_err(error)?;
        offer.offer.receive(mime, &write);
        drop(write);
        let mut reader = File::from(read);
        let deadline = Instant::now() + TRANSFER_TIMEOUT;
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            if Instant::now() >= deadline {
                return Err("Wayland clipboard transfer timed out".into());
            }
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if bytes.len().saturating_add(count) > LIMIT {
                        return Err("Wayland clipboard data exceeds 64 MB".into());
                    }
                    bytes.extend_from_slice(
                        buffer.get(..count).ok_or("Invalid clipboard read size")?,
                    );
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    self.dispatch(Some(&reader))?
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(error(e)),
            }
        }
        // The pipe belongs to this captured offer. A one-shot source may exit
        // at EOF; later selection changes do not invalidate its complete bytes.
        protocol::representation(kind, &bytes).map(Some)
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore ext_manager::ExtDataControlManagerV1);
delegate_noop!(State: ignore wlr_manager::ZwlrDataControlManagerV1);

macro_rules! dispatch_protocol {
    ($device:ident, $device_type:ident, $offer:ident, $offer_type:ident, $source:ident, $source_type:ident, $variant:ident) => {
        impl Dispatch<$device::$device_type, ()> for State {
            fn event(state: &mut Self, device: &$device::$device_type, event: $device::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                match event {
                    $device::Event::DataOffer { id } => state.add_offer(Incoming::$variant(id)),
                    $device::Event::Selection { id } => state.select(id.map(|offer| offer.id())),
                    $device::Event::PrimarySelection { id: Some(offer) } => state.discard(offer.id()),
                    $device::Event::Finished => { device.destroy(); state.active = false; state.failure = Some("Wayland clipboard device is no longer available".into()); },
                    _ => {}
                }
            }
            event_created_child!(State, $device::$device_type, [0 => ($offer::$offer_type, ())]);
        }
        impl Dispatch<$offer::$offer_type, ()> for State {
            fn event(state: &mut Self, offer: &$offer::$offer_type, event: $offer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                if let $offer::Event::Offer { mime_type } = event { state.mime(offer.id(), mime_type); }
            }
        }
        impl Dispatch<$source::$source_type, ()> for State {
            fn event(state: &mut Self, source: &$source::$source_type, event: $source::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                match event {
                    $source::Event::Send { mime_type, fd } => state.send(&mime_type, fd),
                    $source::Event::Cancelled => { state.active = false; source.destroy(); },
                    _ => {}
                }
            }
        }
    };
}
dispatch_protocol!(
    ext_device,
    ExtDataControlDeviceV1,
    ext_offer,
    ExtDataControlOfferV1,
    ext_source,
    ExtDataControlSourceV1,
    Ext
);
dispatch_protocol!(
    wlr_device,
    ZwlrDataControlDeviceV1,
    wlr_offer,
    ZwlrDataControlOfferV1,
    wlr_source,
    ZwlrDataControlSourceV1,
    Wlr
);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stalled_receiver_does_not_block_and_transfer_expires() {
        let (reader, writer) = pipe_with(PipeFlags::CLOEXEC).unwrap();
        let mut state = State {
            active: true,
            ..State::default()
        };
        state
            .data
            .insert("image/png".into(), Arc::from(vec![1_u8; 2 * 1024 * 1024]));
        state.send("image/png", writer);
        let start = Instant::now();
        for _ in 0..128 {
            state.send_pending();
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(state.transfers.len(), 1);
        state.transfers.first_mut().unwrap().deadline = Instant::now();
        state.send_pending();
        assert!(state.transfers.is_empty());
        drop(reader);
    }

    #[test]
    fn cancelled_source_finishes_existing_transfer_but_rejects_new_ones() {
        let (read, write) = pipe_with(PipeFlags::CLOEXEC).unwrap();
        let mut state = State {
            active: true,
            ..State::default()
        };
        state
            .data
            .insert("image/png".into(), Arc::from(&b"payload"[..]));
        state.send("image/png", write);
        state.active = false;
        let (_unused, rejected) = pipe_with(PipeFlags::CLOEXEC).unwrap();
        state.send("image/png", rejected);
        assert_eq!(state.transfers.len(), 1);
        state.send_pending();
        let mut bytes = Vec::new();
        File::from(read).read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"payload");
        assert!(state.transfers.is_empty());
    }
}
