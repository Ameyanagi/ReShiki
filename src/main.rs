#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]

mod app;
mod appearance;
mod branding;
mod canvas;
#[cfg(windows)]
mod rendering;

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: reshiki_process_heap::BoundedHeap = reshiki_process_heap::BoundedHeap;

#[cfg(test)]
pub(crate) use reshiki_process_heap::allocation_metrics;
#[cfg(test)]
#[global_allocator]
static ALLOCATOR: allocation_metrics::MeasuredAllocator<reshiki_process_heap::BoundedHeap> =
    allocation_metrics::MeasuredAllocator::new(reshiki_process_heap::BoundedHeap);

fn main() -> iced::Result {
    if let Some(result) = reshiki::libreoffice::run(std::env::args_os().nth(1).as_deref()) {
        if let Err(error) = result {
            eprintln!("LibreOffice integration failed: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    // Worker modes enter before graphics, AppKit/Office registration or Tokio.
    // Relaunching this executable keeps deadlines and memory failures isolated.
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--inchi-worker")) {
        reshiki::chemistry::inchi::worker::run();
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--clipboard-worker")) {
        if let Err(error) = reshiki_linux::clipboard_worker() {
            use std::io::Write;
            let _ = writeln!(std::io::stderr(), "{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let worker = match std::env::args_os()
            .nth(1)
            .as_deref()
            .and_then(|arg| arg.to_str())
        {
            Some("--clipboard-worker") => {
                Some(reshiki_macos::workers::clipboard as fn() -> Result<(), String>)
            }
            Some("--print-worker") => {
                Some(reshiki_macos::workers::print as fn() -> Result<(), String>)
            }
            _ => None,
        };
        if let Some(worker) = worker {
            if let Err(error) = worker() {
                eprintln!("{error}");
                std::process::exit(1);
            }
            return Ok(());
        }
    }
    #[cfg(windows)]
    {
        if std::env::args().any(|arg| arg == "--graphics-info") {
            println!("{}", rendering::diagnostics());
            return Ok(());
        }
        if std::env::args().any(|arg| arg == "--ole-server") {
            let result = reshiki_windows::run_office_server(|bytes| {
                let document = reshiki::document::Document::from_json(bytes)?;
                reshiki::export::office_preview(&document)
            });
            if let Err(error) = result {
                eprintln!("Office integration failed: {error}");
            }
            return Ok(());
        }
        reshiki_windows::enable_office_embedding();
    }
    if std::env::args().any(|arg| arg == "--engine-check") {
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(error) => {
                use std::io::Write;
                let _ = writeln!(
                    std::io::stderr(),
                    "Could not start chemistry runtime: {error}"
                );
                std::process::exit(1);
            }
        };
        match runtime.block_on(async {
            let engine = reshiki::engine::LocalEngine::default();
            let imported = engine
                .request(reshiki::engine::Request::import_smiles("CCO"))
                .await?;
            let document = imported
                .document
                .ok_or("Chemistry import returned no drawing")?;
            engine
                .request(reshiki::engine::Request::molecule("analyze", document))
                .await
        }) {
            Ok(response) => {
                use std::io::Write;
                if let Err(error) = serde_json::to_writer_pretty(std::io::stdout(), &response) {
                    let _ = writeln!(
                        std::io::stderr(),
                        "Could not write engine response: {error}"
                    );
                    std::process::exit(1);
                }
                return Ok(());
            }
            Err(error) => {
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "{error}");
                std::process::exit(1);
            }
        }
    }
    #[cfg(windows)]
    rendering::configure();
    #[cfg(target_os = "macos")]
    let _document_events = match reshiki_macos::install_document_events() {
        Ok((handler, events)) => {
            app::install_document_events(events);
            Some(handler)
        }
        Err(error) => {
            eprintln!("Could not register macOS document events: {error}");
            None
        }
    };
    // The inline editor can shape a new font before the drawing reaches the
    // canvas. Install variable-weight aliases before either creates text buffers.
    canvas::prepare_fonts();
    let result = iced::application(app::App::new, app::App::update, app::App::view)
        .default_font(iced::Font::with_name(reshiki::style::ui_font_family()))
        .title(app::App::title)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .window(iced::window::Settings {
            visible: !cfg!(any(target_os = "macos", windows)),
            size: iced::Size::new(1280.0, 820.0),
            min_size: Some(iced::Size::new(1040.0, 680.0)),
            icon: branding::window_icon(),
            ..Default::default()
        })
        .exit_on_close_request(false)
        .antialiasing(true)
        .centered()
        .run();
    #[cfg(target_os = "macos")]
    if let Err(error) = reshiki_macos::accessibility::shutdown() {
        eprintln!("{error}");
    }
    #[cfg(windows)]
    if let Err(error) = reshiki_windows::accessibility::shutdown() {
        eprintln!("{error}");
    }
    result
}
