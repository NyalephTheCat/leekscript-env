#![no_main]

use libfuzzer_sys::fuzz_target;
use rand::rngs::StdRng;
use rand::SeedableRng;

use leekscript_run::CompileOptions;

/// Mix all input bytes so long corpora and trailing bytes affect the mutator RNG (not only the prefix).
fn seed_from_bytes(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;
    let mut h = FNV_OFFSET;
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

fuzz_target!(|data: &[u8]| {
    // Avoid pathological allocations / quadratic behavior on huge inputs.
    if data.len() > 64 * 1024 {
        return;
    }

    // Keep the toolchain entrypoints exercised on mostly text inputs.
    let src = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };
    if src.trim().is_empty() {
        return;
    }

    // Parsing is cheap; compilation is gated behind “parses any version”.
    let parses = leekscript_fuzz::source_parses_any_version(src);
    if parses {
        let _ = leekscript_run::compile_source("fuzz.leek", src, &CompileOptions::default());
    }

    // Also exercise the syntax-aware mutator + optional compile gate.
    // Levels 1..=4 always (level 0 is a no-op in the mutator and wastes corpus diversity).
    let level = 1 + (data.first().copied().unwrap_or(0) % 4);

    let mut rng = StdRng::seed_from_u64(seed_from_bytes(data));
    let mut settings = if parses {
        leekscript_fuzz::MutateSettings::require_parseable()
    } else {
        leekscript_fuzz::MutateSettings::accept_all()
    };
    // Optional inject knobs from extra bytes (level-4 statement wrap / injected stmts).
    if data.len() >= 2 {
        settings.inject.complexity = (data[1] % 6).min(5);
    }
    if data.len() >= 3 {
        settings.inject.wrap_percent = data[2] % 101;
    }
    if data.len() >= 4 {
        settings.inject.max_injected_stmts = (data[3] % 16) + 1;
    }
    if data.len() >= 5 {
        // Vary retry budget without defaulting below a reasonable parse-retry floor.
        settings.max_attempts = 32 + u32::from(data[4] % 224);
    }
    let out = match leekscript_fuzz::mutate_leek_source(src, &mut rng, level, &settings) {
        Ok(o) => o.source,
        Err(_) => return,
    };
    let out_parses = leekscript_fuzz::source_parses_any_version(&out);
    if out_parses {
        let _ = leekscript_run::compile_source("fuzz_mutant.leek", &out, &CompileOptions::default());
    }
});

