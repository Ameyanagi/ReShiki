//! A real current-executable worker, without starting the GUI. A normal Rust
//! test harness cannot dispatch worker flags before interpreting test arguments.
#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use std::{
        io::{BufRead, Write},
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    if let Some(mode @ ("--emf-worker" | "--emf-file-worker" | "--emf-office-worker")) =
        std::env::args().nth(1).as_deref()
    {
        return reshiki_windows::emf_worker(mode).map_err(anyhow::Error::msg);
    }
    if std::env::args().nth(1).as_deref() == Some("--deadline-supervisor") {
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--emf-worker")
            .stdin(Stdio::inherit())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn()?;
        println!("{}", child.id());
        std::io::stdout().flush()?;
        child.wait()?;
        return Ok(());
    }
    let source = include_bytes!("../docs/changes/fixtures/emf-import/controlled-spectrum.emf");
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("controlled-spectrum.EMF");
    std::fs::write(&path, source)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let started = Instant::now();
    let picture = runtime
        .block_on(reshiki::metafile::open(&path))
        .map_err(anyhow::Error::msg)?;
    assert!(started.elapsed() < Duration::from_secs(20));
    assert_eq!(picture.emf(), Some(source.as_slice()));
    assert_eq!((picture.width(), picture.height()), (4724, 2834));
    let doc = picture.document();
    doc.validate().map_err(anyhow::Error::msg)?;
    let stored = doc.file_json().map_err(anyhow::Error::msg)?;
    let reopened =
        reshiki::document::Document::from_native_file(&stored).map_err(anyhow::Error::msg)?;
    assert_eq!(reopened.version, reshiki::document::VERSION);
    assert_eq!(
        reopened.graphics[0].picture.as_ref().unwrap().emf(),
        Some(source.as_slice())
    );
    let exported = reshiki::export::figure(&doc, "emf").map_err(anyhow::Error::msg)?;
    let dimensions =
        reshiki::pictures::emf_dimensions(&exported.bytes).map_err(anyhow::Error::msg)?;
    assert!(dimensions.width_pt >= 100. * 72. / 25.4);
    assert!(dimensions.height_pt >= 60. * 72. / 25.4);
    if let Some(folder) = std::env::var_os("RESHIKI_EMF_EVIDENCE_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join("source.emf"), source)?;
        std::fs::write(folder.join("worker-preview.png"), picture.png())?;
        std::fs::write(folder.join("retained-source.rsk"), &stored)?;
        std::fs::write(folder.join("vector-reexport.emf"), &exported.bytes)?;
        for format in ["svg", "png", "pdf"] {
            let figure = reshiki::export::figure(&doc, format).map_err(anyhow::Error::msg)?;
            std::fs::write(
                folder.join(format!("portable-preview.{format}")),
                figure.bytes,
            )?;
        }
    }
    // The asynchronous importer must actually close stdin after writing; the
    // successful round trip above catches a no-op Windows shutdown call.
    // A controller keeps the inherited input pipe open after killing the
    // supervisor. EOF cannot rescue this deliberately stalled child; its own
    // watchdog must terminate it, independently of the supervisor's lifetime.
    let mut supervisor = Command::new(std::env::current_exe()?)
        .arg("--deadline-supervisor")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()?;
    let input = supervisor.stdin.take().expect("supervisor input");
    let mut response =
        std::io::BufReader::new(supervisor.stdout.take().expect("supervisor output"));
    let mut line = String::new();
    response.read_line(&mut line)?;
    let pid: u32 = line.trim().parse()?;
    let started = Instant::now();
    supervisor.kill()?;
    supervisor.wait()?;
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(format!(
            "$ErrorActionPreference='Stop'; $p=Get-Process -Id {pid}; if (!$p.WaitForExit(40000)) {{ Stop-Process -Id {pid} -Force; exit 1 }}"
        ))
        .creation_flags(0x0800_0000)
        .status()?;
    drop(input);
    assert!(
        status.success(),
        "orphan worker has no independent deadline"
    );
    assert!(started.elapsed() >= Duration::from_secs(29));
    assert!(started.elapsed() < Duration::from_secs(40));
    println!("EMF async import, isolated export and independent worker deadline passed");
    Ok(())
}
