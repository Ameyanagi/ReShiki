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
mod canvas;

fn main() -> iced::Result {
    #[cfg(windows)]
    {
        if std::env::args().any(|arg| arg == "--ole-server") {
            let result = reshiki_windows::run_office_server(|bytes| {
                let document: reshiki::document::Document =
                    serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
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
        match runtime.block_on(
            reshiki::engine::LocalEngine::default()
                .request(reshiki::engine::Request::import_smiles("CCO")),
        ) {
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
    iced::application(app::App::new, app::App::update, app::App::view)
        .default_font(iced::Font::with_name(reshiki::style::ui_font_family()))
        .title(app::App::title)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(1280.0, 820.0),
            min_size: Some(iced::Size::new(1040.0, 680.0)),
            ..Default::default()
        })
        .exit_on_close_request(false)
        .antialiasing(true)
        .centered()
        .run()
}
