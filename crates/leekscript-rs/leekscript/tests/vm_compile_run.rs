//! Integration tests for the table-driven VM and V4 bytecode compiler.

use leekscript::vm::{
    BytecodeBuilder, CompileError, NativeFn, NumberBits, Opcode, Value, Vm, VmError,
    compile_chunk_v4, compile_chunk_v4_with_includes, op_illegal, stdlib_global_constant_init,
    stdlib_global_function_init,
};


#[test]
fn loop_bytecode_includes_charge_ops_for_java_style_budget() {
    let chunk = compile_chunk_v4("var i = 0; while (i < 2) { i = i + 1; } return i;").unwrap();
    assert!(
        chunk.bytecode.code.contains(&(Opcode::ChargeOps as u8)),
        "expected ChargeOps in while loop lowering"
    );
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(2));
}

#[test]
fn foreach_over_array_sums() {
    let src = r#"
        var a = [1, 2, 3];
        var s = 0;
        for (x in a) { s = s + x; }
        return s;
    "#;
    let chunk = compile_chunk_v4(src).expect("compile");
    assert!(
        chunk.bytecode.code.contains(&(Opcode::ArrayLen as u8)),
        "foreach lowering should use ArrayLen"
    );
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(6));
}

#[test]
fn global_decl_binds_local_slot() {
    let chunk = compile_chunk_v4("global integer x; return x;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    // Java-style typed integer globals default to 0 (not null).
    assert_eq!(vm.run().unwrap(), Value::num_int(0));
}

#[test]
fn const_decl_compiles_like_var() {
    let chunk = compile_chunk_v4("const n = 40; return n + 2;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(42));
}

#[test]
fn empty_class_compiles_as_noop() {
    let chunk = compile_chunk_v4("class C {} return 1;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(1));
}

#[test]
fn function_decl_and_call() {
    let src = r#"
        function add(a, b) { return a + b; }
        return add(10, 32);
    "#;
    let chunk = compile_chunk_v4(src).expect("compile");
    assert!(
        chunk.bytecode.code.contains(&(Opcode::CallFunction as u8)),
        "expected CallFunction"
    );
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(42));
}

#[test]
fn switch_and_break() {
    let src = r#"
        var x = 2;
        var r = 0;
        switch (x) {
            case 1: r = 10; break;
            case 2: r = 20; break;
            default: r = 99;
        }
        return r;
    "#;
    let chunk = compile_chunk_v4(src).expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(20));
}

#[test]
fn try_catch_throw() {
    let src = r#"
        var r = 0;
        try {
            throw 7;
            r = 1;
        } catch (integer e) {
            r = e + 1;
        }
        return r;
    "#;
    let chunk = compile_chunk_v4(src).expect("compile");
    assert!(chunk.bytecode.code.contains(&(Opcode::Throw as u8)));
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(8));
}

#[test]
fn foreach_key_value_over_map() {
    let src = r#"
        var m = [1: 10, 2: 20];
        var s = 0;
        for (k : v in m) { s = s + v; }
        return s;
    "#;
    let chunk = compile_chunk_v4(src).expect("compile");
    assert!(
        chunk.bytecode.code.contains(&(Opcode::MapEntryAt as u8)),
        "map foreach should use MapEntryAt"
    );
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(30));
}

#[test]
fn compile_with_includes_sees_lib_vars() {
    let root = std::env::temp_dir().join(format!("leek_vm_inc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(root.join("lib.leek"), "var secret = 41;").expect("write lib");
    std::fs::write(
        root.join("main.leek"),
        "include(\"lib.leek\");\nreturn secret;",
    )
    .expect("write main");
    let chunk = compile_chunk_v4_with_includes(&root, &root.join("main.leek")).expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(41));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn numeric_eq_in_var_initializer_compiles() {
    let chunk = compile_chunk_v4("var r = 1 == 2; return r;").expect("compile");
    assert!(chunk.bytecode.code.contains(&(Opcode::EqEquals as u8)));
}

#[test]
fn array_eq_in_var_initializer_compiles_and_runs() {
    let chunk = compile_chunk_v4("var r = [] == []; return r;").expect("compile");
    assert!(chunk.bytecode.code.contains(&(Opcode::EqEquals as u8)));
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::Bool(true));
}

#[test]
fn array_literal_with_more_than_255_elements_runs() {
    let mut src = String::from("return [");
    for i in 0..260 {
        if i > 0 {
            src.push(',');
        }
        src.push('1');
    }
    src.push_str("];");
    let chunk = compile_chunk_v4(&src).expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    let v = vm.run().expect("run");
    let Value::Array(a) = v else {
        panic!("expected array");
    };
    assert_eq!(a.borrow().len(), 260);
}

#[test]
fn string_concat_charges_ops_like_java_ai_add() {
    // Java-style budget: return pre-charges analyzed ops for the value expr; string `+` adds
    // `len(lhs) + len(rhs)` at runtime (same idea as `AI.add` string concat cost).
    let chunk = compile_chunk_v4("return 'ab' + 'cde';").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    vm.run().expect("run");
    assert_eq!(
        vm.operations, 6,
        "return expr ops + 2 + 3 string concat ops"
    );
}

#[test]
fn map_literal_compiles_to_map_build() {
    let chunk = compile_chunk_v4("return [:];").expect("compile");
    assert!(chunk.bytecode.code.contains(&(Opcode::MapBuild as u8)));
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    let v = vm.run().expect("run");
    assert!(matches!(v, Value::Map(m) if m.borrow().is_empty()));
}

#[test]
fn map_merge_put_if_absent_matches_java() {
    let chunk = compile_chunk_v4("return [1: 2] + [1: 3];").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    let v = vm.run().expect("run");
    let Value::Map(m) = v else {
        panic!("expected map");
    };
    let mb = m.borrow();
    assert_eq!(mb.len(), 1);
    assert_eq!(mb[0].0, Value::num_int(1));
    assert_eq!(mb[0].1, Value::num_int(2));
}

#[test]
fn dispatch_table_is_full_and_defaults_to_illegal() {
    use leekscript::vm::DISPATCH;
    assert_eq!(DISPATCH.len(), 256);
    assert_eq!(DISPATCH[0] as usize, op_illegal as *const () as usize);
}

#[test]
fn compile_and_run_mul_only() {
    let chunk = compile_chunk_v4("return 3 * 4;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(12));
}

#[test]
fn compile_and_run_arithmetic_return() {
    let chunk = compile_chunk_v4("return 2 + 3 * 4;").expect("compile");
    assert_eq!(
        chunk.local_slots,
        stdlib_global_constant_init().count() + stdlib_global_function_init().count(),
        "prelude stdlib bindings reserve locals"
    );
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    let v = vm.run().expect("run");
    assert_eq!(v, Value::num_int(14));
}

#[test]
fn compile_var_and_load() {
    let chunk = compile_chunk_v4("var x = 10; return x + 1;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    let v = vm.run().expect("run");
    assert_eq!(v, Value::num_int(11));
}

#[test]
fn unary_minus() {
    let chunk = compile_chunk_v4("return - (3 + 2);").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    let v = vm.run().expect("run");
    assert_eq!(v, Value::num_int(-5));
}

#[test]
fn native_call_roundtrip() {
    fn add_two(_vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
        let a = args
            .first()
            .and_then(|v| v.as_number())
            .ok_or(VmError::ExpectedNumber)?;
        let b = args
            .get(1)
            .and_then(|v| v.as_number())
            .ok_or(VmError::ExpectedNumber)?;
        Ok(Value::Number(NumberBits::coerce_integerish_f64(a + b)))
    }
    let n: NativeFn = add_two;

    let mut b = BytecodeBuilder::new();
    b.emit_push_const(Value::num_int(40));
    b.emit_push_const(Value::num_int(2));
    b.emit_call_native(0, 2);
    b.emit_return();

    let mut vm = Vm::new(b.finish());
    vm.set_natives(vec![n]);
    vm.set_local_count(0).expect("locals");
    let v = vm.run().expect("run");
    assert_eq!(v, Value::num_int(42));
}

#[test]
fn illegal_opcode_errors() {
    let mut b = BytecodeBuilder::new();
    b.emit_opcode(Opcode::Illegal);
    let mut vm = Vm::new(b.finish());
    vm.set_local_count(0).expect("locals");
    let e = vm.run().expect_err("illegal");
    assert!(matches!(e, VmError::IllegalOpcode(0)));
}

#[test]
fn unsupported_stmt_is_compile_error() {
    let r = compile_chunk_v4("class C { var x; }");
    assert!(matches!(r, Err(CompileError::Unsupported(_))));
}

#[test]
fn operation_limit_enforced() {
    let mut b = BytecodeBuilder::new();
    b.emit_charge_ops(1);
    b.emit_charge_ops(1);
    b.emit_charge_ops(1);
    b.emit_return();
    let mut vm = Vm::new(b.finish());
    vm.max_operations = Some(2);
    vm.set_local_count(0).expect("locals");
    let e = vm.run().expect_err("limit");
    assert!(matches!(e, VmError::TooManyOperations { .. }));
}

#[test]
fn ram_limit_enforced_on_stack_growth() {
    let mut b = BytecodeBuilder::new();
    b.emit_opcode(Opcode::PushNull);
    b.emit_opcode(Opcode::PushNull);
    b.emit_opcode(Opcode::PushNull);
    b.emit_return();
    let mut vm = Vm::new(b.finish());
    vm.max_ram_quads = Some(2);
    vm.set_local_count(0).expect("locals");
    let e = vm.run().expect_err("oom");
    assert!(matches!(e, VmError::OutOfMemory { .. }));
}

#[test]
fn operations_and_ram_counters_after_run() {
    let chunk = compile_chunk_v4("return 1 + 2;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    vm.run().expect("run");
    assert!(vm.operations > 0);
    let prelude_ram: u64 = stdlib_global_constant_init()
        .map(|(_, v)| v.ram_quads())
        .sum();
    let prelude_fn_ram: u64 = stdlib_global_function_init()
        .map(|(_, v)| v.ram_quads())
        .sum();
    assert_eq!(
        vm.ram_quads,
        prelude_ram + prelude_fn_ram,
        "locals still hold stdlib bindings"
    );
}

#[test]
fn while_loop_and_assign_expr() {
    let chunk =
        compile_chunk_v4("var i = 0; while (i < 3) { i = i + 1; } return i;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(3));
}

#[test]
fn for_loop_var_init() {
    let chunk =
        compile_chunk_v4("for (var i = 0; i < 4; i = i + 1) { } return i;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(4));
}

#[test]
fn do_while_runs_body_once() {
    let chunk =
        compile_chunk_v4("var n = 0; do { n = n + 1; } while (false); return n;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(1));
}

#[test]
fn break_exits_while() {
    let chunk = compile_chunk_v4(
        "var i = 0; while (i < 100) { if (i == 5) { break; } i = i + 1; } return i;",
    )
    .expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(5));
}

#[test]
fn continue_skips_rest_of_while_body() {
    let chunk = compile_chunk_v4(
        "var i = 0; var s = 0; while (i < 4) { i = i + 1; if (i == 2) { continue; } s = s + i; } return s;",
    )
    .expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    // 1 + 3 + 4 = 8 (skip adding when i is 2)
    assert_eq!(vm.run().expect("run"), Value::num_int(8));
}

#[test]
fn get_elem_opcode_emitted_for_index() {
    let chunk = compile_chunk_v4("return [10][0];").expect("compile");
    assert!(chunk.bytecode.code.contains(&(Opcode::GetElem as u8)));
}

#[test]
fn array_index_and_oob_null() {
    let chunk = compile_chunk_v4("return [7, 8][1];").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(8));

    let chunk = compile_chunk_v4("return [1][9];").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::Null);
}

#[test]
fn map_subscript_and_dot_member() {
    let chunk = compile_chunk_v4("return [1: 2][1];").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(2));

    let chunk = compile_chunk_v4("var m = ['k': 5]; return m.k;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(5));
}

#[test]
fn ternary_expression() {
    let chunk = compile_chunk_v4("return 1 ? 2 : 3;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(2));

    let chunk = compile_chunk_v4("return 0 ? 2 : 3;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(3));

    let chunk = compile_chunk_v4("return 1 + 2 ? 30 : 40;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(30));
}

#[test]
fn compound_assign_in_loop() {
    let chunk =
        compile_chunk_v4("var i = 0; var s = 0; while (i < 3) { i += 1; s += i; } return s;")
            .expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::num_int(6));
}

#[test]
fn compile_paren_grouped_binary() {
    let chunk = compile_chunk_v4("return (1 + 2) / 2;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_real(1.5));
}

#[test]
fn compile_prefix_increment_return() {
    let chunk = compile_chunk_v4("var i = 0; return ++i;").expect("compile ++i");
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(1));
}

#[test]
fn stdlib_global_constants_from_sig() {
    let chunk = compile_chunk_v4("return TYPE_NUMBER + SORT_DESC + COLOR_BLUE;").expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().expect("run"), Value::num_int(257));
}

#[test]
fn stdlib_natives_from_sig_core() {
    let cases = [
        ("return abs(-3);", Value::num_int(3)),
        ("return sqrt(4);", Value::num_int(2)),
        ("return length('ab');", Value::num_int(2)),
        ("return count([1, 2]);", Value::num_int(2)),
        ("return join(['a', 'b'], '-');", Value::String("a-b".into())),
        ("return indexOf('abc', 'b');", Value::num_int(1)),
        ("return abs(-2) + 1;", Value::num_int(3)),
    ];
    for (src, want) in cases {
        let chunk = compile_chunk_v4(src).unwrap_or_else(|e| panic!("compile {src:?}: {e}"));
        let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
        let got = vm.run().unwrap_or_else(|e| panic!("run {src:?}: {e}"));
        assert_eq!(got, want, "src={src:?}");
    }
}

#[test]
fn foreach_over_set_sums_java_normalizer_shape() {
    let src = "var s = <1, 2, 3, 4, 5>; var x = 0; for (var y in s) { x = x + y; } return x;";
    let chunk = compile_chunk_v4(src).expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(15));
}

#[test]
fn compound_assign_stmt_updates_local() {
    let src = "var x = 0; var y = 3; x += y; return x;";
    let chunk = compile_chunk_v4(src).expect("compile");
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    assert_eq!(vm.run().unwrap(), Value::num_int(3));
}

#[test]
fn no_semicolon_between_stmts_after_set_literal_parses() {
    let s = "var i = <1, 2> setPut(i, 3) return i";
    let r = compile_chunk_v4(s);
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn set_put_on_local_after_set_literal_compiles() {
    let just_var = "var i = <1, 2>; return i;";
    let r0 = compile_chunk_v4(just_var);
    assert!(r0.is_ok(), "var with set literal: {r0:?}");
    // Two-part call `setPut(i, x)` is lowered in `try_emit_ident_call_two_part`, not `compile_call_expr`.
    let src_semi = "var i = <1, 2>; setPut(i, 3); return i;";
    let chunk = compile_chunk_v4(src_semi).unwrap_or_else(|e| {
        panic!("setPut on plain local after set literal should compile: {e:?}")
    });
    let mut vm = Vm::from_compiled_chunk(chunk).unwrap();
    let got = vm.run().unwrap();
    assert_eq!(got, Value::Set(vec![Value::num_int(1), Value::num_int(2), Value::num_int(3)]));
}

#[test]
fn testobject_style_class_fields_and_foreach() {
    let fields_only = "class Test { a b c } return Test.fields";
    let chunk_f = compile_chunk_v4(fields_only).expect("compile Test.fields");
    let mut vm = Vm::from_compiled_chunk(chunk_f).unwrap();
    let f = vm.run().unwrap();
    assert_eq!(f.to_leek_export_string(), r#"["a", "b", "c"]"#);

    let full = "class Test { a b c } var test2 = new Test() for (var field in Test.fields) { test2[field] = 8 } return test2";
    let chunk = compile_chunk_v4(full).expect("compile full");
    let mut vm2 = Vm::from_compiled_chunk(chunk).unwrap();
    let got = vm2.run().unwrap();
    assert_eq!(got.to_leek_export_string(), "Test {a: 8, b: 8, c: 8}");
}
