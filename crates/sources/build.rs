//! Help the linker find libvoikko on systems where it lives outside the
//! default search path (notably macOS Homebrew under `/opt/homebrew/lib`).
//!
//! Uses pkg-config when available. Falls back silently if pkg-config or
//! the .pc file is missing — voikko-rs's own `-lvoikko` directive will
//! still apply, so a system install in /usr/lib continues to work.

fn main() {
    if pkg_config::probe_library("libvoikko").is_err() {
        // No pkg-config or no libvoikko.pc — emit nothing extra and let
        // the platform default search path resolve `-lvoikko`.
        println!("cargo:warning=libvoikko not found via pkg-config; relying on default linker search path");
    }
}
