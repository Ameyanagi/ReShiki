//! A bounded typed protocol. Filenames use native bytes/wide units losslessly.
use super::native;
use crate::app::office::{Host, Phase};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub(super) const FRAME_LIMIT: usize = 256 * 1024;
pub(super) const PATH_LIMIT: usize = 64;
const VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Process {
    pub pid: u32,
    pub start: u64,
}
impl From<&native::Peer> for Process {
    fn from(peer: &native::Peer) -> Self {
        Self {
            pid: peer.pid,
            start: peer.start,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum NativePath {
    Unix(Vec<u8>),
    Windows(Vec<u16>),
}
impl NativePath {
    pub(super) fn encode(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Ok(Self::Windows(path.as_os_str().encode_wide().collect()))
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::ffi::OsStrExt;
            Ok(Self::Unix(path.as_os_str().as_bytes().to_vec()))
        }
    }
    pub(super) fn decode(&self) -> io::Result<PathBuf> {
        let path = match self {
            #[cfg(windows)]
            Self::Windows(units)
                if !units.is_empty() && units.len() <= 32767 && !units.contains(&0) =>
            {
                use std::os::windows::ffi::OsStringExt;
                PathBuf::from(std::ffi::OsString::from_wide(units))
            }
            #[cfg(target_os = "linux")]
            Self::Unix(bytes)
                if !bytes.is_empty() && bytes.len() <= 4096 && !bytes.contains(&0) =>
            {
                use std::os::unix::ffi::OsStringExt;
                PathBuf::from(std::ffi::OsString::from_vec(bytes.clone()))
            }
            _ => return Err(io::ErrorKind::InvalidData.into()),
        };
        if path.is_absolute() {
            Ok(path)
        } else {
            Err(io::ErrorKind::InvalidData.into())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum Command {
    Open {
        paths: Vec<NativePath>,
    },
    Prepare {
        token: [u8; 16],
        path: NativePath,
        host: Host,
    },
    Commit {
        token: [u8; 16],
    },
    Query {
        token: [u8; 16],
    },
    Ack {
        token: [u8; 16],
    },
    Abandon {
        token: [u8; 16],
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    version: u16,
    pub id: [u8; 16],
    pub generation: Option<[u8; 16]>,
    pub sender: Process,
    pub command: Command,
}
impl Request {
    pub(super) fn new(
        id: [u8; 16],
        generation: Option<[u8; 16]>,
        sender: Process,
        command: Command,
    ) -> Self {
        Self {
            version: VERSION,
            id,
            generation,
            sender,
            command,
        }
    }
    pub(super) fn valid(&self, peer: &native::Peer) -> bool {
        if self.version != VERSION || self.sender != Process::from(peer) {
            return false;
        }
        match &self.command {
            Command::Open { paths } => {
                paths.len() <= PATH_LIMIT && paths.iter().all(|path| path.decode().is_ok())
            }
            Command::Prepare { path, .. } => path.decode().is_ok(),
            _ => self.generation.is_some(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Status {
    Pending,
    Accepted,
    Prepared,
    Busy,
    Unknown,
    Rejected,
    Session(Phase),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Response {
    version: u16,
    pub generation: [u8; 16],
    pub sender: Process,
    pub status: Status,
}
impl Response {
    pub(super) fn new(generation: [u8; 16], sender: Process, status: Status) -> Self {
        Self {
            version: VERSION,
            generation,
            sender,
            status,
        }
    }
    pub(super) fn valid(&self, peer: &native::Peer) -> bool {
        self.version == VERSION && self.sender == Process::from(peer)
    }
}

pub(super) fn write(stream: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > FRAME_LIMIT {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(&bytes)?;
    stream.flush()
}
pub(super) fn read<T: serde::de::DeserializeOwned>(stream: &mut impl Read) -> io::Result<T> {
    let mut header = [0; 4];
    stream.read_exact(&mut header)?;
    let size = u32::from_le_bytes(header) as usize;
    if size == 0 || size > FRAME_LIMIT {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}
