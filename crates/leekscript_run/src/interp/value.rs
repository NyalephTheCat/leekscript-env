//! Runtime values for the tree-walking interpreter.

use super::map_store::MapStore;
use indexmap::IndexMap;
use leekscript_hir::{HirExpr, HirStmt};
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::rc::Rc;

/// Shared array storage so mutating builtins (`push`, `sort`, …) observe the same container as the caller.
pub type SharedArray = Rc<RefCell<Vec<Value>>>;
/// Insertion-ordered map (`new Map(...)`, `[k : v, ...]`) with hash buckets for key lookup.
pub type SharedMap = Rc<RefCell<MapStore>>;

/// Runtime set storage: `elems` keeps insertion/uniqueness order; `java_hash_export` selects Java-style export ordering.
#[derive(Debug, Clone, PartialEq)]
pub struct SetData {
    pub elems: Vec<Value>,
    /// When true and unchanged since construction, export uses Java `HashSet` iteration order (type-sorted).
    pub java_hash_export: bool,
    /// Set to true after any mutating operation (`setPut`, `setRemove`, ...).
    pub ever_mutated: bool,
}

/// `new Set(...)` / set literals (`Rc` shared, mutating builtins update the same backing).
pub type SharedSet = Rc<RefCell<SetData>>;

/// Bounded interval (`new Interval(...)`, Java `IntervalLeekValue`).
#[derive(Debug, Clone, PartialEq)]
pub struct IntervalValue {
    pub min_closed: bool,
    pub min: f64,
    pub max_closed: bool,
    pub max: f64,
    /// `true` when both endpoints were `integer` literals (`[1..2]`); `false` for `real` (`[1.0 ..2.0]`).
    pub integer_lattice: bool,
    /// Intersection with `]..[` forces v2+ `export` to use `1.0`/`2.0` endpoints while keeping lattice values.
    pub export_endpoints_as_real: bool,
    /// `]..max]` / `]..max[` sugar inserts `Real(-∞)` in HIR; `]-Infinity..max]` does not — Java v2+ `export` differs.
    pub interval_min_neg_inf_from_shorthand: bool,
    /// `[min..[` sugar inserts `Real(+∞)`; `[min..Infinity[` does not.
    pub interval_max_pos_inf_from_shorthand: bool,
}

impl Default for IntervalValue {
    fn default() -> Self {
        Self {
            min_closed: false,
            min: 0.0,
            max_closed: false,
            max: 0.0,
            integer_lattice: true,
            export_endpoints_as_real: false,
            interval_min_neg_inf_from_shorthand: false,
            interval_max_pos_inf_from_shorthand: false,
        }
    }
}

/// One user `class` instance (`new MyClass()`); fields are mutated through [`Rc`]/[`RefCell`].
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceData {
    pub class_name: String,
    /// Superclass from `class A extends B` (used for export / `instanceof` edge cases).
    pub extends: Option<String>,
    /// Element storage when `extends` is `Array` (`push` / `[]` export).
    pub array_backing: Option<SharedArray>,
    /// Cached `string()` override result (Java/Leek `string()` method).
    pub string_override: Option<String>,
    /// Field order matches class declaration (Java `LinkedHashMap` iteration / export order).
    pub fields: IndexMap<String, Value>,
}

/// User-defined function / closure value.
#[derive(Debug, Clone)]
pub struct FunctionValue {
    pub params: Vec<String>,
    /// Same length as `params`: `@` reference parameter (Java Leek).
    pub param_by_ref: Vec<bool>,
    /// Same length as `params`: optional declared parameter type spelling.
    pub param_decl_tys: Vec<Option<String>>,
    /// Same length as `params`: optional default when the argument is omitted.
    pub param_defaults: Vec<Option<HirExpr>>,
    pub body: Vec<HirStmt>,
    pub captured_locals: Option<std::collections::HashMap<String, Value>>,
    pub captured_aliases: Option<std::collections::HashMap<String, String>>,
    pub declared_return_ty: Option<String>,
    /// True when this function value came from `A.m` (instance method read on a class),
    /// and should accept an explicit receiver as the first call argument.
    pub unbound_method_ref: bool,
}

/// Runtime value (minimal; grows with the language).
#[derive(Debug, Clone)]
pub enum Value {
    /// Java / LeekScript `integer` (64-bit in this runtime).
    Integer(i64),
    /// Java / LeekScript `real` (`double`).
    Real(f64),
    /// v1 export quirk: some integral reals keep a `.0` suffix (e.g. coerced return values).
    RealDotZero(f64),
    String(String),
    Bool(bool),
    Null,
    /// Array literal / runtime array (for `for`-`in` and mutating builtins).
    Array(SharedArray),
    /// Bracket map / `new Map(...)` — insertion-ordered key/value pairs (Java `MapLeekValue`).
    Map(SharedMap),
    /// Object literal `{ k: v, ... }` — distinct from [`Value::Map`] (`instanceof` / export).
    Object(SharedMap),
    /// `new Set(v1,...)` — unique values in insertion order (Java `SetLeekValue`).
    Set(SharedSet),
    Interval(IntervalValue),
    /// User-defined `function` / closure (identity is significant for `==`).
    Function(Rc<FunctionValue>),
    /// Built-in registered in the global environment (Java `LeekFunctions` entry points).
    Native(&'static str),
    /// `new UserClass(...)` — methods live on the class table; fields here.
    Instance(Rc<RefCell<InstanceData>>),
    /// User `class C { ... }` binding at file scope (`C.execute` static calls).
    UserClass(String),
    /// `super` keyword: member/call resolution uses the immediate superclass (requires `this`).
    Super,
}

impl Value {
    pub fn array_from(elements: Vec<Value>) -> Self {
        Value::Array(Rc::new(RefCell::new(elements)))
    }

    pub fn map_from(pairs: Vec<(Value, Value)>) -> Self {
        Value::Map(Rc::new(RefCell::new(MapStore::from_pairs(pairs))))
    }

    pub fn map_from_store(store: MapStore) -> Self {
        Value::Map(Rc::new(RefCell::new(store)))
    }

    pub fn object_from(pairs: Vec<(Value, Value)>) -> Self {
        Value::Object(Rc::new(RefCell::new(MapStore::from_pairs(pairs))))
    }

    pub fn object_from_store(store: MapStore) -> Self {
        Value::Object(Rc::new(RefCell::new(store)))
    }

    /// Rebuild key/value storage with the same runtime kind as `template` (`Map` vs `Object`).
    pub fn wrap_keyed_pairs(template: &Value, pairs: Vec<(Value, Value)>) -> Self {
        match template {
            Value::Object(_) => Value::object_from(pairs),
            _ => Value::map_from(pairs),
        }
    }

    pub fn set_from(elements: Vec<Value>) -> Self {
        // `set_from` is used for `new Set()` / `Set()` and should not auto-sort its export.
        let java_hash_export = false;
        Value::Set(Rc::new(RefCell::new(SetData {
            elems: elements,
            // Java suite: sets created non-empty export with HashSet-like ordering; empty sets do not.
            java_hash_export,
            ever_mutated: false,
        })))
    }

    /// Set from angle-bracket literal `new SetLiteral(...)` (non-empty → hash-style export).
    pub fn set_from_literal(elements: Vec<Value>) -> Self {
        // Set literals export with Java `HashSet`-like ordering.
        let java_hash_export = true;
        Value::Set(Rc::new(RefCell::new(SetData {
            java_hash_export,
            elems: elements,
            ever_mutated: false,
        })))
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        use Value::*;
        match (self, other) {
            (Integer(a), Integer(b)) => a == b,
            (Real(a), Real(b)) => a == b,
            (RealDotZero(a), RealDotZero(b)) => a == b,
            (String(a), String(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            (Null, Null) => true,
            (Array(a), Array(b)) => *a.borrow() == *b.borrow(),
            (Map(a), Map(b)) | (Object(a), Object(b)) => *a.borrow() == *b.borrow(),
            (Set(a), Set(b)) => *a.borrow() == *b.borrow(),
            (Interval(a), Interval(b)) => a == b,
            (Function(a), Function(b)) => Rc::ptr_eq(a, b),
            (Native(a), Native(b)) => a == b,
            (Instance(a), Instance(b)) => Rc::ptr_eq(a, b),
            (UserClass(a), UserClass(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_value(self, f, &mut HashSet::new())
    }
}

/// Same idea as the Java runner’s object string (`ClassName { field: value, ... }`), with `<...>` when
/// the same instance is reached again while printing (cycle or shared reference).
fn display_value(
    v: &Value,
    f: &mut fmt::Formatter<'_>,
    visited: &mut HashSet<usize>,
) -> fmt::Result {
    match v {
        Value::Integer(n) => write!(f, "{n}"),
        Value::Real(n) => {
            if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
                write!(f, "{:.0}", n)
            } else {
                write!(f, "{n}")
            }
        }
        Value::RealDotZero(n) => {
            if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
                write!(f, "{:.0}", n)
            } else {
                write!(f, "{n}")
            }
        }
        Value::String(s) => write!(f, "{s:?}"),
        Value::Bool(b) => write!(f, "{b}"),
        Value::Null => write!(f, "null"),
        Value::Array(_) => write!(f, "<Array>"),
        Value::Map(_) => write!(f, "<Map>"),
        Value::Object(_) => write!(f, "<Object>"),
        Value::Set(_) => write!(f, "<Set>"),
        Value::Interval(_) => write!(f, "<Interval>"),
        Value::Function(_) => write!(f, "<Function>"),
        Value::Native(name) => write!(f, "<native {name}>"),
        Value::UserClass(n) => write!(f, "<class {n}>"),
        Value::Super => write!(f, "<super>"),
        Value::Instance(rc) => {
            let ptr = Rc::as_ptr(rc) as usize;
            if !visited.insert(ptr) {
                return write!(f, "<...>");
            }
            let b = rc.borrow();
            write!(f, "{} {{", b.class_name)?;
            let mut keys: Vec<&String> = b.fields.keys().collect();
            keys.sort();
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let fv = &b.fields[*k];
                write!(f, "{}: ", k)?;
                display_value(fv, f, visited)?;
            }
            write!(f, "}}")
        }
    }
}
