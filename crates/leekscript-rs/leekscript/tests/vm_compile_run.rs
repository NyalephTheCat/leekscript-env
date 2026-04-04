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


