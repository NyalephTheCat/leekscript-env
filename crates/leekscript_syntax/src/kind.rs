//! Stable `SyntaxKind` tags for rowan (nodes + tokens + trivia).

/// Kind for every green node and leaf in the LeekScript syntax tree.
///
/// Values are stable `u16` discriminants for [`rowan::SyntaxKind`].
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LeekSyntaxKind {
    /// Placeholder / unknown raw kind.
    Tombstone = 0,

    /// Root node: a source file.
    SourceFile = 1,

    // --- Trivia (leaf tokens; interleaved with lexical tokens) ---
    Whitespace = 10,
    LineComment = 11,
    BlockComment = 12,

    // --- Lexical tokens (mirror [`leekscript_lexer::TokenKind`] where possible) ---
    Ident = 20,
    Number = 21,
    String = 22,
    Lemniscate = 23,
    Pi = 24,
    Operator = 25,
    Semicolon = 26,
    Comma = 27,
    ParenOpen = 28,
    ParenClose = 29,
    BracketOpen = 30,
    BracketClose = 31,
    BraceOpen = 32,
    BraceClose = 33,
    Dot = 34,
    DotDot = 35,
    Arrow = 36,
    /// Any reserved keyword — text disambiguates ([`leekscript_lexer::keyword::classify_word`]).
    Kw = 37,
    /// `and` / `or` / `xor` / `instanceof`.
    WordOp = 38,

    // --- Grammar (composite nodes; parser-produced) ---
    /// `var name = expr ;`
    VarDecl = 40,
    /// `expr ;`
    ExprStmt = 41,
    /// `return` [expr] `;`
    ReturnStmt = 42,
    /// `{` stmt* `}`
    Block = 43,
    /// Expression subtree.
    Expr = 44,
    /// Binary operation (left op right).
    BinaryExpr = 45,
    /// Number / string / `true` / `false` / `null` / …
    LiteralExpr = 46,
    /// Identifier reference.
    IdentExpr = 47,
    /// `(` expr `)`
    ParenExpr = 48,
    /// `function` name `(` param* `)` [ `=>` type ] `{` stmt* `}`
    FunctionDecl = 49,
    /// Single parameter name in a function header.
    FnParam = 50,
    /// Callee `(` arguments `)`
    CallExpr = 51,
    /// `if` `(` cond `)` then [`else` else]
    IfStmt = 52,
    /// `while` `(` cond `)` body
    WhileStmt = 53,
    /// `name` `=` expr `;`
    AssignStmt = 54,
    /// `break` `;`
    BreakStmt = 55,
    /// `continue` `;`
    ContinueStmt = 56,
    /// Unary `-` / `!` / …
    UnaryExpr = 57,
    /// `for` `(` … `;` … `;` … `)` body
    ForStmt = 58,
    /// `var` name `=` expr (no `;`) — only inside `for` `(`.
    ForInitVar = 59,
    /// name `=` expr (no `;`) — `for` init assign or update clause.
    ForAssign = 60,
    /// Lone `;` (Java empty statement).
    EmptyStmt = 61,
    /// `do` `{` body `}` `while` `(` cond `)` `;`
    DoWhileStmt = 62,
    /// `switch` `(` expr `)` `{` clauses `}`
    SwitchStmt = 63,
    /// One `case` arm: one or more `case` labels then statements (fall-through labels merged).
    SwitchCaseClause = 64,
    /// `default` `:` statements
    SwitchDefaultClause = 65,
    /// `case` expr `:` inside a [`SwitchCaseClause`](Self::SwitchCaseClause).
    CaseLabel = 66,
    /// `for` `(` binding `in` expr `)` body — iterator form (`docs/spec/leekscript-language.md` §7.5).
    ForInStmt = 67,
    /// `var` ident or plain ident in a [`ForInStmt`](Self::ForInStmt) header.
    ForInBinding = 68,
    /// `[` expr (`,` expr)* `]`
    ArrayLiteralExpr = 69,
    /// `for` `(` key `:` value `in` expr `)` body — Java `ForeachKeyBlock`.
    ForInKeyValueStmt = 70,
    /// Optional Java-style type prefix inside [`ForInBinding`](Self::ForInBinding).
    ForInTypeAnn = 71,
    /// `new` Ident `(` … `)` — Java `LeekFunctionCall` constructor call.
    NewExpr = 72,
    /// Postfix `[` expr `]` indexing.
    IndexExpr = 73,
    /// Postfix `.` field access.
    MemberExpr = 74,
    /// `try` `{` … `}` [ `catch` `(` ident `)` `{` … `}` ] [ `finally` `{` … `}` ].
    TryStmt = 75,
    /// `throw` [expr] `;`
    ThrowStmt = 76,
    /// `class` name `{` … `}`
    ClassDecl = 77,
    /// `[` key `:` value (`,` key `:` value)* `]` — Java `LeekMap` literal (v4+).
    MapLiteralExpr = 78,
    /// `[` [expr] `..` [expr] `]` — Java `LeekInterval` bracket form.
    IntervalLiteralExpr = 79,
    /// `<` expr (`,` expr)* `>` — Java set literal.
    SetLiteralExpr = 80,
    /// `{` name `:` expr (`,` …)* `}` — Java object literal (v2+).
    ObjectLiteralExpr = 81,
    /// Postfix `[` [start] `:` [end] [`:` step] `]` array slice.
    ArraySliceExpr = 82,
    /// `global` ident [`=` expr] (`,` …)* `;`
    GlobalStmt = 83,
    /// `include` `(` string `)` `;`
    IncludeStmt = 84,
    /// `cond` `?` `then` `:` `else` — Java ternary.
    TernaryExpr = 85,
    /// `expr` `not` `in` `container` — Java `NOT_IN`.
    NotInExpr = 86,
    /// `expr` `as` type — Java cast expression.
    AsCastExpr = 87,
    /// `constructor` `(` params `)` `{` ... `}` — class constructor member.
    ConstructorDecl = 88,
    /// Java typed declaration: `Type` name `=` expr `;`
    TypedVarDecl = 89,
    /// `[private|public]?` type name `;` — class field (Java-style).
    ClassFieldDecl = 90,
    /// `ident` `=>` expr — single-parameter arrow function (Java `LeekFunction` / lambdas).
    ArrowFnExpr = 91,
    /// Java prefix cast: `real` expr.
    PrefixCastExpr = 92,
    /// Postfix `++` / `--`.
    PostUpdateExpr = 93,
    /// `function` `(` typed params `)` `=>` [type] `{` stmts `}` — expression position (e.g. map values).
    FunctionValueExpr = 94,
    /// Optional type between `global` and the first binding (`global integer X = 1`).
    GlobalLeadingType = 95,
    /// Prefix `++` / `--` (e.g. `for` update `++i`).
    PreUpdateExpr = 96,
    /// Assignment expression (`=` / `+=` / …), e.g. `return a += 1` (Java compound assignment as expr).
    AssignExpr = 97,
}

impl LeekSyntaxKind {
    pub(crate) fn from_raw(n: u16) -> Self {
        match n {
            0 => Self::Tombstone,
            1 => Self::SourceFile,
            10 => Self::Whitespace,
            11 => Self::LineComment,
            12 => Self::BlockComment,
            20 => Self::Ident,
            21 => Self::Number,
            22 => Self::String,
            23 => Self::Lemniscate,
            24 => Self::Pi,
            25 => Self::Operator,
            26 => Self::Semicolon,
            27 => Self::Comma,
            28 => Self::ParenOpen,
            29 => Self::ParenClose,
            30 => Self::BracketOpen,
            31 => Self::BracketClose,
            32 => Self::BraceOpen,
            33 => Self::BraceClose,
            34 => Self::Dot,
            35 => Self::DotDot,
            36 => Self::Arrow,
            37 => Self::Kw,
            38 => Self::WordOp,
            40 => Self::VarDecl,
            41 => Self::ExprStmt,
            42 => Self::ReturnStmt,
            43 => Self::Block,
            44 => Self::Expr,
            45 => Self::BinaryExpr,
            46 => Self::LiteralExpr,
            47 => Self::IdentExpr,
            48 => Self::ParenExpr,
            49 => Self::FunctionDecl,
            50 => Self::FnParam,
            51 => Self::CallExpr,
            52 => Self::IfStmt,
            53 => Self::WhileStmt,
            54 => Self::AssignStmt,
            55 => Self::BreakStmt,
            56 => Self::ContinueStmt,
            57 => Self::UnaryExpr,
            58 => Self::ForStmt,
            59 => Self::ForInitVar,
            60 => Self::ForAssign,
            61 => Self::EmptyStmt,
            62 => Self::DoWhileStmt,
            63 => Self::SwitchStmt,
            64 => Self::SwitchCaseClause,
            65 => Self::SwitchDefaultClause,
            66 => Self::CaseLabel,
            67 => Self::ForInStmt,
            68 => Self::ForInBinding,
            69 => Self::ArrayLiteralExpr,
            70 => Self::ForInKeyValueStmt,
            71 => Self::ForInTypeAnn,
            72 => Self::NewExpr,
            73 => Self::IndexExpr,
            74 => Self::MemberExpr,
            75 => Self::TryStmt,
            76 => Self::ThrowStmt,
            77 => Self::ClassDecl,
            78 => Self::MapLiteralExpr,
            79 => Self::IntervalLiteralExpr,
            80 => Self::SetLiteralExpr,
            81 => Self::ObjectLiteralExpr,
            82 => Self::ArraySliceExpr,
            83 => Self::GlobalStmt,
            84 => Self::IncludeStmt,
            85 => Self::TernaryExpr,
            86 => Self::NotInExpr,
            87 => Self::AsCastExpr,
            88 => Self::ConstructorDecl,
            89 => Self::TypedVarDecl,
            90 => Self::ClassFieldDecl,
            91 => Self::ArrowFnExpr,
            92 => Self::PrefixCastExpr,
            93 => Self::PostUpdateExpr,
            94 => Self::FunctionValueExpr,
            95 => Self::GlobalLeadingType,
            96 => Self::PreUpdateExpr,
            97 => Self::AssignExpr,
            _ => Self::Tombstone,
        }
    }

    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::Whitespace | Self::LineComment | Self::BlockComment
        )
    }
}
