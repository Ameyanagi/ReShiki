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
#[cfg(any(windows, target_os = "linux"))]
mod desktop;
mod launch;
#[cfg(windows)]
mod rendering;
#[cfg(any(windows, test))]
mod window_fit;

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
    // Worker modes enter before graphics, AppKit/Office registration or Tokio.
    // Relaunching this executable keeps deadlines and memory failures isolated.
    match std::env::args_os()
        .nth(1)
        .as_deref()
        .and_then(reshiki::workers::run)
    {
        Some(0) => return Ok(()),
        Some(code) => std::process::exit(code),
        None => {}
    }
    // Headless modes enter before Office registration, graphics, Apple events
    // and the std::env::args() checks below.
    match launch::mode(std::env::args_os().skip(1)) {
        launch::Launch::Gui => {}
        launch::Launch::Mcp(args) => launch::mcp(args),
        launch::Launch::Cli(args) => launch::cli(args),
    }
    #[cfg(windows)]
    {
        if launch::gui_flag(std::env::args_os().skip(1), "--graphics-info") {
            println!("{}", rendering::diagnostics());
            return Ok(());
        }
        if launch::gui_flag(std::env::args_os().skip(1), "--ole-server") {
            let result = reshiki_windows::run_office_server(|bytes| {
                let document = reshiki::document::Document::from_json(bytes)?;
                reshiki::export::office_preview(&document)
            });
            if let Err(error) = result {
                eprintln!("Office integration failed: {error}");
            }
            return Ok(());
        }
    }
    if launch::gui_flag(std::env::args_os().skip(1), "--engine-check") {
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
    #[cfg(any(windows, target_os = "linux"))]
    let _desktop = desktop::launch(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &app::startup::parse(std::env::args_os().skip(1)),
    );
    #[cfg(windows)]
    reshiki_windows::enable_office_embedding();
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
            // Windows fits the hidden real HWND before publishing its accessible
            // tree/showing it. A fixed logical minimum would defeat that fit.
            min_size: (!cfg!(windows)).then_some(iced::Size::new(1040.0, 680.0)),
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
