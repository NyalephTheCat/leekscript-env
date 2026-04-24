//! Full Java `Test*.java` parity suite (generated `cases_generated.rs` + grouped tests).
//!
//! Regenerate from the reference JUnit sources:
//!
//! ```text
//! python3 scripts/extract_java_vm_cases.py
//! ```
//!
//! **Default run** (all parity groups except stress and I/O): expectations come from the extracted
//! Java suite; no JVM. Stress files (`*Stress.java`) and the **`io`** module (file/json/system
//! tests) use `#[ignore]` because they are slow—everything else is a normal `#[test]`.
//!
//! ```text
//! cargo test -p leekscript_run --test java_vm_suite
//! ```
//!
//! **Ignored suites** (stress, I/O) when you need them:
//!
//! ```text
//! cargo test -p leekscript_run --test java_vm_suite -- --ignored
//! ```
//!
//! **I/O only**:
//!
//! ```text
//! cargo test -p leekscript_run --test java_vm_suite io:: -- --ignored
//! ```
//!
//! Parity runs each group on a large-stack thread so deep recursion fixtures do not overflow.
//! JVM-only `JavaError` rows in stress (RAM / op limits, map growth) are skipped; megaloop
//! `ExportEqual` rows that are too slow for the tree interpreter are skipped by case id (see
//! `runner.rs`).
//!
//! File-based cases resolve under `leek-wars-generator/leekscript/src/test/resources` (override with
//! `LEEKSCRIPT_TEST_RESOURCES`).

#[path = "java_vm_suite/cases_generated.rs"]
mod cases_generated;
#[path = "java_vm_suite/runner.rs"]
pub mod runner;

include!("java_vm_suite/java_vm_export_group_tests.inc.rs");
