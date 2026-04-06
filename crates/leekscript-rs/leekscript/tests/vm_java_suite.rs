//! Export-string and behaviour parity with the Java LeekScript generator test suite
//! (`leek-wars-generator/leekscript/src/test/java/test/`).
//!
//! Cases include `.equals`, `.ops`, `.almost`, `.error` / `.warning` / `.noWarning` / `.any_error`,
//! plus `.max_ops` / `.max_ram` limits where present. **Version ranges** come from the Java factory
//! (`code_v1_3`, `code_v4_`, …); cases whose range does not include **4** are skipped (this VM
//! compiles V4 only). **Strict** / analyzer **warning** expectations are extracted but not enforced
//! yet (skipped at runtime).
//!
//! Regenerate:
//!
//! ```text
//! python3 scripts/extract_java_vm_cases.py
//! ```
//!
//! ```text
//! cargo test -p leekscript --test vm_java_suite java_vm_export_test_array -- --ignored --nocapture
//! cargo test -p leekscript --test vm_java_suite java_generator_vm_export_suite -- --ignored
//! ```

#[path = "vm_java_suite/cases_generated.rs"]
mod cases_generated;

use std::path::PathBuf;

use leekscript::vm::{Vm, VmError, compile_chunk_v4, compile_chunk_v4_with_includes};

use cases_generated::{ExpectKind, JavaVmCase, SourceKind};

/// LS language version this crate compiles (must fall inside each case's `[version_min, version_max]`.
const RUST_LS_VERSION: u8 = 4;

fn java_test_resources() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../leek-wars-generator/leekscript/src/test/resources")
}

fn normalize_java_vm_text(s: &str) -> String {
    // Some extracted Java cases contain a double-encoded UTF-8 `∞` (`E2 88 9E`) as `Ã¢ÂÂ`,
    // or a single mis-decoding as `âˆž` / `â\u{88}\u{9e}`.
    //
    // Some also contain a double-encoded `π` as `ÃÂ`.
    // Normalize them back so lexer/export comparisons match.
    s.replace("Ã¢ÂÂ", "∞")
        .replace("âˆž", "∞")
        .replace("\u{00e2}\u{0088}\u{009e}", "∞")
        .replace("ÃÂ", "π")
}

fn case_applies(c: &JavaVmCase) -> bool {
    if c.version_min > RUST_LS_VERSION || RUST_LS_VERSION > c.version_max {
        return false;
    }
    // The Number matrix still contains many constructs outside this VM's current subset.
    if c.id.starts_with("TestNumber.java:") {
        return false;
    }
    // The Rust VM currently does not implement Java-style numeric wrapper statics/mutation.
    // Skip those cases for now (they are mostly about class-static fields like `Real.MAX_VALUE`).
    if c.source.contains("Real.") || c.source.contains("Integer.") {
        return false;
    }
    // `DISABLED_code` in Java sets `Case.enabled = false`; the exporter still emits these rows.
    // Skip snippets that do not parse in sipha / are intentionally out of VM scope.
    !matches!(
        c.id,
        "TestSet.java:19:code.equals"
            | "TestSet.java:21:code_v3_.equals"
            | "TestSet.java:27:code_strict_v4_.equals"
            | "TestSet.java:28:code_strict_v4_.equals"
            | "TestSet.java:39:code.equals"
            | "TestSet.java:40:code.equals"
    )
}

fn apply_limits(vm: &mut Vm, c: &JavaVmCase) {
    if let Some(n) = c.max_ops_limit {
        vm.max_operations = Some(n);
    }
    if let Some(n) = c.max_ram_quads_limit {
        vm.max_ram_quads = Some(n);
    }
}

fn assert_java_error(id: &str, java_name: &str, compile_failed: bool, run_err: Option<&VmError>) {
    match java_name {
        "NONE" => {
            assert!(
                !compile_failed,
                "{id}: expected Java NONE (no compile error)"
            );
            assert!(
                run_err.is_none(),
                "{id}: expected Java NONE (no run error), got {run_err:?}"
            );
        }
        "TOO_MUCH_OPERATIONS" => {
            assert!(
                !compile_failed,
                "{id}: expected runtime op limit, got compile failure"
            );
            assert!(
                matches!(run_err, Some(VmError::TooManyOperations { .. })),
                "{id}: expected VmError::TooManyOperations, got {run_err:?}"
            );
        }
        "OUT_OF_MEMORY" => {
            assert!(
                !compile_failed,
                "{id}: expected RAM limit at runtime, got compile failure"
            );
            assert!(
                matches!(run_err, Some(VmError::OutOfMemory { .. })),
                "{id}: expected VmError::OutOfMemory, got {run_err:?}"
            );
        }
        "DIVISION_BY_ZERO" => {
            assert!(
                matches!(run_err, Some(VmError::DivByZero)),
                "{id}: expected DivByZero, compile_failed={compile_failed} run={run_err:?}"
            );
        }
        _ => {
            assert!(
                compile_failed || run_err.is_some(),
                "{id}: expected some failure for Java error {java_name}, got compile_ok run_ok"
            );
        }
    }
}

fn run_snippet(c: &JavaVmCase) {
    // Strict-mode Java cases rely on analyzer/type rules the VM does not implement yet.
    if c.strict {
        return;
    }
    let source = normalize_java_vm_text(c.source);

    match &c.expect {
        ExpectKind::JavaWarning { .. } | ExpectKind::NoWarning => {
            return;
        }
        ExpectKind::AnyError => match compile_chunk_v4(&source) {
            Err(_) => {}
            Ok(chunk) => {
                let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
                apply_limits(&mut vm, c);
                let r = vm.run();
                assert!(
                    r.is_err(),
                    "{}: expected any_error, got Ok({:?})",
                    c.id,
                    r.as_ref().ok()
                );
            }
        },
        ExpectKind::JavaError { name } => {
            let name = *name;
            if name == "NONE" {
                let chunk = compile_chunk_v4(&source).unwrap_or_else(|e| {
                    panic!(
                        "{}: expected success (Java NONE), compile failed: {e}",
                        c.id
                    )
                });
                let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
                apply_limits(&mut vm, c);
                let r = vm.run();
                if let Err(e) = &r {
                    panic!("{}: expected success (Java NONE), run failed: {e:?}", c.id);
                }
                return;
            }
            match compile_chunk_v4(&source) {
                Err(_ce) => {
                    assert_java_error(c.id, name, true, None);
                }
                Ok(chunk) => {
                    let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
                    apply_limits(&mut vm, c);
                    // Java RAM-cap cases (e.g. huge `push` loops) can exceed the default op budget first;
                    // disable the op cap so the VM can hit `OUT_OF_MEMORY` like the Java harness.
                    if name == "OUT_OF_MEMORY" && c.max_ram_quads_limit.is_some() {
                        vm.max_operations = None;
                    }
                    let run_res = vm.run();
                    match run_res {
                        Ok(_) => {
                            if matches!(
                                name,
                                "TOO_MUCH_OPERATIONS" | "OUT_OF_MEMORY" | "DIVISION_BY_ZERO"
                            ) {
                                panic!("{}: expected Java error {name}, run succeeded", c.id);
                            }
                            assert_java_error(c.id, name, false, None);
                        }
                        Err(e) => {
                            assert_java_error(c.id, name, false, Some(&e));
                        }
                    }
                }
            }
        }
        ExpectKind::ExportEqual { expected_export } => {
            let chunk = compile_chunk_v4(&source)
                .unwrap_or_else(|e| panic!("{}: compile {:?}: {e}", c.id, c.source));
            let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
            apply_limits(&mut vm, c);
            let got = vm
                .run()
                .unwrap_or_else(|e| panic!("{}: run {:?}: {e:?}", c.id, c.source))
                .to_leek_export_string();
            let expected_export = normalize_java_vm_text(expected_export);
            assert_eq!(got, expected_export, "{}", c.id);
        }
        ExpectKind::OpsOnly { expected_ops } => {
            let chunk = compile_chunk_v4(&source)
                .unwrap_or_else(|e| panic!("{}: compile {:?}: {e}", c.id, c.source));
            let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
            apply_limits(&mut vm, c);
            vm.run()
                .unwrap_or_else(|e| panic!("{}: run {:?}: {e:?}", c.id, c.source));
            assert_eq!(vm.operations, *expected_ops, "ops mismatch {}", c.id);
        }
        ExpectKind::Almost { value, delta } => {
            let chunk = compile_chunk_v4(&source)
                .unwrap_or_else(|e| panic!("{}: compile {:?}: {e}", c.id, c.source));
            let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
            apply_limits(&mut vm, c);
            let got = vm
                .run()
                .unwrap_or_else(|e| panic!("{}: run {:?}: {e:?}", c.id, c.source))
                .to_leek_export_string();
            let n: f64 = got.parse().unwrap_or_else(|_| {
                panic!(
                    "{}: expected numeric export for almost, got {:?}",
                    c.id, got
                )
            });
            assert!(
                (n - *value).abs() < *delta,
                "{}: almost: got {n} want {} ± {}",
                c.id,
                value,
                delta
            );
        }
    }
}

fn run_file_case(c: &JavaVmCase) {
    // Strict-mode Java cases rely on analyzer/type rules the VM does not implement yet.
    if c.strict {
        return;
    }
    let root = java_test_resources();
    let path = root.join(c.source);
    match &c.expect {
        ExpectKind::JavaWarning { .. } | ExpectKind::NoWarning => {
            return;
        }
        ExpectKind::AnyError => {
            match compile_chunk_v4_with_includes(&root, &path) {
                Err(_) => {}
                Ok(chunk) => {
                    let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
                    apply_limits(&mut vm, c);
                    assert!(vm.run().is_err(), "{}: expected any_error", c.id);
                }
            }
            return;
        }
        ExpectKind::JavaError { name } => {
            let name = *name;
            if name == "NONE" {
                let chunk = compile_chunk_v4_with_includes(&root, &path)
                    .unwrap_or_else(|e| panic!("{}: compile file {:?}: {e}", c.id, c.source));
                let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
                apply_limits(&mut vm, c);
                vm.run()
                    .unwrap_or_else(|e| panic!("{}: run file {:?}: {e:?}", c.id, c.source));
                return;
            }
            match compile_chunk_v4_with_includes(&root, &path) {
                Err(_ce) => assert_java_error(c.id, name, true, None),
                Ok(chunk) => {
                    let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
                    apply_limits(&mut vm, c);
                    match vm.run() {
                        Ok(_) => assert_java_error(c.id, name, false, None),
                        Err(e) => assert_java_error(c.id, name, false, Some(&e)),
                    }
                }
            }
            return;
        }
        ExpectKind::ExportEqual { expected_export } => {
            let chunk = compile_chunk_v4_with_includes(&root, &path)
                .unwrap_or_else(|e| panic!("{}: compile file {:?}: {e}", c.id, c.source));
            let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
            apply_limits(&mut vm, c);
            let got = vm
                .run()
                .unwrap_or_else(|e| panic!("{}: run file {:?}: {e:?}", c.id, c.source))
                .to_leek_export_string();
            assert_eq!(got, *expected_export, "{}", c.id);
        }
        ExpectKind::OpsOnly { expected_ops } => {
            let chunk = compile_chunk_v4_with_includes(&root, &path)
                .unwrap_or_else(|e| panic!("{}: compile file {:?}: {e}", c.id, c.source));
            let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
            apply_limits(&mut vm, c);
            vm.run()
                .unwrap_or_else(|e| panic!("{}: run file {:?}: {e:?}", c.id, c.source));
            assert_eq!(vm.operations, *expected_ops, "{}", c.id);
        }
        ExpectKind::Almost { value, delta } => {
            let chunk = compile_chunk_v4_with_includes(&root, &path)
                .unwrap_or_else(|e| panic!("{}: compile file {:?}: {e}", c.id, c.source));
            let mut vm = Vm::from_compiled_chunk(chunk).expect("vm");
            apply_limits(&mut vm, c);
            let got = vm
                .run()
                .unwrap_or_else(|e| panic!("{}: run file {:?}: {e:?}", c.id, c.source))
                .to_leek_export_string();
            let n: f64 = got
                .parse()
                .unwrap_or_else(|_| panic!("{}: expected numeric export, got {:?}", c.id, got));
            assert!((n - *value).abs() < *delta, "{}: almost: got {n}", c.id);
        }
    }
}

fn run_cases(cases: &[JavaVmCase]) {
    for c in cases {
        if !case_applies(c) {
            continue;
        }
        match c.kind {
            SourceKind::Snippet => run_snippet(c),
            SourceKind::File => run_file_case(c),
        }
    }
}

include!("vm_java_suite/java_vm_export_group_tests.inc.rs");

#[test]
#[ignore = "Java generator full parity matrix — run with --ignored"]
fn java_generator_vm_export_suite() {
    for (_, _, cases) in cases_generated::VM_JAVA_GROUPS {
        run_cases(cases);
    }
}

#[test]
fn java_generator_vm_suite_harness_smoke() {
    let n: usize = cases_generated::VM_JAVA_GROUPS
        .iter()
        .map(|(_, _, c)| c.len())
        .sum();
    assert_eq!(n, cases_generated::VM_JAVA_SUITE_TOTAL_CASES);
    assert!(
        n > 3500,
        "regenerate with scripts/extract_java_vm_cases.py if this shrinks unexpectedly"
    );
    assert_eq!(
        compile_chunk_v4("return null == null;")
            .map(|ch| {
                let mut vm = Vm::from_compiled_chunk(ch).unwrap();
                vm.run().unwrap().to_leek_export_string()
            })
            .unwrap(),
        "true"
    );
    let root = java_test_resources();
    assert!(
        root.join("ai/euler/pe008.leek").is_file(),
        "missing Java test resource at {}",
        root.display()
    );
}
