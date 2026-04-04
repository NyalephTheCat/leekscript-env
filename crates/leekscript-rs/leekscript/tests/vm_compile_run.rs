//! Integration tests for the table-driven VM and V4 bytecode compiler.

use leekscript::vm::{
    BytecodeBuilder, CompileError, NativeFn, Opcode, Value, Vm, VmError, compile_chunk_v4,
    op_illegal,
};

#[test]
fn numeric_eq_in_var_initializer_compiles() {
    let chunk = compile_chunk_v4("var r = 1 == 2; return r;").expect("compile");
    assert!(chunk.bytecode.code.contains(&(Opcode::EqEquals as u8)));
}

#[test]
fn array_eq_in_var_initializer_compiles_and_runs() {
    let chunk = compile_chunk_v4("var r = [] == []; return r;").expect("compile");
    assert!(chunk.bytecode.code.contains(&(Opcode::EqEquals as u8)));
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
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
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    let v = vm.run().expect("run");
    let Value::Array(a) = v else {
        panic!("expected array");
    };
    assert_eq!(a.len(), 260);
}

#[test]
fn string_concat_charges_ops_like_java_ai_add() {
    // `AI.add` charges `string(a).length() + string(b).length()` on top of per-line/step costs.
    // VM charges +1 per opcode after the handler; `Add` on two strings adds len sum inside `op_add`.
    let chunk = compile_chunk_v4("return 'ab' + 'cde';").expect("compile");
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    vm.run().expect("run");
    assert_eq!(vm.operations, 9, "4 opcode ticks + 2 + 3 string concat ops");
}

#[test]
fn map_literal_compiles_to_map_build() {
    let chunk = compile_chunk_v4("return [:];").expect("compile");
    assert!(chunk.bytecode.code.contains(&(Opcode::MapBuild as u8)));
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    let v = vm.run().expect("run");
    assert!(matches!(v, Value::Map(m) if m.is_empty()));
}

#[test]
fn map_merge_put_if_absent_matches_java() {
    let chunk = compile_chunk_v4("return [1: 2] + [1: 3];").expect("compile");
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    let v = vm.run().expect("run");
    let Value::Map(m) = v else {
        panic!("expected map");
    };
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].0, Value::Number(1.0));
    assert_eq!(m[0].1, Value::Number(2.0));
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
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::Number(12.0));
}

#[test]
fn compile_and_run_arithmetic_return() {
    let chunk = compile_chunk_v4("return 2 + 3 * 4;").expect("compile");
    assert_eq!(chunk.local_slots, 0);
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    let v = vm.run().expect("run");
    assert_eq!(v, Value::Number(14.0));
}

#[test]
fn compile_var_and_load() {
    let chunk = compile_chunk_v4("var x = 10; return x + 1;").expect("compile");
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    let v = vm.run().expect("run");
    assert_eq!(v, Value::Number(11.0));
}

#[test]
fn unary_minus() {
    let chunk = compile_chunk_v4("return - (3 + 2);").expect("compile");
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    let v = vm.run().expect("run");
    assert_eq!(v, Value::Number(-5.0));
}

#[test]
fn native_call_roundtrip() {
    fn add_two(args: &[Value]) -> Result<Value, VmError> {
        let a = args.first().and_then(|v| v.as_number()).ok_or(VmError::ExpectedNumber)?;
        let b = args.get(1).and_then(|v| v.as_number()).ok_or(VmError::ExpectedNumber)?;
        Ok(Value::Number(a + b))
    }
    let n: NativeFn = add_two;

    let mut b = BytecodeBuilder::new();
    b.emit_push_const(Value::Number(40.0));
    b.emit_push_const(Value::Number(2.0));
    b.emit_call_native(0, 2);
    b.emit_return();

    let mut vm = Vm::new(b.finish());
    vm.set_natives(vec![n]);
    vm.set_local_count(0).expect("locals");
    let v = vm.run().expect("run");
    assert_eq!(v, Value::Number(42.0));
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
    let r = compile_chunk_v4("class C {}");
    assert!(matches!(r, Err(CompileError::Unsupported(_))));
}

#[test]
fn operation_limit_enforced() {
    let mut b = BytecodeBuilder::new();
    b.emit_opcode(Opcode::Nop);
    b.emit_opcode(Opcode::Nop);
    b.emit_opcode(Opcode::Nop);
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
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    vm.run().expect("run");
    assert!(vm.operations > 0);
    assert_eq!(vm.ram_quads, 0);
}

#[test]
fn while_loop_and_assign_expr() {
    let chunk = compile_chunk_v4(
        "var i = 0; while (i < 3) { i = i + 1; } return i;",
    )
    .expect("compile");
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::Number(3.0));
}

#[test]
fn for_loop_var_init() {
    let chunk = compile_chunk_v4(
        "for (var i = 0; i < 4; i = i + 1) { } return i;",
    )
    .expect("compile");
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::Number(4.0));
}

#[test]
fn do_while_runs_body_once() {
    let chunk = compile_chunk_v4("var n = 0; do { n = n + 1; } while (false); return n;")
        .expect("compile");
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::Number(1.0));
}

#[test]
fn break_exits_while() {
    let chunk = compile_chunk_v4(
        "var i = 0; while (i < 100) { if (i == 5) { break; } i = i + 1; } return i;",
    )
    .expect("compile");
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    assert_eq!(vm.run().expect("run"), Value::Number(5.0));
}

#[test]
fn continue_skips_rest_of_while_body() {
    let chunk = compile_chunk_v4(
        "var i = 0; var s = 0; while (i < 4) { i = i + 1; if (i == 2) { continue; } s = s + i; } return s;",
    )
    .expect("compile");
    let mut vm = Vm::new(chunk.bytecode);
    vm.set_local_count(chunk.local_slots).expect("locals");
    // 1 + 3 + 4 = 8 (skip adding when i is 2)
    assert_eq!(vm.run().expect("run"), Value::Number(8.0));
}


