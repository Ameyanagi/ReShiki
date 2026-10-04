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
            self.wait_io()?;
        }
    }

    fn wait_io(&self) -> Result<(), String> {
        let mut fds = [PollFd::new(self.connection.stream(), PollFlags::IN)];
        let timeout = Timespec {
            tv_sec: 0,
            tv_nsec: 100_000_000,
        };
        match poll(&mut fds, Some(&timeout)) {
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
                let Some(event) = self.connection.poll_for_event().map_err(error)? else {
                    break;
                };
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
                self.wait_io()?;
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
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::time::Duration;

    #[test]
    fn incremental_progress_refreshes_only_the_idle_deadline() {
        let start = Instant::now();
        let mut timing = TransferTiming::new(start);
        timing.progress(start + Duration::from_secs(4)).unwrap();
        assert_eq!(timing.deadline(), start + Duration::from_secs(9));
        timing.progress(start + Duration::from_secs(8)).unwrap();
        assert_eq!(timing.deadline(), start + Duration::from_secs(13));
        for seconds in (12..60).step_by(4) {
            timing
                .progress(start + Duration::from_secs(seconds))
                .unwrap();
        }
        assert_eq!(timing.deadline(), start + TRANSFER_TOTAL_TIMEOUT);
        assert!(timing.progress(start + TRANSFER_TOTAL_TIMEOUT).is_err());
        assert_eq!(timing.deadline(), start + TRANSFER_TOTAL_TIMEOUT);
    }

    #[test]
    fn expired_idle_deadline_cannot_be_revived() {
        let start = Instant::now();
        let mut timing = TransferTiming::new(start);
        let deadline = timing.deadline();
        assert_eq!(deadline, start + TRANSFER_TIMEOUT);
        assert!(timing.progress(deadline).is_err());
        assert_eq!(timing.deadline(), deadline);
        assert!(timing.progress(deadline + Duration::from_secs(1)).is_err());
    }

    fn image_owner(size: usize) -> Clipboard {
        let mut owner = Clipboard::connect().unwrap();
        owner
            .publish(
                protocol::prepare_offer(&[Representation {
                    kind: "public.png".into(),
                    data: STANDARD.encode(vec![37_u8; size]),
                }])
                .unwrap(),
            )
            .unwrap();
        owner
    }

    fn read_offered_targets(count: usize) -> Result<Option<Representation>, String> {
        let mut owner = image_owner(100);
        let target = *owner.data.keys().next().unwrap();
        let mut targets = Vec::new();
        for index in 1..count {
            targets.push(
                owner
                    .connection
                    .intern_atom(false, format!("_RESHIKI_TEST_TARGET_{index}").as_bytes())
                    .unwrap()
                    .reply()
                    .unwrap()
                    .atom,
            );
        }
        // The supported format must be found beyond the outgoing offer limit.
        targets.push(target);
        let worker = std::thread::spawn(move || -> Result<(), String> {
            loop {
                if let Event::SelectionRequest(request) =
                    owner.wait_event(Instant::now() + TRANSFER_TIMEOUT)?
                {
                    if request.target == owner.atoms.targets {
                        owner
                            .connection
                            .change_property32(
                                PropMode::REPLACE,
                                request.requestor,
                                request.property,
                                AtomEnum::ATOM,
                                &targets,
                            )
                            .map_err(error)?
                            .check()
                            .map_err(error)?;
                        owner.notify(&request, request.property)?;
                    } else {
                        owner.request(request)?;
                    }
                    owner.connection.flush().map_err(error)?;
                    if request.target == target || count > MAX_TARGETS {
                        return Ok(());
                    }
                }
            }
        });
        let result = Clipboard::connect().unwrap().read(true);
        let served = worker.join().unwrap();
        if result.is_ok() {
            served.unwrap();
        }
        result
    }

    #[test]
    #[ignore = "requires a dedicated X11 server (run under Xvfb)"]
    fn supported_target_after_many_unknown_targets_is_read() {
        let result = read_offered_targets(MAX_TARGETS).unwrap().unwrap();
        assert_eq!(result.kind, "public.png");
        assert_eq!(STANDARD.decode(result.data).unwrap(), vec![37_u8; 100]);
    }

    #[test]
    #[ignore = "requires a dedicated X11 server (run under Xvfb)"]
    fn oversized_target_list_is_rejected() {
        let result = read_offered_targets(MAX_TARGETS + 1).unwrap_err();
        assert!(result.contains("transfer limit"), "{result}");
    }

    #[test]
    #[ignore = "requires a dedicated X11 server (run under Xvfb)"]
    fn incremental_read_accepts_progress_beyond_the_idle_timeout() {
        let mut owner = image_owner(128);
        owner.chunk = 16;
        let worker = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                match owner.wait_event(deadline).unwrap() {
                    Event::SelectionRequest(request) => owner.request(request).unwrap(),
                    Event::PropertyNotify(event)
                        if event.state == Property::DELETE
                            && owner.transfers.iter().any(|transfer| {
                                transfer.window == event.window && transfer.property == event.atom
                            }) =>
                    {
                        // Nine handshakes exceed five seconds; none is idle that long.
                        std::thread::sleep(Duration::from_millis(750));
                        owner.advance(event.window, event.atom);
                        owner.connection.flush().unwrap();
                        if owner.transfers.is_empty() {
                            break;
                        }
                    }
                    Event::DestroyNotify(_) => break,
                    _ => {}
                }
                owner.connection.flush().unwrap();
            }
        });
        let result = Clipboard::connect().unwrap().read(true);
        worker.join().unwrap();
        assert_eq!(
            STANDARD.decode(result.unwrap().unwrap().data).unwrap(),
            vec![37_u8; 128]
        );
    }

    fn read_incremental_slowly(reader: &Clipboard, target: Atom) -> Result<Vec<u8>, String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let header = reader.request_property(target, LIMIT, deadline)?;
        assert_eq!(header.type_, reader.atoms.incr);
        let mut bytes = Vec::new();
        loop {
            // The real owner's serve loop must extend its deadline on each ACK.
            std::thread::sleep(Duration::from_millis(750));
            reader
                .connection
                .delete_property(reader.window, reader.atoms.property)
                .map_err(error)?
                .check()
                .map_err(error)?;
            reader.connection.flush().map_err(error)?;
            loop {
                if let Event::PropertyNotify(event) = reader.wait_event(deadline)?
                    && event.window == reader.window
                    && event.atom == reader.atoms.property
                    && event.state == Property::NEW_VALUE
                {
                    let chunk = reader.property(128_usize.saturating_sub(bytes.len()))?;
                    if chunk.type_ == NONE {
                        continue;
                    }
                    assert_eq!(chunk.type_, target);
                    assert_eq!(chunk.format, 8);
                    if chunk.value.is_empty() {
                        return Ok(bytes);
                    }
                    bytes.extend_from_slice(&chunk.value);
                    break;
                }
            }
        }
    }

    #[test]
    #[ignore = "requires a dedicated X11 server (run under Xvfb)"]
    fn incremental_write_accepts_progress_beyond_the_idle_timeout() {
        let mut owner = image_owner(128);
        owner.chunk = 16;
        let target = *owner.data.keys().next().unwrap();
        let worker = std::thread::spawn(move || owner.serve().unwrap());
        let reader = Clipboard::connect().unwrap();
        let result = read_incremental_slowly(&reader, target);
        drop(reader);
        let _replacement = image_owner(1);
        worker.join().unwrap();
        assert_eq!(result.unwrap(), vec![37_u8; 128]);
    }

    #[test]
    #[ignore = "requires a dedicated X11 server (run under Xvfb)"]
    fn completed_data_survives_one_shot_owner_exit() {
        let mut owner = image_owner(100);
        let target = *owner.data.keys().next().unwrap();
        let worker = std::thread::spawn(move || {
            loop {
                if let Event::SelectionRequest(request) =
                    owner.wait_event(Instant::now() + TRANSFER_TIMEOUT).unwrap()
                {
                    owner.request(request).unwrap();
                    owner.connection.flush().unwrap();
                    if request.target == target {
                        break;
                    }
                }
            }
        });
        let result = Clipboard::connect().unwrap().read(true).unwrap().unwrap();
        assert_eq!(STANDARD.decode(result.data).unwrap(), vec![37_u8; 100]);
        worker.join().unwrap();
    }

    #[test]
    #[ignore = "requires a dedicated X11 server (run under Xvfb)"]
    fn oversized_incremental_header_is_rejected_before_allocation() {
        let mut owner = image_owner(100);
        let target = *owner.data.keys().next().unwrap();
        let worker = std::thread::spawn(move || {
            loop {
                if let Event::SelectionRequest(request) =
                    owner.wait_event(Instant::now() + TRANSFER_TIMEOUT).unwrap()
                {
                    if request.target == target {
                        owner
                            .connection
                            .change_property32(
                                PropMode::REPLACE,
                                request.requestor,
                                request.property,
                                owner.atoms.incr,
                                &[(LIMIT + 1) as u32],
                            )
                            .unwrap()
                            .check()
                            .unwrap();
                        owner.notify(&request, request.property).unwrap();
                        owner.connection.flush().unwrap();
                        break;
                    }
                    owner.request(request).unwrap();
                    owner.connection.flush().unwrap();
                }
            }
        });
        let result = Clipboard::connect().unwrap().read(true).unwrap_err();
        assert!(result.contains("oversized"), "{result}");
        worker.join().unwrap();
    }

    #[test]
    #[ignore = "requires a dedicated X11 server (run under Xvfb)"]
    fn stalled_incremental_reader_does_not_block_other_readers_or_exit() {
        let mut owner = image_owner(2 * 1024 * 1024);
        let target = *owner.data.keys().next().unwrap();
        let (finished, completion) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = owner.serve();
            finished.send(result).unwrap();
        });
        let stalled = Clipboard::connect().unwrap();
        let header = stalled
            .request_property(target, LIMIT, Instant::now() + TRANSFER_TIMEOUT)
            .unwrap();
        assert_eq!(header.type_, stalled.atoms.incr);
        // Leave INCR unacknowledged, keeping this requestor alive.
        assert!(Clipboard::connect().unwrap().read(true).unwrap().is_some());
        let _replacement = image_owner(1);
        completion
            .recv_timeout(TRANSFER_TIMEOUT + std::time::Duration::from_secs(1))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
    }

    #[test]
    #[ignore = "requires a dedicated X11 server (run under Xvfb)"]
    fn native_and_incremental_image_roundtrip_and_replacement() {
        let png = vec![37_u8; 2 * 1024 * 1024];
        let offer = protocol::prepare_offer(&[
            Representation {
                kind: "dev.reshiki.drawing".into(),
                data: STANDARD.encode(b"{\"version\":1}"),
            },
            Representation {
                kind: "public.png".into(),
                data: STANDARD.encode(&png),
            },
        ])
        .unwrap();
        let mut owner = Clipboard::connect().unwrap();
        owner.publish(offer).unwrap();
        let worker = std::thread::spawn(move || owner.serve().unwrap());
        let native = Clipboard::connect().unwrap().read(false).unwrap().unwrap();
        assert_eq!(native.kind, "dev.reshiki.drawing");
        assert_eq!(STANDARD.decode(native.data).unwrap(), b"{\"version\":1}");
        let image = Clipboard::connect().unwrap().read(true).unwrap().unwrap();
        assert_eq!(image.kind, "public.png");
        assert_eq!(STANDARD.decode(image.data).unwrap(), png);
        let mut replacement = Clipboard::connect().unwrap();
        replacement
            .publish(
                protocol::prepare_offer(&[Representation {
                    kind: "public.utf8-plain-text".into(),
                    data: STANDARD.encode(b"new"),
                }])
                .unwrap(),
            )
            .unwrap();
        worker.join().unwrap();
    }
}
