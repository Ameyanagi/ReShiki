//! The launcher side: request calls and the watched Office proxy.
use super::{
    ACCEPT_POLL, HANDOFF, PROXY_CALL_TIMEOUT, RETRY_INTERVAL, WATCH_INTERVAL,
    launch::{absolute, bootstrap},
    native,
    protocol::{self, Command, NativePath, Process, Request, Response, Status},
};
use crate::session::{Arguments, Host, Phase};
use std::{
    io, thread,
    time::{Duration, Instant},
};

pub(super) struct Client {
    endpoint: native::Endpoint,
    pub(super) process: Process,
}
impl Client {
    pub(super) fn new(endpoint: native::Endpoint) -> io::Result<Self> {
        Ok(Self {
            process: Process::from(&endpoint.own_process()?),
            endpoint,
        })
    }
    pub(super) fn call(
        &self,
        request: &Request,
        timeout: Duration,
    ) -> io::Result<(Response, native::Peer)> {
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
    pub(super) fn open_request(&self, request: &mut Request, deadline: Instant) -> bool {
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
            thread::sleep(RETRY_INTERVAL);
        }
        false
    }
    pub(super) fn open(&self, paths: Vec<NativePath>, deadline: Instant) -> bool {
        let Ok(id) = native::random_token() else {
            return false;
        };
        self.open_request(
            &mut Request::new(id, None, self.process, Command::Open { paths }),
            deadline,
        )
    }
}

// The watched Office child is always this proxy, including a cold first launch.
pub(super) fn office_proxy(endpoint: native::Endpoint, startup: &Arguments, host: Host) -> i32 {
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
    let session = match negotiate_lease(&endpoint, source, host) {
        Ok(session) => session,
        Err(code) => return code,
    };
    watch_session(session, paths.into_iter().skip(1).collect())
}

struct PreparedSession {
    client: Client,
    token: [u8; 16],
    generation: [u8; 16],
    owner: native::Peer,
    abandoned: Option<(Client, [u8; 16])>,
}

/// Prepares a lease with the shared host, bootstrapping one if needed. No Commit
/// has been sent yet, so this is the only place that may fall back to a private
/// host.
fn negotiate_lease(
    endpoint: &native::Endpoint,
    source: &NativePath,
    host: Host,
) -> Result<PreparedSession, i32> {
    let token = match native::random_token() {
        Ok(token) => token,
        Err(_) => return Err(1),
    };
    let mut client = match Client::new(endpoint.clone()) {
        Ok(client) => client,
        Err(_) => return Err(1),
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
        match client.call(&request, PROXY_CALL_TIMEOUT) {
            Ok((response, owner)) if response.status == Status::Prepared => {
                if !private {
                    abandoned = None;
                }
                break (response.generation, owner);
            }
            Ok((response, _)) if response.status == Status::Rejected => return Err(2),
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
                return Err(1);
            }
            started = true;
        }
        if Instant::now() >= deadline {
            if private {
                return Err(1);
            }
            // No Commit has been sent. A private host is therefore a safe
            // timeout fallback, without taking over a live desktop endpoint.
            let nonce = match native::random_token() {
                Ok(nonce) => nonce,
                Err(_) => return Err(1),
            };
            client = match endpoint.private(nonce).and_then(Client::new) {
                Ok(client) => client,
                Err(_) => return Err(1),
            };
            if bootstrap(Some(nonce)).is_err() {
                return Err(1);
            }
            private = true;
            deadline = Instant::now() + HANDOFF;
        }
        thread::sleep(RETRY_INTERVAL);
    };
    Ok(PreparedSession {
        client,
        token,
        generation,
        owner,
        abandoned,
    })
}

/// After Commit may have arrived, IPC failure is not editing completion and
/// no private fallback happens.
fn watch_session(session: PreparedSession, mut extras: Vec<NativePath>) -> i32 {
    let PreparedSession {
        client,
        token,
        generation,
        owner,
        mut abandoned,
    } = session;
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
            if old.call(&abandon, ACCEPT_POLL).is_ok() {
                let _ = old.call(
                    &Request::new(token, Some(generation), old.process, Command::Ack { token }),
                    ACCEPT_POLL,
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
        thread::sleep(WATCH_INTERVAL);
    }
}
