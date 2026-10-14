//! Launch classification: forward to a running host, become the host, run the
//! Office proxy, or continue as a plain GUI.
use super::{
    ACCEPT_POLL, GuiHost, HANDOFF,
    client::{Client, office_proxy},
    native,
    protocol::{self, Command, NativePath, Request},
};
use crate::session::{Arguments, Host};
use std::{
    io,
    process::{Command as Child, Stdio},
    time::Instant,
};

pub(super) fn absolute(args: &Arguments) -> io::Result<Vec<NativePath>> {
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

pub(super) fn private_nonce(args: &[std::ffi::OsString]) -> io::Result<Option<[u8; 16]>> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--open" {
            let _ = args.next();
        } else if arg == "--desktop-private" {
            let text = args
                .next()
                .and_then(|arg| arg.to_str())
                .ok_or(io::ErrorKind::InvalidInput)?;
            return protocol::parse_token(text).map(Some);
        }
    }
    Ok(None)
}

/// Some GUI actions have their own launch semantics; preserve them separately.
pub(super) fn forwardable(args: &[std::ffi::OsString]) -> bool {
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

pub(super) fn office_role(
    args: &[std::ffi::OsString],
    startup: &Arguments,
) -> io::Result<Option<Host>> {
    match startup.office_host {
        Some(host) if !startup.paths.is_empty() && forwardable(args) => Ok(Some(host)),
        Some(_) => Err(io::ErrorKind::InvalidInput.into()),
        None => Ok(None),
    }
}

/// Continue carries the elected GUI host guard if any. Exit means this process
/// must exit with that code; main performs the exit.
pub(crate) enum LaunchOutcome {
    Continue(Option<GuiHost>),
    Exit(i32),
}

/// Classifies this launch. `Exit` covers every Office launch (bad arguments, no
/// endpoint, or the proxy's result), a bootstrap child that lost the election
/// or could not start its host, and an ordinary launch whose files a running
/// host accepted. Everything else continues as the GUI.
pub(crate) fn launch(args: &[std::ffi::OsString], startup: &Arguments) -> LaunchOutcome {
    // Classify Office before the explicit-GUI bypass: every valid Office
    // launch must be a watched proxy, even when it is the first launch.
    let office = match office_role(args, startup) {
        Ok(office) => office,
        Err(_) => {
            eprintln!("Office editing requires --open and supported edit arguments");
            return LaunchOutcome::Exit(2);
        }
    };
    if !forwardable(args) {
        return LaunchOutcome::Continue(None);
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
                return LaunchOutcome::Exit(1);
            }
            return LaunchOutcome::Continue(None);
        }
    };
    if let Some(host) = office {
        return LaunchOutcome::Exit(office_proxy(endpoint, startup, host));
    }
    let paths = match absolute(startup) {
        Ok(paths) => paths,
        Err(_) => return LaunchOutcome::Continue(None),
    };
    let bootstrap = crate::launch::gui_flag(args.iter().cloned(), "--desktop-host")
        || private_nonce(args).ok().flatten().is_some();
    if bootstrap {
        match endpoint.elect() {
            Ok(Some(election)) => match GuiHost::start(&endpoint, election) {
                Ok(host) => return LaunchOutcome::Continue(Some(host)),
                Err(error) => {
                    eprintln!("Desktop host startup failed: {error}");
                    return LaunchOutcome::Exit(1);
                }
            },
            // A bootstrap child never creates an extra unregistered GUI. Its
            // watching proxy will connect to the winner or make a private host.
            _ => return LaunchOutcome::Exit(0),
        }
    }
    let client = match Client::new(endpoint.clone()) {
        Ok(client) => client,
        Err(_) => return LaunchOutcome::Continue(None),
    };
    let deadline = Instant::now() + HANDOFF;
    let Ok(id) = native::random_token() else {
        return LaunchOutcome::Continue(None);
    };
    let mut request = Request::new(id, None, client.process, Command::Open { paths });
    if client.open_request(&mut request, Instant::now() + ACCEPT_POLL) {
        return LaunchOutcome::Exit(0);
    }
    match endpoint.elect() {
        Ok(Some(election)) => match GuiHost::start(&endpoint, election) {
            Ok(host) => LaunchOutcome::Continue(Some(host)),
            Err(error) => {
                eprintln!("Desktop handoff unavailable: {error}");
                LaunchOutcome::Continue(None)
            }
        },
        Ok(None) => {
            if client.open_request(&mut request, deadline) {
                return LaunchOutcome::Exit(0);
            }
            LaunchOutcome::Continue(None)
        }
        Err(_) => LaunchOutcome::Continue(None),
    }
}

pub(super) fn bootstrap(private: Option<[u8; 16]>) -> io::Result<()> {
    let mut child = Child::new(std::env::current_exe()?);
    if let Some(token) = private {
        child
            .arg("--desktop-private")
            .arg(protocol::token_hex(&token));
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
