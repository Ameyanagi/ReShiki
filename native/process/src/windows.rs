//! Windows 10+ creation-time job membership; no spawn/assign interval.
use super::Limits;
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Write},
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    process::{Command, ExitStatus},
    sync::atomic::{AtomicU64, Ordering},
};
use windows::{
    Win32::{
        Foundation::{
            ERROR_BROKEN_PIPE, ERROR_NO_DATA, GENERIC_READ, GENERIC_WRITE, HANDLE, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_MODE,
            OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile,
        },
        System::{
            JobObjects::{
                CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_TIME,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject,
            },
            Pipes::{CreateNamedPipeW, PIPE_NOWAIT, PIPE_REJECT_REMOTE_CLIENTS},
            Threading::{
                CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
                DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
                InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST,
                PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
                UpdateProcThreadAttribute, WaitForSingleObject,
            },
        },
    },
    core::{HRESULT, PCWSTR, PWSTR},
};

pub(super) fn handle(value: &OwnedHandle) -> HANDLE {
    HANDLE(value.as_raw_handle())
}
pub(super) fn owned(value: HANDLE) -> io::Result<OwnedHandle> {
    if value.is_invalid() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: callers transfer each successful owned API handle exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(value.0) })
}
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut bytes: Vec<_> = value.encode_wide().collect();
    if bytes.contains(&0) {
        return Err(io::Error::other("NUL in local worker command"));
    }
    bytes.push(0);
    Ok(bytes)
}
/// Microsoft C-runtime argument quoting; all arguments are ordinary OsStrs,
/// never cmd.exe scripts or raw shell text.
fn quoted(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut out = vec![b'"' as u16];
    let mut slashes = 0;
    for c in value.encode_wide() {
        if c == 0 {
            return Err(io::Error::other("NUL in worker argument"));
        }
        if c == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            b'\\' as u16,
            slashes * if c == b'"' as u16 { 2 } else { 1 },
        ));
        if c == b'"' as u16 {
            out.push(b'\\' as u16);
        }
        out.push(c);
        slashes = 0;
    }
    out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    out.push(b'"' as u16);
    Ok(out)
}
fn environment(command: &Command) -> io::Result<Vec<u16>> {
    // Windows environment names are case insensitive. Only the original
    // values are emitted; the normalized key is used solely for replacement.
    let mut values = BTreeMap::<String, (OsString, OsString)>::new();
    for (key, value) in std::env::vars_os() {
        values.insert(key.to_string_lossy().to_uppercase(), (key, value));
    }
    for (key, value) in command.get_envs() {
        let normalized = key.to_string_lossy().to_uppercase();
        if let Some(value) = value {
            values.insert(normalized, (key.into(), value.into()));
        } else {
            values.remove(&normalized);
        }
    }
    let mut block = Vec::new();
    for (key, value) in values.values() {
        let mut item = key.clone();
        item.push("=");
        item.push(value);
        block.extend(wide(&item)?);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

/// Parent endpoint is PIPE_NOWAIT; child endpoint remains normal blocking
/// byte mode, compatible with JVM/std Rust stdin/stdout. Only child handles
/// are inheritable and are explicitly listed in STARTUPINFOEX.
fn pipe() -> io::Result<(File, OwnedHandle)> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = wide(OsStr::new(&format!(
        r"\\.\pipe\reshiki-naming-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )))?;
    // SAFETY: the name is terminated; one local instance, default security,
    // no remote clients. FIRST_PIPE_INSTANCE refuses a preexisting name.
    let parent = owned(unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65_536,
            65_536,
            0,
            None,
        )
    })?;
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: true.into(),
    };
    // SAFETY: connects only to the newly owned server pipe; all pointers remain
    // live through this call. The child handle is inherited only via HANDLE_LIST.
    let child = owned(
        unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_MODE(0),
                Some(&security),
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                HANDLE::default(),
            )
        }
        .map_err(io::Error::other)?,
    )?;
    Ok((File::from(parent), child))
}
fn job(limits: Limits) -> io::Result<OwnedHandle> {
    // SAFETY: unnamed noninheritable job; owned immediately for failure cleanup.
    let job = owned(unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(io::Error::other)?)?;
    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_JOB_MEMORY
        | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_PROCESS_TIME;
    info.BasicLimitInformation.ActiveProcessLimit = 1;
    info.BasicLimitInformation.PerProcessUserTimeLimit =
        i64::try_from(limits.cpu_seconds.saturating_mul(10_000_000)).map_err(io::Error::other)?;
    info.JobMemoryLimit = usize::try_from(limits.memory_bytes).map_err(io::Error::other)?;
    // SAFETY: initialized information of the documented exact size.
    unsafe {
        SetInformationJobObject(
            handle(&job),
            JobObjectExtendedLimitInformation,
            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    }
    .map_err(io::Error::other)?;
    Ok(job)
}
struct Attributes {
    storage: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    fn pointer(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.storage.as_ptr().cast_mut().cast())
    }
    fn new() -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: documented sizing call; its expected failure reports size.
        let _ = unsafe {
            InitializeProcThreadAttributeList(
                LPPROC_THREAD_ATTRIBUTE_LIST::default(),
                2,
                0,
                &mut bytes,
            )
        };
        if bytes == 0 || bytes > 65_536 {
            return Err(io::Error::other("Invalid process attribute-list size"));
        }
        let mut value = Self {
            storage: vec![0; bytes.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        // SAFETY: aligned allocation is at least the requested size and remains
        // live and stationary until DeleteProcThreadAttributeList on Drop.
        unsafe { InitializeProcThreadAttributeList(value.pointer(), 2, 0, &mut bytes) }
            .map_err(io::Error::other)?;
        value.initialized = true;
        Ok(value)
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: the list was initialized and its allocation is still live.
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }
}

pub(super) fn spawn(command: &mut Command, limits: Limits) -> io::Result<Child> {
    if !std::path::Path::new(command.get_program()).is_absolute() {
        return Err(io::Error::other(
            "Local worker executable must be an absolute path",
        ));
    }
    let application = wide(command.get_program())?;
    let mut line = quoted(command.get_program())?;
    for arg in command.get_args() {
        line.push(b' ' as u16);
        line.extend(quoted(arg)?);
    }
    line.push(0);
    let environment = environment(command)?;
    let cwd = command
        .get_current_dir()
        .map(|p| wide(p.as_os_str()))
        .transpose()?;
    let job = job(limits)?;
    let (input, input_child) = pipe()?;
    let (output, output_child) = pipe()?;
    let (errors, errors_child) = pipe()?;
    let jobs = [handle(&job)];
    let pipes = [
        handle(&input_child),
        handle(&output_child),
        handle(&errors_child),
    ];
    let attributes = Attributes::new()?;
    // SAFETY: these arrays remain live until attributes is destroyed. Jobs are
    // assigned by CreateProcess itself BEFORE the initial child thread executes.
    // The job handle is deliberately excluded from the inherited handle list.
    unsafe {
        UpdateProcThreadAttribute(
            attributes.pointer(),
            0,
            PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
            Some(jobs.as_ptr().cast()),
            size_of::<HANDLE>(),
            None,
            None,
        )
        .map_err(io::Error::other)?;
        UpdateProcThreadAttribute(
            attributes.pointer(),
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            Some(pipes.as_ptr().cast()),
            size_of::<[HANDLE; 3]>(),
            None,
            None,
        )
        .map_err(io::Error::other)?;
    }
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = pipes[0];
    startup.StartupInfo.hStdOutput = pipes[1];
    startup.StartupInfo.hStdError = pipes[2];
    startup.lpAttributeList = attributes.pointer();
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: all strings/arrays are correctly terminated, mutable command-line
    // storage and initialized structures remain live. Only explicitly listed
    // pipe handles are inherited, so editor death closes the final job handle.
    unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            PWSTR(line.as_mut_ptr()),
            None,
            None,
            true,
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            Some(environment.as_ptr().cast()),
            cwd.as_ref().map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
            &startup.StartupInfo,
            &mut info,
        )
    }
    .map_err(io::Error::other)?;
    let process = owned(info.hProcess)?;
    drop(owned(info.hThread)?);
    drop(attributes);
    drop((input_child, output_child, errors_child));
    Ok(Child {
        process,
        job,
        pid: info.dwProcessId,
        input: Some(input),
        output,
        errors,
        status: None,
        terminated: false,
    })
}

pub struct Child {
    process: OwnedHandle,
    job: OwnedHandle,
    pid: u32,
    input: Option<File>,
    output: File,
    errors: File,
    status: Option<ExitStatus>,
    terminated: bool,
}
fn read(pipe: &mut File, bytes: &mut [u8]) -> io::Result<usize> {
    if bytes.is_empty() {
        return Ok(0);
    }
    // std File::read normalizes ERROR_NO_DATA through BrokenPipe to Ok(0),
    // losing the distinction between an empty connected PIPE_NOWAIT pipe and
    // EOF. Preserve the Win32 result before that normalization occurs.
    let length = bytes.len().min(u32::MAX as usize);
    let bytes = bytes
        .get_mut(..length)
        .ok_or_else(|| io::Error::other("Invalid local worker read buffer"))?;
    let mut count = 0;
    // SAFETY: this owned, synchronous PIPE_NOWAIT handle cannot retain the
    // buffer after return; its length fits DWORD and count remains writable.
    let result = unsafe {
        ReadFile(
            HANDLE(pipe.as_raw_handle()),
            Some(bytes),
            Some(&mut count),
            None,
        )
    };
    match result {
        // A peer zero-byte WriteFile can complete a read without supplying
        // data. Only BROKEN_PIPE below proves EOF for a nonempty read buffer.
        Ok(()) if count == 0 => Err(io::ErrorKind::WouldBlock.into()),
        Ok(()) => Ok(count as usize),
        Err(e) if e.code() == HRESULT::from_win32(ERROR_NO_DATA.0) => {
            Err(io::ErrorKind::WouldBlock.into())
        }
        Err(e) if e.code() == HRESULT::from_win32(ERROR_BROKEN_PIPE.0) => Ok(0),
        Err(e) => Err(io::Error::other(e)),
    }
}
impl Child {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn write_input(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self
            .input
            .as_mut()
            .ok_or_else(|| io::Error::other("Input closed"))?
            .write(bytes)
        {
            Ok(0) => Err(io::ErrorKind::WouldBlock.into()),
            Err(e) if e.raw_os_error() == Some(ERROR_NO_DATA.0 as i32) => {
                Err(io::ErrorKind::WouldBlock.into())
            }
            result => result,
        }
    }
    pub fn close_input(&mut self) {
        self.input.take();
    }
    pub fn read_output(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        read(&mut self.output, bytes)
    }
    pub fn read_errors(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        read(&mut self.errors, bytes)
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.status.is_none() {
            // SAFETY: owned process handle remains live; zero timeout cannot block.
            let state = unsafe { WaitForSingleObject(handle(&self.process), 0) };
            if state == WAIT_TIMEOUT {
                return Ok(None);
            }
            if state != WAIT_OBJECT_0 {
                return Err(io::Error::last_os_error());
            }
            let mut code = 0;
            // SAFETY: correctly sized writable code and live signaled handle.
            unsafe { GetExitCodeProcess(handle(&self.process), &mut code) }
                .map_err(io::Error::other)?;
            self.status = Some(ExitStatus::from_raw(code));
        }
        Ok(self.status)
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    /// Kernel job enforces commit, CPU and the single-process membership limit.
    pub fn check(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn terminate(&mut self) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        // SAFETY: owned job, containing only our preassigned worker.
        unsafe {
            let _ = TerminateJobObject(handle(&self.job), 75);
        }
        let _ = self.wait();
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(test)]
mod quoting_tests {
    use super::*;
    #[test]
    fn nonblocking_pipe_keeps_empty_connected_reads_open_between_chunks() {
        let (mut reader, writer) = pipe().unwrap();
        let mut writer = File::from(writer);
        let mut buffer = [0_u8; 128];
        assert_eq!(read(&mut reader, &mut []).unwrap(), 0);
        let mut written = 0;
        // SAFETY: the live synchronous client handle receives a zero-length
        // slice and a writable DWORD; no operation retains either pointer.
        unsafe {
            windows::Win32::Storage::FileSystem::WriteFile(
                HANDLE(writer.as_raw_handle()),
                Some(&[]),
                Some(&mut written),
                None,
            )
        }
        .unwrap();
        assert_eq!(written, 0);
        assert_eq!(
            read(&mut reader, &mut buffer).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        let mut captured = Vec::new();
        for chunk in [b"{\"protocol\":1,".as_slice(), b"\"value\":\"delayed\"}"] {
            writer.write_all(chunk).unwrap();
            let count = read(&mut reader, &mut buffer).unwrap();
            captured.extend_from_slice(&buffer[..count]);
            assert_eq!(
                read(&mut reader, &mut buffer).unwrap_err().kind(),
                io::ErrorKind::WouldBlock,
                "A live writer may produce another response chunk later"
            );
        }
        assert_eq!(captured, b"{\"protocol\":1,\"value\":\"delayed\"}");
        drop(writer);
        assert_eq!(read(&mut reader, &mut buffer).unwrap(), 0);
    }

    #[test]
    fn windows_argument_quoting_preserves_spaces_quotes_and_trailing_slashes() {
        for (input, expected) in [
            ("", "\"\""),
            ("a b", "\"a b\""),
            ("a\"b", "\"a\\\"b\""),
            ("C:\\a b\\", "\"C:\\a b\\\\\""),
        ] {
            assert_eq!(
                String::from_utf16(&quoted(OsStr::new(input)).unwrap()).unwrap(),
                expected
            );
        }
    }
}
