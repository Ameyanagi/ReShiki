//! Windows/Linux desktop handoff and long-lived Office launcher proxies.
#[cfg(target_os = "linux")]
use reshiki_linux::desktop as native;
#[cfg(windows)]
use reshiki_windows::desktop as native;
pub(crate) mod protocol;

use crate::app::{
    office::{Binding, Lease, Phase},
    startup::Arguments,
};
use protocol::{Command, NativePath, Process, Request, Response, Status};
use std::{
    collections::VecDeque,
    io,
    process::{Command as Child, Stdio},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

const HANDOFF: Duration = Duration::from_secs(2);
const UI_REPLY: Duration = Duration::from_millis(750);
const LEASE_LIMIT: usize = 32;
const REQUEST_LIMIT: usize = 64;

#[derive(Debug, Clone)]
pub(crate) enum Event {
    Open(Vec<std::path::PathBuf>, Arc<Reply>),
    Prepare(Binding, Arc<Reply>),
    Commit(Binding),
}

#[derive(Debug)]
pub(crate) struct Reply {
    value: Mutex<Status>,
    changed: Condvar,
}
impl Reply {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            value: Mutex::new(Status::Pending),
            changed: Condvar::new(),
        })
    }
    pub(crate) fn set(&self, status: Status) {
        if let Ok(mut value) = self.value.lock() {
            // UI admission is a one-shot decision. A late callback must not
            // change a rejection/expiry into an accepted request.
            if *value == Status::Pending && status != Status::Pending {
                *value = status;
                self.changed.notify_all();
            }
        }
    }
    pub(crate) fn get(&self, timeout: Duration) -> Status {
        let Ok(value) = self.value.lock() else {
            return Status::Busy;
        };
        self.changed
            .wait_timeout_while(value, timeout, |value| *value == Status::Pending)
            .map_or(Status::Busy, |(value, _)| *value)
    }
}

struct Ordinary {
    id: [u8; 16],
    owner: Process,
    paths: Vec<NativePath>,
    reply: Arc<Reply>,
}
struct Session {
    binding: Binding,
    owner: Process,
    request: [u8; 16],
    reply: Arc<Reply>,
    committed: bool,
    created: Instant,
}
#[derive(Default)]
struct Records {
    ordinary: VecDeque<Ordinary>,
    sessions: Vec<Session>,
}

// Guards stay on the listener thread (Windows mutexes are thread-owned).
// A save worker's cloned binding keeps its lease referenced until host ACK and
// write completion, even if the proxy dies and the UI revokes that binding.
struct Fence {
    owner: Process,
    binding: Binding,
    _guard: native::Election,
}
#[derive(Default)]
struct Fences(Vec<Fence>);
impl Fences {
    fn reap(&mut self) {
        self.0.retain(|fence| {
            fence
                .binding
                .lease
                .as_ref()
                .is_some_and(|lease| lease.phase() != Phase::Rejected && lease.references() > 1)
        });
    }
    fn dispatch(
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
            let held = self.0.iter().any(|fence| {
                fence.owner == request.sender
                    && fence
                        .binding
                        .lease
                        .as_ref()
                        .is_some_and(|lease| lease.token == token)
            });
            if !held {
                let binding = core.records.lock().ok().and_then(|records| {
                    records
                        .sessions
                        .iter()
                        .find(|entry| {
                            entry.owner == request.sender
                                && entry.reply.get(Duration::ZERO) == Status::Prepared
                                && entry.binding.lease.as_ref().is_some_and(|lease| {
                                    lease.token == token && lease.phase() == Phase::Prepared
                                })
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
                    && entry
                        .binding
                        .lease
                        .as_ref()
                        .zip(binding.lease.as_ref())
                        .is_some_and(|(a, b)| a.token == b.token)
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

struct Core {
    generation: [u8; 16],
    process: Process,
    events: mpsc::Sender<Event>,
    records: Mutex<Records>,
}

impl Core {
    fn prepare_reply(&self, token: [u8; 16], owner: Process, reply: Arc<Reply>) -> Status {
        let status = reply.get(UI_REPLY);
        if matches!(status, Status::Busy | Status::Rejected)
            && let Ok(mut records) = self.records.lock()
            && let Some(index) = records.sessions.iter().position(|entry| {
                entry.owner == owner
                    && !entry.committed
                    && Arc::ptr_eq(&entry.reply, &reply)
                    && entry
                        .binding
                        .lease
                        .as_ref()
                        .is_some_and(|lease| lease.token == token)
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
    fn dispatch(&self, request: Request, peer: native::Peer) -> Status {
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
                if let Some(previous) = records
                    .ordinary
                    .iter()
                    .find(|entry| entry.id == request.id && entry.owner == request.sender)
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
                let reply = Reply::new();
                if self
                    .events
                    .try_send(Event::Open(files, reply.clone()))
                    .is_err()
                {
                    return Status::Busy;
                }
                records.ordinary.push_back(Ordinary {
                    id: request.id,
                    owner: request.sender,
                    paths,
                    reply: reply.clone(),
                });
                drop(records);
                reply.get(UI_REPLY)
            }
            Command::Prepare { token, path, host } => {
                if let Some(previous) = records.sessions.iter().find(|entry| {
                    entry
                        .binding
                        .lease
                        .as_ref()
                        .is_some_and(|lease| lease.token == token)
                }) {
                    if previous.owner != request.sender
                        || previous.request != request.id
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
                    return self.prepare_reply(token, request.sender, reply);
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
                let reply = Reply::new();
                if self
                    .events
                    .try_send(Event::Prepare(binding.clone(), reply.clone()))
                    .is_err()
                {
                    return Status::Busy;
                }
                records.sessions.push(Session {
                    binding,
                    owner: request.sender,
                    request: request.id,
                    reply: reply.clone(),
                    committed: false,
                    created: Instant::now(),
                });
                drop(records);
                self.prepare_reply(token, request.sender, reply)
            }
            command => {
                let (token, kind) = match command {
                    Command::Commit { token } => (token, 0),
                    Command::Query { token } => (token, 1),
                    Command::Ack { token } => (token, 2),
                    Command::Abandon { token } => (token, 3),
                    _ => return Status::Rejected,
                };
                let Some(index) = records.sessions.iter().position(|entry| {
                    entry
                        .binding
                        .lease
                        .as_ref()
                        .is_some_and(|lease| lease.token == token)
                        && entry.owner == request.sender
                }) else {
                    return Status::Unknown;
                };
                let Some(entry) = records.sessions.get_mut(index) else {
                    return Status::Unknown;
                };
                let Some(lease) = entry.binding.lease.as_ref() else {
                    return Status::Rejected;
                };
                if kind == 0 && !entry.committed {
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
                } else if kind == 3
                    && !entry.committed
                    && lease.transition(Phase::Prepared, Phase::Rejected)
                {
                    entry.reply.set(Status::Rejected);
                }
                let phase = lease.phase();
                let committed = entry.committed;
                if kind == 2 && matches!(phase, Phase::Closed | Phase::Rejected | Phase::Lost) {
                    records.sessions.remove(index);
                }
                if committed && phase == Phase::Prepared {
                    Status::Pending
                } else {
                    Status::Session(phase)
                }
            }
        }
    }
    fn reap(&self) {
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
                    && entry.created.elapsed() > Duration::from_secs(5)
                    && lease.transition(Phase::Prepared, Phase::Rejected)
                {
                    entry.reply.set(Status::Rejected);
                }
                true
            });
        }
    }
}

pub(crate) struct Host {
    _election: native::Election,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Host {
    fn start(endpoint: &native::Endpoint, election: native::Election) -> io::Result<Self> {
        let mut listener = endpoint.listen(&election)?;
        let process = Process::from(&endpoint.own_process()?);
        let (events, receiver) = mpsc::channel(16);
        crate::app::install_desktop_events(receiver);
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
                    let Ok((mut stream, peer)) = listener.accept(Duration::from_millis(100)) else {
                        // A temporarily full pipe must not spin the CPU.
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    };
                    let Ok(request) = protocol::read::<Request>(&mut stream) else {
                        continue;
                    };
                    let status = if stopping.load(Ordering::Acquire) {
                        Status::Busy
                    } else {
                        fences.dispatch(&endpoint, &core, request, peer)
                    };
                    let _ = protocol::write(
                        &mut stream,
                        &Response::new(core.generation, core.process, status),
                    );
                }
            })?;
        Ok(Self {
            _election: election,
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Client {
    endpoint: native::Endpoint,
    process: Process,
}
impl Client {
    fn new(endpoint: native::Endpoint) -> io::Result<Self> {
        Ok(Self {
            process: Process::from(&endpoint.own_process()?),
            endpoint,
        })
    }
    fn call(&self, request: &Request, timeout: Duration) -> io::Result<(Response, native::Peer)> {
        let (mut stream, peer) = self.endpoint.connect(timeout)?;
        protocol::write(&mut stream, request)?;
        let response: Response = protocol::read(&mut stream)?;
        if !response.valid(&peer)
            || request
                .generation
                .is_some_and(|generation| generation != response.generation)
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok((response, peer))
    }
    fn open_request(&self, request: &mut Request, deadline: Instant) -> bool {
        while Instant::now() < deadline {
            match self.call(request, deadline.saturating_duration_since(Instant::now())) {
                Ok((response, _)) if response.status == Status::Accepted => return true,
                Ok((response, _)) if response.status == Status::Rejected => return false,
                Ok((response, _)) if response.status == Status::Busy => {
                    // Busy explicitly owns no request; retrying may use a new ID.
                    // Pending/lost ACK always retains the original ID.
                    if let Ok(id) = native::random_token() {
                        request.id = id;
                    }
                }
                _ => {}
            }
            thread::sleep(Duration::from_millis(40));
        }
        false
    }
    fn open(&self, paths: Vec<NativePath>, deadline: Instant) -> bool {
        let Ok(id) = native::random_token() else {
            return false;
        };
        self.open_request(
            &mut Request::new(id, None, self.process, Command::Open { paths }),
            deadline,
        )
    }
}

fn absolute(args: &Arguments) -> io::Result<Vec<NativePath>> {
    if args.paths.len() > protocol::PATH_LIMIT {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let cwd = std::env::current_dir()?;
    args.paths
        .iter()
        .map(|path| {
            NativePath::encode(&if path.is_absolute() {
                path.clone()
            } else {
                cwd.join(path)
            })
        })
        .collect()
}

fn private_nonce(args: &[std::ffi::OsString]) -> io::Result<Option<[u8; 16]>> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--open" {
            let _ = args.next();
        } else if arg == "--desktop-private" {
            let text = args
                .next()
                .and_then(|arg| arg.to_str())
                .filter(|text| text.len() == 32 && text.is_ascii())
                .ok_or(io::ErrorKind::InvalidInput)?;
            let mut token = [0; 16];
            for (index, byte) in token.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
                    .map_err(|_| io::ErrorKind::InvalidInput)?;
            }
            return Ok(Some(token));
        }
    }
    Ok(None)
}

/// Some GUI actions have their own launch semantics; preserve them separately.
fn forwardable(args: &[std::ffi::OsString]) -> bool {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--open" || arg == "--desktop-private" {
            if args.next().is_none() {
                return false;
            }
        } else if ![
            "--office-edit",
            "--libreoffice-edit",
            "--office-addin-edit",
            "--desktop-host",
        ]
        .iter()
        .any(|flag| arg == flag)
        {
            return false;
        }
    }
    true
}

fn office_role(
    args: &[std::ffi::OsString],
    startup: &Arguments,
) -> io::Result<Option<crate::app::office::Host>> {
    match startup.office_host {
        Some(host) if !startup.paths.is_empty() && forwardable(args) => Ok(Some(host)),
        Some(_) => Err(io::ErrorKind::InvalidInput.into()),
        None => Ok(None),
    }
}

/// Returns the elected GUI host guard; an accepted secondary launch exits here.
pub(crate) fn launch(args: &[std::ffi::OsString], startup: &Arguments) -> Option<Host> {
    // Classify Office before the explicit-GUI bypass: every valid Office
    // launch must be a watched proxy, even when it is the first launch.
    let office = match office_role(args, startup) {
        Ok(office) => office,
        Err(_) => {
            eprintln!("Office editing requires --open and supported edit arguments");
            std::process::exit(2);
        }
    };
    if !forwardable(args) {
        return None;
    }
    let endpoint = native::Endpoint::current().and_then(|endpoint| match private_nonce(args)? {
        Some(token) => endpoint.private(token),
        None => Ok(endpoint),
    });
    let endpoint = match endpoint {
        Ok(endpoint) => endpoint,
        Err(error) => {
            eprintln!("Desktop handoff unavailable: {error}");
            if office.is_some() {
                std::process::exit(1);
            }
            return None;
        }
    };
    if let Some(host) = office {
        std::process::exit(office_proxy(endpoint, startup, host));
    }
    let paths = match absolute(startup) {
        Ok(paths) => paths,
        Err(_) => return None,
    };
    let bootstrap = crate::launch::gui_flag(args.iter().cloned(), "--desktop-host")
        || private_nonce(args).ok().flatten().is_some();
    if bootstrap {
        match endpoint.elect() {
            Ok(Some(election)) => match Host::start(&endpoint, election) {
                Ok(host) => return Some(host),
                Err(error) => {
                    eprintln!("Desktop host startup failed: {error}");
                    std::process::exit(1);
                }
            },
            // A bootstrap child never creates an extra unregistered GUI. Its
            // watching proxy will connect to the winner or make a private host.
            _ => std::process::exit(0),
        }
    }
    let client = match Client::new(endpoint.clone()) {
        Ok(client) => client,
        Err(_) => return None,
    };
    let deadline = Instant::now() + HANDOFF;
    let mut request = Request::new(
        native::random_token().ok()?,
        None,
        client.process,
        Command::Open { paths },
    );
    if client.open_request(&mut request, Instant::now() + Duration::from_millis(100)) {
        std::process::exit(0);
    }
    match endpoint.elect() {
        Ok(Some(election)) => match Host::start(&endpoint, election) {
            Ok(host) => Some(host),
            Err(error) => {
                eprintln!("Desktop handoff unavailable: {error}");
                None
            }
        },
        Ok(None) => {
            if client.open_request(&mut request, deadline) {
                std::process::exit(0);
            }
            None
        }
        Err(_) => None,
    }
}

fn bootstrap(private: Option<[u8; 16]>) -> io::Result<()> {
    let mut child = Child::new(std::env::current_exe()?);
    if let Some(token) = private {
        child.arg("--desktop-private").arg(
            token
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        );
    } else {
        child.arg("--desktop-host");
    }
    child
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

// The watched Office child is always this proxy, including a cold first launch.
// After Commit may have arrived, IPC failure is not editing completion.
fn office_proxy(
    endpoint: native::Endpoint,
    startup: &Arguments,
    host: crate::app::office::Host,
) -> i32 {
    let paths = match absolute(startup) {
        Ok(paths) => paths,
        Err(_) => return 2,
    };
    let Some(source) = paths.first() else {
        return 2;
    };
    let canonical_source = match source.decode() {
        Ok(path) => path,
        Err(_) => return 2,
    };
    let _path_owner = match endpoint.claim(&canonical_source) {
        Ok(Some(owner)) => owner,
        _ => return 2,
    };
    let token = match native::random_token() {
        Ok(token) => token,
        Err(_) => return 1,
    };
    let mut client = match Client::new(endpoint.clone()) {
        Ok(client) => client,
        Err(_) => return 1,
    };
    let request = Request::new(
        token,
        None,
        client.process,
        Command::Prepare {
            token,
            path: source.clone(),
            host,
        },
    );
    let mut private = false;
    let mut started = false;
    let mut deadline = Instant::now() + HANDOFF;
    let mut abandoned: Option<(Client, [u8; 16])> = None;
    let (generation, owner) = loop {
        match client.call(&request, Duration::from_secs(1)) {
            Ok((response, owner)) if response.status == Status::Prepared => {
                if !private {
                    abandoned = None;
                }
                break (response.generation, owner);
            }
            Ok((response, _)) if response.status == Status::Rejected => return 2,
            Ok((response, _)) => {
                if !private {
                    abandoned = Client::new(endpoint.clone())
                        .ok()
                        .map(|client| (client, response.generation));
                }
            }
            Err(_) => {}
        }
        if !started {
            if bootstrap(None).is_err() {
                return 1;
            }
            started = true;
        }
        if Instant::now() >= deadline {
            if private {
                return 1;
            }
            // No Commit has been sent. A private host is therefore a safe
            // timeout fallback, without taking over a live desktop endpoint.
            let nonce = match native::random_token() {
                Ok(nonce) => nonce,
                Err(_) => return 1,
            };
            client = match endpoint.private(nonce).and_then(Client::new) {
                Ok(client) => client,
                Err(_) => return 1,
            };
            if bootstrap(Some(nonce)).is_err() {
                return 1;
            }
            private = true;
            deadline = Instant::now() + HANDOFF;
        }
        thread::sleep(Duration::from_millis(40));
    };
    let commit = Request::new(
        token,
        Some(generation),
        client.process,
        Command::Commit { token },
    );
    let _ = client.call(&commit, HANDOFF);
    let query = Request::new(
        token,
        Some(generation),
        client.process,
        Command::Query { token },
    );
    let mut extras = paths.into_iter().skip(1).collect::<Vec<_>>();
    loop {
        if !owner.is_alive() {
            return 1;
        }
        if let Some((old, generation)) = abandoned.take() {
            let abandon = Request::new(
                token,
                Some(generation),
                old.process,
                Command::Abandon { token },
            );
            if old.call(&abandon, Duration::from_millis(100)).is_ok() {
                let _ = old.call(
                    &Request::new(token, Some(generation), old.process, Command::Ack { token }),
                    Duration::from_millis(100),
                );
            } else {
                abandoned = Some((old, generation));
            }
        }
        if let Ok((response, _)) = client.call(&query, HANDOFF) {
            match response.status {
                Status::Session(Phase::Closed | Phase::Rejected | Phase::Lost) => {
                    let _ = client.call(
                        &Request::new(
                            token,
                            Some(generation),
                            client.process,
                            Command::Ack { token },
                        ),
                        HANDOFF,
                    );
                    return if response.status == Status::Session(Phase::Closed) {
                        0
                    } else {
                        1
                    };
                }
                // The server explicitly confirms Commit never entered its UI
                // queue. Retry that same lease; ambiguous/pending commits wait.
                Status::Session(Phase::Prepared) => {
                    let _ = client.call(&commit, HANDOFF);
                }
                Status::Session(Phase::Open) if !extras.is_empty() => {
                    let _ = client.open(std::mem::take(&mut extras), Instant::now() + HANDOFF);
                }
                _ => {}
            }
        }
        thread::sleep(Duration::from_millis(250));
    }
}

/// Save As and ordinary saves cannot overwrite a path owned by an Office tab
/// in this desktop session. Called and dropped inside the same blocking worker.
pub(crate) fn write_target(path: &std::path::Path) -> Result<native::Election, String> {
    native::Endpoint::current()
        .and_then(|endpoint| endpoint.write_claim(path))
        .map_err(|error| format!("Could not protect the save destination: {error}"))?
        .ok_or_else(|| {
            "This file belongs to an active Office edit session; choose another file".into()
        })
}

#[cfg(test)]
mod tests;
