mod app;
mod canvas;

fn main() -> iced::Result {
    if std::env::args().any(|arg| arg == "--engine-check") {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime");
        match runtime.block_on(
            moruno::engine::PythonEngine::default()
                .request(moruno::engine::Request::import_smiles("CCO")),
        ) {
            Ok(response) => {
                println!("{}", serde_json::to_string_pretty(&response).unwrap());
                return Ok(());
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    iced::application(app::App::new, app::App::update, app::App::view)
        .title(app::App::title)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .window_size((1280.0, 820.0))
        .exit_on_close_request(false)
        .antialiasing(true)
        .centered()
        .run()
}
