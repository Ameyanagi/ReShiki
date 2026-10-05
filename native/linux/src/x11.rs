use crate::protocol::{
    self, LIMIT, MAX_FORMATS, MAX_TRANSFERS, Offer, Representation, TRANSFER_TIMEOUT,
    TRANSFER_TOTAL_TIMEOUT,
};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use std::{collections::BTreeMap, sync::Arc, time::Instant};
use x11rb::{
    CURRENT_TIME, NONE,
    connection::{Connection, RequestConnection},
    protocol::{Event, xproto::*},
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

fn error(error: impl std::fmt::Display) -> String {
    format!("X11 clipboard: {error}")
}

// Other applications may advertise many unsupported targets. Bound their atom
// list separately from our own offer/MULTIPLE limit (16 KiB, without interning it).
const MAX_TARGETS: usize = 4096;

struct TransferTiming {
    idle: Instant,
    total: Instant,
}

impl TransferTiming {
    fn new(now: Instant) -> Self {
        Self {
            idle: now + TRANSFER_TIMEOUT,
            total: now + TRANSFER_TOTAL_TIMEOUT,
        }
    }

    fn deadline(&self) -> Instant {
        self.idle.min(self.total)
    }

    fn progress(&mut self, now: Instant) -> Result<(), String> {
        if now >= self.deadline() {
            return Err("X11 clipboard transfer timed out".into());
        }
        self.idle = now + TRANSFER_TIMEOUT;
        Ok(())
    }
}

struct Atoms {
    clipboard: Atom,
    targets: Atom,
    timestamp: Atom,
    incr: Atom,
    multiple: Atom,
    pair: Atom,
    property: Atom,
}
struct Transfer {
    window: Window,
    property: Atom,
    target: Atom,
    data: Arc<[u8]>,
    offset: usize,
    timing: TransferTiming,
}

pub struct Clipboard {
    connection: RustConnection,
    window: Window,
    atoms: Atoms,
    data: BTreeMap<Atom, Arc<[u8]>>,
    transfers: Vec<Transfer>,
    owned: bool,
    timestamp: Timestamp,
    chunk: usize,
}

impl Clipboard {
    pub fn connect() -> Result<Self, String> {
        let (connection, screen) = x11rb::connect(None).map_err(error)?;
        let root = connection
            .setup()
            .roots
            .get(screen)
            .ok_or("X11 screen is unavailable")?
            .root;
        let window = connection.generate_id().map_err(error)?;
        connection
            .create_window(
                0,
                window,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                0,
                &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )
            .map_err(error)?
            .check()
            .map_err(error)?;
        let atom = |name: &[u8]| -> Result<Atom, String> {
            Ok(connection
                .intern_atom(false, name)
                .map_err(error)?
                .reply()
                .map_err(error)?
                .atom)
        };
        let atoms = Atoms {
            clipboard: atom(b"CLIPBOARD")?,
            targets: atom(b"TARGETS")?,
            timestamp: atom(b"TIMESTAMP")?,
            incr: atom(b"INCR")?,
            multiple: atom(b"MULTIPLE")?,
            pair: atom(b"ATOM_PAIR")?,
            property: atom(b"_RESHIKI_CLIPBOARD")?,
        };
        let chunk = connection
            .maximum_request_bytes()
            .saturating_sub(1024)
            .min(64 * 1024);
        if chunk == 0 {
            return Err("X11 maximum request size is too small".into());
        }
        Ok(Self {
            connection,
            window,
            atoms,
            data: BTreeMap::new(),
            transfers: Vec::new(),
            owned: false,
            timestamp: 0,
            chunk,
        })
    }

    fn wait_event(&self, deadline: Instant) -> Result<Event, String> {
        loop {
            if Instant::now() >= deadline {
                return Err("X11 clipboard transfer timed out".into());
            }
            if let Some(event) = self.connection.poll_for_event().map_err(error)? {
                return Ok(event);
            }
            self.wait_io(Some(deadline))?;
        }
    }

    fn wait_io(&self, deadline: Option<Instant>) -> Result<(), String> {
        let mut fds = [PollFd::new(self.connection.stream(), PollFlags::IN)];
        let timeout = deadline.map(|deadline| {
            let remaining = deadline.saturating_duration_since(Instant::now());
            Timespec {
                tv_sec: remaining.as_secs() as _,
                tv_nsec: remaining.subsec_nanos().into(),
            }
        });
        match poll(&mut fds, timeout.as_ref()) {
            Ok(_) | Err(rustix::io::Errno::INTR) => Ok(()),
            Err(e) => Err(error(e)),
        }
    }

    pub fn publish(&mut self, offer: Offer) -> Result<(), String> {
        // Intern every format before acquiring ownership: no partly built offer.
        for (mime, data) in offer {
            let atom = self
                .connection
                .intern_atom(false, mime.as_bytes())
                .map_err(error)?
                .reply()
                .map_err(error)?
                .atom;
            self.data.insert(atom, data);
        }
        self.connection
            .change_property8(
                PropMode::REPLACE,
                self.window,
                self.atoms.property,
                AtomEnum::STRING,
                b"time",
            )
            .map_err(error)?
            .check()
            .map_err(error)?;
        self.connection.flush().map_err(error)?;
        let deadline = Instant::now() + TRANSFER_TIMEOUT;
        self.timestamp = loop {
            if let Event::PropertyNotify(event) = self.wait_event(deadline)?
                && event.window == self.window
                && event.atom == self.atoms.property
            {
                break event.time;
            }
        };
        self.connection
            .set_selection_owner(self.window, self.atoms.clipboard, self.timestamp)
            .map_err(error)?
            .check()
            .map_err(error)?;
        self.connection.flush().map_err(error)?;
        if self.owner()? != self.window {
            return Err("X11 clipboard ownership changed before publication completed".into());
        }
        self.owned = true;
        Ok(())
    }

    fn owner(&self) -> Result<Window, String> {
        Ok(self
            .connection
            .get_selection_owner(self.atoms.clipboard)
            .map_err(error)?
            .reply()
            .map_err(error)?
            .owner)
    }

    fn notify(&self, request: &SelectionRequestEvent, property: Atom) -> Result<(), String> {
        let event = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property,
        };
        self.connection
            .send_event(false, request.requestor, EventMask::NO_EVENT, event)
            .map_err(error)?
            .check()
            .map_err(error)
    }

    fn convert(&mut self, window: Window, target: Atom, property: Atom) -> Result<(), String> {
        if property == NONE
            || self
                .transfers
                .iter()
                .any(|t| t.window == window && t.property == property)
        {
            return Err("Invalid or active X11 transfer property".into());
        }
        if target == self.atoms.targets {
            let mut targets: Vec<Atom> = self.data.keys().copied().collect();
            targets.extend([
                self.atoms.targets,
                self.atoms.timestamp,
                self.atoms.multiple,
            ]);
            self.connection
                .change_property32(
                    PropMode::REPLACE,
                    window,
                    property,
                    AtomEnum::ATOM,
                    &targets,
                )
                .map_err(error)?
                .check()
                .map_err(error)?;
        } else if target == self.atoms.timestamp {
            self.connection
                .change_property32(
                    PropMode::REPLACE,
                    window,
                    property,
                    AtomEnum::INTEGER,
                    &[self.timestamp],
                )
                .map_err(error)?
                .check()
                .map_err(error)?;
        } else {
            let data = self
                .data
                .get(&target)
                .ok_or("Unsupported X11 clipboard target")?
                .clone();
            if data.len() <= self.chunk {
                self.connection
                    .change_property8(PropMode::REPLACE, window, property, target, &data)
                    .map_err(error)?
                    .check()
                    .map_err(error)?;
            } else {
                if self.transfers.len() >= MAX_TRANSFERS {
                    return Err("Too many active X11 transfers".into());
                }
                self.connection
                    .change_window_attributes(
                        window,
                        &ChangeWindowAttributesAux::new()
                            .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
                    )
                    .map_err(error)?
                    .check()
                    .map_err(error)?;
                self.connection
                    .change_property32(
                        PropMode::REPLACE,
                        window,
                        property,
                        self.atoms.incr,
                        &[data.len() as u32],
                    )
                    .map_err(error)?
                    .check()
                    .map_err(error)?;
                self.transfers.push(Transfer {
                    window,
                    property,
                    target,
                    data,
                    offset: 0,
                    timing: TransferTiming::new(Instant::now()),
                });
            }
        }
        Ok(())
    }

    fn request(&mut self, request: SelectionRequestEvent) -> Result<(), String> {
        if !self.owned || request.selection != self.atoms.clipboard {
            return self.notify(&request, NONE);
        }
        let property = if request.property == NONE {
            request.target
        } else {
            request.property
        };
        let converted = if request.target == self.atoms.multiple {
            self.multiple(request.requestor, property)
        } else {
            self.convert(request.requestor, request.target, property)
        };
        self.notify(&request, if converted.is_ok() { property } else { NONE })
    }

    fn multiple(&mut self, window: Window, property: Atom) -> Result<(), String> {
        let reply = self
            .connection
            .get_property(
                false,
                window,
                property,
                self.atoms.pair,
                0,
                (MAX_FORMATS * 2) as u32,
            )
            .map_err(error)?
            .reply()
            .map_err(error)?;
        if reply.type_ != self.atoms.pair || reply.format != 32 || reply.bytes_after != 0 {
            return Err("Invalid X11 MULTIPLE request".into());
        }
        let mut pairs: Vec<u32> = reply.value32().ok_or("Invalid X11 atom pairs")?.collect();
        if !pairs.len().is_multiple_of(2) {
            return Err("Unpaired X11 MULTIPLE request".into());
        }
        for [target, destination] in pairs.as_chunks_mut::<2>().0 {
            if *target == self.atoms.multiple
                || *destination == property
                || self.convert(window, *target, *destination).is_err()
            {
                *destination = NONE;
            }
        }
        self.connection
            .change_property32(PropMode::REPLACE, window, property, self.atoms.pair, &pairs)
            .map_err(error)?
            .check()
            .map_err(error)
    }

    fn advance(&mut self, window: Window, property: Atom) {
        let Some(index) = self
            .transfers
            .iter()
            .position(|t| t.window == window && t.property == property)
        else {
            return;
        };
        // An event queued during this dispatch batch cannot revive an expired
        // transfer before serve() reaches its next pruning pass.
        if self.transfers[index].timing.deadline() <= Instant::now() {
            self.transfers.remove(index);
            self.unwatch(window);
            return;
        }
        let Some(transfer) = self.transfers.get_mut(index) else {
            return;
        };
        let end = transfer
            .offset
            .saturating_add(self.chunk)
            .min(transfer.data.len());
        let data = transfer.data.get(transfer.offset..end).unwrap_or_default();
        let done = data.is_empty();
        let result = self
            .connection
            .change_property8(PropMode::REPLACE, window, property, transfer.target, data)
            .map_err(error)
            .and_then(|cookie| cookie.check().map_err(error))
            .and_then(|()| transfer.timing.progress(Instant::now()));
        transfer.offset = end;
        if done || result.is_err() {
            self.transfers.remove(index);
            self.unwatch(window);
        }
    }

    fn unwatch(&self, window: Window) {
        if !self.transfers.iter().any(|t| t.window == window) {
            let _ = self.connection.change_window_attributes(
                window,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
            );
        }
    }

    pub fn serve(&mut self) -> Result<(), String> {
        let mut pending = None;
        while self.owned || !self.transfers.is_empty() {
            let now = Instant::now();
            let expired: Vec<Window> = self
                .transfers
                .iter()
                .filter(|t| t.timing.deadline() <= now)
                .map(|t| t.window)
                .collect();
            self.transfers.retain(|t| t.timing.deadline() > now);
            for window in expired {
                self.unwatch(window);
            }
            // Limit each dispatch batch so a noisy requestor cannot starve deadlines.
            for _ in 0..256 {
                let event = match pending.take() {
                    Some(event) => Some(event),
                    None => self.connection.poll_for_event().map_err(error)?,
                };
                let Some(event) = event else { break };
                match event {
                    Event::SelectionRequest(request) => {
                        let _ = self.request(request);
                    }
                    Event::SelectionClear(event) if event.selection == self.atoms.clipboard => {
                        self.owned = false
                    }
                    Event::PropertyNotify(event) if event.state == Property::DELETE => {
                        self.advance(event.window, event.atom)
                    }
                    Event::DestroyNotify(event) => {
                        self.transfers.retain(|t| t.window != event.window)
                    }
                    _ => {}
                }
            }
            self.connection.flush().map_err(error)?;
            if self.owned || !self.transfers.is_empty() {
                // Flush can enqueue events too; never block with a buffered event.
                pending = self.connection.poll_for_event().map_err(error)?;
                if pending.is_none() {
                    self.wait_io(self.transfers.iter().map(|t| t.timing.deadline()).min())?;
                }
            }
        }
        Ok(())
    }

    fn request_property(
        &self,
        target: Atom,
        limit: usize,
        deadline: Instant,
    ) -> Result<GetPropertyReply, String> {
        self.connection
            .delete_property(self.window, self.atoms.property)
            .map_err(error)?
            .check()
            .map_err(error)?;
        self.connection
            .convert_selection(
                self.window,
                self.atoms.clipboard,
                target,
                self.atoms.property,
                CURRENT_TIME,
            )
            .map_err(error)?
            .check()
            .map_err(error)?;
        self.connection.flush().map_err(error)?;
        loop {
            if let Event::SelectionNotify(event) = self.wait_event(deadline)?
                && event.requestor == self.window
                && event.selection == self.atoms.clipboard
                && event.target == target
            {
                if event.property == NONE {
                    return Err("X11 clipboard owner refused the requested format".into());
                }
                if event.property != self.atoms.property {
                    return Err("Unexpected X11 clipboard property".into());
                }
                return self.property(limit);
            }
        }
    }

    fn property(&self, limit: usize) -> Result<GetPropertyReply, String> {
        let reply = self
            .connection
            .get_property(
                false,
                self.window,
                self.atoms.property,
                AtomEnum::ANY,
                0,
                limit.div_ceil(4) as u32,
            )
            .map_err(error)?
            .reply()
            .map_err(error)?;
        if reply.bytes_after != 0 || reply.value.len() > limit {
            return Err("X11 clipboard data exceeds the transfer limit".into());
        }
        Ok(reply)
    }

    pub fn read(&mut self, picture_only: bool) -> Result<Option<Representation>, String> {
        let owner = self.owner()?;
        if owner == NONE {
            return Ok(None);
        }
        let mut timing = TransferTiming::new(Instant::now());
        let targets =
            self.request_property(self.atoms.targets, MAX_TARGETS * 4, timing.deadline())?;
        if targets.type_ != u32::from(AtomEnum::ATOM) || targets.format != 32 {
            return Err("Invalid X11 clipboard TARGETS response".into());
        }
        let targets: Vec<u32> = targets
            .value32()
            .ok_or("Invalid X11 clipboard atom list")?
            .collect();
        timing.progress(Instant::now())?;
        let mut known = BTreeMap::new();
        for (_, types, _) in protocol::READABLE {
            for mime in *types {
                let atom = self
                    .connection
                    .intern_atom(true, mime.as_bytes())
                    .map_err(error)?
                    .reply()
                    .map_err(error)?
                    .atom;
                if atom != NONE && targets.contains(&atom) {
                    known.insert(*mime, atom);
                }
            }
        }
        let Some((kind, mime)) =
            protocol::choose_type(|mime| known.contains_key(mime), picture_only)
        else {
            return Ok(None);
        };
        let target = *known.get(mime).ok_or("Clipboard target disappeared")?;
        if self.owner()? != owner {
            return Err("X11 clipboard changed during the read".into());
        }
        let reply = self.request_property(target, LIMIT, timing.deadline())?;
        let bytes = if reply.type_ == self.atoms.incr {
            if reply.format != 32
                || reply.value.len() != 4
                || reply
                    .value32()
                    .and_then(|mut values| values.next())
                    .is_none_or(|size| size as usize > LIMIT)
            {
                return Err("Invalid or oversized X11 incremental transfer".into());
            }
            timing.progress(Instant::now())?;
            // Do not reserve the untrusted announced size. Grow only for received bytes.
            let mut bytes = Vec::new();
            self.connection
                .delete_property(self.window, self.atoms.property)
                .map_err(error)?
                .check()
                .map_err(error)?;
            self.connection.flush().map_err(error)?;
            loop {
                if let Event::PropertyNotify(event) = self.wait_event(timing.deadline())?
                    && event.window == self.window
                    && event.atom == self.atoms.property
                    && event.state == Property::NEW_VALUE
                {
                    let chunk = self.property(LIMIT.saturating_sub(bytes.len()))?;
                    // A stale PropertyNotify for the INCR header can already be queued.
                    if chunk.type_ == NONE {
                        continue;
                    }
                    if chunk.type_ != target || chunk.format != 8 {
                        return Err("Invalid X11 incremental clipboard chunk".into());
                    }
                    timing.progress(Instant::now())?;
                    self.connection
                        .delete_property(self.window, self.atoms.property)
                        .map_err(error)?
                        .check()
                        .map_err(error)?;
                    self.connection.flush().map_err(error)?;
                    if chunk.value.is_empty() {
                        break;
                    }
                    bytes.extend_from_slice(&chunk.value);
                }
            }
            bytes
        } else {
            if reply.type_ != target || reply.format != 8 {
                return Err("Invalid X11 clipboard format".into());
            }
            timing.progress(Instant::now())?;
            reply.value
        };
        // A one-shot owner may release CLIPBOARD immediately after sending the
        // completed payload. The requested type and transfer bounds above are
        // sufficient; this read never combines data from different owners.
        protocol::representation(kind, &bytes).map(Some)
    }
}

#[cfg(test)]
mod tests;
