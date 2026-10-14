//! The elected host: request records, write fences, the dispatch state machine
//! and the listener thread.
use super::{
    ACCEPT_POLL, BUSY_BACKOFF, Event, HANDOFF, LEASE_LIMIT, PREPARE_EXPIRY, REQUEST_LIMIT,
    UI_REPLY,
    admission::Admission,
    native,
    protocol::{self, Command, NativePath, Process, Request, Response, Status},
};
use crate::session::{Binding, Lease, Phase};
use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

pub(super) struct Ordinary {
    id: [u8; 16],
    owner: Process,
    paths: Vec<NativePath>,
    reply: Arc<Admission>,
}
pub(super) struct Session {
    binding: Binding,
    owner: Process,
    request: [u8; 16],
    reply: Arc<Admission>,
    pub(super) committed: bool,
    pub(super) created: Instant,
}
impl Session {
    fn owned_by(&self, owner: Process, token: [u8; 16]) -> bool {
        self.binding.has_token(token) && self.owner == owner
    }
}
#[derive(Default)]
pub(super) struct Records {
    pub(super) ordinary: VecDeque<Ordinary>,
    pub(super) sessions: Vec<Session>,
}

// Guards stay on the listener thread (Windows mutexes are thread-owned).
// A save worker's cloned binding keeps its lease referenced until host ACK and
// write completion, even if the proxy dies and the UI revokes that binding.
pub(super) struct Fence {
    owner: Process,
    binding: Binding,
    _guard: native::Election,
}
#[derive(Default)]
pub(super) struct Fences(pub(super) Vec<Fence>);
impl Fences {
    pub(super) fn reap(&mut self) {
        self.0.retain(|fence| {
            fence
                .binding
                .lease
                .as_ref()
                .is_some_and(|lease| lease.phase() != Phase::Rejected && lease.references() > 1)
        });
    }
    pub(super) fn dispatch(
        &mut self,
        endpoint: &native::Endpoint,
        core: &Core,
        request: Request,
        peer: native::Peer,
    ) -> Status {
        self.reap();
        let mut fenced = None;
        if let Command::Commit { token } = request.command {
            if !request.valid(&peer) || request.generation != Some(core.generation) {
                return Status::Rejected;
            }
            let held = self
                .0
                .iter()
                .any(|fence| fence.owner == request.sender && fence.binding.has_token(token));
            if !held {
                let binding = core.records.lock().ok().and_then(|records| {
                    records
                        .sessions
                        .iter()
                        .find(|entry| {
                            entry.owner == request.sender
                                && entry.reply.get(Duration::ZERO) == Status::Prepared
                                && entry.binding.has_token(token)
                                && entry
                                    .binding
                                    .lease
                                    .as_ref()
                                    .is_some_and(|lease| lease.phase() == Phase::Prepared)
                        })
                        .map(|entry| entry.binding.clone())
                });
                if let Some(binding) = binding {
                    // Windows mutexes recurse on their owning thread. The
                    // listener therefore also excludes a distinct live lease
                    // by file identity before trying the native lock.
                    if self.0.len() >= LEASE_LIMIT
                        || self
                            .0
                            .iter()
                            .any(|fence| fence.binding.same_path(&binding.path))
                    {
                        return Status::Busy;
                    }
                    match endpoint.write_claim(&binding.path) {
                        Ok(Some(guard)) => fenced = Some((binding, guard)),
                        Ok(None) => return Status::Busy,
                        Err(_) => return Status::Rejected,
                    }
                }
            }
        }
        let owner = request.sender;
        let status = core.dispatch(request, peer);
        if let Some((binding, guard)) = fenced
            && let Ok(records) = core.records.lock()
            && records.sessions.iter().any(|entry| {
                entry.owner == owner
                    && entry.committed
                    && binding
                        .lease
                        .as_ref()
                        .is_some_and(|lease| entry.binding.has_token(lease.token))
            })
        {
            self.0.push(Fence {
                owner,
                binding,
                _guard: guard,
            });
        }
        status
    }
}

pub(super) struct Core {
    pub(super) generation: [u8; 16],
    pub(super) process: Process,
    pub(super) events: mpsc::Sender<Event>,
    pub(super) records: Mutex<Records>,
}

impl Core {
    fn prepare_reply(&self, token: [u8; 16], owner: Process, reply: Arc<Admission>) -> Status {
        let status = reply.get(UI_REPLY);
        if matches!(status, Status::Busy | Status::Rejected)
            && let Ok(mut records) = self.records.lock()
            && let Some(index) = records.sessions.iter().position(|entry| {
                entry.owner == owner
                    && !entry.committed
                    && Arc::ptr_eq(&entry.reply, &reply)
                    && entry.binding.has_token(token)
            })
        {
            if let Some(lease) = records
                .sessions
                .get(index)
                .and_then(|entry| entry.binding.lease.as_ref())
            {
                lease.transition(Phase::Prepared, Phase::Rejected);
            }
            // Busy explicitly took no ownership. Release the reservation so
            // startup/modal readiness can recover using the same launch ID.
            if status == Status::Busy {
                records.sessions.remove(index);
            }
        }
        status
    }
    pub(super) fn dispatch(&self, request: Request, peer: native::Peer) -> Status {
        if !request.valid(&peer)
            || request
                .generation
                .is_some_and(|generation| generation != self.generation)
        {
            return Status::Rejected;
        }
        let Ok(mut records) = self.records.lock() else {
            return Status::Busy;
        };
        match request.command {
            Command::Open { paths } => {
                self.dispatch_open(records, request.id, request.sender, paths)
            }
            command @ Command::Prepare { .. } => {
                self.dispatch_prepare(records, request.id, request.sender, command, peer)
            }
            command => self.dispatch_session(&mut records, request.sender, command),
        }
    }
    fn dispatch_open(
        &self,
        mut records: MutexGuard<'_, Records>,
        id: [u8; 16],
        owner: Process,
        paths: Vec<NativePath>,
    ) -> Status {
        if let Some(previous) = records
            .ordinary
            .iter()
            .find(|entry| entry.id == id && entry.owner == owner)
        {
            if previous.paths != paths {
                return Status::Rejected;
            }
            let reply = previous.reply.clone();
            drop(records);
            return reply.get(UI_REPLY);
        }
        if records.ordinary.len() == REQUEST_LIMIT {
            if records
                .ordinary
                .front()
                .is_some_and(|entry| entry.reply.get(Duration::ZERO) != Status::Pending)
            {
                records.ordinary.pop_front();
            } else {
                return Status::Busy;
            }
        }
        let files = match paths
            .iter()
            .map(NativePath::decode)
            .collect::<io::Result<Vec<_>>>()
        {
            Ok(files) => files,
            Err(_) => return Status::Rejected,
        };
        let reply = Admission::new();
        if self
            .events
            .try_send(Event::Open(files, reply.clone()))
            .is_err()
        {
            return Status::Busy;
        }
        records.ordinary.push_back(Ordinary {
            id,
            owner,
            paths,
            reply: reply.clone(),
        });
        drop(records);
        reply.get(UI_REPLY)
    }
    fn dispatch_prepare(
        &self,
        mut records: MutexGuard<'_, Records>,
        id: [u8; 16],
        owner: Process,
        command: Command,
        peer: native::Peer,
    ) -> Status {
        let Command::Prepare { token, path, host } = command else {
            return Status::Rejected;
        };
        // A token collision must still reject a different owner before any
        // capacity/liveness checks; do not scope Prepare deduplication by owner.
        if let Some(previous) = records
            .sessions
            .iter()
            .find(|entry| entry.binding.has_token(token))
        {
            if previous.owner != owner
                || previous.request != id
                || previous.binding.host != host
                || NativePath::encode(&previous.binding.path).ok().as_ref() != Some(&path)
            {
                return Status::Rejected;
            }
            if previous.binding.lease.as_ref().is_some_and(|lease| {
                matches!(lease.phase(), Phase::Closed | Phase::Rejected | Phase::Lost)
            }) {
                return Status::Rejected;
            }
            let reply = previous.reply.clone();
            drop(records);
            return self.prepare_reply(token, owner, reply);
        }
        if records.sessions.len() >= LEASE_LIMIT || !peer.is_alive() {
            return Status::Busy;
        }
        let path = match path.decode() {
            Ok(path) => path,
            Err(_) => return Status::Rejected,
        };
        let lease = Lease::new(token, move || peer.is_alive());
        let binding = Binding {
            path,
            host,
            lease: Some(lease),
        };
        let reply = Admission::new();
        if self
            .events
            .try_send(Event::Prepare(binding.clone(), reply.clone()))
            .is_err()
        {
            return Status::Busy;
        }
        records.sessions.push(Session {
            binding,
            owner,
            request: id,
            reply: reply.clone(),
            committed: false,
            created: Instant::now(),
        });
        drop(records);
        self.prepare_reply(token, owner, reply)
    }
    fn dispatch_session(&self, records: &mut Records, owner: Process, command: Command) -> Status {
        let token = match command {
            Command::Commit { token }
            | Command::Query { token }
            | Command::Ack { token }
            | Command::Abandon { token } => token,
            _ => return Status::Rejected,
        };
        let Some(index) = records
            .sessions
            .iter()
            .position(|entry| entry.owned_by(owner, token))
        else {
            return Status::Unknown;
        };
        let Some(entry) = records.sessions.get_mut(index) else {
            return Status::Unknown;
        };
        let Some(lease) = entry.binding.lease.as_ref() else {
            return Status::Rejected;
        };
        match command {
            Command::Commit { .. } if !entry.committed => {
                if entry.reply.get(Duration::ZERO) != Status::Prepared
                    || lease.phase() != Phase::Prepared
                {
                    return Status::Rejected;
                }
                if self
                    .events
                    .try_send(Event::Commit(entry.binding.clone()))
                    .is_err()
                {
                    return Status::Busy;
                }
                entry.committed = true;
            }
            Command::Abandon { .. }
                if !entry.committed && lease.transition(Phase::Prepared, Phase::Rejected) =>
            {
                entry.reply.set(Status::Rejected);
            }
            _ => {}
        }
        let phase = lease.phase();
        let committed = entry.committed;
        if matches!(command, Command::Ack { .. })
            && matches!(phase, Phase::Closed | Phase::Rejected | Phase::Lost)
        {
            records.sessions.remove(index);
        }
        if committed && phase == Phase::Prepared {
            Status::Pending
        } else {
            Status::Session(phase)
        }
    }
    pub(super) fn reap(&self) {
        if let Ok(mut records) = self.records.lock() {
            records.sessions.retain(|entry| {
                let Some(lease) = entry.binding.lease.as_ref() else {
                    return false;
                };
                if !lease.alive() {
                    lease.transition(Phase::Prepared, Phase::Lost);
                    lease.transition(Phase::Open, Phase::Lost);
                    return false;
                }
                if !entry.committed
                    && entry.created.elapsed() > PREPARE_EXPIRY
                    && lease.transition(Phase::Prepared, Phase::Rejected)
                {
                    entry.reply.set(Status::Rejected);
                }
                true
            });
        }
    }
}

/// One accept, read, dispatch and write round of the listener. GuiHost's thread
/// and the fake-host test child both run it.
pub(super) fn serve_once(
    listener: &mut native::Listener,
    fences: &mut Fences,
    endpoint: &native::Endpoint,
    core: &Core,
    stopping: &AtomicBool,
) {
    let Ok((mut stream, peer)) = listener.accept(ACCEPT_POLL, HANDOFF) else {
        // A temporarily full pipe must not spin the CPU.
        thread::sleep(BUSY_BACKOFF);
        return;
    };
    let Ok(request) = protocol::read::<Request>(&mut stream) else {
        return;
    };
    let status = if stopping.load(Ordering::Acquire) {
        Status::Busy
    } else {
        fences.dispatch(endpoint, core, request, peer)
    };
    let _ = protocol::write(
        &mut stream,
        &Response::new(core.generation, core.process, status),
    );
}

pub(crate) struct GuiHost {
    _election: native::Election,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    events: Option<mpsc::Receiver<Event>>,
}
impl GuiHost {
    pub(super) fn start(
        endpoint: &native::Endpoint,
        election: native::Election,
    ) -> io::Result<Self> {
        let mut listener = endpoint.listen(&election)?;
        let process = Process::from(&endpoint.own_process()?);
        let (events, receiver) = mpsc::channel(16);
        let core = Arc::new(Core {
            generation: native::random_token()?,
            process,
            events,
            records: Mutex::new(Records::default()),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let endpoint = endpoint.clone();
        let thread = thread::Builder::new()
            .name("reshiki-desktop".into())
            .spawn(move || {
                let mut fences = Fences::default();
                while !stopping.load(Ordering::Acquire) {
                    core.reap();
                    fences.reap();
                    serve_once(&mut listener, &mut fences, &endpoint, &core, &stopping);
                }
            })?;
        Ok(Self {
            _election: election,
            stop,
            thread: Some(thread),
            events: Some(receiver),
        })
    }
    pub(crate) fn take_events(&mut self) -> Option<mpsc::Receiver<Event>> {
        self.events.take()
    }
}
impl Drop for GuiHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
