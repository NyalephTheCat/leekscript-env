//! CLI harness: timed Java `generator.jar` vs Rust engine, or `--fuzz` mode.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    leek_wars_gen::compare_fuzz_cli::run(std::env::args_os())
}
