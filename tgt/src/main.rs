use std::process::ExitCode;

use clap::Parser;
use tgt::{app, cli::Cli, logger, settings::Settings, ui::terminal::TuiBackend};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let settings = match Settings::resolve(cli.into(), |key| std::env::var_os(key)) {
        Ok(settings) => settings,
        Err(err) => {
            eprintln!("tgt: {err}");
            return ExitCode::from(2);
        }
    };

    let _guard = match logger::init(&settings) {
        Ok(guard) => guard,
        Err(err) => {
            eprintln!("tgt: failed to initialize logging: {err}");
            return ExitCode::FAILURE;
        }
    };

    // Restore the terminal before the panic message is printed.
    let (mouse, paste) = (settings.app.mouse_support, settings.app.paste_support);
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = TuiBackend::force_exit(mouse, paste);
        tracing::error!("{info}");
        default_hook(info);
    }));

    match app::run(&settings) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("{err}");
            eprintln!("tgt: {err}");
            ExitCode::FAILURE
        }
    }
}
