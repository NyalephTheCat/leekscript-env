//! Randomized Rust fight runs — thin wrapper around `leekgen-compare --fuzz` with legacy flag names.

use std::ffi::{OsStr, OsString};

fn map_flag(arg: &OsStr) -> Option<&'static str> {
    if arg == OsStr::new("--root") {
        Some("--fuzz-root")
    } else if arg == OsStr::new("--scenarios-dir") {
        Some("--fuzz-scenarios-dir")
    } else if arg == OsStr::new("--scenario") {
        Some("--fuzz-scenario")
    } else if arg == OsStr::new("--ai-dir") {
        Some("--fuzz-ai-dir")
    } else if arg == OsStr::new("--ai") {
        Some("--fuzz-ai")
    } else if arg == OsStr::new("--master-seed") {
        Some("--fuzz-master-seed")
    } else if arg == OsStr::new("--no-fuzz-seed") {
        Some("--fuzz-no-seed")
    } else if arg == OsStr::new("--no-shuffle-ai") {
        Some("--fuzz-no-shuffle-ai")
    } else if arg == OsStr::new("--allow-external-ais") {
        Some("--fuzz-allow-external-ais")
    } else if arg == OsStr::new("--keep-temps") {
        Some("--fuzz-keep-temps")
    } else if arg == OsStr::new("--continue-on-error") {
        Some("--fuzz-continue-on-error")
    } else if arg == OsStr::new("--quiet") {
        Some("--fuzz-quiet")
    } else {
        None
    }
}

fn flag_takes_value(mapped: &str) -> bool {
    matches!(
        mapped,
        "--fuzz-root"
            | "--fuzz-scenarios-dir"
            | "--fuzz-scenario"
            | "--fuzz-ai-dir"
            | "--fuzz-ai"
            | "--fuzz-master-seed"
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out: Vec<OsString> = Vec::new();
    let mut args = std::env::args_os();
    out.push(args.next().unwrap_or_default());
    out.push(OsString::from("--fuzz"));

    while let Some(arg) = args.next() {
        if arg == OsStr::new("-n") || arg == OsStr::new("--iterations") {
            out.push(OsString::from("--fuzz-n"));
            if let Some(v) = args.next() {
                out.push(v);
            }
            continue;
        }

        if let Some(mapped) = map_flag(&arg) {
            out.push(OsString::from(mapped));
            if flag_takes_value(mapped) {
                if let Some(v) = args.next() {
                    out.push(v);
                }
            }
            continue;
        }

        out.push(arg);
    }

    leek_wars_gen::compare_fuzz_cli::run(out)
}
