#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use cannon::app::{self, AppResult};
use cannon::args::parser::Args;
use clap::{CommandFactory, FromArgMatches};
use colored::Colorize;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{} {error}", "error:".red().bold());
            ExitCode::FAILURE
        }
    }
}

fn run() -> AppResult<ExitCode> {
    let matches = Args::command().get_matches();
    let mut args = Args::from_arg_matches(&matches)?;

    if args.update {
        app::update::update()?;
        return Ok(ExitCode::SUCCESS);
    }

    cannon::args::config::merge_with_yaml(&mut args, &matches)?;
    args.validate()
        .map_err(|error| format!("invalid configuration: {error}"))?;

    app::environment::prepare(&args);

    let runtime = app::environment::build_runtime(&args)?;

    runtime.block_on(app::run(args))
}
