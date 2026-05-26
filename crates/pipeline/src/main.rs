//! niinku: build a Finnish puhekieli/slang dictionary for HeliBoard.
//!
//! Two stages:
//!   - `ingest`   — heavy, per-source corpus crunching → cached freq tables
//!   - `assemble` — merge cached + live sources, filter, score, emit `.combined`

use std::process::ExitCode;

fn usage() {
    eprintln!("Usage: niinku <ingest|assemble> [args...]");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("assemble") => {
            eprintln!("niinku assemble: not yet implemented");
            ExitCode::from(2)
        }
        Some("ingest") => {
            eprintln!("niinku ingest: not yet implemented");
            ExitCode::from(2)
        }
        Some("-h") | Some("--help") | None => {
            usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("niinku: unknown subcommand: {other}");
            usage();
            ExitCode::from(2)
        }
    }
}
