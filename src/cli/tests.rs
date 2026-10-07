use super::*;

async fn call(tokens: &[&str]) -> (i32, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let args = tokens.iter().map(OsString::from).collect();
    let code = run_with(args, &mut out, &mut err).await;
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[tokio::test]
async fn help_prints_the_experimental_usage_to_stdout() {
    for name in ["help", "--help", "-h"] {
        let (code, out, err) = call(&[name]).await;
        assert_eq!(code, SUCCESS, "{name}");
        assert!(out.starts_with("Experimental:"), "{name}: {out}");
        assert_eq!(
            out.lines().nth(1),
            Some("Usage: reshiki --cli <command> [options]")
        );
        assert!(err.is_empty(), "{name}: {err}");
    }
}

#[tokio::test]
async fn help_takes_no_arguments() {
    let (code, out, err) = call(&["help", "x"]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
    assert_eq!(err, "reshiki: help takes no arguments\n");
}

#[tokio::test]
async fn info_prints_one_json_object_line() {
    let (code, out, err) = call(&["info"]).await;
    assert_eq!(code, SUCCESS);
    assert!(err.is_empty(), "{err}");
    let line = out.strip_suffix('\n').unwrap();
    assert!(!line.contains('\n'), "{out}");
    let info: serde_json::Value = serde_json::from_str(line).unwrap();
    assert!(info.is_object());
    assert_eq!(info["document"], crate::document::VERSION);
    assert_eq!(info["app"], crate::updates::CURRENT_VERSION);
    assert_eq!(info["platform"], std::env::consts::OS);
    assert_eq!(info["api"]["stability"], "experimental");
    assert_eq!(
        info["api"]["operation_api"],
        crate::envelope::OPERATION_API_VERSION
    );
}

#[tokio::test]
async fn info_takes_no_arguments() {
    let (code, out, _) = call(&["info", "x"]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
}

#[tokio::test]
async fn no_command_prints_usage_to_stderr() {
    let (code, out, err) = call(&[]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
    assert!(err.starts_with("Experimental:"), "{err}");
}

#[tokio::test]
async fn unknown_commands_point_to_help() {
    let (code, out, err) = call(&["bogus"]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
    assert_eq!(
        err,
        "reshiki: unknown command `bogus`; run `reshiki --cli help`\n"
    );
}

/// Fails every write when `on_write`, and every flush, counting the flushes.
struct Broken {
    on_write: bool,
    flushes: usize,
}

impl Write for Broken {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.on_write {
            Err(io::Error::other("closed"))
        } else {
            Ok(bytes.len())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Err(io::Error::other("closed"))
    }
}

#[tokio::test]
async fn output_write_and_flush_errors_fail_with_one_line() {
    for on_write in [true, false] {
        for name in ["help", "info"] {
            let mut out = Broken {
                on_write,
                flushes: 0,
            };
            let mut err = Vec::new();
            let args = vec![OsString::from(name)];
            let code = run_with(args, &mut out, &mut err).await;
            assert_eq!(code, FAILURE, "{name}, on_write {on_write}");
            assert_eq!(out.flushes, 1, "{name}, on_write {on_write}");
            let err = String::from_utf8(err).unwrap();
            assert_eq!(err.lines().count(), 1, "{err}");
            assert!(err.starts_with("reshiki: could not write output:"), "{err}");
        }
    }
}
