//! User/session-scoped local desktop IPC, with OS file locks and peer UID checks.
use rustix::{
    fs::{self, FlockOperation, Mode, OFlags},
    net::{self, AddressFamily, SocketAddrUnix, SocketFlags, SocketType},
    process,
};
use std::{
    collections::hash_map::DefaultHasher,
    fs::{DirBuilder, File, Metadata},
    hash::{Hash, Hasher},
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

fn denied() -> io::Error {
    io::ErrorKind::PermissionDenied.into()
}
fn metadata(path: &Path, uid: u32, mode: u32) -> io::Result<Metadata> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.uid() != uid || meta.mode() & 0o777 != mode || meta.file_type().is_symlink() {
        return Err(denied());
    }
    Ok(meta)
}
fn private_directory(path: &Path, uid: u32) -> io::Result<File> {
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let fd = fs::open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let stat = fs::fstat(&fd)?;
    if stat.st_uid != uid || stat.st_mode & 0o777 != 0o700 {
        return Err(denied());
    }
    Ok(File::from(fd))
}

#[derive(Clone)]
pub struct Endpoint {
    directory: Arc<File>,
    root: Arc<File>,
    uid: u32,
}
impl Endpoint {
    pub fn current() -> io::Result<Self> {
        let uid = process::getuid().as_raw();
        let mut hash = DefaultHasher::new();
        for name in ["XDG_SESSION_ID", "WAYLAND_DISPLAY", "DISPLAY"] {
            std::env::var_os(name).hash(&mut hash);
        }
        let session = hash.finish();
        let path = if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
            let runtime = PathBuf::from(runtime);
            let meta = metadata(&runtime, uid, 0o700)?;
            if !meta.is_dir() {
                return Err(denied());
            }
            runtime.join(format!("reshiki-desktop-{session:016x}"))
        } else {
            PathBuf::from(format!("/tmp/reshiki-desktop-{uid}-{session:016x}"))
        };
        let directory = Arc::new(private_directory(&path, uid)?);
        Ok(Self {
            root: directory.clone(),
            directory,
            uid,
        })
    }

    pub fn private(&self, token: [u8; 16]) -> io::Result<Self> {
        let suffix = token
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = self.path(&format!("private-{suffix}"));
        Ok(Self {
            directory: Arc::new(private_directory(&path, self.uid)?),
            root: self.root.clone(),
            uid: self.uid,
        })
    }

    // Resolve through a held verified directory FD rather than re-traversing
    // mutable/symlinkable parent components. This still names a filesystem socket.
    fn path(&self, name: &str) -> PathBuf {
        PathBuf::from(format!(
            "/proc/self/fd/{}/{name}",
            self.directory.as_raw_fd()
        ))
    }

    /// A stable separate file lock covers the source path even after choosing
    /// a private desktop fallback. The host's atomic file rename cannot replace it.
    pub fn claim(&self, path: &Path) -> io::Result<Option<Election>> {
        self.path_claim(path, "launch")
    }
    pub fn write_claim(&self, path: &Path) -> io::Result<Option<Election>> {
        self.path_claim(path, "write")
    }
    fn path_claim(&self, path: &Path, role: &str) -> io::Result<Option<Election>> {
        let mut hash = DefaultHasher::new();
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_owned())
            .hash(&mut hash);
        let name = format!("office-{role}-{:016x}.lock", hash.finish());
        self.lock(&self.root, &name)
    }

    fn lock(&self, directory: &Arc<File>, name: &str) -> io::Result<Option<Election>> {
        let lock = fs::openat(
            &**directory,
            name,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )?;
        let stat = fs::fstat(&lock)?;
        if stat.st_uid != self.uid
            || stat.st_mode & 0o777 != 0o600
            || stat.st_nlink != 1
            || stat.st_mode & 0o170000 != 0o100000
        {
            return Err(denied());
        }
        match fs::flock(&lock, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(Some(Election {
                _lock: File::from(lock),
                directory: directory.clone(),
                name: name.to_owned(),
            })),
            Err(rustix::io::Errno::WOULDBLOCK) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn elect(&self) -> io::Result<Option<Election>> {
        self.lock(&self.directory, "host.lock")
    }

    pub fn listen(&self, election: &Election) -> io::Result<Listener> {
        let a = self.directory.metadata()?;
        let b = election.directory.metadata()?;
        if election.name != "host.lock" || (a.dev(), a.ino()) != (b.dev(), b.ino()) {
            return Err(denied());
        }
        let path = self.path("desktop.sock");
        match std::fs::symlink_metadata(&path) {
            Ok(meta) => {
                if !meta.file_type().is_socket()
                    || meta.uid() != self.uid
                    || meta.mode() & 0o777 != 0o600
                {
                    return Err(denied());
                }
                // Do not unlink a live or unknown endpoint, even with the lock.
                let deadline = Duration::from_millis(50);
                match self.connect(deadline) {
                    Ok(_) => return Err(io::ErrorKind::AlreadyExists.into()),
                    Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                        std::fs::remove_file(&path)?;
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let fd = net::socket_with(
            AddressFamily::UNIX,
            SocketType::STREAM,
            SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
            None,
        )?;
        net::bind(&fd, &SocketAddrUnix::new(&path)?)?;
        net::listen(&fd, 16)?;
        let listener = UnixListener::from(fd);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let meta = std::fs::symlink_metadata(&path)?;
        Ok(Listener {
            socket: listener,
            endpoint: self.clone(),
            inode: (meta.dev(), meta.ino()),
        })
    }

    pub fn connect(&self, timeout: Duration) -> io::Result<(Stream, Peer)> {
        let deadline = Instant::now() + timeout;
        let path = self.path("desktop.sock");
        loop {
            let meta = metadata(&path, self.uid, 0o600)?;
            if !meta.file_type().is_socket() {
                return Err(denied());
            }
            let fd = net::socket_with(
                AddressFamily::UNIX,
                SocketType::STREAM,
                SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
                None,
            )?;
            match net::connect(&fd, &SocketAddrUnix::new(&path)?) {
                Ok(()) => {
                    let stream = UnixStream::from(fd);
                    stream.set_nonblocking(false)?;
                    let peer = Peer::verified(&stream, self.uid)?;
                    return Ok((
                        Stream {
                            socket: stream,
                            deadline,
                        },
                        peer,
                    ));
                }
                Err(error) if error == rustix::io::Errno::AGAIN => {}
                Err(error) => return Err(error.into()),
            }
            if Instant::now() >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn own_process(&self) -> io::Result<Peer> {
        Peer::process(process::getpid().as_raw_pid() as u32)
    }
}

pub struct Election {
    _lock: File,
    directory: Arc<File>,
    name: String,
}

#[derive(Clone)]
pub struct Peer {
    pub pid: u32,
    pub start: u64,
}
impl Peer {
    fn process(pid: u32) -> io::Result<Self> {
        let (state, start) = process_state(pid)?;
        if matches!(state, "Z" | "X") {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(Self { pid, start })
    }
    fn verified(stream: &UnixStream, uid: u32) -> io::Result<Self> {
        let peer = net::sockopt::socket_peercred(stream)?;
        if peer.uid.as_raw() != uid {
            return Err(denied());
        }
        Self::process(peer.pid.as_raw_pid() as u32)
    }
    pub fn is_alive(&self) -> bool {
        process_state(self.pid)
            .is_ok_and(|(state, start)| start == self.start && !matches!(state, "Z" | "X"))
    }
}

fn process_state(pid: u32) -> io::Result<(&'static str, u64)> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let tail = text
        .rsplit_once(')')
        .ok_or_else(|| io::Error::other("Invalid process identity"))?
        .1;
    let fields: Vec<_> = tail.split_whitespace().collect();
    let start = fields
        .get(19)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| io::Error::other("Missing process generation"))?;
    let state = match fields.first().copied() {
        Some("Z") => "Z",
        Some("X") => "X",
        Some(_) => "live",
        None => return Err(io::Error::other("Missing process state")),
    };
    Ok((state, start))
}

pub fn random_token() -> io::Result<[u8; 16]> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub struct Listener {
    socket: UnixListener,
    endpoint: Endpoint,
    inode: (u64, u64),
}
impl Listener {
    pub fn accept(&mut self, timeout: Duration) -> io::Result<(Stream, Peer)> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.socket.accept() {
                Ok((socket, _)) => {
                    let peer = Peer::verified(&socket, self.endpoint.uid)?;
                    return Ok((
                        Stream {
                            socket,
                            deadline: Instant::now() + Duration::from_secs(2),
                        },
                        peer,
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        let path = self.endpoint.path("desktop.sock");
        if std::fs::symlink_metadata(&path).is_ok_and(|meta| {
            (meta.dev(), meta.ino()) == self.inode && meta.file_type().is_socket()
        }) {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub struct Stream {
    socket: UnixStream,
    deadline: Instant,
}
impl Stream {
    fn remaining(&self) -> io::Result<Duration> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            Err(io::ErrorKind::TimedOut.into())
        } else {
            Ok(left)
        }
    }
}
impl Read for Stream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.remaining()?))?;
        self.socket.read(buffer)
    }
}
impl Write for Stream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        self.socket.write(buffer)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
