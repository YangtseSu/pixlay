//! Headless render entry point: a thin shell over [`pixlay_cli::cli::run`], so
//! the binary and the integration tests exercise exactly the same code.

use std::ffi::OsString;
use std::process::ExitCode;

use pixlay_cli::cli::{EXIT_FAILURE, EXIT_USAGE, Failure, run};

fn main() -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().skip(1).collect();
    match run(&argv) {
        Ok(code) => ExitCode::from(code),
        Err(failure) => {
            let (code, message) = match &failure {
                Failure::Usage(message) => (EXIT_USAGE, message),
                Failure::Failed(message) => (EXIT_FAILURE, message),
            };
            eprintln!("pixlay-render: {message}");
            if matches!(failure, Failure::Usage(_)) {
                eprintln!("Try 'pixlay-render --help' for usage.");
            }
            ExitCode::from(code)
        }
    }
}
