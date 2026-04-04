//! Behavioural parity with the Java LeekScript test suite (`leek-wars-generator/leekscript`).
//!
//! Expected strings are taken from `src/test/java/test/TestOperators.java` and `TestBoolean.java`
//! (`.equals("…")` assertions, V4 / unversioned cases). Run the Java tests locally to re-validate
//! when changing semantics.

use leekscript::vm::{Vm, compile_chunk_v4};

fn run_export(source: &str) -> String {
    let chunk = compile_chunk_v4(source).unwrap_or_else(|e| panic!("compile {source:?}: {e}"));
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots)
        .unwrap_or_else(|e| panic!("locals {source:?}: {e}"));
    vm.run()
        .unwrap_or_else(|e| panic!("run {source:?}: {e}"))
        .to_leek_export_string()
}

#[test]
fn parity_equals_equals_v4_primitives() {
    // TestOperators.testOperator_EqualsEquals (V4-style strict equality for VM subset)
    assert_eq!(run_export("return null == null;"), "true");
    assert_eq!(run_export("return false == false;"), "true");
    assert_eq!(run_export("return true == true;"), "true");
    assert_eq!(run_export("return false == true;"), "false");
    assert_eq!(run_export("return true == false;"), "false");
    assert_eq!(run_export("return true == 'true';"), "false");
    assert_eq!(run_export("return false == 0;"), "false");
    assert_eq!(run_export("return true == 1;"), "false");
    assert_eq!(run_export("return 0 == false;"), "false");
    assert_eq!(run_export("return 1 == true;"), "false");
    assert_eq!(run_export("return 0 == 0;"), "true");
    assert_eq!(run_export("return 1 == 2;"), "false");
    assert_eq!(run_export("return 50 == 50;"), "true");
    assert_eq!(run_export("return 'Chaine1' == 'Chaine1';"), "true");
    assert_eq!(run_export("return 'Chaine1' == 'Chaine2';"), "false");
    assert_eq!(run_export("return '1' == 1;"), "false");
    assert_eq!(run_export("return 0 != null;"), "true");
}

#[test]
fn parity_strict_triple_equals_numbers() {
    // TestOperators.testOperator_EqualsEqualsEquals — same discriminant + value for numbers
    assert_eq!(run_export("return 1 === 1.0;"), "true");
    assert_eq!(run_export("return 12 === 12.0;"), "true");
    assert_eq!(run_export("return 0 === 1;"), "false");
}

#[test]
fn parity_comparison_real_semantics() {
    // TestOperators.testOperator_EqualsEqualsEquals (comparison / real)
    assert_eq!(run_export("return null < 3;"), "true");
    assert_eq!(run_export("return true < 10;"), "true");
    assert_eq!(run_export("return false < 10;"), "true");
    assert_eq!(run_export("return 10 < true;"), "false");
    assert_eq!(run_export("return 10 < false;"), "false");
    assert_eq!(run_export("return 10 > true;"), "true");
    assert_eq!(run_export("return 10 > false;"), "true");
    assert_eq!(run_export("return true > 10;"), "false");
    assert_eq!(run_export("return false > 10;"), "false");
}

#[test]
fn parity_not_and_v4() {
    // `not` parses in LS v3 (`TestBoolean`); V4 snippet parse uses `!` here.
    assert_eq!(run_export("return !true;"), "false");
    assert_eq!(run_export("return !false;"), "true");
    // Java: `!` binds tighter than `==` — `!null == 50` is `(!null) == 50` → false.
    assert_eq!(run_export("return !null == 50;"), "false");
    assert_eq!(run_export("var x = !null; return x == 50;"), "false");
}

#[test]
fn parity_logical_short_circuit() {
    assert_eq!(run_export("return true && true;"), "true");
    assert_eq!(run_export("return true && false;"), "false");
    assert_eq!(run_export("return false && true;"), "false");
    assert_eq!(run_export("return false || true;"), "true");
    assert_eq!(run_export("return true || false;"), "true");
    assert_eq!(run_export("return false || false;"), "false");
    // Lexer maps `and` / `or` to the same kinds as `&&` / `||`.
    assert_eq!(run_export("return true and false;"), "false");
    assert_eq!(run_export("return false or true;"), "true");
}

#[test]
fn parity_mixed_compare_and_arith() {
    assert_eq!(
        run_export("var sum = 1, ops = 10; return sum < ops * 0.95 || sum > ops;"),
        "true"
    );
    assert_eq!(
        run_export("var sum = 98, ops = 100; return sum < ops * 0.95 || sum > ops;"),
        "false"
    );
}

#[test]
fn parity_string_export() {
    assert_eq!(run_export("return 'hi';"), "'hi'");
}

#[test]
fn parity_map_literal_export_and_merge() {
    // `MapLeekValue.string` empty `[:]`; `mapMerge` uses `putIfAbsent` (left wins on duplicate keys).
    assert_eq!(run_export("return [:];"), "[:]");
    assert_eq!(run_export("return [1: 2];"), "[1 : 2]");
    assert_eq!(run_export("return [1: 2] + [3: 4];"), "[1 : 2, 3 : 4]");
    assert_eq!(run_export("return [1: 2] + [1: 99];"), "[1 : 2]");
}

#[test]
fn parity_operator_plus_java() {
    // TestOperators.testOperator_Plus
    assert_eq!(run_export("return false + 1;"), "1");
    assert_eq!(run_export("return 1 + false;"), "1");
    assert_eq!(run_export("return true + 1;"), "2");
    assert_eq!(run_export("return 1 + true;"), "2");
    assert_eq!(run_export("return true + null;"), "1");
    assert_eq!(run_export("return null + true;"), "1");
    assert_eq!(run_export("return false + null;"), "0");
    assert_eq!(run_export("return null + false;"), "0");
}

#[test]
fn parity_array_equals_v4() {
    assert_eq!(run_export("return [] == [];"), "true");
    assert_eq!(run_export("return ([] == []);"), "true");
    assert_eq!(run_export("var a = []; var b = []; return a == b;"), "true");
    assert_eq!(run_export("var a = [0]; var b = [0]; return a == b;"), "true");
    assert_eq!(run_export("var a = [0, 1]; var b = [0, 1]; return a == b;"), "true");
    assert_eq!(run_export("var a = [0, 1]; var b = [0]; return a == b;"), "false");
    assert_eq!(
        run_export("var a = ['Chaine1']; var b = ['Chaine2']; return a == b;"),
        "false"
    );
    assert_eq!(
        run_export("var a = ['Chaine1']; var b = ['Chaine1']; return a == b;"),
        "true"
    );
}

#[test]
fn parity_if_stmt() {
    assert_eq!(
        run_export("var a = 20; if (15 > a) { return 1; } return 0;"),
        "0"
    );
    assert_eq!(
        run_export("var a = 10; if (15 > a) { return 1; } return 0;"),
        "1"
    );
    assert_eq!(
        run_export("var a = 20; if (15 > a > 11) { return true; } return false;"),
        "false"
    );
}
