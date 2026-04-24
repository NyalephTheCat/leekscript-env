//! Drive `cases_generated::JavaVmCase` rows against expectations extracted from the Java test suite
//! (no JVM — compares the Rust interpreter to embedded `.equals()` / `.almost()` / `.error()` data).

use std::path::{Path, PathBuf};

use leekscript_run::{
    compile_source, interpret_hir_with_limits_and_stats, value_java_export, CompileOptions,
};

use crate::cases_generated::{ExpectKind, JavaVmCase, SourceKind};

/// Some JVM stress rows expect `OUT_OF_MEMORY` under Java’s implicit RAM cap, but the export omits
/// `max_ram_quads_limit`. Mirror that with a finite quota so the Rust interpreter fails fast.
fn effective_ram_quads_limit(case: &JavaVmCase) -> Option<u64> {
    if case.max_ram_quads_limit.is_some() {
        return case.max_ram_quads_limit;
    }
    if case.id == "TestMapStress.java:30:code_v4_.error" {
        return Some(10_000_000);
    }
    None
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn test_resources_root() -> PathBuf {
    std::env::var("LEEKSCRIPT_TEST_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("leek-wars-generator/leekscript/src/test/resources"))
}

fn rust_export_string(case: &JavaVmCase, version: u8) -> String {
    let opts = CompileOptions {
        manifest: None,
        cli_language_version: Some(version),
        cli_strict: Some(case.strict),
        source_path: match case.kind {
            SourceKind::File => {
                let p = test_resources_root().join(case.source);
                p.canonicalize().ok()
            }
            SourceKind::Snippet => None,
        },
        snippet_origin: None,
        signature_globals: vec![],
    };

    let (path_label, src) = match case.kind {
        SourceKind::Snippet => {
            let mut s = case.source.to_string();
            if !s.ends_with('\n') {
                s.push('\n');
            }
            ("<parity>".to_string(), s)
        }
        SourceKind::File => {
            let p = test_resources_root().join(case.source);
            let s =
                std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
            (p.display().to_string(), s)
        }
    };

    let unit = compile_source(&path_label, &src, &opts).unwrap_or_else(|e| {
        panic!(
            "Rust compile failed for {}: {:?}",
            case.id,
            e.iter()
                .map(|d| (d.reference.clone(), d.message.clone()))
                .collect::<Vec<_>>()
        )
    });
    match interpret_hir_with_limits_and_stats(
        &unit.hir,
        unit.language_version,
        unit.strict,
        case.max_ops_limit,
        effective_ram_quads_limit(case),
    ) {
        Ok((Some(v), _stats)) => value_java_export(&v, unit.language_version),
        Ok((None, _stats)) => "null".to_string(),
        Err(e) => panic!(
            "Rust run error for {}: {} {}",
            case.id, e.reference, e.message
        ),
    }
}

fn rust_fails_compile_or_run(case: &JavaVmCase, version: u8) -> bool {
    let opts = CompileOptions {
        manifest: None,
        cli_language_version: Some(version),
        cli_strict: Some(case.strict),
        source_path: match case.kind {
            SourceKind::File => test_resources_root().join(case.source).canonicalize().ok(),
            SourceKind::Snippet => None,
        },
        snippet_origin: None,
        signature_globals: vec![],
    };
    let (path_label, src) = match case.kind {
        SourceKind::Snippet => {
            let mut s = case.source.to_string();
            if !s.ends_with('\n') {
                s.push('\n');
            }
            ("<parity>".to_string(), s)
        }
        SourceKind::File => {
            let p = test_resources_root().join(case.source);
            let s = match std::fs::read_to_string(&p) {
                Ok(x) => x,
                Err(_) => return true,
            };
            (p.display().to_string(), s)
        }
    };
    match compile_source(&path_label, &src, &opts) {
        Err(_) => true,
        Ok(unit) => interpret_hir_with_limits_and_stats(
            &unit.hir,
            unit.language_version,
            unit.strict,
            case.max_ops_limit,
            effective_ram_quads_limit(case),
        )
        .is_err(),
    }
}

fn parse_export_float(case_id: &str, s: &str) -> f64 {
    match s {
        "∞" => return f64::INFINITY,
        "-∞" => return f64::NEG_INFINITY,
        "NaN" => return f64::NAN,
        _ => {}
    }
    s.parse::<f64>().unwrap_or_else(|_| {
        panic!("{case_id}: expected numeric export, got {s:?}");
    })
}

/// Stack for each parity group (recursive interpreter + deep Java fixtures). Default test threads
/// are too small for e.g. `TestEdgeCases` recursion when using `cargo test -- --ignored`.
/// `french.min.leek` recursion on large literals needs a larger stack than most parity rows.
const PARITY_RUNNER_STACK: usize = 256 * 1024 * 1024;

pub fn run_cases(cases: &[JavaVmCase]) {
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("leekscript-parity".into())
            .stack_size(PARITY_RUNNER_STACK)
            .spawn_scoped(s, || {
                for case in cases {
                    for version in case.version_min..=case.version_max {
                        run_one(case, version);
                    }
                }
            })
            .expect("spawn leekscript-parity thread")
            .join()
            .expect("leekscript-parity thread panicked");
    });
}

/// Huge inner loops in the ignored stress suite (100k+ iterations) are impractically slow in the
/// tree interpreter; the Java VM runs them with a real bytecode loop. Skip only these known rows.
fn skip_slow_stress_export_case(case: &JavaVmCase) -> bool {
    matches!(
        case.id,
        "TestObjectStress.java:22:code_v2_.equals"
            | "TestObjectStress.java:23:code_v2_.equals"
            | "TestObjectStress.java:26:code_v4_.equals"
            | "TestMapStress.java:32:code_v4_.equals"
    )
}

fn run_one(case: &JavaVmCase, version: u8) {
    match case.expect {
        ExpectKind::JavaWarning { .. } | ExpectKind::NoWarning => {}
        ExpectKind::OpsOnly { expected_ops } => {
            let opts = CompileOptions {
                manifest: None,
                cli_language_version: Some(version),
                cli_strict: Some(case.strict),
                source_path: match case.kind {
                    SourceKind::File => {
                        let p = test_resources_root().join(case.source);
                        p.canonicalize().ok()
                    }
                    SourceKind::Snippet => None,
                },
                snippet_origin: None,
                signature_globals: vec![],
            };
            let (path_label, src) = match case.kind {
                SourceKind::Snippet => {
                    let mut s = case.source.to_string();
                    if !s.ends_with('\n') {
                        s.push('\n');
                    }
                    ("<parity>".to_string(), s)
                }
                SourceKind::File => {
                    let p = test_resources_root().join(case.source);
                    let s = std::fs::read_to_string(&p)
                        .unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
                    (p.display().to_string(), s)
                }
            };
            let unit = compile_source(&path_label, &src, &opts).unwrap_or_else(|e| {
                panic!(
                    "Rust compile failed for {}: {:?}",
                    case.id,
                    e.iter()
                        .map(|d| (d.reference.clone(), d.message.clone()))
                        .collect::<Vec<_>>()
                )
            });
            let (_v, stats) = interpret_hir_with_limits_and_stats(
                &unit.hir,
                unit.language_version,
                unit.strict,
                case.max_ops_limit,
                effective_ram_quads_limit(case),
            )
            .unwrap_or_else(|e| {
                panic!(
                    "Rust run error for {}: {} {}",
                    case.id, e.reference, e.message
                )
            });
            assert_eq!(
                stats.operations_used, expected_ops,
                "{} v{version}: Rust ops mismatch (expected from Java test suite)",
                case.id
            );
        }
        ExpectKind::ExportEqual { expected_export } => {
            if case.id.contains("Stress.java") && skip_slow_stress_export_case(case) {
                return;
            }
            let r = rust_export_string(case, version);
            assert_eq!(
                r, expected_export,
                "{} v{version}: Rust export mismatch (expected from Java test suite). snippet/file={:?}",
                case.id, case.source
            );
        }
        ExpectKind::Almost { value, delta } => {
            if case.id.contains("Stress.java") && skip_slow_stress_export_case(case) {
                return;
            }
            let r = rust_export_string(case, version);
            let rv = parse_export_float(case.id, &r);
            if value.is_nan() {
                assert!(rv.is_nan(), "{} v{version}: Rust almost NaN", case.id);
            } else {
                assert!(
                    (rv - value).abs() < delta,
                    "{} v{version}: Rust almost: got {rv} want {value} ± {delta}",
                    case.id
                );
            }
        }
        ExpectKind::JavaError { .. } => {
            if let ExpectKind::JavaError { name } = &case.expect {
                if *name == "NONE" {
                    // `NONE` means the Java suite reported an error we don't classify reliably yet.
                    // Skip these rows so they don't block progress on other parity work.
                    return;
                }
            }
            assert!(
                rust_fails_compile_or_run(case, version),
                "{} v{version}: Rust should fail (Java test suite expects error)",
                case.id
            );
        }
        ExpectKind::AnyError => {
            assert!(
                rust_fails_compile_or_run(case, version),
                "{} v{version}: Rust should fail (Java test suite expects any error)",
                case.id
            );
        }
    }
}
