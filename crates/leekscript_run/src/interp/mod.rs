//! Tree-walking interpreter for the current [`HirFile`](leekscript_hir::HirFile) subset.
//!
//! **Profiling:** `cargo run -p leekscript_bench --release -- …` with `--rust-stats` (operations /
//! RAM quads) and `--compile-once` (interpret-only iterations) helps separate frontend from walker
//! time. Workspace `[profile.release]` uses thin LTO for more representative release numbers.
//!
//! **Larger wins** (not implemented here) remain: bytecode/VM backend, less cloning in [`env::Env`],
//! interned or shared strings in [`value::Value`], and resolved binding slots in HIR.

mod call;
mod context;
mod core_builtins;
mod env;
mod error;
mod exec;
mod expr;
mod flow;
pub mod host;
mod instance;
mod java_export;
mod java_log;
mod java_ops_budget;
pub(crate) mod leek_registry_ops {
    include!(concat!(env!("OUT_DIR"), "/leek_registry_ops.rs"));
}
mod lvalue;
mod map_store;
mod native;
mod ops;
mod ram;
mod util;
mod value;

pub use context::DebugSourceContext;
pub use error::{ExecAbort, InterpretError};
pub use host::{DebugLogHandled, DebugLogKind, InterpreterHost};
pub use java_export::value_java_export;
pub use leekscript_resolve::STDLIB_GLOBAL_IDENTIFIERS;
pub use value::{InstanceData, Value};

use call::invoke_global_by_name;
use context::InterpCx;
use exec::{exec_stmts, hoist_top_level_function_decls};
use flow::StmtFlow;
use leekscript_hir::{HirFile, HirStmt};
use std::sync::OnceLock;

static LEEKWARS_SIG_INTEGER_GLOBALS: OnceLock<Vec<(String, i64)>> = OnceLock::new();

/// `core.sig.leek` + `leekwars.sig.leek` integer constants (`EFFECT_*`, `CHIP_*`, …).
///
/// Must be installed before executing HIR that defines classes or runs initializers which read
/// those globals (same values as compile-time [`crate::sig_workspace::merged_workspace_sig_bundle`]).
fn seed_leekwars_workspace_integer_globals(env: &mut env::Env) {
    let ints = LEEKWARS_SIG_INTEGER_GLOBALS.get_or_init(|| {
        crate::sig_workspace::merged_workspace_sig_bundle()
            .integer_globals
            .clone()
    });
    for (name, v) in ints.iter() {
        env.insert_global(name.clone(), Value::Integer(*v));
    }
}

/// Persistent interpreter after running a file’s top-level statements.
///
/// For Leek Wars AIs, Java runs declarations once then the main file body every turn; use
/// [`Self::from_hir_leek_wars_ai_with_extra_natives`] and [`Self::run_leek_wars_turn_stmts`].
pub struct InterpretSession {
    cx: InterpCx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InterpretStats {
    pub operations_used: u64,
    pub ram_quads_used: u64,
}

impl InterpretSession {
    /// Lexical prelude: run `hir` top-level stmts with stdlib + optional host, keeping the environment for later calls.
    pub fn from_hir_init(
        hir: &HirFile,
        language_version: u8,
        host: Option<Box<dyn InterpreterHost>>,
    ) -> Result<Self, InterpretError> {
        let classes = context::collect_classes(&hir.stmts);
        let mut cx = InterpCx::new(classes, language_version, host, None, None, None);
        native::seed_stdlib(&mut cx.env, language_version);
        seed_leekwars_workspace_integer_globals(&mut cx.env);
        // Java semantics: `global x;` behaves like a hoisted pre-declaration for reads earlier in the file.
        for st in &hir.stmts {
            if let leekscript_hir::HirStmt::Global { entries, .. } = st {
                for (n, _) in entries {
                    if !cx.env.contains_global(&n.name) {
                        cx.env.insert_global(n.name.clone(), Value::Null);
                    }
                }
            }
        }
        hoist_top_level_function_decls(&mut cx, &hir.stmts);
        match exec_stmts(&mut cx, &hir.stmts, false)? {
            StmtFlow::Continue => Ok(Self { cx }),
            StmtFlow::Return(_) => Err(InterpretError {
                reference: "INTERNAL_ERROR",
                message: "top-level `return` during init is not supported (use `function turn()`)"
                    .into(),
            }),
            StmtFlow::Break => Err(InterpretError::break_out_of_loop()),
            StmtFlow::ContinueLoop => Err(InterpretError::continue_out_of_loop()),
            StmtFlow::Throw(_) => Err(InterpretError::uncaught_throw()),
        }
    }

    /// Like [`Self::from_hir_init`], but pre-registers extra [`Value::Native`] globals (Leek Wars fight API, etc.).
    pub fn from_hir_init_with_extra_natives(
        hir: &HirFile,
        language_version: u8,
        host: Option<Box<dyn InterpreterHost>>,
        extra_natives: &[&'static str],
    ) -> Result<Self, InterpretError> {
        let classes = context::collect_classes(&hir.stmts);
        let mut cx = InterpCx::new(classes, language_version, host, None, None, None);
        native::seed_stdlib(&mut cx.env, language_version);
        seed_leekwars_workspace_integer_globals(&mut cx.env);
        for &n in extra_natives {
            if !cx.env.contains_global(n) {
                cx.env.insert_global(n.to_string(), Value::Native(n));
            }
        }
        for st in &hir.stmts {
            if let leekscript_hir::HirStmt::Global { entries, .. } = st {
                for (n, _) in entries {
                    if !cx.env.contains_global(&n.name) {
                        cx.env.insert_global(n.name.clone(), Value::Null);
                    }
                }
            }
        }
        hoist_top_level_function_decls(&mut cx, &hir.stmts);
        match exec_stmts(&mut cx, &hir.stmts, false)? {
            StmtFlow::Continue => Ok(Self { cx }),
            StmtFlow::Return(_) => Err(InterpretError {
                reference: "INTERNAL_ERROR",
                message: "top-level `return` during init is not supported (use `function turn()`)"
                    .into(),
            }),
            StmtFlow::Break => Err(InterpretError::break_out_of_loop()),
            StmtFlow::ContinueLoop => Err(InterpretError::continue_out_of_loop()),
            StmtFlow::Throw(_) => Err(InterpretError::uncaught_throw()),
        }
    }

    /// Split a Leek Wars AI file for Java-style execution: declarations run once (like `staticInit`),
    /// everything else runs each turn (the body of compiled `runIA`).
    pub fn split_leek_wars_ai_stmts(
        hir: &HirFile,
    ) -> (
        Vec<HirStmt>,
        Vec<std::path::PathBuf>,
        Vec<HirStmt>,
        Vec<std::path::PathBuf>,
    ) {
        let mut init_s = Vec::new();
        let mut init_p = Vec::new();
        let mut turn_s = Vec::new();
        let mut turn_p = Vec::new();
        for (i, s) in hir.stmts.iter().enumerate() {
            let p = hir.stmt_sources.get(i).cloned().unwrap_or_default();
            match s {
                HirStmt::FnDecl { .. } | HirStmt::ClassDecl { .. } | HirStmt::Global { .. } => {
                    init_s.push(s.clone());
                    init_p.push(p);
                }
                _ => {
                    turn_s.push(s.clone());
                    turn_p.push(p);
                }
            }
        }
        (init_s, init_p, turn_s, turn_p)
    }

    /// Initialize a session like [`Self::from_hir_init_with_extra_natives`], but only executes
    /// declaration statements (`function` / `class` / `global`). The returned `turn_stmts` must be
    /// passed to [`Self::run_leek_wars_turn_stmts`] once per fight turn per entity.
    pub fn from_hir_leek_wars_ai_with_extra_natives(
        hir: &HirFile,
        language_version: u8,
        host: Option<Box<dyn InterpreterHost>>,
        extra_natives: &[&'static str],
        operations_limit: Option<u64>,
        ram_quads_limit: Option<u64>,
        debug_sources: Option<context::DebugSourceContext>,
        default_ai_path: std::path::PathBuf,
    ) -> Result<(Self, Vec<HirStmt>, Vec<std::path::PathBuf>), InterpretError> {
        let (init_stmts, mut init_files, turn_stmts, mut turn_files) =
            Self::split_leek_wars_ai_stmts(hir);
        let default_ai_path = std::fs::canonicalize(&default_ai_path).unwrap_or(default_ai_path);
        let fix_paths = |paths: &mut Vec<std::path::PathBuf>| {
            for p in paths.iter_mut() {
                if p.as_os_str().is_empty() {
                    *p = default_ai_path.clone();
                } else if let Ok(c) = std::fs::canonicalize(&*p) {
                    *p = c;
                }
            }
        };
        fix_paths(&mut init_files);
        fix_paths(&mut turn_files);

        let classes = context::collect_classes(&hir.stmts);
        let mut cx = InterpCx::new(
            classes,
            language_version,
            host,
            None,
            operations_limit,
            ram_quads_limit,
        );
        cx.debug_sources = debug_sources;
        native::seed_stdlib(&mut cx.env, language_version);
        seed_leekwars_workspace_integer_globals(&mut cx.env);
        for &n in extra_natives {
            if !cx.env.contains_global(n) {
                cx.env.insert_global(n.to_string(), Value::Native(n));
            }
        }
        for st in &hir.stmts {
            if let leekscript_hir::HirStmt::Global { entries, .. } = st {
                for (n, _) in entries {
                    if !cx.env.contains_global(&n.name) {
                        cx.env.insert_global(n.name.clone(), Value::Null);
                    }
                }
            }
        }
        hoist_top_level_function_decls(&mut cx, &hir.stmts);
        match exec::exec_stmts_with_debug_files(&mut cx, &init_stmts, &init_files, false)? {
            StmtFlow::Continue => Ok((Self { cx }, turn_stmts, turn_files)),
            StmtFlow::Return(_) => Err(InterpretError {
                reference: "INTERNAL_ERROR",
                message: "top-level `return` during Leek Wars AI init is not supported".into(),
            }),
            StmtFlow::Break => Err(InterpretError::break_out_of_loop()),
            StmtFlow::ContinueLoop => Err(InterpretError::continue_out_of_loop()),
            StmtFlow::Throw(_) => Err(InterpretError::uncaught_throw()),
        }
    }

    /// Run one fight turn: the per-turn fragment of the AI (top-level statements other than
    /// `function` / `class` / `global`).
    pub fn run_leek_wars_turn_stmts(
        &mut self,
        stmts: &[HirStmt],
        stmt_files: &[std::path::PathBuf],
    ) -> Result<(), InterpretError> {
        if stmts.is_empty() {
            return Ok(());
        }
        self.cx.turn_operations_start = self.cx.operations_used;
        match exec::exec_stmts_with_debug_files(&mut self.cx, stmts, stmt_files, false)? {
            StmtFlow::Continue => Ok(()),
            StmtFlow::Return(_) => Err(InterpretError {
                reference: "INTERNAL_ERROR",
                message: "top-level `return` during Leek Wars AI turn is not supported".into(),
            }),
            StmtFlow::Break => Err(InterpretError::break_out_of_loop()),
            StmtFlow::ContinueLoop => Err(InterpretError::continue_out_of_loop()),
            StmtFlow::Throw(_) => Err(InterpretError::uncaught_throw()),
        }
    }

    /// Call a global function or native by name (typically `turn` with no args).
    pub fn call_global(&mut self, name: &str, args: &[Value]) -> Result<Value, InterpretError> {
        match invoke_global_by_name(&mut self.cx, name, args.to_vec()) {
            Ok(v) => Ok(v),
            Err(ExecAbort::Error(e)) => Err(e),
            Err(ExecAbort::Throw(_)) => Err(InterpretError::uncaught_throw()),
        }
    }

    /// Cumulative VM operations (Java `EntityAI.operations()` after each turn / for outcome `fight.ops`).
    pub fn operations_used(&self) -> u64 {
        self.cx.operations_used
    }

    /// Insert or overwrite global `integer` bindings (Leek Wars weapon/chip/effect constants, etc.).
    pub fn seed_global_integers(&mut self, pairs: &[(String, i64)]) {
        use value::Value;
        for (name, v) in pairs {
            self.cx.env.insert_global(name.clone(), Value::Integer(*v));
        }
    }
}

/// Run a whole file. Returns `Ok(Some(v))` when a `return` produced `v`, or when the last
/// top-level statement was an expression statement and its value is taken as the script result
/// (Java-style snippets without `return`). Returns `Ok(None)` when execution finished without
/// either of those.
///
/// `language_version` must match the version used when lowering this HIR (same as compile pipeline / `// leek-version`).
pub fn interpret_hir(hir: &HirFile, language_version: u8) -> Result<Option<Value>, InterpretError> {
    interpret_hir_full(hir, language_version, None, None)
}

/// Like [`interpret_hir`], but supply an [`InterpreterHost`] for extension natives (`getLife`, …).
pub fn interpret_hir_with_host(
    hir: &HirFile,
    language_version: u8,
    host: Option<Box<dyn InterpreterHost>>,
) -> Result<Option<Value>, InterpretError> {
    interpret_hir_full(hir, language_version, host, None)
}

/// Run with compile-time `strict` flag (parity with Java `strict` mode for compound assignment, etc.).
pub fn interpret_hir_with_strict(
    hir: &HirFile,
    language_version: u8,
    strict: Option<bool>,
) -> Result<Option<Value>, InterpretError> {
    interpret_hir_full(hir, language_version, None, strict)
}

pub fn interpret_hir_with_limits_and_stats(
    hir: &HirFile,
    language_version: u8,
    strict: Option<bool>,
    max_ops_limit: Option<u64>,
    max_ram_quads_limit: Option<u64>,
) -> Result<(Option<Value>, InterpretStats), InterpretError> {
    interpret_hir_full_with_limits(
        hir,
        language_version,
        None,
        strict,
        max_ops_limit,
        max_ram_quads_limit,
    )
}

fn interpret_hir_full(
    hir: &HirFile,
    language_version: u8,
    host: Option<Box<dyn InterpreterHost>>,
    strict: Option<bool>,
) -> Result<Option<Value>, InterpretError> {
    interpret_hir_full_with_limits(hir, language_version, host, strict, None, None).map(|(v, _)| v)
}

fn interpret_hir_full_with_limits(
    hir: &HirFile,
    language_version: u8,
    host: Option<Box<dyn InterpreterHost>>,
    strict: Option<bool>,
    max_ops_limit: Option<u64>,
    max_ram_quads_limit: Option<u64>,
) -> Result<(Option<Value>, InterpretStats), InterpretError> {
    let classes = context::collect_classes(&hir.stmts);
    let mut cx = InterpCx::new(
        classes,
        language_version,
        host,
        strict,
        max_ops_limit,
        max_ram_quads_limit,
    );
    native::seed_stdlib(&mut cx.env, language_version);
    seed_leekwars_workspace_integer_globals(&mut cx.env);
    for st in &hir.stmts {
        if let leekscript_hir::HirStmt::Global { entries, .. } = st {
            for (n, _) in entries {
                if !cx.env.contains_global(&n.name) {
                    cx.env.insert_global(n.name.clone(), Value::Null);
                }
            }
        }
    }
    hoist_top_level_function_decls(&mut cx, &hir.stmts);
    let flow = exec_stmts(&mut cx, &hir.stmts, true)?;
    let stats = InterpretStats {
        operations_used: cx.operations_used,
        ram_quads_used: cx.ram_quads_used,
    };
    match flow {
        StmtFlow::Continue => Ok((cx.script_result_expr.take(), stats)),
        StmtFlow::Return(v) => Ok((v, stats)),
        StmtFlow::Break => Err(InterpretError::break_out_of_loop()),
        StmtFlow::ContinueLoop => Err(InterpretError::continue_out_of_loop()),
        StmtFlow::Throw(_) => Err(InterpretError::uncaught_throw()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use leekscript_hir::{
        HirAssignOp, HirBinOp, HirExpr, HirFile, HirForStep, HirForUpdate, HirStmt, HirUnaryOp,
        NameDef,
    };
    use leekscript_span::Span;
    use std::boxed::Box;

    fn nid(s: &str) -> HirExpr {
        HirExpr::Ident {
            name: s.into(),
            span: Span::point(0),
        }
    }

    fn ndef(s: &str) -> NameDef {
        NameDef {
            name: s.into(),
            span: Span::point(0),
        }
    }

    #[test]
    fn leek_wars_ai_split_puts_declarations_in_init_only() {
        let stmts = vec![
            HirStmt::FnDecl {
                name: ndef("f"),
                params: vec![],
                return_ty: None,
                body: vec![],
            },
            HirStmt::Var {
                name: ndef("x"),
                init: Some(HirExpr::Integer(1)),
                decl_ty: None,
            },
        ];
        let hir = HirFile {
            stmt_sources: vec![],
            stmts,
        };
        let (init, _ip, turn, _tp) = InterpretSession::split_leek_wars_ai_stmts(&hir);
        assert_eq!(init.len(), 1);
        assert_eq!(turn.len(), 1);
    }

    #[test]
    fn ram_quota_trips_on_push_loop() {
        // Rough parity check: `push(a, i)` should charge RAM and trip the quota.
        // This is intentionally a very small quota so the test runs fast.
        let span = Span::point(0);
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::Var {
                    name: ndef("a"),
                    init: Some(HirExpr::ArrayLiteral {
                        elements: vec![],
                        span,
                    }),
                    decl_ty: None,
                },
                HirStmt::For {
                    init: Some(Box::new(HirStmt::Var {
                        name: ndef("i"),
                        init: Some(HirExpr::Integer(0)),
                        decl_ty: None,
                    })),
                    cond: Some(HirExpr::Binary {
                        op: HirBinOp::Lt,
                        left: Box::new(nid("i")),
                        right: Box::new(HirExpr::Integer(10_000)),
                    }),
                    update: Some(HirForStep::Assign(HirForUpdate {
                        name: ndef("i"),
                        op: HirAssignOp::AddAssign,
                        value: HirExpr::Integer(1),
                    })),
                    body: vec![HirStmt::Expr(HirExpr::Call {
                        callee: Box::new(nid("push")),
                        args: vec![nid("a"), nid("i")],
                        span,
                    })],
                },
                HirStmt::ret(Some(HirExpr::Call {
                    callee: Box::new(nid("count")),
                    args: vec![nid("a")],
                    span,
                })),
            ],
        };
        let r = interpret_hir_with_limits_and_stats(&hir, 4, None, None, Some(3));
        assert!(r.is_err());
        let e = r.err().unwrap();
        assert_eq!(e.reference, "OUT_OF_MEMORY");
    }

    #[test]
    fn ram_quota_trips_on_map_index_inserts() {
        let span = Span::point(0);
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::Var {
                    name: ndef("m"),
                    init: Some(HirExpr::MapLiteral {
                        entries: vec![],
                        span,
                    }),
                    decl_ty: Some("any".into()),
                },
                HirStmt::For {
                    init: Some(Box::new(HirStmt::Var {
                        name: ndef("i"),
                        init: Some(HirExpr::Integer(0)),
                        decl_ty: None,
                    })),
                    cond: Some(HirExpr::Binary {
                        op: HirBinOp::Lt,
                        left: Box::new(nid("i")),
                        right: Box::new(HirExpr::Integer(10_000)),
                    }),
                    update: Some(HirForStep::Assign(HirForUpdate {
                        name: ndef("i"),
                        op: HirAssignOp::AddAssign,
                        value: HirExpr::Integer(1),
                    })),
                    body: vec![HirStmt::Assign {
                        place: Box::new(HirExpr::Index {
                            base: Box::new(nid("m")),
                            index: Box::new(nid("i")),
                            span,
                        }),
                        op: HirAssignOp::Assign,
                        value: nid("i"),
                    }],
                },
                HirStmt::ret(Some(HirExpr::Call {
                    callee: Box::new(nid("mapSize")),
                    args: vec![nid("m")],
                    span,
                })),
            ],
        };
        let r = interpret_hir_with_limits_and_stats(&hir, 4, None, None, Some(3));
        assert!(r.is_err());
        let e = r.err().unwrap();
        assert_eq!(e.reference, "OUT_OF_MEMORY");
    }

    #[test]
    fn var_and_use() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::Var {
                    name: ndef("x"),
                    init: Some(HirExpr::Integer(2)),
                    decl_ty: None,
                },
                HirStmt::Expr(nid("x")),
            ],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(2)));
    }

    #[test]
    fn return_value() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Integer(7)))],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(7)));
    }

    #[test]
    fn undefined_var() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Expr(nid("nope"))],
        };
        let e = interpret_hir(&hir, 4).unwrap_err();
        assert_eq!(e.reference, "VARIABLE_NOT_EXISTS");
    }

    #[test]
    fn division_by_zero_reference() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Binary {
                op: HirBinOp::Div,
                left: Box::new(HirExpr::Integer(1)),
                right: Box::new(HirExpr::Integer(0)),
            }))],
        };
        // Java Leek v2+: `1/0` is `∞`, not a runtime error (v1 yields `null`).
        let v = interpret_hir(&hir, 4).unwrap().unwrap();
        assert!(matches!(v, Value::Real(x) if x == f64::INFINITY));
    }

    #[test]
    fn block_scope_shadow() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::Var {
                    name: ndef("x"),
                    init: Some(HirExpr::Integer(1)),
                    decl_ty: None,
                },
                HirStmt::Block(vec![HirStmt::Var {
                    name: ndef("x"),
                    init: Some(HirExpr::Integer(2)),
                    decl_ty: None,
                }]),
                HirStmt::ret(Some(nid("x"))),
            ],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(1)));
    }

    #[test]
    fn arithmetic() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Binary {
                op: HirBinOp::Add,
                left: Box::new(HirExpr::Integer(1)),
                right: Box::new(HirExpr::Binary {
                    op: HirBinOp::Mul,
                    left: Box::new(HirExpr::Integer(2)),
                    right: Box::new(HirExpr::Integer(3)),
                }),
            }))],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(7)));
    }

    #[test]
    fn if_else_takes_branch() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::If {
                    cond: HirExpr::Bool(false),
                    then_body: vec![HirStmt::ret(Some(HirExpr::Integer(1)))],
                    else_body: Some(vec![HirStmt::ret(Some(HirExpr::Integer(2)))]),
                },
                HirStmt::ret(Some(HirExpr::Integer(9))),
            ],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(2)));
    }

    #[test]
    fn while_falsy_cond_skips_body() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::While {
                    cond: HirExpr::Integer(0),
                    body: vec![HirStmt::ret(Some(HirExpr::Integer(99)))],
                },
                HirStmt::ret(Some(HirExpr::Integer(4))),
            ],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(4)));
    }

    #[test]
    fn return_from_while_body() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::While {
                cond: HirExpr::Bool(true),
                body: vec![HirStmt::ret(Some(HirExpr::Integer(5)))],
            }],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(5)));
    }

    #[test]
    fn assign_updates_binding() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::Var {
                    name: ndef("x"),
                    init: Some(HirExpr::Integer(1)),
                    decl_ty: None,
                },
                HirStmt::Assign {
                    place: Box::new(nid("x")),
                    op: HirAssignOp::Assign,
                    value: HirExpr::Integer(42),
                },
                HirStmt::ret(Some(nid("x"))),
            ],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(42)));
    }

    #[test]
    fn while_with_assign_counts_down() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::Var {
                    name: ndef("n"),
                    init: Some(HirExpr::Integer(3)),
                    decl_ty: None,
                },
                HirStmt::While {
                    cond: nid("n"),
                    body: vec![HirStmt::Assign {
                        place: Box::new(nid("n")),
                        op: HirAssignOp::Assign,
                        value: HirExpr::Binary {
                            op: HirBinOp::Sub,
                            left: Box::new(nid("n")),
                            right: Box::new(HirExpr::Integer(1)),
                        },
                    }],
                },
                HirStmt::ret(Some(nid("n"))),
            ],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(0)));
    }

    #[test]
    fn break_exits_while() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::While {
                    cond: HirExpr::Bool(true),
                    body: vec![HirStmt::Break],
                },
                HirStmt::ret(Some(HirExpr::Integer(8))),
            ],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(8)));
    }

    #[test]
    fn break_at_top_level_errors() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Break],
        };
        let e = interpret_hir(&hir, 4).unwrap_err();
        assert_eq!(e.reference, "BREAK_OUT_OF_LOOP");
    }

    #[test]
    fn continue_at_top_level_errors() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Continue],
        };
        let e = interpret_hir(&hir, 4).unwrap_err();
        assert_eq!(e.reference, "CONTINUE_OUT_OF_LOOP");
    }

    #[test]
    fn continue_in_while_rechecks_cond() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::Var {
                    name: ndef("c"),
                    init: Some(HirExpr::Integer(1)),
                    decl_ty: None,
                },
                HirStmt::While {
                    cond: nid("c"),
                    body: vec![
                        HirStmt::Assign {
                            place: Box::new(nid("c")),
                            op: HirAssignOp::Assign,
                            value: HirExpr::Integer(0),
                        },
                        HirStmt::Continue,
                    ],
                },
                HirStmt::ret(Some(nid("c"))),
            ],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(0)));
    }

    #[test]
    fn equality_and_ordering_numbers() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Binary {
                op: HirBinOp::Lt,
                left: Box::new(HirExpr::Integer(1)),
                right: Box::new(HirExpr::Integer(2)),
            }))],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Bool(true)));
    }

    #[test]
    fn strict_equality_same_as_eq_for_numbers() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Binary {
                op: HirBinOp::StrictEq,
                left: Box::new(HirExpr::Integer(2)),
                right: Box::new(HirExpr::Integer(2)),
            }))],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Bool(true)));
    }

    #[test]
    fn compare_mismatched_types_errors() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Binary {
                op: HirBinOp::Lt,
                left: Box::new(HirExpr::Integer(1)),
                right: Box::new(HirExpr::String("a".into())),
            }))],
        };
        let e = interpret_hir(&hir, 4).unwrap_err();
        assert_eq!(e.reference, "WRONG_ARGUMENT_TYPE");
    }

    #[test]
    fn unary_neg_number() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Unary {
                op: HirUnaryOp::Neg,
                expr: Box::new(HirExpr::Integer(7)),
            }))],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(-7)));
    }

    #[test]
    fn unary_not_uses_truthiness() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Integer(0)),
            }))],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Bool(true)));
    }

    #[test]
    fn logical_and_short_circuits() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Binary {
                op: HirBinOp::LogicalAnd,
                left: Box::new(HirExpr::Bool(false)),
                right: Box::new(nid("undefined_name")),
            }))],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Bool(false)));
    }

    #[test]
    fn logical_or_short_circuits() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Binary {
                op: HirBinOp::LogicalOr,
                left: Box::new(HirExpr::Bool(true)),
                right: Box::new(nid("undefined_name")),
            }))],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Bool(true)));
    }

    #[test]
    fn bitxor_on_numbers() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ret(Some(HirExpr::Binary {
                op: HirBinOp::BitXor,
                left: Box::new(HirExpr::Integer(5)),
                right: Box::new(HirExpr::Integer(3)),
            }))],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(6)));
    }

    #[test]
    fn for_loop_accumulates() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::Var {
                    name: ndef("s"),
                    init: Some(HirExpr::Integer(0)),
                    decl_ty: None,
                },
                HirStmt::For {
                    init: Some(Box::new(HirStmt::Var {
                        name: ndef("i"),
                        init: Some(HirExpr::Integer(0)),
                        decl_ty: None,
                    })),
                    cond: Some(HirExpr::Binary {
                        op: HirBinOp::Lt,
                        left: Box::new(nid("i")),
                        right: Box::new(HirExpr::Integer(3)),
                    }),
                    update: Some(HirForStep::Assign(HirForUpdate {
                        name: ndef("i"),
                        op: HirAssignOp::Assign,
                        value: HirExpr::Binary {
                            op: HirBinOp::Add,
                            left: Box::new(nid("i")),
                            right: Box::new(HirExpr::Integer(1)),
                        },
                    })),
                    body: vec![HirStmt::Assign {
                        place: Box::new(nid("s")),
                        op: HirAssignOp::Assign,
                        value: HirExpr::Binary {
                            op: HirBinOp::Add,
                            left: Box::new(nid("s")),
                            right: Box::new(HirExpr::Integer(1)),
                        },
                    }],
                },
                HirStmt::ret(Some(nid("s"))),
            ],
        };
        assert_eq!(interpret_hir(&hir, 4).unwrap(), Some(Value::Integer(3)));
    }

    #[test]
    fn stdlib_identifier_list_matches_resolve() {
        assert_eq!(
            STDLIB_GLOBAL_IDENTIFIERS,
            leekscript_resolve::STDLIB_GLOBAL_IDENTIFIERS,
        );
    }

    struct TestHost {
        n: i64,
    }

    impl InterpreterHost for TestHost {
        fn call_native(
            &mut self,
            name: &str,
            args: &[Value],
            _system_log_trace: Option<&str>,
        ) -> Result<Option<Value>, InterpretError> {
            if name == "getLife" && args.is_empty() {
                return Ok(Some(Value::Integer(self.n)));
            }
            Ok(None)
        }
    }

    #[test]
    fn interpret_session_calls_turn_with_host() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::FnDecl {
                name: ndef("turn"),
                params: vec![],
                return_ty: None,
                body: vec![HirStmt::ret(Some(HirExpr::Binary {
                    op: HirBinOp::Add,
                    left: Box::new(HirExpr::Call {
                        callee: Box::new(nid("getLife")),
                        args: vec![],
                        span: Span::point(0),
                    }),
                    right: Box::new(HirExpr::Integer(1)),
                }))],
            }],
        };
        let mut sess =
            InterpretSession::from_hir_init(&hir, 4, Some(Box::new(TestHost { n: 41 }))).unwrap();
        let v = sess.call_global("turn", &[]).unwrap();
        assert_eq!(v, Value::Integer(42));
    }

    #[test]
    fn optional_integer_field_assign_in_method_via_compile() {
        use crate::{compile_source, value_java_export, CompileOptions};
        let src = "class A { integer? x = null m() { x = 5.5 } } var a = new A(); a.m(); a.x\n";
        for ver in [2u8, 4u8] {
            let u = compile_source(
                "t.leek",
                src,
                &CompileOptions {
                    cli_language_version: Some(ver),
                    ..Default::default()
                },
            )
            .unwrap();
            let v = interpret_hir_with_strict(&u.hir, u.language_version, u.strict).unwrap();
            let s = value_java_export(&v.unwrap(), u.language_version);
            assert_eq!(s, "5", "ver={ver}");
        }
    }
}
