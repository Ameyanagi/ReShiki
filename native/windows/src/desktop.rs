//! Local, authenticated desktop IPC. No GUI or arbitrary-command dispatch.
use std::{
    io::{self, Read, Write},
    marker::PhantomData,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{
            ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, FILETIME, HANDLE, HLOCAL,
            LocalFree, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            },
            Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
            GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_GROUPS,
            TOKEN_QUERY, TOKEN_USER, TokenLogonSid, TokenUser,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_SHARE_MODE,
            OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION,
            SECURITY_SQOS_PRESENT, WriteFile,
        },
        System::{
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId,
                GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_TYPE_BYTE, PIPE_WAIT,
            },
            RemoteDesktop::ProcessIdToSessionId,
            Threading::{
                CreateEventW, CreateMutexW, GetCurrentProcess, GetCurrentProcessId,
                GetProcessTimes, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_SYNCHRONIZE, ReleaseMutex, WaitForSingleObject,
            },
        },
    },
    core::{PCWSTR, PWSTR},
};

fn error(error: windows::core::Error) -> io::Error {
    io::Error::other(error.to_string())
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}
fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}
fn own(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_invalid() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: Every caller transfers a newly created, non-pseudo handle once.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

#[derive(Clone)]
struct Identity {
    user: String,
    logon: String,
    session: u32,
}

fn sid(
    token: HANDLE,
    class: windows::Win32::Security::TOKEN_INFORMATION_CLASS,
) -> io::Result<String> {
    let mut length = 0;
    // SAFETY: This size query writes only length. TOKEN_* output is stored in
    // usize-aligned backing memory and remains alive while its SID is copied.
    unsafe {
        let _ = GetTokenInformation(token, class, None, 0, &mut length);
    }
    if length == 0 || length > 65536 {
        return Err(io::Error::other("Invalid token size"));
    }
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    unsafe {
        GetTokenInformation(
            token,
            class,
            Some(buffer.as_mut_ptr().cast()),
            length,
            &mut length,
        )
        .map_err(error)?;
        let sid = if class == TokenUser {
            if (length as usize) < size_of::<TOKEN_USER>() {
                return Err(io::Error::other("Invalid user token"));
            }
            (*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid
        } else {
            if (length as usize) < size_of::<TOKEN_GROUPS>() {
                return Err(io::Error::other("Invalid logon token"));
            }
            let groups = &*buffer.as_ptr().cast::<TOKEN_GROUPS>();
            if groups.GroupCount != 1 {
                return Err(io::Error::other("Missing logon SID"));
            }
            groups.Groups[0].Sid
        };
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(sid, &mut text).map_err(error)?;
        let result = text
            .to_string()
            .map_err(|error| io::Error::other(error.to_string()));
        let _ = LocalFree(HLOCAL(text.0.cast()));
        result
    }
}

fn identity(process: HANDLE, pid: u32) -> io::Result<Identity> {
    let mut token = HANDLE::default();
    let mut session = 0;
    // SAFETY: The supplied process handle is live for these queries; its token
    // handle is transferred to OwnedHandle and closed on every return path.
    unsafe {
        OpenProcessToken(process, TOKEN_QUERY, &mut token).map_err(error)?;
        let token = own(token)?;
        ProcessIdToSessionId(pid, &mut session).map_err(error)?;
        Ok(Identity {
            user: sid(raw(&token), TokenUser)?,
            logon: sid(raw(&token), TokenLogonSid)?,
            session,
        })
    }
}

struct Security(PSECURITY_DESCRIPTOR);
impl Security {
    fn new(logon: &str) -> io::Result<Self> {
        let text = wide(&format!("D:P(A;;GA;;;{logon})"));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: The NUL-terminated SDDL is valid for this call. Windows owns
        // the allocated descriptor until LocalFree in Drop.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(text.as_ptr()),
                1,
                &mut descriptor,
                None,
            )
            .map_err(error)?;
        }
        Ok(Self(descriptor))
    }
    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: false.into(),
        }
    }
}
impl Drop for Security {
    fn drop(&mut self) {
        // SAFETY: This is the allocation returned by the SDDL conversion.
        unsafe {
            let _ = LocalFree(HLOCAL(self.0.0));
        }
    }
}

#[derive(Clone)]
pub struct Endpoint {
    identity: Identity,
    name: String,
}

impl Endpoint {
    pub fn current() -> io::Result<Self> {
        // SAFETY: Current-process pseudo handle is only borrowed, never owned.
        let identity = unsafe { identity(GetCurrentProcess(), GetCurrentProcessId())? };
        let name = format!("ReShiki.Desktop.v1.{}.{}", identity.logon, identity.session);
        Ok(Self { identity, name })
    }

    pub fn private(&self, token: [u8; 16]) -> io::Result<Self> {
        let suffix = token
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Ok(Self {
            identity: self.identity.clone(),
            name: format!("{}.{suffix}", self.name),
        })
    }

    /// One watched Office proxy owns a canonical host path across private-host
    /// fallbacks. The mutex remains held on that proxy's launching thread.
    pub fn claim(&self, path: &std::path::Path) -> io::Result<Option<Election>> {
        self.path_claim(path, "Launch")
    }
    pub fn write_claim(&self, path: &std::path::Path) -> io::Result<Option<Election>> {
        self.path_claim(path, "Write")
    }
    fn path_claim(&self, path: &std::path::Path, role: &str) -> io::Result<Option<Election>> {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_owned())
            .hash(&mut hash);
        let name = format!(
            "ReShiki.Office.v1.{role}.{}.{}.{:016x}",
            self.identity.logon,
            self.identity.session,
            hash.finish()
        );
        Self {
            identity: self.identity.clone(),
            name,
        }
        .elect()
    }

    pub fn elect(&self) -> io::Result<Option<Election>> {
        let security = Security::new(&self.identity.logon)?;
        let attributes = security.attributes();
        let name = wide(&format!("Local\\{}", self.name));
        // SAFETY: The descriptor and name remain alive for creation. Mutex
        // ownership is acquired nonblocking and stays on this calling thread.
        let handle = unsafe {
            own(CreateMutexW(Some(&attributes), false, PCWSTR(name.as_ptr())).map_err(error)?)?
        };
        match unsafe { WaitForSingleObject(raw(&handle), 0) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Some(Election {
                handle,
                name: self.name.clone(),
                _thread: PhantomData,
            })),
            WAIT_TIMEOUT => Ok(None),
            _ => Err(io::Error::last_os_error()),
        }
    }

    fn pipe(&self, first: bool) -> io::Result<OwnedHandle> {
        let security = Security::new(&self.identity.logon)?;
        let attributes = security.attributes();
        let name = wide(&format!("\\\\.\\pipe\\{}", self.name));
        let mut mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
        if first {
            mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        // SAFETY: Windows copies name/security during creation. A local-only
        // pipe explicitly grants the current logon SID, never Everyone.
        unsafe {
            own(CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                16,
                4096,
                4096,
                0,
                Some(&attributes),
            ))
        }
    }

    pub fn listen(&self, election: &Election) -> io::Result<Listener> {
        if self.name != election.name {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(Listener {
            endpoint: self.clone(),
            pending: Some(self.pipe(true)?),
        })
    }

    pub fn connect(&self, timeout: Duration) -> io::Result<(Stream, Peer)> {
        let deadline = Instant::now() + timeout;
        let name = wide(&format!("\\\\.\\pipe\\{}", self.name));
        loop {
            // SAFETY: Name is NUL-terminated. Identification-only SQOS prevents
            // an unexpected server from impersonating the launching process.
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    0xc0000000,
                    FILE_SHARE_MODE::default(),
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    None,
                )
            };
            if let Ok(handle) = handle {
                let handle = own(handle)?;
                let mut pid = 0;
                unsafe {
                    GetNamedPipeServerProcessId(raw(&handle), &mut pid).map_err(error)?;
                }
                let peer = Peer::verified(pid, &self.identity)?;
                return Ok((Stream { handle, deadline }, peer));
            }
            if Instant::now() >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn own_process(&self) -> io::Result<Peer> {
        // SAFETY: The returned PID belongs to this process.
        Peer::verified(unsafe { GetCurrentProcessId() }, &self.identity)
    }
}

pub struct Election {
    handle: OwnedHandle,
    name: String,
    _thread: PhantomData<Rc<()>>,
}
impl Drop for Election {
    fn drop(&mut self) {
        // SAFETY: PhantomData<Rc<()>> prevents moving this owner to another thread.
        unsafe {
            let _ = ReleaseMutex(raw(&self.handle));
        }
    }
}

#[derive(Clone)]
pub struct Peer {
    handle: Arc<OwnedHandle>,
    pub pid: u32,
    pub start: u64,
}
impl Peer {
    fn verified(pid: u32, expected: &Identity) -> io::Result<Self> {
        // SAFETY: These query/synchronize rights neither change nor impersonate
        // the peer. OwnedHandle keeps the exact process alive as an identity.
        let handle = unsafe {
            own(OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
            .map_err(error)?)?
        };
        let identity = identity(raw(&handle), pid)?;
        if identity.user != expected.user
            || identity.logon != expected.logon
            || identity.session != expected.session
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe {
            GetProcessTimes(
                raw(&handle),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
            .map_err(error)?;
        }
        let start = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        Ok(Self {
            handle: Arc::new(handle),
            pid,
            start,
        })
    }
    pub fn is_alive(&self) -> bool {
        // SAFETY: The owned process handle remains valid, including after exit.
        unsafe { WaitForSingleObject(raw(&self.handle), 0) == WAIT_TIMEOUT }
    }
}

pub fn random_token() -> io::Result<[u8; 16]> {
    let mut bytes = [0; 16];
    // SAFETY: The OS RNG writes only this supplied buffer; no provider handle.
    unsafe {
        BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
            .ok()
            .map_err(error)?;
    }
    Ok(bytes)
}

pub struct Listener {
    endpoint: Endpoint,
    pending: Option<OwnedHandle>,
}
impl Listener {
    pub fn accept(&mut self, timeout: Duration) -> io::Result<(Stream, Peer)> {
        let handle = match self.pending.take() {
            Some(handle) => handle,
            None => self.endpoint.pipe(false)?,
        };
        let deadline = Instant::now() + timeout;
        let mut pending = Operation::new()?;
        // SAFETY: Pending OVERLAPPED and its event live until operation finishes
        // or is cancelled and drained by Operation::finish.
        let result = unsafe { ConnectNamedPipe(raw(&handle), Some(&mut pending.overlapped)) };
        if !result
            .as_ref()
            .is_err_and(|error| error.code() == ERROR_PIPE_CONNECTED.to_hresult())
        {
            pending.finish(&handle, result, deadline)?;
        }
        let mut pid = 0;
        unsafe {
            GetNamedPipeClientProcessId(raw(&handle), &mut pid).map_err(error)?;
        }
        let peer = Peer::verified(pid, &self.endpoint.identity)?;
        Ok((
            Stream {
                handle,
                deadline: Instant::now() + Duration::from_secs(2),
            },
            peer,
        ))
    }
}

struct Operation {
    overlapped: OVERLAPPED,
    _event: OwnedHandle,
}
impl Operation {
    fn new() -> io::Result<Self> {
        // SAFETY: Event is newly created, non-inherited and owned once.
        let event = unsafe { own(CreateEventW(None, true, false, None).map_err(error)?)? };
        Ok(Self {
            overlapped: OVERLAPPED {
                hEvent: raw(&event),
                ..Default::default()
            },
            _event: event,
        })
    }
    fn finish(
        &mut self,
        handle: &OwnedHandle,
        result: windows::core::Result<()>,
        deadline: Instant,
    ) -> io::Result<usize> {
        if let Err(error) = result
            && error.code() != ERROR_IO_PENDING.to_hresult()
        {
            return Err(io::Error::other(error.to_string()));
        }
        let milliseconds = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u128::from(u32::MAX - 1)) as u32;
        let mut count = 0;
        // SAFETY: OVERLAPPED/event/buffer remain alive until completion. Even on
        // timeout, cancellation is drained before their storage is released.
        unsafe {
            if WaitForSingleObject(self.overlapped.hEvent, milliseconds) != WAIT_OBJECT_0 {
                let _ = CancelIoEx(raw(handle), Some(&self.overlapped));
                let _ = GetOverlappedResult(raw(handle), &self.overlapped, &mut count, true);
                return Err(io::ErrorKind::TimedOut.into());
            }
            GetOverlappedResult(raw(handle), &self.overlapped, &mut count, false).map_err(error)?;
        }
        Ok(count as usize)
    }
}

pub struct Stream {
    handle: OwnedHandle,
    deadline: Instant,
}
impl Read for Stream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let mut pending = Operation::new()?;
        // SAFETY: Buffer is mutably borrowed until completion/cancellation drain.
        let result = unsafe {
            ReadFile(
                raw(&self.handle),
                Some(buffer),
                None,
                Some(&mut pending.overlapped),
            )
        };
        if result
            .as_ref()
            .is_err_and(|error| error.code() == ERROR_BROKEN_PIPE.to_hresult())
        {
            return Ok(0);
        }
        pending.finish(&self.handle, result, self.deadline)
    }
}
impl Write for Stream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let mut pending = Operation::new()?;
        // SAFETY: Buffer stays borrowed until completion/cancellation drain.
        let result = unsafe {
            WriteFile(
                raw(&self.handle),
                Some(buffer),
                None,
                Some(&mut pending.overlapped),
            )
        };
        pending.finish(&self.handle, result, self.deadline)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
