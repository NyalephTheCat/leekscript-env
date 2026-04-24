//! Simplified statement and expression shapes for tooling and future backends.

use leekscript_span::Span;
use std::path::PathBuf;

/// A binding name with its source span (definition site).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameDef {
    pub name: String,
    pub span: Span,
}

/// Function / method parameter after lowering (`@name` is a reference parameter in Java Leek).
#[derive(Debug, Clone, PartialEq)]
pub struct HirParam {
    pub name: NameDef,
    pub by_ref: bool,
    /// Optional declared type spelling (`integer`, `Function<integer => integer>`, ...).
    pub decl_ty: Option<String>,
    /// `= expr` default when the argument is omitted at the call site (Java optional parameters).
    pub default: Option<HirExpr>,
}

/// A whole `.leek` file after lowering.
#[derive(Debug, Clone, PartialEq)]
pub struct HirFile {
    pub stmts: Vec<HirStmt>,
    /// After `include` expansion: canonical path of the `.leek` file each **top-level** stmt came from.
    /// When empty, all statements belong to the main unit path passed to analysis.
    pub stmt_sources: Vec<PathBuf>,
}

impl HirFile {
    pub fn new(stmts: Vec<HirStmt>) -> Self {
        Self {
            stmts,
            stmt_sources: Vec::new(),
        }
    }
}

/// One arm of a [`HirStmt::Switch`](HirStmt::Switch) (C-style fall-through order).
#[derive(Debug, Clone, PartialEq)]
pub enum HirSwitchClause {
    Case {
        labels: Vec<HirExpr>,
        body: Vec<HirStmt>,
    },
    Default {
        body: Vec<HirStmt>,
    },
}

/// `=` / `+=` / … (Java compound assign).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirAssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    PowAssign,
    IntDivAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    ShlAssign,
    ShrAssign,
    UShrAssign,
}

/// `for` `(` … `;` … `;` `i = …` `)` update clause.
#[derive(Debug, Clone, PartialEq)]
pub struct HirForUpdate {
    pub name: NameDef,
    pub op: HirAssignOp,
    pub value: HirExpr,
}

/// Third clause of a C-style `for` header (`i++`, `i += 1`, …).
#[derive(Debug, Clone, PartialEq)]
pub enum HirForStep {
    Assign(HirForUpdate),
    Expr(HirExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirStmt {
    Var {
        name: NameDef,
        /// `None` for `var x;` (initialized to `null`).
        init: Option<HirExpr>,
        /// Lowercased type prefix from `integer x = …` / `real y;` (`None` for `var` / for-init).
        decl_ty: Option<String>,
    },
    /// `expr;`
    Expr(HirExpr),
    /// `return` [ `?` ] [ `@` ] [ expr ] [ `;` ] — `if_truthy` matches Java `return ? expr` (return only if value is truthy).
    /// `by_ref`: `return @ expr` — v1 shares array/map/set with caller; primitives behave like a normal return.
    Return {
        value: Option<HirExpr>,
        if_truthy: bool,
        by_ref: bool,
    },
    Block(Vec<HirStmt>),
    /// `function` name `(` params `)` `{` body `}`
    FnDecl {
        name: NameDef,
        params: Vec<HirParam>,
        /// Optional declared return type (currently only used for strict v4 parity).
        return_ty: Option<String>,
        body: Vec<HirStmt>,
    },
    /// `class` Name [`extends` Super] `{` members `}`.
    ClassDecl {
        name: NameDef,
        extends: Option<NameDef>,
        members: Vec<HirClassMember>,
    },
    /// `if (cond) { ... }` with optional `else` branch (`else if` is a nested `If` in `else_body`).
    If {
        cond: HirExpr,
        then_body: Vec<HirStmt>,
        else_body: Option<Vec<HirStmt>>,
    },
    /// `while (cond) { ... }`
    While {
        cond: HirExpr,
        body: Vec<HirStmt>,
    },
    /// `do { ... } while (cond);`
    DoWhile {
        body: Vec<HirStmt>,
        cond: HirExpr,
    },
    /// `switch (discr) { ... }` with Java-style fall-through between arms.
    Switch {
        discr: HirExpr,
        clauses: Vec<HirSwitchClause>,
    },
    /// `for ( init ; cond ; update ) { body }` — `cond: None` means always true.
    For {
        init: Option<Box<HirStmt>>,
        cond: Option<HirExpr>,
        update: Option<HirForStep>,
        body: Vec<HirStmt>,
    },
    /// `for (x in container)` / `for (var x in container)` — iterable: array (Java `ForeachBlock`).
    ForIn {
        name: NameDef,
        /// `var` or type prefix declares a new binding; plain `x in` assigns into an existing variable.
        is_declaration: bool,
        /// `for (var @x in arr)` — loop variable aliases `arr[i]` when iterating an array.
        name_by_ref: bool,
        container: HirExpr,
        body: Vec<HirStmt>,
    },
    /// `for (key : value in container)` — array: index + element (Java `ForeachKeyBlock`).
    ForInKeyValue {
        key: NameDef,
        key_is_declaration: bool,
        /// `for (... @k : ...)` — only affects array iteration when the key aliases a cell (unused for plain integers).
        key_by_ref: bool,
        value: NameDef,
        value_is_declaration: bool,
        /// `for (... : var @v in arr)` — value aliases `arr[k]` for arrays.
        value_by_ref: bool,
        container: HirExpr,
        body: Vec<HirStmt>,
    },
    /// `lhs = rhs` / `lhs += rhs` — `place` is [`Ident`](HirExpr::Ident), [`Index`](HirExpr::Index), or [`Member`](HirExpr::Member).
    Assign {
        place: Box<HirExpr>,
        op: HirAssignOp,
        value: HirExpr,
    },
    /// `try { … }` with optional `catch` / `finally` (Java ordering: catch then finally).
    Try {
        try_body: Vec<HirStmt>,
        catch: Option<(NameDef, Vec<HirStmt>)>,
        finally_body: Option<Vec<HirStmt>>,
    },
    /// `throw` [expr] `;`
    Throw(Option<HirExpr>),
    /// `break;`
    Break,
    /// `continue;`
    Continue,
    /// Empty `;` statement.
    Empty,
    /// `global` bindings (install in outermost scope at runtime).
    Global {
        /// Optional leading declared type: `global real x = 1, y = 2`.
        decl_ty: Option<String>,
        entries: Vec<(NameDef, Option<HirExpr>)>,
    },
    /// `include("path")` — expanded into included file statements during compile when a source path is set.
    Include {
        path: String,
        span: Span,
    },
}

/// Java-style visibility for class **fields** and **methods**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HirFieldVisibility {
    #[default]
    Public,
    Protected,
    Private,
}

/// Class members (subset of Java `endClassMember`).
#[derive(Debug, Clone, PartialEq)]
pub enum HirClassMember {
    /// Class field (instance or `static`); optional initializer expression.
    Field {
        name: NameDef,
        /// Declared field type (`real?`, `integer`, …) when present in source; used for assign coercion.
        decl_ty: Option<String>,
        init: Option<HirExpr>,
        is_static: bool,
        is_final: bool,
        visibility: HirFieldVisibility,
    },
    Method {
        name: NameDef,
        /// `static` method (callable on the class name, not on instances).
        is_static: bool,
        visibility: HirFieldVisibility,
        params: Vec<HirParam>,
        body: Vec<HirStmt>,
    },
    Constructor {
        params: Vec<HirParam>,
        body: Vec<HirStmt>,
        visibility: HirFieldVisibility,
    },
}

impl HirStmt {
    /// Plain `return` / `return expr` (not Java `return ? expr`).
    pub fn ret(value: Option<HirExpr>) -> Self {
        HirStmt::Return {
            value,
            if_truthy: false,
            by_ref: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `===` (same evaluation as [`Eq`](Self::Eq) in the tree interpreter today).
    StrictEq,
    /// `!==`
    StrictNe,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `&&` (short-circuit in interpreter).
    LogicalAnd,
    /// `||` (short-circuit in interpreter).
    LogicalOr,
    /// `xor` keyword / bitwise xor on integral numeric values (Java `xor` operator).
    BitXor,
    /// `instanceof` — rhs is a type name ([`HirExpr::Ident`](HirExpr::Ident)), not evaluated as a value.
    Instanceof,
    /// `in` — membership (Java `AI.operatorIn`).
    In,
    /// `**` — Java `Operators.POWER` / `pow(...)`.
    Pow,
    /// `\` — Java `INTEGER_DIVISION` (`getInt` / `getInt`).
    IntDiv,
    /// `??` — Java `COALESCE` (`a != null ? a : b`).
    NullishCoalesce,
    /// `&` — Java `BITAND`.
    BitAnd,
    /// `|` — Java `BITOR`.
    BitOr,
    /// `<<` — Java `SHIFT_LEFT`.
    Shl,
    /// `>>` — Java `SHIFT_RIGHT` (sign-preserving).
    Shr,
    /// `>>>` — Java `SHIFT_UNSIGNED_RIGHT`.
    UShr,
    /// `not in` — Java `NOT_IN` / `!operatorIn(...)`.
    NotIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnaryOp {
    /// Unary `-`
    Neg,
    /// Logical `!`
    Not,
    /// Bitwise `~` — Java `BITNOT` / `bnot(...)`.
    BitNot,
    /// Unary `typeof` — same numeric codes as the `typeOf` native (Java LeekConstants).
    Typeof,
}

/// Parsed type expression (syntax-level), used for casts and typed APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirTypeExpr {
    /// `integer`, `real`, `string`, `boolean`, `any`, `Object`, `Class`, `void`, or user class name.
    Named(String),
    /// `T?`
    Nullable(Box<HirTypeExpr>),
    /// `A | B | C`
    Union(Vec<HirTypeExpr>),
    /// `Base<...>` for `Array`, `Set`, `Map`, `Function`, and generic class types.
    ///
    /// For `Function<...>`, the optional return type is represented by `ret`.
    Generic {
        base: String,
        args: Vec<HirTypeExpr>,
        ret: Option<Box<HirTypeExpr>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirExpr {
    /// Integer literal (no `.` / exponent in source).
    Integer(i64),
    /// Floating literal or value that must stay real (`∞`, `π`, `1e3`, …).
    Real(f64),
    String(String),
    Bool(bool),
    Null,
    /// `this` keyword.
    This,
    /// `class` — enclosing user class as a value (`class['a']`, `class.x`), v2+ Java parity.
    ClassSelf {
        span: Span,
    },
    Ident {
        name: String,
        span: Span,
    },
    Unary {
        op: HirUnaryOp,
        expr: Box<HirExpr>,
    },
    /// `@expr` in expression position — v1 uses reference-style passing for containers
    /// (see Java `@` / `pass_parameter_value` with `by_ref`).
    RefTo {
        expr: Box<HirExpr>,
        span: Span,
    },
    Binary {
        op: HirBinOp,
        left: Box<HirExpr>,
        right: Box<HirExpr>,
    },
    /// `cond ? then_expr : else_expr` — Java `TERNAIRE` / `DOUBLE_POINT`.
    Ternary {
        cond: Box<HirExpr>,
        then_expr: Box<HirExpr>,
        else_expr: Box<HirExpr>,
        span: Span,
    },
    /// `expr as Type` — Java cast.
    Cast {
        expr: Box<HirExpr>,
        ty: HirTypeExpr,
        span: Span,
    },
    /// Call expression; `span` covers the whole call for diagnostics.
    Call {
        callee: Box<HirExpr>,
        args: Vec<HirExpr>,
        span: Span,
    },
    /// `[` … `]` array literal.
    ArrayLiteral {
        elements: Vec<HirExpr>,
        span: Span,
    },
    /// `[` key `:` value (`,` …)* `]` — bracket map / `[:]`, distinct from [`HirExpr::ObjectLiteral`].
    MapLiteral {
        entries: Vec<(HirExpr, HirExpr)>,
        span: Span,
    },
    /// `{` key `:` value (`,` …)* `}` — object literal (v2+), distinct from [`HirExpr::MapLiteral`].
    ObjectLiteral {
        entries: Vec<(HirExpr, HirExpr)>,
        span: Span,
    },
    /// `new` `Map` / `Set` / `Interval` / user class `(` … `)`.
    New {
        type_name: String,
        args: Vec<HirExpr>,
        span: Span,
    },
    /// Postfix `[` index `]` (not an array literal). At runtime, matches Java `ArrayLeekValue.get`:
    /// negative integral indices count from the end (`-1` is last), then must be in range.
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
        span: Span,
    },
    /// Postfix `.` field (Java field / method name).
    Member {
        base: Box<HirExpr>,
        field: String,
        span: Span,
    },
    /// Array slice `base[start:end]` — runtime matches Java `ArrayLeekValue.arraySlice`: half-open
    /// `[start, end)` for `step > 0`, or `i > end` stepping by `step` when `step < 0`. Omitted bounds
    /// and normalization/clamping follow that implementation; slice `step == 0` is treated as `1`.
    ArraySlice {
        base: Box<HirExpr>,
        start: Option<Box<HirExpr>>,
        end: Option<Box<HirExpr>>,
        step: Option<Box<HirExpr>>,
        span: Span,
    },
    /// `param => expr` or `(a, b) => expr` — expression body (see [`HirExpr::FunctionLiteral`] for `=> { ... }`).
    ArrowClosure {
        params: Vec<HirParam>,
        body: Box<HirExpr>,
        span: Span,
    },
    /// `function` `(` … `)` (`=>` [type])? `{` stmts `}` or arrow with a braced body.
    FunctionLiteral {
        params: Vec<HirParam>,
        body: Vec<HirStmt>,
        span: Span,
    },
    /// Postfix `++` / `--` on an lvalue (`i++`, `a[i]--`, …). Evaluates to `null` in the tree interpreter (Java value differs).
    PostUpdate {
        target: Box<HirExpr>,
        increment: bool,
        span: Span,
    },
    /// Prefix `++` / `--` on an lvalue (`++i`, `return ++a`). Value is the **new** value after update.
    PreUpdate {
        target: Box<HirExpr>,
        increment: bool,
        span: Span,
    },
    /// Assignment expression (`=` / `+=` / …): evaluates to the assigned lvalue’s value after the op (Java).
    AssignExpr {
        place: Box<HirExpr>,
        op: HirAssignOp,
        value: Box<HirExpr>,
        span: Span,
    },
}
