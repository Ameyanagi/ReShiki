//! Native file playback is isolated from the editor and bounded by a Windows
//! job, including allocations GDI+ makes outside the Rust global allocator.
use super::Result;
use std::io::{Read, Write};
use windows::Win32::{
    Foundation::*,
    System::{JobObjects::*, Threading::*},
};

struct Job(HANDLE);
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn limited_job() -> Result<Job> {
    // SAFETY: valid initialized policy buffers and owned handles. The job lives
    // until worker exit; the process cannot escape its 512 MB committed-memory
    // limit. Failure is reported before any native input decoding.
    unsafe {
        let job = Job(CreateJobObjectW(None, windows::core::PCWSTR::null())?);
        let mut information = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        information.ProcessMemoryLimit = 512 * 1024 * 1024;
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&information).cast(),
            std::mem::size_of_val(&information) as u32,
        )?;
        AssignProcessToJobObject(job.0, GetCurrentProcess())?;
        Ok(job)
    }
}

const MAX_SNAPSHOT: usize = 128 * 1024 * 1024;

fn deadline() -> Result<std::sync::mpsc::Sender<()>> {
    let (finished, waiting) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("emf-deadline".into())
        .spawn(move || {
            if matches!(
                waiting.recv_timeout(std::time::Duration::from_secs(30)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                // SAFETY: this disposable worker owns only this process. An
                // independent deadline also applies if its supervisor exits.
                unsafe {
                    let _ = TerminateProcess(GetCurrentProcess(), 1);
                }
            }
        })?;
    Ok(finished)
}

pub(super) fn run(mode: &str) -> Result<()> {
    let _job = limited_job()?;
    let _deadline = deadline()?;
    let mut bytes = Vec::new();
    let limit = if mode == "--emf-worker" {
        reshiki_metafile::MAX_BYTES
    } else {
        MAX_SNAPSHOT
    };
    std::io::stdin()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(anyhow::anyhow!("EMF worker input limit exceeded"));
    }
    let output = match mode {
        "--emf-worker" => super::printing::import_metafile(&bytes)?,
        "--emf-file-worker" => super::printing::file_metafile(&bytes)?,
        "--emf-office-worker" => super::printing::metafile(&bytes)?,
        _ => return Err(anyhow::anyhow!("Unknown EMF worker operation")),
    };
    if output.len() > MAX_SNAPSHOT {
        return Err(anyhow::anyhow!("EMF worker output limit exceeded"));
    }
    std::io::stdout().write_all(&output)?;
    Ok(())
}

/// Keep imported EMF playback out of both the editor and its Office server.
pub(super) fn record(bytes: &[u8], file: bool) -> Result<Vec<u8>> {
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    if bytes.len() > MAX_SNAPSHOT {
        return Err(anyhow::anyhow!("EMF snapshot exceeds 128 MB"));
    }
    let mut child = Command::new(std::env::current_exe()?)
        .arg(if file {
            "--emf-file-worker"
        } else {
            "--emf-office-worker"
        })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()?;
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("Missing EMF worker input"))?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Missing EMF worker output"))?;
    let owned = bytes.to_vec();
    let write = std::thread::spawn(move || input.write_all(&owned));
    let read = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        output
            .take(MAX_SNAPSHOT as u64 + 1)
            .read_to_end(&mut bytes)?;
        Ok::<_, std::io::Error>(bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => break Err(anyhow::anyhow!("EMF export exceeded its 30-second limit")),
            Err(error) => break Err(error.into()),
        }
    };
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    // A terminated child closes the pipes; joining cannot wait for playback.
    let written = write
        .join()
        .map_err(|_| anyhow::anyhow!("EMF worker input thread failed"))?;
    let output = read
        .join()
        .map_err(|_| anyhow::anyhow!("EMF worker output thread failed"))??;
    let status = status?;
    if !status.success() {
        return Err(anyhow::anyhow!("Windows could not export this EMF picture"));
    }
    written?;
    if output.len() > MAX_SNAPSHOT {
        return Err(anyhow::anyhow!("EMF export exceeds 128 MB"));
    }
    Ok(output)
}
