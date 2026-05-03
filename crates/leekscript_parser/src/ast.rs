//! Concrete syntax tree shape produced by the hand-written parser (before rowan lowering).

/// Top-level parse result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Var(VarDecl),
    TypedVar(TypedVarDecl),
    Return(ReturnStmt),
    Block(Block),
    /// `function` name `(` … `)` `{` … `}`
    Function(FunctionDecl),
    /// `if` `(` cond `)` `{` … `}` [ `else` ( `{` … `}` | `if` … ) ]
    If(IfStmt),
    /// `while` `(` cond `)` `{` … `}`
    While(WhileStmt),
    /// `do` `{` … `}` `while` `(` cond `)` `;`
    DoWhile(DoWhileStmt),
    /// `switch` `(` expr `)` `{` … `}`
    Switch(SwitchStmt),
    /// `for` `(` init `;` cond `;` update `)` `{` … `}`
    For(Box<ForStmt>),
    /// `for` `(` (`var`)? ident `in` expr `)` `{` … `}`
    ForIn(ForInStmt),
    /// `for` `(` key `:` value `in` expr `)` `{` … `}` (`key` / `value`: `ident` or `var` `ident`).
    ForInKeyValue(ForInKeyValueStmt),
    /// `lhs` `=` / `+=` expr `;` (`lhs`: ident or indexed).
    Assign(AssignStmt),
    /// `try` `{` … `}` `catch` `(` ident `)` `{` … `}`
    Try(TryStmt),
    /// `throw` [expr] `;`
    Throw(ThrowStmt),
    /// `class` name `{` … `}`
    Class(ClassDecl),
    /// `break` `;`
    Break(BreakStmt),
    /// `continue` `;`
    Continue(ContinueStmt),
    /// Expression statement: `expr` [ `;` ] (Java `WordCompiler`: `END_INSTRUCTION` optional).
    ExprSemi(Expr, Option<usize>),
    /// Empty statement `;` (Java `WordCompiler` accepts it).
    Empty {
        semi: usize,
    },
    /// `global` name [`=` expr] (`,` name [`=` expr])* `;`
    Global(GlobalDecl),
    /// `include` `(` string `)` `;` (Java main block); also `include` `(` `(` … string … `)` `)` as accepted by Java.
    Include(IncludeStmt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalDecl {
    pub global_kw: usize,
    /// Tokens for optional `global` `<Type>` prefix (same as parameter / typed-var types).
    pub leading_type_tokens: Vec<usize>,
    pub items: Vec<GlobalItem>,
    /// Comma token after `items[i]` when `i + 1 < items.len()`.
    pub item_commas: Vec<usize>,
    pub semi: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalItem {
    pub name: usize,
    pub eq: Option<usize>,
    pub init: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeStmt {
    pub include_kw: usize,
    pub open_paren: usize,
    /// Legacy field: always empty; `include("p")` is the only accepted shape (matches Java).
    pub inner_open_parens: Vec<usize>,
    pub path: usize,
    /// Closing `)` for `include(` … `)`.
    pub close_parens: Vec<usize>,
    pub semi: Option<usize>,
}

/// One binding in `var a = 1, b = 2` or `var a;`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarDeclarator {
    pub name: usize,
    pub eq: Option<usize>,
    pub init: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarDecl {
    pub var_kw: usize,
    pub decls: Vec<VarDeclarator>,
    pub commas: Vec<usize>,
    pub semi: Option<usize>,
}

/// Java typed leading form: `Type` name [`=` expr] [`;`] (initializer optional for locals).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedVarDecl {
    pub ty: TypeExpr,
    /// Lossless replay of Java `eatType` tokens for formatting/CST.
    pub type_tokens: Vec<usize>,
    pub name: usize,
    pub eq: Option<usize>,
    pub init: Option<Expr>,
    pub semi: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnStmt {
    pub return_kw: usize,
    /// Java `return ?` marker (operator `?` token).
    pub optional_question: Option<usize>,
    /// `return @ expr` — reference return (Java Leek v1 container sharing).
    pub at_kw: Option<usize>,
    pub value: Option<Expr>,
    pub semi: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub open: usize,
    pub stmts: Vec<Stmt>,
    pub close: usize,
}

/// `=` expr after a parameter name (Java default argument).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDefault {
    pub eq: usize,
    pub value: Expr,
}

/// Function body: block (executable) or signature-only stub (`function f() => T;` for API / `.sig.leek`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBody {
    Block(Block),
    /// `;` after optional `=>` return type (declaration-only).
    SignatureStub {
        semi: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDecl {
    /// `private` / `public` / `static` / `final` before the return type (class members).
    pub member_modifiers: Vec<usize>,
    /// `None` for Java-style class methods: `ReturnType name(...)`.
    pub function_kw: Option<usize>,
    /// Leading return type tokens (`Logger` in `Logger log(...)`); empty for `function` methods.
    pub return_type_tokens: Vec<usize>,
    pub name: usize,
    pub open_paren: usize,
    pub params: Vec<usize>,
    /// Type tokens before `params[i]` (Java parameter annotations); same length as `params`.
    pub param_type_tokens: Vec<Vec<usize>>,
    /// Comma token after `params[i]` when `i + 1 < params.len()`.
    pub param_commas: Vec<usize>,
    /// Optional default value for `params[i]` (`None` when omitted).
    pub param_defaults: Vec<Option<ParamDefault>>,
    /// `@` token before `params[i]` when present (Java reference parameter).
    pub param_at: Vec<Option<usize>>,
    pub close_paren: usize,
    /// `function name(...) => ReturnType {` — `=>` token and return type tokens after `)` (tooling / parity).
    pub arrow_return: Option<(usize, Vec<usize>)>,
    pub body: FunctionBody,
}

/// `function` `(` … `)` (`=>` [return type])? `{` … `}` in expression position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionValueExpr {
    pub function_kw: usize,
    pub open_paren: usize,
    pub params: Vec<usize>,
    pub param_type_tokens: Vec<Vec<usize>>,
    pub param_defaults: Vec<Option<ParamDefault>>,
    pub param_commas: Vec<usize>,
    /// `@` token before `params[i]` when present.
    pub param_at: Vec<Option<usize>>,
    pub close_paren: usize,
    /// `None` for Java-style `function (...) {` without `=>`.
    pub arrow: Option<usize>,
    pub return_type_tokens: Vec<usize>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStmt {
    pub if_kw: usize,
    pub open_paren: usize,
    pub cond: Expr,
    pub close_paren: usize,
    pub then_body: StmtBody,
    pub else_kw: Option<usize>,
    pub else_branch: Option<ElseBranch>,
}

/// `else { ... }` or `else if (...) ...` (chain).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElseBranch {
    Body(StmtBody),
    If(Box<IfStmt>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StmtBody {
    Block(Block),
    Single(Box<Stmt>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhileStmt {
    pub while_kw: usize,
    pub open_paren: usize,
    pub cond: Expr,
    pub close_paren: usize,
    pub body: StmtBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoWhileStmt {
    pub do_kw: usize,
    pub body: Block,
    pub while_kw: usize,
    pub open_paren: usize,
    pub cond: Expr,
    pub close_paren: usize,
    pub semi: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchStmt {
    pub switch_kw: usize,
    pub open_paren: usize,
    pub discr: Expr,
    pub close_paren: usize,
    pub open_brace: usize,
    pub clauses: Vec<SwitchClause>,
    pub close_brace: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchClause {
    Case {
        labels: Vec<CaseLabel>,
        body: Vec<Stmt>,
    },
    Default {
        default_kw: usize,
        colon: usize,
        body: Vec<Stmt>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseLabel {
    pub case_kw: usize,
    pub value: Expr,
    pub colon: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForInStmt {
    pub for_kw: usize,
    pub open_paren: usize,
    pub binding: ForInBinding,
    pub in_kw: usize,
    pub container: Expr,
    pub close_paren: usize,
    pub body: StmtBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForInKeyValueStmt {
    pub for_kw: usize,
    pub open_paren: usize,
    pub key: ForInBinding,
    pub colon: usize,
    pub value: ForInBinding,
    pub in_kw: usize,
    pub container: Expr,
    pub close_paren: usize,
    pub body: StmtBody,
}

/// `for`-`in` / key-value header binding: optional Java-style type, optional `var`, optional `@`, name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForInBinding {
    /// Tokens from [`WordCompiler.eatType`](https://github.com/leek-wars/leekscript) (replay only).
    pub type_tokens: Option<Vec<usize>>,
    pub var_kw: Option<usize>,
    pub at_kw: Option<usize>,
    pub name: usize,
}

impl ForInBinding {
    /// Java: `var` keyword or a type prefix introduces a new local.
    #[must_use]
    pub fn is_declaration(&self) -> bool {
        self.var_kw.is_some() || self.type_tokens.is_some()
    }
}

/// Third clause of a C-style `for` (`i++`, `i += 1`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForUpdate {
    Assign(ForAssign),
    Expr(Expr),
}

/// `for` `(` … `;` … `;` … `)` `{` … `}` — each clause may be empty (`;` with nothing before it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForStmt {
    pub for_kw: usize,
    pub open_paren: usize,
    pub init: Option<ForInit>,
    pub first_semi: usize,
    pub cond: Option<Expr>,
    pub second_semi: usize,
    pub update: Option<ForUpdate>,
    pub close_paren: usize,
    pub body: StmtBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForInit {
    Var(VarDeclFor),
    Assign(ForAssign),
}

/// `var` / typed name `=` expr without trailing `;` (only in `for` header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarDeclFor {
    /// Java-style type prefix (`integer k = …`); mutually optional with [`Self::var_kw`].
    pub type_tokens: Option<Vec<usize>>,
    pub var_kw: Option<usize>,
    pub name: usize,
    pub eq: usize,
    pub init: Expr,
}

/// name `=` expr without `;` (`for` init or update).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForAssign {
    pub name: usize,
    /// `=` or `+=` token index.
    pub op: usize,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignStmt {
    pub target: Expr,
    /// `=` or `+=` token index.
    pub op: usize,
    pub value: Expr,
    pub semi: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchClause {
    pub catch_kw: usize,
    pub open: usize,
    pub param: usize,
    pub close: usize,
    pub body: Block,
}

/// `try` with optional `catch` and/or `finally` (Java requires at least one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TryStmt {
    pub try_kw: usize,
    pub try_body: Block,
    pub catch: Option<CatchClause>,
    /// `(finally_kw, block)` when present.
    pub finally_block: Option<(usize, Block)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThrowStmt {
    pub throw_kw: usize,
    pub value: Option<Expr>,
    pub semi: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDecl {
    pub class_kw: usize,
    pub name: usize,
    /// `extends` keyword and superclass name token (`class Weapon extends Item`).
    pub extends: Option<(usize, usize)>,
    pub open_brace: usize,
    pub members: Vec<ClassMember>,
    pub close_brace: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassMember {
    Method(FunctionDecl),
    Constructor(ConstructorDecl),
    /// `[private|public]?` type name [`;`]
    Field(ClassFieldDecl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassFieldInit {
    pub eq: usize,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassFieldDecl {
    pub modifiers: Vec<usize>,
    pub type_tokens: Vec<usize>,
    pub name: usize,
    pub init: Option<ClassFieldInit>,
    pub semi: Option<usize>,
}

/// `constructor` `(` params `)` block (v2+).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructorDecl {
    /// `private` / `public` / `protected` / `static` / `final` before `constructor`.
    pub member_modifiers: Vec<usize>,
    pub constructor_kw: usize,
    pub open_paren: usize,
    pub params: Vec<usize>,
    pub param_type_tokens: Vec<Vec<usize>>,
    /// Comma token after `params[i]` when `i + 1 < params.len()`.
    pub param_commas: Vec<usize>,
    /// Optional default for each constructor parameter.
    pub param_defaults: Vec<Option<ParamDefault>>,
    /// `@` token before `params[i]` when present.
    pub param_at: Vec<Option<usize>>,
    pub close_paren: usize,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakStmt {
    pub break_kw: usize,
    pub semi: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinueStmt {
    pub continue_kw: usize,
    pub semi: Option<usize>,
}

/// Body of an [`Expr::ArrowFn`]: expression or braced statement list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrowFnBody {
    Expr(Box<Expr>),
    Block(Block),
}

/// Expression with token indices into the original lexer stream (for lossless lowering + trivia).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// Single token: number, string, identifier, or keyword literal (`true`, `false`, `null`, …).
    Leaf(usize),
    /// `class` in expression position, before `.` or `[` (enclosing class reference).
    ClassSelf {
        class_kw: usize,
    },
    /// Prefix `-` or `!` (token index of operator).
    Unary {
        op: usize,
        expr: Box<Expr>,
    },
    /// `@expr` — Java reference prefix in expression position (v1 container copy bypass).
    Ref {
        at_kw: usize,
        expr: Box<Expr>,
    },
    Binary(Box<Expr>, usize, Box<Expr>),
    Paren {
        open: usize,
        expr: Box<Expr>,
        close: usize,
    },
    /// Function call: `callee` `(` args `)`.
    Call {
        callee: Box<Expr>,
        open: usize,
        args: Vec<Expr>,
        /// Comma tokens between arguments; `None` = minified adjacent string args (`split('a' 'b')`).
        arg_commas: Vec<Option<usize>>,
        close: usize,
    },
    /// `[` … `]` **comma-separated values** only (no `:` between items) — Java array / legacy list literal.
    ///
    /// Distinct from [`Expr::MapLiteral`] (`[` … `key` `:` `value` … `]`), [`Expr::IntervalLiteral`]
    /// (`..` inside brackets), [`Expr::SetLiteral`] (`<` … `>`), and [`Expr::ObjectLiteral`] (`{` … `}`).
    ArrayLiteral {
        open: usize,
        elements: Vec<Expr>,
        commas: Vec<usize>,
        close: usize,
    },
    /// `[:]` or `[` key `:` value (`,` …)* `]` — **bracket map** literal (Java `LeekMap` / legacy map shape).
    ///
    /// Not an [`Expr::ObjectLiteral`] (`{` … `}`): delimiters and CST kind (`MapLiteralExpr`) differ.
    MapLiteral {
        open: usize,
        entries: Vec<MapEntry>,
        commas: Vec<usize>,
        close: usize,
    },
    /// `[` or `]` … `..` … `]` or `[` — **interval** literal; never uses comma-separated elements like [`Expr::ArrayLiteral`].
    IntervalLiteral {
        /// Left delimiter token: `[` (min inclusive) or `]` (min exclusive).
        open: usize,
        min: Option<Box<Expr>>,
        dotdot: usize,
        max: Option<Box<Expr>>,
        close: usize,
    },
    /// `<` expr (`,` expr)* `>` — **set** literal (angle brackets), not a map or object.
    /// `close` is `None` when the closing `>` is absorbed from a child’s `>>` / `>>>` token (outer nested set).
    SetLiteral {
        open: usize,
        elements: Vec<Expr>,
        commas: Vec<usize>,
        close: Option<usize>,
    },
    /// `{` property `:` expr (`,` …)* `}` — **object** literal (v2+). Keys: ident, string/number literal, or `true`/`false`/`null`.
    ///
    /// Lowered separately from [`Expr::MapLiteral`]: brace + `ObjectLiteralExpr` vs bracket + `MapLiteralExpr`.
    ObjectLiteral {
        open: usize,
        properties: Vec<ObjectProperty>,
        commas: Vec<usize>,
        close: usize,
    },
    /// `new` name `(` arguments `)` (Java `new Map()`, `new Interval(a,b,c,d)`, …).
    New {
        new_kw: usize,
        type_name: usize,
        /// `None` for `new Integer` / `new Real` / `new Number` without `()` (Java v3+).
        open: Option<usize>,
        args: Vec<Expr>,
        arg_commas: Vec<Option<usize>>,
        close: Option<usize>,
    },
    /// Postfix indexing: `base` `[` `index` `]`.
    Index {
        base: Box<Expr>,
        open: usize,
        index: Box<Expr>,
        close: usize,
    },
    /// Array slice `base` `[` [start] `:` [end] [`:` step] `]` (Java v4+ `LeekArray` slice style).
    ArraySlice {
        base: Box<Expr>,
        open: usize,
        start: Option<Box<Expr>>,
        colon: usize,
        end: Option<Box<Expr>>,
        /// Second `:` before step, when present.
        colon_step: Option<usize>,
        step: Option<Box<Expr>>,
        close: usize,
    },
    /// Postfix member: `base` `.` field (field token index).
    Member {
        base: Box<Expr>,
        dot: usize,
        field: usize,
    },
    /// Java `TERNAIRE`: `cond` `?` `then_expr` `:` `else_expr`.
    Ternary {
        cond: Box<Expr>,
        question: usize,
        then_expr: Box<Expr>,
        colon: usize,
        else_expr: Box<Expr>,
    },
    /// Java `NOT_IN`: `elem` `not` `in` `container`.
    NotIn {
        elem: Box<Expr>,
        not_kw: usize,
        in_kw: usize,
        container: Box<Expr>,
    },
    /// Java cast: `expr` `as` type.
    AsCast {
        expr: Box<Expr>,
        as_kw: usize,
        ty: TypeExpr,
        /// Lossless replay of Java `eatType` tokens for formatting/CST.
        type_tokens: Vec<usize>,
    },
    /// Java prefix cast: `real` expr (binds like a unary; looser than `as`).
    PrefixCast {
        ty: usize,
        expr: Box<Expr>,
    },
    /// Arrow function: `ident` `=>` expr/block or `(` ident (`,` ident)* `)` `=>` expr/block.
    ArrowFn {
        open_paren: Option<usize>,
        params: Vec<usize>,
        param_commas: Vec<usize>,
        close_paren: Option<usize>,
        arrow: usize,
        body: ArrowFnBody,
    },
    /// `function` `(` typed params `)` `=>` [type] `{` … `}`.
    FunctionValue(FunctionValueExpr),
    /// Postfix `++` / `--` (Java-style update; result of expression is discarded at runtime today).
    PostUpdate {
        expr: Box<Expr>,
        increment: bool,
        op1: usize,
        op2: usize,
    },
    /// Prefix `++` / `--` (`++i` in `for` headers, `return ++a`, …).
    PreUpdate {
        expr: Box<Expr>,
        increment: bool,
        op1: usize,
        op2: usize,
    },
    /// `lhs` `=` / `+=` / … `rhs` in expression position (e.g. `return a += b`).
    AssignExpr {
        target: Box<Expr>,
        op: usize,
        value: Box<Expr>,
    },
}

/// Type expression (Java `eatType` / `Type` parsing), represented as a small tree.
///
/// This is *not* a full type system: it mirrors the syntactic forms we can parse today and is
/// intended to power cast/type diagnostics and HIR lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    /// `integer`, `real`, `string`, `boolean`, `any`, `Object`, `Class`, or a user class identifier.
    Named { name: usize },
    /// `T?`
    Nullable {
        inner: Box<TypeExpr>,
        question: usize,
    },
    /// `A | B | C`
    Union {
        first: Box<TypeExpr>,
        rest: Vec<(usize, TypeExpr)>,
    },
    /// `Base<...>` for `Array`, `Set`, `Map`, `Function`, and generic class types.
    ///
    /// For `Function<...>`, the optional return type is represented by `arrow_ret`.
    Generic {
        base: usize,
        lt: usize,
        args: Vec<TypeExpr>,
        commas: Vec<usize>,
        arrow_ret: Option<(usize, Box<TypeExpr>)>,
        gt: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEntry {
    pub key: Expr,
    pub colon: usize,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectProperty {
    pub key_tok: usize,
    pub colon: usize,
    pub value: Expr,
}
