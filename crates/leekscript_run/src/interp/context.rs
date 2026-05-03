//! Per-run interpreter state: environment, class table, `this` stack.

use super::env::Env;
use super::host::InterpreterHost;
use super::java_log;
use super::value::Value;
use leekscript_hir::{HirClassMember, HirExpr, HirFieldVisibility, HirStmt};
use leekscript_span::Span;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Paths + sources for `debug*()` log lines (Java `FarmerLog.addLog` file id + leek line).
#[derive(Clone, Debug)]
pub struct DebugSourceContext {
    pub generator_root: PathBuf,
    pub texts: HashMap<PathBuf, Arc<str>>,
}

impl DebugSourceContext {
    fn path_key(&self, path: &Path) -> String {
        let path = path.strip_prefix(&self.generator_root).unwrap_or(path);
        path.to_string_lossy().replace('\\', "/")
    }

    #[must_use]
    pub fn file_id(&self, path: &Path) -> i32 {
        java_log::java_path_file_id(&self.path_key(path))
    }

    #[must_use]
    pub fn line_1_based(&self, path: &Path, span: Span) -> Option<i32> {
        let t = self.texts.get(path)?;
        Some(leekscript_span::line_col_at(t, span.start as usize).0 as i32)
    }
}

/// Methods and declared instance fields for one user `class`.
#[derive(Debug, Clone)]
pub(super) struct ClassDef {
    /// Instance methods: Java overloads share a name; arity picks the callee at call sites.
    pub(super) methods: HashMap<String, Vec<Value>>,
    pub(super) static_methods: HashMap<String, Vec<Value>>,
    pub(super) static_method_visibility: HashMap<String, HirFieldVisibility>,
    /// Static field storage (`Class['x']`, `Class.x`); initialized at `class` declaration.
    pub(super) static_fields: HashMap<String, Value>,
    pub(super) static_field_order: Vec<String>,
    pub(super) static_field_inits: HashMap<String, HirExpr>,
    pub(super) static_field_decl_tys: HashMap<String, String>,
    pub(super) static_field_decl_class: HashMap<String, String>,
    pub(super) static_field_visibility: HashMap<String, HirFieldVisibility>,
    pub(super) static_field_final: HashMap<String, bool>,
    pub(super) instance_fields: Vec<String>,
    /// Declared instance field types (`real?`, …) for assign/export coercion.
    pub(super) field_decl_tys: HashMap<String, String>,
    /// Instance field initializers (`a = expr` at class scope), evaluated in `new` before the constructor body.
    pub(super) field_inits: HashMap<String, HirExpr>,
    pub(super) extends: Option<String>,
    /// Instance fields declared on **this** class only (before [`merge_extends_instance_layout_flat`]).
    pub(super) own_instance_fields: Vec<String>,
    /// Instance field → class that introduced it (for visibility after merge).
    pub(super) field_decl_class: HashMap<String, String>,
    pub(super) field_visibility: HashMap<String, HirFieldVisibility>,
    /// Instance field is `final` (strict mode: assignments fail).
    pub(super) field_final: HashMap<String, bool>,
    /// Instance / `constructor` method visibility (declaring class is this [`ClassDef`]'s name).
    pub(super) method_visibility: HashMap<String, HirFieldVisibility>,
}

pub(super) struct InterpCx {
    pub(super) env: Env,
    /// Class name → methods + field names (`ClassDecl` at file scope).
    pub(super) classes: HashMap<String, ClassDef>,
    /// `this` for the innermost instance call / constructor.
    pub(super) this_stack: Vec<Value>,
    /// Matches compile-time lexer / HIR lowering version (`// leek-version`, manifest, CLI).
    pub(super) language_version: u8,
    /// `strict` mode from compile options (affects compound assignment on integer slots in v2+).
    pub(super) strict: Option<bool>,
    /// Optional embedding hooks (`getLife`, …).
    pub(super) host: Option<Box<dyn InterpreterHost>>,
    /// Last evaluated top-level expression statement (Java AI snippets often omit `return`).
    pub(super) script_result_expr: Option<Value>,
    /// Value produced by the most recent assignment expression (`a[i++] = 1`, …).
    pub(super) last_assign_value: Option<Value>,
    /// `decl_ty` from typed locals (`any`, `integer`, …); `None` means plain `var` / untyped.
    pub(super) binding_decl_ty: HashMap<String, Option<String>>,
    /// Nesting depth of user calls invoked from builtins (v1 `arrayMap` / … return semantics).
    pub(super) v1_array_cb_depth: u32,
    /// Enclosing user class name for `class['x']` / `class.x` inside methods and constructors.
    pub(super) enclosing_class_stack: Vec<String>,
    /// Lexical constructor body only: `true` while executing `constructor`/`ClassName` init stmts;
    /// nested `invoke_value` frames push `false` so `final` fields cannot be assigned from callees.
    pub(super) final_field_assign_stack: Vec<bool>,
    /// Declared return types for nested user call frames.
    pub(super) fn_return_ty_stack: Vec<Option<String>>,

    /// Java VM "operations" counter (runtime quota + `getOperations()` builtin).
    pub(super) operations_used: u64,
    pub(super) operations_limit: Option<u64>,
    /// Operations counted at the start of the current Leek Wars turn (`getInstructionCount()`).
    pub(super) turn_operations_start: u64,

    /// Approximate Java VM RAM usage in "quads" (quota only; not byte-accurate in tree interpreter).
    pub(super) ram_quads_used: u64,
    pub(super) ram_quads_limit: Option<u64>,

    /// When set, `debug*()` can append Java-style `[fileId, line]` to farmer logs.
    pub(super) debug_sources: Option<DebugSourceContext>,
    /// Source file for the current top-level stmt (or inherited in nested blocks / calls).
    pub(super) debug_active_file: Option<PathBuf>,
    pub(super) pending_call_span: Option<Span>,
}

impl InterpCx {
    pub(super) fn new(
        classes: HashMap<String, ClassDef>,
        language_version: u8,
        host: Option<Box<dyn InterpreterHost>>,
        strict: Option<bool>,
        operations_limit: Option<u64>,
        ram_quads_limit: Option<u64>,
    ) -> Self {
        Self {
            env: Env::new(),
            classes,
            this_stack: Vec::new(),
            language_version,
            strict,
            host,
            script_result_expr: None,
            last_assign_value: None,
            binding_decl_ty: HashMap::new(),
            v1_array_cb_depth: 0,
            enclosing_class_stack: Vec::new(),
            final_field_assign_stack: Vec::new(),
            fn_return_ty_stack: Vec::new(),
            operations_used: 0,
            operations_limit,
            turn_operations_start: 0,
            ram_quads_used: 0,
            ram_quads_limit,
            debug_sources: None,
            debug_active_file: None,
            pending_call_span: None,
        }
    }

    pub(super) fn debug_log_position(&self, span: Span) -> Option<(i32, i32)> {
        let ctx = self.debug_sources.as_ref()?;
        let file = self.debug_active_file.as_ref()?;
        let line = ctx.line_1_based(file, span)?;
        Some((ctx.file_id(file), line))
    }

    /// Java `AI.getErrorLocalisation` string used in `FarmerLog` system rows (`EntityAI.addSystemLog`).
    pub(super) fn java_style_system_log_trace(&self) -> Option<String> {
        let sp = self.pending_call_span?;
        let ctx = self.debug_sources.as_ref()?;
        let file = self.debug_active_file.as_ref()?;
        let line = ctx.line_1_based(file, sp)?;
        let rel = ctx.path_key(file);
        let base = Path::new(&rel)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?");
        Some(format!("\t\u{25B6} AI {base}, line {line}\n"))
    }

    /// Java-style operation quota. Hot path when [`Self::operations_limit`] is `None` is add-only.
    #[inline]
    pub(super) fn charge_ops(&mut self, n: u64) -> Result<(), super::error::InterpretError> {
        self.operations_used = self.operations_used.saturating_add(n);
        let Some(limit) = self.operations_limit else {
            return Ok(());
        };
        if self.operations_used > limit {
            return Err(super::error::InterpretError::too_much_operations());
        }
        Ok(())
    }

    /// Java-style RAM quota (quads). Hot path when [`Self::ram_quads_limit`] is `None` is add-only.
    #[inline]
    pub(super) fn charge_ram_quads(&mut self, n: u64) -> Result<(), super::error::InterpretError> {
        self.ram_quads_used = self.ram_quads_used.saturating_add(n);
        let Some(limit) = self.ram_quads_limit else {
            return Ok(());
        };
        if self.ram_quads_used > limit {
            return Err(super::error::InterpretError::out_of_memory());
        }
        Ok(())
    }

    /// Mirror Java `decreaseRAM` / `RamUsage::free` subtracting from the global quad counter.
    pub(super) fn release_ram_quads(&mut self, n: u64) {
        self.ram_quads_used = self.ram_quads_used.saturating_sub(n);
    }

    pub(super) fn assign_with_ram(
        &mut self,
        name: &str,
        val: Value,
    ) -> Result<(), super::error::InterpretError> {
        super::ram::charge_top_level_container_ram(self, &val)?;
        let prev = self.env.assign(name, val)?;
        if let Some(o) = prev {
            super::ram::release_owned_binding_value_ram(self, o);
        }
        Ok(())
    }

    pub(super) fn insert_local_var(
        &mut self,
        name: String,
        val: Value,
    ) -> Result<(), super::error::InterpretError> {
        super::ram::charge_top_level_container_ram(self, &val)?;
        self.env.insert(name, val);
        Ok(())
    }

    pub(super) fn insert_global_var(
        &mut self,
        name: String,
        val: Value,
    ) -> Result<(), super::error::InterpretError> {
        super::ram::charge_top_level_container_ram(self, &val)?;
        self.env.insert_global(name, val);
        Ok(())
    }
}

pub(super) fn collect_classes(stmts: &[HirStmt]) -> HashMap<String, ClassDef> {
    let mut out: HashMap<String, ClassDef> = HashMap::new();
    for s in stmts {
        let HirStmt::ClassDecl {
            name,
            extends,
            members,
        } = s
        else {
            continue;
        };
        let mut methods: HashMap<String, Vec<Value>> = HashMap::new();
        let mut static_methods: HashMap<String, Vec<Value>> = HashMap::new();
        let mut static_method_visibility = HashMap::new();
        let mut static_fields: HashMap<String, Value> = HashMap::new();
        let mut static_field_order = Vec::new();
        let mut static_field_inits: HashMap<String, HirExpr> = HashMap::new();
        let mut static_field_decl_tys: HashMap<String, String> = HashMap::new();
        let mut static_field_decl_class = HashMap::new();
        let mut static_field_visibility = HashMap::new();
        let mut static_field_final = HashMap::new();
        let mut instance_fields = Vec::new();
        let mut field_decl_tys = HashMap::new();
        let mut field_inits = HashMap::new();
        let mut field_decl_class = HashMap::new();
        let mut field_visibility = HashMap::new();
        let mut field_final = HashMap::new();
        let mut method_visibility = HashMap::new();
        for m in members {
            match m {
                HirClassMember::Field {
                    name: fnm,
                    decl_ty,
                    init,
                    is_static,
                    is_final,
                    visibility,
                } => {
                    if *is_static {
                        static_field_order.push(fnm.name.clone());
                        static_fields.entry(fnm.name.clone()).or_insert(Value::Null);
                        static_field_decl_class.insert(fnm.name.clone(), name.name.clone());
                        static_field_visibility.insert(fnm.name.clone(), *visibility);
                        static_field_final.insert(fnm.name.clone(), *is_final);
                        if let Some(dt) = decl_ty {
                            static_field_decl_tys.insert(fnm.name.clone(), dt.clone());
                        }
                        if let Some(ref e) = init {
                            static_field_inits.insert(fnm.name.clone(), e.clone());
                        }
                    } else {
                        instance_fields.push(fnm.name.clone());
                        field_decl_class.insert(fnm.name.clone(), name.name.clone());
                        field_visibility.insert(fnm.name.clone(), *visibility);
                        field_final.insert(fnm.name.clone(), *is_final);
                        if let Some(dt) = decl_ty {
                            field_decl_tys.insert(fnm.name.clone(), dt.clone());
                        }
                        if let Some(ref e) = init {
                            field_inits.insert(fnm.name.clone(), e.clone());
                        }
                    }
                }
                HirClassMember::Method {
                    name: mn,
                    is_static,
                    visibility,
                    params,
                    body,
                } => {
                    let v = Value::Function(std::rc::Rc::new(super::value::FunctionValue {
                        params: params.iter().map(|p| p.name.name.clone()).collect(),
                        param_by_ref: params.iter().map(|p| p.by_ref).collect(),
                        param_decl_tys: params.iter().map(|p| p.decl_ty.clone()).collect(),
                        param_defaults: params.iter().map(|p| p.default.clone()).collect(),
                        body: body.clone(),
                        captured_locals: None,
                        captured_aliases: None,
                        declared_return_ty: None,
                        unbound_method_ref: false,
                    }));
                    if *is_static {
                        static_method_visibility.insert(mn.name.clone(), *visibility);
                        static_methods.entry(mn.name.clone()).or_default().push(v);
                    } else {
                        method_visibility.insert(mn.name.clone(), *visibility);
                        methods.entry(mn.name.clone()).or_default().push(v);
                    }
                }
                HirClassMember::Constructor {
                    params,
                    body,
                    visibility,
                } => {
                    method_visibility.insert("constructor".into(), *visibility);
                    methods
                        .entry("constructor".into())
                        .or_default()
                        .push(Value::Function(std::rc::Rc::new(
                            super::value::FunctionValue {
                                params: params.iter().map(|p| p.name.name.clone()).collect(),
                                param_by_ref: params.iter().map(|p| p.by_ref).collect(),
                                param_decl_tys: params.iter().map(|p| p.decl_ty.clone()).collect(),
                                param_defaults: params.iter().map(|p| p.default.clone()).collect(),
                                body: body.clone(),
                                captured_locals: None,
                                captured_aliases: None,
                                declared_return_ty: None,
                                unbound_method_ref: false,
                            },
                        )));
                }
            }
        }
        let own_instance_fields = instance_fields.clone();
        out.insert(
            name.name.clone(),
            ClassDef {
                methods,
                static_methods,
                static_method_visibility,
                static_fields,
                static_field_order,
                static_field_inits,
                static_field_decl_tys,
                static_field_decl_class,
                static_field_visibility,
                static_field_final,
                instance_fields,
                field_decl_tys,
                field_inits,
                extends: extends.as_ref().map(|e| e.name.clone()),
                own_instance_fields,
                field_decl_class,
                field_visibility,
                field_final,
                method_visibility,
            },
        );
    }
    let names: Vec<String> = out.keys().cloned().collect();
    merge_extends_instance_layout_flat(&mut out, &names);
    out
}

fn inheritance_depth(out: &HashMap<String, ClassDef>, name: &str) -> usize {
    let mut d = 0usize;
    let mut seen = HashSet::new();
    let mut cn: &str = name;
    loop {
        if !seen.insert(cn.to_string()) {
            break;
        }
        let Some(cdef) = out.get(cn) else {
            break;
        };
        let Some(p) = cdef.extends.as_deref() else {
            break;
        };
        d += 1;
        cn = p;
    }
    d
}

fn classes_sorted_by_inheritance_depth(
    out: &HashMap<String, ClassDef>,
    names: &[String],
) -> Vec<String> {
    let mut ordered: Vec<String> = names.to_vec();
    ordered.sort_by_key(|n| inheritance_depth(out, n));
    ordered
}

fn merge_extends_instance_layout_flat(out: &mut HashMap<String, ClassDef>, names: &[String]) {
    for name in classes_sorted_by_inheritance_depth(&*out, names) {
        let Some(parent_name) = out.get(&name).and_then(|c| c.extends.clone()) else {
            continue;
        };
        let (merged_fields, inits, decl_tys, fdc, fv, ff) = {
            let Some(parent_ref) = out.get(&parent_name) else {
                continue;
            };
            let Some(child_ref) = out.get(&name) else {
                continue;
            };
            let mut merged_fields: Vec<String> = Vec::new();
            for f in &parent_ref.instance_fields {
                if !child_ref.instance_fields.iter().any(|c| c == f) {
                    merged_fields.push(f.clone());
                }
            }
            for f in &child_ref.instance_fields {
                merged_fields.push(f.clone());
            }
            let mut inits = parent_ref.field_inits.clone();
            for (k, v) in &child_ref.field_inits {
                inits.insert(k.clone(), v.clone());
            }
            let mut decl_tys = parent_ref.field_decl_tys.clone();
            for (k, v) in &child_ref.field_decl_tys {
                decl_tys.insert(k.clone(), v.clone());
            }
            let mut fdc = parent_ref.field_decl_class.clone();
            let mut fv = parent_ref.field_visibility.clone();
            let mut ff = parent_ref.field_final.clone();
            for f in &child_ref.own_instance_fields {
                if let Some(d) = child_ref.field_decl_class.get(f) {
                    fdc.insert(f.clone(), d.clone());
                }
                if let Some(v) = child_ref.field_visibility.get(f) {
                    fv.insert(f.clone(), *v);
                }
                if let Some(fin) = child_ref.field_final.get(f) {
                    ff.insert(f.clone(), *fin);
                }
            }
            (merged_fields, inits, decl_tys, fdc, fv, ff)
        };
        let Some(child) = out.get_mut(&name) else {
            continue;
        };
        child.instance_fields = merged_fields;
        child.field_inits = inits;
        child.field_decl_tys = decl_tys;
        child.field_decl_class = fdc;
        child.field_visibility = fv;
        child.field_final = ff;
    }
}

/// Whether `sub` is `ancestor` or a transitive subclass (walks [`ClassDef::extends`]).
pub(super) fn is_subclass_of(cx: &InterpCx, sub: &str, ancestor: &str) -> bool {
    let mut cur: Option<String> = Some(sub.to_string());
    let mut seen = HashSet::new();
    for _ in 0..64 {
        let Some(cn) = cur else {
            break;
        };
        if cn == ancestor {
            return true;
        }
        if !seen.insert(cn.clone()) {
            break;
        }
        let Some(d) = cx.classes.get(&cn) else {
            break;
        };
        cur = d.extends.clone();
    }
    false
}
