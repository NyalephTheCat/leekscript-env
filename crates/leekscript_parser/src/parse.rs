//! Hand-written recursive-descent parser over the lexer token stream.

use crate::ast::{
    ArrowFnBody, AssignStmt, Block, BreakStmt, CaseLabel, ClassDecl, ClassFieldDecl, ClassFieldInit, ContinueStmt,
    DoWhileStmt, ElseBranch, ClassMember, ConstructorDecl, Expr, ForAssign, ForInBinding,
    ForInKeyValueStmt,
    ForInStmt, ForInit, ForStmt, ForUpdate, FunctionBody, FunctionDecl, FunctionValueExpr, GlobalDecl,
    GlobalItem,
    IfStmt, IncludeStmt, ParamDefault,
    MapEntry, ObjectProperty, ParsedFile, ReturnStmt, Stmt, StmtBody, SwitchClause, SwitchStmt,
    CatchClause, ThrowStmt, TryStmt, TypeExpr, TypedVarDecl, VarDecl, VarDeclarator, VarDeclFor,
    WhileStmt,
};
use crate::ParseDiagnostic;
use leekscript_lexer::{Kw, Token, TokenKind, WordOp};
use leekscript_span::Span;

#[cfg(feature = "parser-trace")]
macro_rules! ptrace {
    ($p:expr, $($arg:tt)*) => {{
        let i = ($p).cur.min(($p).tokens.len().saturating_sub(1));
        let t = &($p).tokens[i];
        let txt = ($p).src.get(t.span.start as usize..t.span.end as usize).unwrap_or("");
        eprintln!("[parser] cur={i} kind={:?} text={:?} :: {}", t.kind, txt, format_args!($($arg)*));
    }};
}
#[cfg(not(feature = "parser-trace"))]
macro_rules! ptrace {
    ($p:expr, $($arg:tt)*) => {{}};
}

/// A `[` token closes a bracket interval (`[a..b[`) when it follows `..` and the upper bound
/// (Java `readInterval` / `BRACKET_LEFT`). Used for delimiter balance and to avoid parsing `3[` as
/// index when the `[` is the interval closer.
/// `]` that begins `]..` / `]expr..` interval literals (Java `readInterval` right-open left).
pub(crate) fn bracket_close_may_start_interval(tokens: &[Token], idx: usize) -> bool {
    if idx >= tokens.len() || tokens[idx].kind != TokenKind::BracketClose {
        return false;
    }
    let mut j = idx + 1;
    let mut par = 0u32;
    let mut bra = 0u32;
    let mut brc = 0u32;
    while j < tokens.len() {
        match tokens[j].kind {
            TokenKind::Semicolon if par == 0 && bra == 0 && brc == 0 => return false,
            TokenKind::DotDot if par == 0 && bra == 0 && brc == 0 => return true,
            TokenKind::ParOpen => par += 1,
            TokenKind::ParClose => par = par.saturating_sub(1),
            TokenKind::BracketOpen => bra += 1,
            TokenKind::BracketClose if bra > 0 => bra -= 1,
            TokenKind::BracketClose if bra == 0 && par == 0 && brc == 0 => return false,
            TokenKind::BraceOpen => brc += 1,
            TokenKind::BraceClose => brc = brc.saturating_sub(1),
            TokenKind::Eof => return false,
            _ => {}
        }
        j += 1;
    }
    false
}

pub(crate) fn bracket_open_closes_leek_interval(tokens: &[Token], open_idx: usize) -> bool {
    if open_idx == 0 {
        return false;
    }
    let mut j = open_idx - 1;
    let mut paren = 0u32;
    let mut bracket = 0u32;
    loop {
        match tokens[j].kind {
            TokenKind::DotDot if paren == 0 && bracket == 0 => return true,
            TokenKind::ParClose => paren = paren.saturating_add(1),
            TokenKind::ParOpen => {
                if paren == 0 {
                    return false;
                }
                paren -= 1;
            }
            TokenKind::BracketClose => bracket = bracket.saturating_add(1),
            TokenKind::BracketOpen => {
                if bracket == 0 {
                    return false;
                }
                bracket -= 1;
            }
            _ => {}
        }
        if j == 0 {
            return false;
        }
        j -= 1;
    }
}

pub(crate) fn parse_file(src: &str, tokens: &[Token]) -> (ParsedFile, Vec<ParseDiagnostic>) {
    let mut p = Parser {
        src,
        tokens,
        cur: 0,
        errors: Vec::new(),
        type_gt_slack: 0,
        type_gt_close_last: None,
        set_gt_slack: 0,
    };
    let stmts = p.parse_stmt_list();
    if !p.at_eof() && p.errors.is_empty() {
        p.error(
            "UNEXPECTED_TOKEN",
            p.peek().span,
            "unexpected token after end of statement",
        );
    }
    (ParsedFile { stmts }, p.errors)
}

struct Parser<'a> {
    src: &'a str,
    tokens: &'a [Token],
    cur: usize,
    errors: Vec<ParseDiagnostic>,
    /// When closing generics, `>>` / `>>>` are single operators; extra `>` counts live here (Java).
    type_gt_slack: u8,
    /// Last `>` / `>>` token index consumed by [`Self::eat_type_gt_close`]; reused when slack closes.
    type_gt_close_last: Option<usize>,
    /// Same idea as [`Self::type_gt_slack`] but for nested set literals (`<'a', <1, 2>>` lexes `>>`).
    set_gt_slack: u8,
}

impl<'a> Parser<'a> {
    fn text_tok(&self, i: usize) -> &'a str {
        let t = &self.tokens[i];
        &self.src[t.span.start as usize..t.span.end as usize]
    }

    fn is_operator_text(&self, text: &str) -> bool {
        matches!(self.peek().kind, TokenKind::Operator) && self.text_tok(self.cur).trim() == text
    }

    /// One closing `>` for a generic type argument list (`Map<K,V>>` lexes as `>` then `>>`).
    /// Returns `None` when this close uses a `>` already consumed from a `>>` / `>>>` token (no new leaf).
    fn eat_type_gt_close(&mut self) -> Result<Option<usize>, ()> {
        if self.type_gt_slack > 0 {
            self.type_gt_slack -= 1;
            return Ok(None);
        }
        if !matches!(self.peek().kind, TokenKind::Operator) {
            return Err(());
        }
        let t = self.text_tok(self.cur);
        match t {
            ">" => {
                let i = self.bump();
                self.type_gt_close_last = Some(i);
                Ok(Some(i))
            }
            ">>" => {
                let i = self.bump();
                self.type_gt_close_last = Some(i);
                self.type_gt_slack = 1;
                Ok(Some(i))
            }
            ">>>" => {
                let i = self.bump();
                self.type_gt_close_last = Some(i);
                self.type_gt_slack = 2;
                Ok(Some(i))
            }
            _ => Err(()),
        }
    }

    /// Token at `idx` can start an expression (operand after a prefix type like `real` in `real x`).
    fn token_starts_expr_operand(&self, idx: usize) -> bool {
        if idx >= self.tokens.len() {
            return false;
        }
        match &self.tokens[idx].kind {
            TokenKind::Ident
            | TokenKind::Number
            | TokenKind::String
            | TokenKind::ParOpen
            | TokenKind::BracketOpen
            | TokenKind::BraceOpen
            | TokenKind::Kw(Kw::New)
            | TokenKind::Kw(Kw::This)
            | TokenKind::Kw(Kw::Super)
            | TokenKind::Kw(Kw::True | Kw::False | Kw::Null)
            | TokenKind::Lemniscate
            | TokenKind::Pi => true,
            TokenKind::Operator => {
                let t = self.text_tok(idx);
                matches!(t, "-" | "!" | "~")
            }
            _ => false,
        }
    }

    fn not_in_follows(&self) -> bool {
        self.cur + 1 < self.tokens.len()
            && matches!(self.tokens[self.cur + 1].kind, TokenKind::Kw(Kw::In))
    }

    /// `]` or `[` — Java `readInterval` accepts `BRACKET_RIGHT` or `BRACKET_LEFT` as the closer.
    fn at_interval_closer(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::BracketClose | TokenKind::BracketOpen
        )
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.cur.min(self.tokens.len().saturating_sub(1))]
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn bump(&mut self) -> usize {
        let i = self.cur;
        if !self.at_eof() {
            self.cur += 1;
        }
        i
    }

    /// Java `TokenType.END_INSTRUCTION` (`;`) — optional after many statements (`WordCompiler.compileWord`).
    fn eat_optional_semicolon(&mut self) -> Option<usize> {
        if matches!(self.peek().kind, TokenKind::Semicolon) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn error(&mut self, reference: &'static str, span: Span, message: &'static str) {
        self.errors.push(ParseDiagnostic {
            reference,
            span,
            message,
        });
    }

    fn parse_stmt_list(&mut self) -> Vec<Stmt> {
        let mut out = Vec::new();
        while !self.at_eof() {
            if matches!(self.peek().kind, TokenKind::BraceClose) {
                break;
            }
            match self.parse_stmt() {
                Some(s) => out.push(s),
                None => {
                    if self.errors.is_empty() {
                        self.error("UNEXPECTED_TOKEN", self.peek().span, "expected a statement");
                    }
                    if !self.at_eof() {
                        self.bump();
                    }
                }
            }
        }
        out
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        match &self.peek().kind {
            TokenKind::Semicolon => Some(Stmt::Empty { semi: self.bump() }),
            TokenKind::Kw(Kw::Var) => self.parse_var_decl().map(Stmt::Var),
            TokenKind::Kw(Kw::Return) => self.parse_return().map(Stmt::Return),
            TokenKind::Kw(Kw::Function) => {
                // v1–v2: `function` matches case-insensitively, so `Function<…>` may be a type.
                let next_is_type_param = self
                    .tokens
                    .get(self.cur + 1)
                    .is_some_and(|t| matches!(t.kind, TokenKind::Operator) && self.text_tok(self.cur + 1).trim() == "<");
                if next_is_type_param {
                    if let Some(tv) = self.try_parse_typed_var_decl() {
                        Some(Stmt::TypedVar(tv))
                    } else {
                        self.parse_assign_or_expr_stmt()
                    }
                } else {
                    self.parse_function_decl(Vec::new()).map(Stmt::Function)
                }
            }
            TokenKind::Kw(Kw::If) => self.parse_if_stmt().map(Stmt::If),
            TokenKind::Kw(Kw::While) => self.parse_while_stmt().map(Stmt::While),
            TokenKind::Kw(Kw::Do) => self.parse_do_while_stmt().map(Stmt::DoWhile),
            TokenKind::Kw(Kw::Switch) => self.parse_switch_stmt().map(Stmt::Switch),
            TokenKind::Kw(Kw::For) => self.parse_for_stmt(),
            TokenKind::Kw(Kw::Break) => self.parse_break().map(Stmt::Break),
            TokenKind::Kw(Kw::Continue) => self.parse_continue().map(Stmt::Continue),
            TokenKind::Kw(Kw::Try) => self.parse_try_stmt().map(Stmt::Try),
            TokenKind::Kw(Kw::Throw) => self.parse_throw_stmt().map(Stmt::Throw),
            TokenKind::Kw(Kw::Class) => {
                let n = self.cur + 1;
                let class_expr = n < self.tokens.len()
                    && (matches!(
                        self.tokens[n].kind,
                        TokenKind::BracketOpen | TokenKind::Dot
                    ) || (matches!(self.tokens[n].kind, TokenKind::Operator)
                        && self.text_tok(n) == "."));
                if class_expr {
                    self.parse_assign_or_expr_stmt()
                } else {
                    self.parse_class_decl().map(Stmt::Class)
                }
            }
            TokenKind::Kw(Kw::Global) => self.parse_global_decl().map(Stmt::Global),
            TokenKind::Kw(Kw::Include) => self.parse_include_stmt().map(Stmt::Include),
            TokenKind::BraceOpen => self.parse_block().map(Stmt::Block),
            TokenKind::Ident => {
                // Avoid mis-parsing call expressions like `push(...)` as typed declarations.
                let n = self.cur + 1;
                let looks_like_call_or_access = n < self.tokens.len()
                    && matches!(
                        self.tokens[n].kind,
                        TokenKind::ParOpen | TokenKind::Dot | TokenKind::BracketOpen
                    );
                if !looks_like_call_or_access {
                    if let Some(tv) = self.try_parse_typed_var_decl() {
                        return Some(Stmt::TypedVar(tv));
                    }
                }
                self.parse_assign_or_expr_stmt()
            }
            TokenKind::Kw(Kw::This) | TokenKind::Kw(Kw::Super) => self.parse_assign_or_expr_stmt(),
            _ => self
                .parse_expr_stmt()
                .map(|(expr, semi)| Stmt::ExprSemi(expr, semi)),
        }
    }

    fn try_parse_typed_var_decl(&mut self) -> Option<TypedVarDecl> {
        let save = self.cur;
        let save_slack = self.type_gt_slack;
        self.type_gt_slack = 0;
        let (ty, type_tokens) = match self.try_parse_type_expression_union_ast() {
            Ok(v) => v,
            Err(()) => {
                self.cur = save;
                self.type_gt_slack = save_slack;
                return None;
            }
        };
        if !matches!(self.peek().kind, TokenKind::Ident) {
            self.cur = save;
            self.type_gt_slack = save_slack;
            return None;
        }
        let name = self.bump();
        let (eq, init) = if matches!(self.peek().kind, TokenKind::Operator) && self.text_tok(self.cur) == "="
        {
            let eq = self.bump();
            let init = match self.parse_expr(0) {
                Some(e) => e,
                None => {
                    self.cur = save;
                    self.type_gt_slack = save_slack;
                    return None;
                }
            };
            (Some(eq), Some(init))
        } else {
            (None, None)
        };
        let semi = self.eat_optional_semicolon();
        self.type_gt_slack = save_slack;
        Some(TypedVarDecl {
            ty,
            type_tokens,
            name,
            eq,
            init,
            semi,
        })
    }

    fn parse_assign_or_expr_stmt(&mut self) -> Option<Stmt> {
        let e = self.parse_expr(0)?;
        if matches!(self.peek().kind, TokenKind::Operator) {
            let op = self.text_tok(self.cur);
            if is_compound_assign_op(op) {
                if !self.expr_is_lvalue(&e) {
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expression is not a valid assignment target",
                    );
                    return None;
                }
                let op_tok = self.bump();
                let value = self.parse_expr(0)?;
                let semi = self.eat_optional_semicolon();
                return Some(Stmt::Assign(AssignStmt {
                    target: e,
                    op: op_tok,
                    value,
                    semi,
                }));
            }
        }
        let semi = self.eat_optional_semicolon();
        Some(Stmt::ExprSemi(e, semi))
    }

    fn expr_is_lvalue(&self, e: &Expr) -> bool {
        match e {
            Expr::Leaf(i) => matches!(self.tokens[*i].kind, TokenKind::Ident),
            Expr::Index { .. } => true,
            Expr::Member { .. } => true,
            _ => false,
        }
    }

    fn parse_break(&mut self) -> Option<BreakStmt> {
        let break_kw = self.bump();
        let semi = self.eat_optional_semicolon();
        Some(BreakStmt { break_kw, semi })
    }

    fn parse_continue(&mut self) -> Option<ContinueStmt> {
        let continue_kw = self.bump();
        let semi = self.eat_optional_semicolon();
        Some(ContinueStmt { continue_kw, semi })
    }

    fn parse_global_decl(&mut self) -> Option<GlobalDecl> {
        let global_kw = self.bump();
        let checkpoint = self.cur;
        let save_slack = self.type_gt_slack;
        self.type_gt_slack = 0;
        let leading_type_tokens = match self.try_parse_type_expression_union() {
            Ok(t) if !t.is_empty() && matches!(self.peek().kind, TokenKind::Ident) => {
                self.type_gt_slack = save_slack;
                t
            }
            _ => {
                self.cur = checkpoint;
                self.type_gt_slack = save_slack;
                Vec::new()
            }
        };
        let mut items = Vec::new();
        let mut item_commas = Vec::new();
        loop {
            if !matches!(self.peek().kind, TokenKind::Ident) {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected identifier in `global` declaration",
                );
                return None;
            }
            let name = self.bump();
            let (eq, init) = if self.is_operator_text("=") {
                let eq = self.bump();
                (Some(eq), Some(self.parse_expr(0)?))
            } else {
                (None, None)
            };
            items.push(GlobalItem { name, eq, init });
            match self.peek().kind {
                TokenKind::Comma => item_commas.push(self.bump()),
                TokenKind::Semicolon => {
                    let semi = self.bump();
                    return Some(GlobalDecl {
                        global_kw,
                        leading_type_tokens,
                        items,
                        item_commas,
                        semi: Some(semi),
                    });
                }
                _ => {
                    return Some(GlobalDecl {
                        global_kw,
                        leading_type_tokens,
                        items,
                        item_commas,
                        semi: None,
                    });
                }
            }
        }
    }

    fn parse_include_stmt(&mut self) -> Option<IncludeStmt> {
        let include_kw = self.bump();
        if !matches!(self.peek().kind, TokenKind::ParOpen) {
            self.error(
                "OPENING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `(` after `include`",
            );
            return None;
        }
        let open_paren = self.bump();
        // Official LeekScript / Java compiler: `include` only accepts `include("path")`, not
        // `include(("path"))` — extra `(` yields `AI_NAME_EXPECTED` in the JVM compiler.
        if matches!(self.peek().kind, TokenKind::ParOpen) {
            self.error(
                "AI_NAME_EXPECTED",
                self.peek().span,
                "expected string literal path in `include`",
            );
            return None;
        }
        if !matches!(self.peek().kind, TokenKind::String) {
            self.error(
                "AI_NAME_EXPECTED",
                self.peek().span,
                "expected string literal path in `include`",
            );
            return None;
        }
        let path = self.bump();
        let mut close_parens = Vec::with_capacity(1);
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)` after include path",
            );
            return None;
        }
        close_parens.push(self.bump());
        let semi = self.eat_optional_semicolon();
        Some(IncludeStmt {
            include_kw,
            open_paren,
            inner_open_parens: Vec::new(),
            path,
            close_parens,
            semi,
        })
    }

    fn parse_try_stmt(&mut self) -> Option<TryStmt> {
        let try_kw = self.bump();
        let try_body = self.parse_block()?;
        let mut catch = None;
        let mut finally_block = None;
        if matches!(self.peek().kind, TokenKind::Kw(Kw::Catch)) {
            let catch_kw = self.bump();
            if !matches!(self.peek().kind, TokenKind::ParOpen) {
                self.error(
                    "OPENING_PARENTHESIS_EXPECTED",
                    self.peek().span,
                    "expected `(` after `catch`",
                );
                return None;
            }
            let open = self.bump();
            if !matches!(self.peek().kind, TokenKind::Ident) {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected catch parameter name",
                );
                return None;
            }
            let param = self.bump();
            if !matches!(self.peek().kind, TokenKind::ParClose) {
                self.error(
                    "CLOSING_PARENTHESIS_EXPECTED",
                    self.peek().span,
                    "expected `)` after catch parameter",
                );
                return None;
            }
            let close = self.bump();
            let body = self.parse_block()?;
            catch = Some(CatchClause {
                catch_kw,
                open,
                param,
                close,
                body,
            });
        }
        if matches!(self.peek().kind, TokenKind::Kw(Kw::Finally)) {
            let finally_kw = self.bump();
            let fb = self.parse_block()?;
            finally_block = Some((finally_kw, fb));
        }
        if catch.is_none() && finally_block.is_none() {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `catch` or `finally` after `try` block",
            );
            return None;
        }
        Some(TryStmt {
            try_kw,
            try_body,
            catch,
            finally_block,
        })
    }

    fn parse_throw_stmt(&mut self) -> Option<ThrowStmt> {
        let throw_kw = self.bump();
        let value = if matches!(self.peek().kind, TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expr(0)?)
        };
        let semi = self.eat_optional_semicolon();
        Some(ThrowStmt {
            throw_kw,
            value,
            semi,
        })
    }

    fn parse_class_decl(&mut self) -> Option<ClassDecl> {
        let class_kw = self.bump();
        if !matches!(self.peek().kind, TokenKind::Ident) {
            self.error("UNEXPECTED_TOKEN", self.peek().span, "expected class name");
            return None;
        }
        let name = self.bump();
        let extends = if matches!(self.peek().kind, TokenKind::Kw(Kw::Extends)) {
            let extends_kw = self.bump();
            if !matches!(self.peek().kind, TokenKind::Ident) {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected superclass name after `extends`",
                );
                return None;
            }
            let super_name = self.bump();
            Some((extends_kw, super_name))
        } else {
            None
        };
        if !matches!(self.peek().kind, TokenKind::BraceOpen) {
            self.error("UNEXPECTED_TOKEN", self.peek().span, "expected `{` after class name");
            return None;
        }
        let open_brace = self.bump();
        let mut members = Vec::new();
        while !matches!(self.peek().kind, TokenKind::BraceClose) {
            if self.at_eof() {
                self.error("END_OF_SCRIPT_UNEXPECTED", self.peek().span, "expected `}`");
                return None;
            }
            self.type_gt_slack = 0;
            let member_modifiers = self.parse_class_member_modifiers();
            match &self.peek().kind {
                TokenKind::Kw(Kw::Constructor) => {
                    let ctor = self.parse_constructor_decl(member_modifiers)?;
                    members.push(ClassMember::Constructor(ctor));
                }
                TokenKind::Kw(Kw::Function) => {
                    // v1–2: `function` matches case-insensitively, so `Function<…>` is a **generic type**, not `function` method syntax.
                    let next_is_type_param = self
                        .tokens
                        .get(self.cur + 1)
                        .is_some_and(|t| matches!(t.kind, TokenKind::Operator) && self.text_tok(self.cur + 1) == "<");
                    let next_is_function_decl = self
                        .tokens
                        .get(self.cur + 1)
                        .is_some_and(|t| matches!(t.kind, TokenKind::Ident))
                        && self
                            .tokens
                            .get(self.cur + 2)
                            .is_some_and(|t| matches!(t.kind, TokenKind::ParOpen));
                    if next_is_type_param || !next_is_function_decl {
                        let m = self.parse_class_field_or_typed_method(member_modifiers)?;
                        members.push(m);
                    } else {
                        let f = self.parse_function_decl(member_modifiers)?;
                        members.push(ClassMember::Method(f));
                    }
                }
                _ => {
                    let m = self.parse_class_field_or_typed_method(member_modifiers)?;
                    members.push(m);
                }
            }
        }
        let close_brace = self.bump();
        Some(ClassDecl {
            class_kw,
            name,
            extends,
            open_brace,
            members,
            close_brace,
        })
    }

    fn parse_class_member_modifiers(&mut self) -> Vec<usize> {
        let mut out = Vec::new();
        loop {
            match &self.peek().kind {
                TokenKind::Kw(Kw::Private)
                | TokenKind::Kw(Kw::Public)
                | TokenKind::Kw(Kw::Protected)
                | TokenKind::Kw(Kw::Static)
                | TokenKind::Kw(Kw::Final) => out.push(self.bump()),
                _ => break,
            }
        }
        out
    }

    /// After optional modifiers, a type and name: field (optional `=` init), or `name(` method.
    fn parse_class_field_or_typed_method(
        &mut self,
        member_modifiers: Vec<usize>,
    ) -> Option<ClassMember> {
        let save = self.cur;
        let type_tokens = match self.try_parse_type_expression_union() {
            Ok(t) => t,
            Err(()) => {
                self.cur = save;
                // `static foo(...)` — omitted / `void` return type (Java `eatReturnType` empty).
                Vec::new()
            }
        };
        // Java allows omitted return type; method names can collide with type keywords (e.g. `string()`).
        // If the "type" parse consumed exactly one token and we're now looking at `(`, treat that token
        // as the method name and clear the return type.
        let mut type_tokens = type_tokens;
        let name = if matches!(self.peek().kind, TokenKind::Ident) {
            self.bump()
        } else if matches!(self.peek().kind, TokenKind::ParOpen)
            && type_tokens.len() == 1
            && matches!(self.tokens[type_tokens[0]].kind, TokenKind::Ident | TokenKind::Kw(_))
        {
            let n = type_tokens[0];
            type_tokens.clear();
            n
        } else {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected field or method name after type",
            );
            return None;
        };
        if matches!(self.peek().kind, TokenKind::ParOpen) {
            let open_paren = self.bump();
            let (params, param_type_tokens, param_defaults, param_commas, param_at) =
                self.parse_param_list_inner()?;
            if !matches!(self.peek().kind, TokenKind::ParClose) {
                self.error(
                    "CLOSING_PARENTHESIS_EXPECTED",
                    self.peek().span,
                    "expected `)`",
                );
                return None;
            }
            let close_paren = self.bump();
            let arrow_return = if matches!(self.peek().kind, TokenKind::Arrow) {
                let arrow = self.bump();
                let rt = match self.try_parse_type_expression_union() {
                    Ok(t) if !t.is_empty() => t,
                    Ok(_) => {
                        self.error(
                            "UNEXPECTED_TOKEN",
                            self.peek().span,
                            "expected return type after `=>`",
                        );
                        return None;
                    }
                    Err(()) => {
                        self.error(
                            "UNEXPECTED_TOKEN",
                            self.peek().span,
                            "expected return type after `=>`",
                        );
                        return None;
                    }
                };
                Some((arrow, rt))
            } else {
                None
            };
            let body = if matches!(self.peek().kind, TokenKind::Semicolon) {
                FunctionBody::SignatureStub { semi: self.bump() }
            } else {
                FunctionBody::Block(self.parse_block()?)
            };
            return Some(ClassMember::Method(FunctionDecl {
                member_modifiers,
                function_kw: None,
                return_type_tokens: type_tokens,
                name,
                open_paren,
                params,
                param_type_tokens,
                param_defaults,
                param_commas,
                param_at,
                close_paren,
                arrow_return,
                body,
            }));
        }
        let init = if matches!(self.peek().kind, TokenKind::Operator) && self.text_tok(self.cur) == "="
        {
            let eq = self.bump();
            let value = self.parse_expr(0)?;
            Some(ClassFieldInit { eq, value })
        } else {
            None
        };
        let semi = self.eat_optional_semicolon();
        Some(ClassMember::Field(ClassFieldDecl {
            modifiers: member_modifiers,
            type_tokens,
            name,
            init,
            semi,
        }))
    }

    fn parse_constructor_decl(&mut self, member_modifiers: Vec<usize>) -> Option<ConstructorDecl> {
        let constructor_kw = self.bump();
        if !matches!(self.peek().kind, TokenKind::ParOpen) {
            self.error(
                "OPENING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `(` after `constructor`",
            );
            return None;
        }
        let open_paren = self.bump();
        let (params, param_type_tokens, param_defaults, param_commas, param_at) =
            self.parse_param_list_inner()?;
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)` after constructor parameter list",
            );
            return None;
        }
        let close_paren = self.bump();
        let body = self.parse_block()?;
        Some(ConstructorDecl {
            member_modifiers,
            constructor_kw,
            open_paren,
            params,
            param_type_tokens,
            param_defaults,
            param_commas,
            param_at,
            close_paren,
            body,
        })
    }

    fn parse_if_stmt(&mut self) -> Option<IfStmt> {
        let if_kw = self.bump();
        if !matches!(self.peek().kind, TokenKind::ParOpen) {
            self.error(
                "OPENING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `(` after `if`",
            );
            return None;
        }
        let open_paren = self.bump();
        let cond = self.parse_expr(0)?;
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)`",
            );
            return None;
        }
        let close_paren = self.bump();
        let then_body = self.parse_stmt_body()?;
        let (else_kw, else_branch) = if matches!(self.peek().kind, TokenKind::Kw(Kw::Else)) {
            let ek = self.bump();
            match &self.peek().kind {
                TokenKind::BraceOpen => {
                    let b = self.parse_block()?;
                    (Some(ek), Some(ElseBranch::Body(StmtBody::Block(b))))
                }
                TokenKind::Kw(Kw::If) => {
                    let inner = self.parse_if_stmt()?;
                    (Some(ek), Some(ElseBranch::If(Box::new(inner))))
                }
                _ => {
                    let b = self.parse_stmt_body()?;
                    (Some(ek), Some(ElseBranch::Body(b)))
                }
            }
        } else {
            (None, None)
        };
        Some(IfStmt {
            if_kw,
            open_paren,
            cond,
            close_paren,
            then_body,
            else_kw,
            else_branch,
        })
    }

    fn parse_stmt_body(&mut self) -> Option<StmtBody> {
        match &self.peek().kind {
            TokenKind::BraceOpen => self.parse_block().map(StmtBody::Block),
            _ => self.parse_stmt().map(|s| StmtBody::Single(Box::new(s))),
        }
    }

    fn parse_while_stmt(&mut self) -> Option<WhileStmt> {
        let while_kw = self.bump();
        if !matches!(self.peek().kind, TokenKind::ParOpen) {
            self.error(
                "OPENING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `(` after `while`",
            );
            return None;
        }
        let open_paren = self.bump();
        let cond = self.parse_expr(0)?;
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)`",
            );
            return None;
        }
        let close_paren = self.bump();
        let body = self.parse_stmt_body()?;
        Some(WhileStmt {
            while_kw,
            open_paren,
            cond,
            close_paren,
            body,
        })
    }

    fn parse_do_while_stmt(&mut self) -> Option<DoWhileStmt> {
        let do_kw = self.bump();
        let body = self.parse_block()?;
        if !matches!(self.peek().kind, TokenKind::Kw(Kw::While)) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `while` after `do` block",
            );
            return None;
        }
        let while_kw = self.bump();
        if !matches!(self.peek().kind, TokenKind::ParOpen) {
            self.error(
                "OPENING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `(` after `while`",
            );
            return None;
        }
        let open_paren = self.bump();
        let cond = self.parse_expr(0)?;
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)`",
            );
            return None;
        }
        let close_paren = self.bump();
        // Java Leek accepts `do { ... } while (cond)` without a trailing `;` (test suite fixture).
        // Keep parsing even when the semicolon is omitted; the formatter/emitter may reinsert it.
        let semi = if matches!(self.peek().kind, TokenKind::Semicolon) {
            self.bump()
        } else {
            close_paren
        };
        Some(DoWhileStmt {
            do_kw,
            body,
            while_kw,
            open_paren,
            cond,
            close_paren,
            semi,
        })
    }

    /// Statements inside a `switch` arm until `case`, `default`, or `}`.
    fn parse_switch_inner_stmts(&mut self) -> Vec<Stmt> {
        let mut out = Vec::new();
        while !self.at_eof() {
            match &self.peek().kind {
                TokenKind::BraceClose => break,
                TokenKind::Kw(Kw::Case) | TokenKind::Kw(Kw::Default) => break,
                _ => {}
            }
            match self.parse_stmt() {
                Some(s) => out.push(s),
                None => {
                    if self.errors.is_empty() {
                        self.error("UNEXPECTED_TOKEN", self.peek().span, "expected a statement");
                    }
                    if !self.at_eof() && !matches!(self.peek().kind, TokenKind::BraceClose) {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
        }
        out
    }

    fn parse_switch_stmt(&mut self) -> Option<SwitchStmt> {
        let switch_kw = self.bump();
        if !matches!(self.peek().kind, TokenKind::ParOpen) {
            self.error(
                "OPENING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `(` after `switch`",
            );
            return None;
        }
        let open_paren = self.bump();
        let discr = self.parse_expr(0)?;
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)`",
            );
            return None;
        }
        let close_paren = self.bump();
        if !matches!(self.peek().kind, TokenKind::BraceOpen) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `{` after `switch` `(` … `)`",
            );
            return None;
        }
        let open_brace = self.bump();
        let mut clauses: Vec<SwitchClause> = Vec::new();
        let mut saw_default = false;
        while !matches!(self.peek().kind, TokenKind::BraceClose) && !self.at_eof() {
            match &self.peek().kind {
                TokenKind::Kw(Kw::Case) => {
                    let mut labels: Vec<CaseLabel> = Vec::new();
                    while matches!(self.peek().kind, TokenKind::Kw(Kw::Case)) {
                        let case_kw = self.bump();
                        let value = self.parse_expr(0)?;
                        if !(matches!(self.peek().kind, TokenKind::Operator)
                            && self.text_tok(self.cur) == ":")
                        {
                            self.error(
                                "UNEXPECTED_TOKEN",
                                self.peek().span,
                                "expected `:` after `case` expression",
                            );
                            return None;
                        }
                        let colon = self.bump();
                        labels.push(CaseLabel {
                            case_kw,
                            value,
                            colon,
                        });
                    }
                    let body = self.parse_switch_inner_stmts();
                    clauses.push(SwitchClause::Case { labels, body });
                }
                TokenKind::Kw(Kw::Default) => {
                    if saw_default {
                        self.error(
                            "UNEXPECTED_TOKEN",
                            self.peek().span,
                            "duplicate `default` in `switch`",
                        );
                        return None;
                    }
                    saw_default = true;
                    let default_kw = self.bump();
                    if !(matches!(self.peek().kind, TokenKind::Operator)
                        && self.text_tok(self.cur) == ":")
                    {
                        self.error(
                            "UNEXPECTED_TOKEN",
                            self.peek().span,
                            "expected `:` after `default`",
                        );
                        return None;
                    }
                    let colon = self.bump();
                    let body = self.parse_switch_inner_stmts();
                    clauses.push(SwitchClause::Default {
                        default_kw,
                        colon,
                        body,
                    });
                }
                _ => {
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expected `case`, `default`, or `}` in `switch`",
                    );
                    return None;
                }
            }
        }
        if !matches!(self.peek().kind, TokenKind::BraceClose) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `}` to close `switch`",
            );
            return None;
        }
        let close_brace = self.bump();
        Some(SwitchStmt {
            switch_kw,
            open_paren,
            discr,
            close_paren,
            open_brace,
            clauses,
            close_brace,
        })
    }

    fn parse_for_assign_in_header(&mut self) -> Option<ForAssign> {
        if !matches!(self.peek().kind, TokenKind::Ident) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected identifier in for-loop assign clause",
            );
            return None;
        }
        let name = self.bump();
        if !matches!(self.peek().kind, TokenKind::Operator) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected assignment operator",
            );
            return None;
        }
        let op_txt = self.text_tok(self.cur);
        if !is_compound_assign_op(op_txt) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected assignment operator",
            );
            return None;
        }
        let op = self.bump();
        let value = self.parse_expr(0)?;
        Some(ForAssign { name, op, value })
    }

    /// `for` `(` … `;` … `;` **update** `)` — assignment form or any expression (`i++`, …).
    fn parse_for_update_clause(&mut self) -> Option<ForUpdate> {
        let save = self.cur;
        let save_slack = self.type_gt_slack;
        self.type_gt_slack = 0;
        if matches!(self.peek().kind, TokenKind::Ident) && self.cur + 1 < self.tokens.len() {
            if let TokenKind::Operator = self.tokens[self.cur + 1].kind {
                let op_txt = self.text_tok(self.cur + 1);
                if is_compound_assign_op(op_txt) {
                    let name = self.bump();
                    let op = self.bump();
                    let value = self.parse_expr(0)?;
                    return Some(ForUpdate::Assign(ForAssign { name, op, value }));
                }
            }
        }
        self.cur = save;
        self.type_gt_slack = save_slack;
        Some(ForUpdate::Expr(self.parse_expr(0)?))
    }

    fn peek_op_text(&self, op: &str) -> bool {
        matches!(self.peek().kind, TokenKind::Operator) && self.text_tok(self.cur).trim() == op
    }

    fn optional_at_tok(&mut self) -> Option<usize> {
        if self.peek_op_text("@") {
            Some(self.bump())
        } else {
            None
        }
    }

    fn next_tok_is_in_or_colon_after_ident(&self) -> bool {
        if !matches!(self.peek().kind, TokenKind::Ident) {
            return false;
        }
        let n = self.cur + 1;
        n < self.tokens.len()
            && match &self.tokens[n].kind {
                TokenKind::Kw(Kw::In) => true,
                TokenKind::Operator => self.text_tok(n) == ":",
                _ => false,
            }
    }

    fn next_tok_is_eq_after_ident(&self) -> bool {
        if !matches!(self.peek().kind, TokenKind::Ident) {
            return false;
        }
        let n = self.cur + 1;
        n < self.tokens.len()
            && matches!(&self.tokens[n].kind, TokenKind::Operator)
            && self.text_tok(n) == "="
    }

    fn expr_is_string_leaf(&self, e: &Expr) -> bool {
        matches!(e, Expr::Leaf(i) if self.tokens[*i].kind == TokenKind::String)
    }

    fn simple_type_word(w: &str) -> bool {
        matches!(
            w,
            "boolean" | "any" | "integer" | "real" | "string" | "Class" | "Object"
        )
    }

    fn user_class_type_follows(&self) -> bool {
        if !matches!(self.peek().kind, TokenKind::Ident) {
            return false;
        }
        let n = self.cur + 1;
        if n >= self.tokens.len() {
            return false;
        }
        match &self.tokens[n].kind {
            TokenKind::Kw(Kw::Var) => true,
            TokenKind::Ident => {
                // `name part` in a class body are two untyped fields, not user-type `name` + field `part`.
                let type_word = self.text_tok(self.cur);
                let next_word = self.text_tok(n);
                let type_lc = type_word
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase());
                let next_lc = next_word
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase());
                !(type_lc && next_lc)
            }
            // `Function<Item, Cell, boolean => R>` — class names as type arguments are comma-separated.
            TokenKind::Comma => true,
            TokenKind::Operator => {
                let t = self.text_tok(n);
                // `Array<Cell>`, `T|U`, trailing `>` / `>>` closes generics (`>>` token).
                // Not `)`: that follows parameter *names* (`a)`).
                t == "@"
                    || t == "<"
                    || t == "|"
                    || t == "?"
                    || t == ">"
                    || t == ">>"
                    || t == ">>>"
            }
            _ => false,
        }
    }

    fn ident_followed_by_lt(&self) -> bool {
        if !matches!(self.peek().kind, TokenKind::Ident) {
            return false;
        }
        let n = self.cur + 1;
        n < self.tokens.len()
            && matches!(self.tokens[n].kind, TokenKind::Operator)
            && self.text_tok(n) == "<"
    }

    fn parse_optional_for_in_type(&mut self) -> Option<Vec<usize>> {
        let save = self.cur;
        let save_slack = self.type_gt_slack;
        self.type_gt_slack = 0;
        match self.try_parse_type_expression_union() {
            Ok(v) => {
                self.type_gt_slack = save_slack;
                Some(v)
            }
            Err(()) => {
                self.cur = save;
                self.type_gt_slack = save_slack;
                None
            }
        }
    }

    fn try_parse_type_expression_union_ast(&mut self) -> Result<(TypeExpr, Vec<usize>), ()> {
        let (ty, mut toks) = self.try_parse_type_nullable_ast()?;
        let mut rest = Vec::new();
        while self.peek_op_text("|") {
            let pipe = self.bump();
            let (rhs, rhs_toks) = self.try_parse_type_nullable_ast()?;
            toks.push(pipe);
            toks.extend(rhs_toks);
            rest.push((pipe, rhs));
        }
        if rest.is_empty() {
            Ok((ty, toks))
        } else {
            Ok((
                TypeExpr::Union {
                    first: Box::new(ty),
                    rest,
                },
                toks,
            ))
        }
    }

    fn try_parse_type_nullable_ast(&mut self) -> Result<(TypeExpr, Vec<usize>), ()> {
        let (mut ty, mut toks) = self.try_parse_type_primary_ast()?;
        if self.peek_op_text("?") {
            let q = self.bump();
            toks.push(q);
            ty = TypeExpr::Nullable {
                inner: Box::new(ty),
                question: q,
            };
        }
        Ok((ty, toks))
    }

    fn try_parse_type_primary_ast(&mut self) -> Result<(TypeExpr, Vec<usize>), ()> {
        if matches!(self.peek().kind, TokenKind::Kw(Kw::Void)) {
            let v = self.bump();
            return Ok((TypeExpr::Named { name: v }, vec![v]));
        }
        if matches!(self.peek().kind, TokenKind::Kw(Kw::Null)) {
            let v = self.bump();
            return Ok((TypeExpr::Named { name: v }, vec![v]));
        }
        // v1–v2: `Function<...>` can lex as `Kw(Function)` (case-insensitive keyword); still a type.
        if matches!(self.peek().kind, TokenKind::Kw(Kw::Function)) {
            let base = self.bump();
            let mut toks = vec![base];
            if !self.peek_op_text("<") {
                return Ok((TypeExpr::Named { name: base }, toks));
            }
            let lt = self.bump();
            toks.push(lt);
            let mut args = Vec::new();
            let mut commas = Vec::new();
            let mut arrow_ret: Option<(usize, Box<TypeExpr>)> = None;
            loop {
                // `Function< => R>` — nullary function type.
                if matches!(self.peek().kind, TokenKind::Arrow) {
                    let arrow = self.bump();
                    toks.push(arrow);
                    let (ret, ret_toks) = self.try_parse_type_expression_union_ast()?;
                    toks.extend(ret_toks);
                    arrow_ret = Some((arrow, Box::new(ret)));
                    break;
                }
                let (arg, arg_toks) = self.try_parse_type_expression_union_ast()?;
                toks.extend(arg_toks);
                args.push(arg);
                if matches!(self.peek().kind, TokenKind::Comma) {
                    let c = self.bump();
                    toks.push(c);
                    commas.push(c);
                    continue;
                }
                if matches!(self.peek().kind, TokenKind::Arrow) {
                    let arrow = self.bump();
                    toks.push(arrow);
                    let (ret, ret_toks) = self.try_parse_type_expression_union_ast()?;
                    toks.extend(ret_toks);
                    arrow_ret = Some((arrow, Box::new(ret)));
                }
                break;
            }
            let gt = match self.eat_type_gt_close()? {
                Some(i) => {
                    toks.push(i);
                    i
                }
                None => self.type_gt_close_last.ok_or(())?,
            };
            return Ok((
                TypeExpr::Generic {
                    base,
                    lt,
                    args,
                    commas,
                    arrow_ret,
                    gt,
                },
                toks,
            ));
        }
        if !matches!(self.peek().kind, TokenKind::Ident) {
            return Err(());
        }
        let base = self.bump();
        let mut toks = vec![base];

        if !self.peek_op_text("<") {
            return Ok((TypeExpr::Named { name: base }, toks));
        }

        let lt = self.bump();
        toks.push(lt);

        let mut args = Vec::new();
        let mut commas = Vec::new();
        let mut arrow_ret: Option<(usize, Box<TypeExpr>)> = None;

        // `Function< => R>` — nullary function type (no parameter type before `=>`).
        if matches!(self.peek().kind, TokenKind::Arrow)
            && self.text_tok(base).eq_ignore_ascii_case("Function")
        {
            let arrow = self.bump();
            toks.push(arrow);
            let (ret, ret_toks) = self.try_parse_type_expression_union_ast()?;
            toks.extend(ret_toks);
            arrow_ret = Some((arrow, Box::new(ret)));
        } else {
        loop {
            let (arg, arg_toks) = self.try_parse_type_expression_union_ast()?;
            toks.extend(arg_toks);
            args.push(arg);

            if matches!(self.peek().kind, TokenKind::Comma) {
                let c = self.bump();
                toks.push(c);
                commas.push(c);
                continue;
            }
            if matches!(self.peek().kind, TokenKind::Arrow) {
                let arrow = self.bump();
                toks.push(arrow);
                let (ret, ret_toks) = self.try_parse_type_expression_union_ast()?;
                toks.extend(ret_toks);
                arrow_ret = Some((arrow, Box::new(ret)));
            }
            break;
        }
        }

        let gt = match self.eat_type_gt_close()? {
            Some(i) => {
                toks.push(i);
                i
            }
            None => self.type_gt_close_last.ok_or(())?,
        };

        Ok((
            TypeExpr::Generic {
                base,
                lt,
                args,
                commas,
                arrow_ret,
                gt,
            },
            toks,
        ))
    }

    fn try_parse_type_expression_union(&mut self) -> Result<Vec<usize>, ()> {
        let mut out = self.try_parse_type_nullable()?;
        while self.peek_op_text("|") {
            out.push(self.bump());
            out.extend(self.try_parse_type_nullable()?);
        }
        Ok(out)
    }

    fn try_parse_type_nullable(&mut self) -> Result<Vec<usize>, ()> {
        let mut out = self.try_parse_type_primary()?;
        if self.peek_op_text("?") {
            out.push(self.bump());
        }
        Ok(out)
    }

    fn try_parse_type_primary(&mut self) -> Result<Vec<usize>, ()> {
        if matches!(self.peek().kind, TokenKind::Kw(Kw::Void)) {
            return Ok(vec![self.bump()]);
        }
        if matches!(self.peek().kind, TokenKind::Kw(Kw::Null)) {
            return Ok(vec![self.bump()]);
        }
        // v1–2: `function` is case-insensitive, so `Function<…>` lexes as `Kw(Function)`; still a generic function type.
        if matches!(self.peek().kind, TokenKind::Kw(Kw::Function)) {
            return self.parse_function_type();
        }
        if matches!(self.peek().kind, TokenKind::Ident) {
            let w = self.text_tok(self.cur);
            match w {
                "Array" | "Set" => return self.parse_array_or_set_type(),
                "Map" => return self.parse_map_type(),
                "Function" => return self.parse_function_type(),
                _ => {}
            }
            if Self::simple_type_word(w) {
                return Ok(vec![self.bump()]);
            }
            if self.ident_followed_by_lt() {
                return self.parse_generic_class_type();
            }
            if self.user_class_type_follows() {
                return Ok(vec![self.bump()]);
            }
            return Err(());
        }
        Err(())
    }

    fn parse_generic_class_type(&mut self) -> Result<Vec<usize>, ()> {
        let mut v = vec![self.bump()];
        if !self.peek_op_text("<") {
            return Err(());
        }
        v.push(self.bump());
        v.extend(self.try_parse_type_expression_union()?);
        if let Some(i) = self.eat_type_gt_close()? {
            v.push(i);
        }
        Ok(v)
    }

    fn parse_array_or_set_type(&mut self) -> Result<Vec<usize>, ()> {
        let mut v = vec![self.bump()];
        if self.peek_op_text("<") {
            v.push(self.bump());
            v.extend(self.try_parse_type_expression_union()?);
            if let Some(i) = self.eat_type_gt_close()? {
                v.push(i);
            }
        }
        Ok(v)
    }

    fn parse_map_type(&mut self) -> Result<Vec<usize>, ()> {
        let mut v = vec![self.bump()];
        if !self.peek_op_text("<") {
            return Ok(v);
        }
        v.push(self.bump());
        v.extend(self.try_parse_type_expression_union()?);
        if !matches!(self.peek().kind, TokenKind::Comma) {
            return Err(());
        }
        v.push(self.bump());
        v.extend(self.try_parse_type_expression_union()?);
        if let Some(i) = self.eat_type_gt_close()? {
            v.push(i);
        }
        Ok(v)
    }

    fn parse_function_type(&mut self) -> Result<Vec<usize>, ()> {
        let mut v = vec![self.bump()];
        if !self.peek_op_text("<") {
            return Ok(v);
        }
        v.push(self.bump());
        loop {
            // `Function< => R>` — nullary function type (no parameter type before `=>`).
            if matches!(self.peek().kind, TokenKind::Arrow) {
                v.push(self.bump());
                v.extend(self.try_parse_type_expression_union()?);
                break;
            }
            v.extend(self.try_parse_type_expression_union()?);
            if matches!(self.peek().kind, TokenKind::Comma) {
                v.push(self.bump());
                continue;
            }
            if matches!(self.peek().kind, TokenKind::Arrow) {
                v.push(self.bump());
                v.extend(self.try_parse_type_expression_union()?);
            }
            break;
        }
        if let Some(i) = self.eat_type_gt_close()? {
            v.push(i);
        }
        Ok(v)
    }

    /// After optional type: optional `var`, optional `@`, identifier (Java `WordCompiler.forBlock` order).
    fn parse_for_in_binding_after_optional_type(
        &mut self,
        type_tokens: Option<Vec<usize>>,
    ) -> Option<ForInBinding> {
        let var_kw = if matches!(self.peek().kind, TokenKind::Kw(Kw::Var)) {
            Some(self.bump())
        } else {
            None
        };
        let at_kw = self.optional_at_tok();
        if !matches!(self.peek().kind, TokenKind::Ident) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected identifier in `for` header",
            );
            return None;
        }
        let name = self.bump();
        Some(ForInBinding {
            type_tokens,
            var_kw,
            at_kw,
            name,
        })
    }

    fn finish_for_in_loop(
        &mut self,
        for_kw: usize,
        open_paren: usize,
        binding: ForInBinding,
    ) -> Option<Stmt> {
        if !matches!(self.peek().kind, TokenKind::Kw(Kw::In)) {
            self.error(
                "KEYWORD_IN_EXPECTED",
                self.peek().span,
                "expected `in` after `for` header binding",
            );
            return None;
        }
        let in_kw = self.bump();
        let container = self.parse_expr(0)?;
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)` after `for` `in` expression",
            );
            return None;
        }
        let close_paren = self.bump();
        let body = self.parse_stmt_body()?;
        Some(Stmt::ForIn(ForInStmt {
            for_kw,
            open_paren,
            binding,
            in_kw,
            container,
            close_paren,
            body,
        }))
    }

    fn finish_for_in_key_value(
        &mut self,
        for_kw: usize,
        open_paren: usize,
        key: ForInBinding,
        colon: usize,
        value: ForInBinding,
    ) -> Option<Stmt> {
        if !matches!(self.peek().kind, TokenKind::Kw(Kw::In)) {
            self.error(
                "KEYWORD_IN_EXPECTED",
                self.peek().span,
                "expected `in` after `for` key `: value`",
            );
            return None;
        }
        let in_kw = self.bump();
        let container = self.parse_expr(0)?;
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)` after `for` `in` expression",
            );
            return None;
        }
        let close_paren = self.bump();
        let body = self.parse_stmt_body()?;
        Some(Stmt::ForInKeyValue(ForInKeyValueStmt {
            for_kw,
            open_paren,
            key,
            colon,
            value,
            in_kw,
            container,
            close_paren,
            body,
        }))
    }

    fn parse_after_for_in_first_binding(
        &mut self,
        for_kw: usize,
        open_paren: usize,
        binding: ForInBinding,
    ) -> Option<Stmt> {
        if matches!(self.peek().kind, TokenKind::Kw(Kw::In)) {
            return self.finish_for_in_loop(for_kw, open_paren, binding);
        }
        if self.peek_op_text(":") {
            let colon = self.bump();
            let vtype = self.parse_optional_for_in_type();
            let value = self.parse_for_in_binding_after_optional_type(vtype)?;
            return self.finish_for_in_key_value(for_kw, open_paren, binding, colon, value);
        }
        if self.peek_op_text("=") {
            if binding.at_kw.is_some() {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "unexpected `@` in `for` C-style header",
                );
                return None;
            }
            if let Some(var_kw) = binding.var_kw {
                let eq = self.bump();
                let init_expr = self.parse_expr(0)?;
                let init = Some(ForInit::Var(VarDeclFor {
                    type_tokens: binding.type_tokens,
                    var_kw: Some(var_kw),
                    name: binding.name,
                    eq,
                    init: init_expr,
                }));
                return self
                    .parse_for_c_style_after_init(for_kw, open_paren, init)
                    .map(Stmt::For);
            }
            self.cur = binding.name;
            let init = Some(ForInit::Assign(self.parse_for_assign_in_header()?));
            return self
                .parse_for_c_style_after_init(for_kw, open_paren, init)
                .map(Stmt::For);
        }
        self.error(
            "UNEXPECTED_TOKEN",
            self.peek().span,
            "expected `in`, `:`, or `=` after `for` header binding",
        );
        None
    }

    /// `for` `(` C-style `)` body or iterator `for` `(` binding `in` expr `)` body.
    fn parse_for_stmt(&mut self) -> Option<Stmt> {
        let for_kw = self.bump();
        if !matches!(self.peek().kind, TokenKind::ParOpen) {
            self.error(
                "OPENING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `(` after `for`",
            );
            return None;
        }
        let open_paren = self.bump();
        let checkpoint = self.cur;
        let opt_type = self.parse_optional_for_in_type();

        if matches!(self.peek().kind, TokenKind::Kw(Kw::Var)) {
            let var_kw = self.bump();
            let at_kw = self.optional_at_tok();
            let name = if matches!(self.peek().kind, TokenKind::Ident) {
                self.bump()
            } else {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected identifier after `var` in `for` header",
                );
                return None;
            };
            let binding = ForInBinding {
                type_tokens: opt_type,
                var_kw: Some(var_kw),
                at_kw,
                name,
            };
            return self.parse_after_for_in_first_binding(for_kw, open_paren, binding);
        }

        if self.peek_op_text("@") {
            let at_kw = self.optional_at_tok();
            let name = if matches!(self.peek().kind, TokenKind::Ident) {
                self.bump()
            } else {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected identifier after `@` in `for` header",
                );
                return None;
            };
            let binding = ForInBinding {
                type_tokens: opt_type,
                var_kw: None,
                at_kw,
                name,
            };
            return self.parse_after_for_in_first_binding(for_kw, open_paren, binding);
        }

        if matches!(self.peek().kind, TokenKind::Ident)
            && self.next_tok_is_in_or_colon_after_ident()
        {
            let name = self.bump();
            let binding = ForInBinding {
                type_tokens: opt_type,
                var_kw: None,
                at_kw: None,
                name,
            };
            return self.parse_after_for_in_first_binding(for_kw, open_paren, binding);
        }

        if let Some(type_toks) = opt_type.clone() {
            if matches!(self.peek().kind, TokenKind::Ident) && self.next_tok_is_eq_after_ident() {
                let name = self.bump();
                let eq = self.bump();
                let init_expr = self.parse_expr(0)?;
                let init = Some(ForInit::Var(VarDeclFor {
                    type_tokens: Some(type_toks),
                    var_kw: None,
                    name,
                    eq,
                    init: init_expr,
                }));
                return self
                    .parse_for_c_style_after_init(for_kw, open_paren, init)
                    .map(Stmt::For);
            }
        }
        if opt_type.is_some() {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "unexpected type in `for` C-style header",
            );
            return None;
        }
        self.cur = checkpoint;

        let init = if matches!(self.peek().kind, TokenKind::Semicolon) {
            None
        } else if matches!(self.peek().kind, TokenKind::Ident) {
            Some(ForInit::Assign(self.parse_for_assign_in_header()?))
        } else {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected assignment or `;` after `(` in `for` header",
            );
            return None;
        };
        self.parse_for_c_style_after_init(for_kw, open_paren, init)
            .map(Stmt::For)
    }

    fn parse_for_c_style_after_init(
        &mut self,
        for_kw: usize,
        open_paren: usize,
        init: Option<ForInit>,
    ) -> Option<ForStmt> {
        if !matches!(self.peek().kind, TokenKind::Semicolon) {
            self.error("UNEXPECTED_TOKEN", self.peek().span, "expected `;`");
            return None;
        }
        let first_semi = self.bump();
        let cond = if matches!(self.peek().kind, TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expr(0)?)
        };
        if !matches!(self.peek().kind, TokenKind::Semicolon) {
            self.error("UNEXPECTED_TOKEN", self.peek().span, "expected `;`");
            return None;
        }
        let second_semi = self.bump();
        let update = if matches!(self.peek().kind, TokenKind::ParClose) {
            None
        } else {
            Some(self.parse_for_update_clause()?)
        };
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)`",
            );
            return None;
        }
        let close_paren = self.bump();
        let body = self.parse_stmt_body()?;
        Some(ForStmt {
            for_kw,
            open_paren,
            init,
            first_semi,
            cond,
            second_semi,
            update,
            close_paren,
            body,
        })
    }

    /// Parameter list after `(`; leaves `)` to be consumed by caller.
    fn parse_param_list_inner(
        &mut self,
    ) -> Option<(
        Vec<usize>,
        Vec<Vec<usize>>,
        Vec<Option<ParamDefault>>,
        Vec<usize>,
        Vec<Option<usize>>,
    )> {
        let mut params = Vec::new();
        let mut param_type_tokens = Vec::new();
        let mut param_defaults = Vec::new();
        let mut param_commas = Vec::new();
        let mut param_at = Vec::new();
        if matches!(self.peek().kind, TokenKind::ParClose) {
            return Some((
                params,
                param_type_tokens,
                param_defaults,
                param_commas,
                param_at,
            ));
        }
        loop {
            let checkpoint = self.cur;
            let save_slack = self.type_gt_slack;
            self.type_gt_slack = 0;
            let mut pt = match self.try_parse_type_expression_union() {
                Ok(t) => t,
                Err(()) => {
                    self.cur = checkpoint;
                    self.type_gt_slack = save_slack;
                    Vec::new()
                }
            };
            // User-class types may be followed by `,` inside `Function<A, B => R>`; the same
            // rule lets bare `a` look like a type before `,` in `(a, b)`. If we parsed a type but
            // there is no parameter name next, rewind and treat the tokens as an untyped parameter.
            if !pt.is_empty() && !matches!(self.peek().kind, TokenKind::Ident) {
                self.cur = checkpoint;
                self.type_gt_slack = save_slack;
                pt = Vec::new();
            }
            let at_tok = self.optional_at_tok();
            if !matches!(self.peek().kind, TokenKind::Ident) {
                self.error(
                    "PARAMETER_NAME_EXPECTED",
                    self.peek().span,
                    "expected parameter name",
                );
                return None;
            }
            params.push(self.bump());
            param_at.push(at_tok);
            param_type_tokens.push(pt);
            let default = if matches!(self.peek().kind, TokenKind::Operator) && self.text_tok(self.cur) == "="
            {
                let eq = self.bump();
                let value = self.parse_expr(0)?;
                Some(ParamDefault { eq, value })
            } else {
                None
            };
            param_defaults.push(default);
            if matches!(self.peek().kind, TokenKind::ParClose) {
                break;
            }
            if matches!(self.peek().kind, TokenKind::Comma) {
                param_commas.push(self.bump());
                continue;
            }
            self.error("UNEXPECTED_TOKEN", self.peek().span, "expected `,` or `)`");
            return None;
        }
        Some((
            params,
            param_type_tokens,
            param_defaults,
            param_commas,
            param_at,
        ))
    }

    /// `function` `(` … `)` (`=>` [type])? `{` … `}` — only when `function` is immediately followed by `(`.
    fn parse_function_value_expr(&mut self) -> Option<Expr> {
        let function_kw = self.bump();
        if !matches!(self.peek().kind, TokenKind::ParOpen) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `(` after `function`",
            );
            return None;
        }
        let open_paren = self.bump();
        let (params, param_type_tokens, param_defaults, param_commas, param_at) =
            self.parse_param_list_inner()?;
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)`",
            );
            return None;
        }
        let close_paren = self.bump();
        let (arrow, return_type_tokens) = if matches!(self.peek().kind, TokenKind::BraceOpen) {
            (None, Vec::new())
        } else if matches!(self.peek().kind, TokenKind::Arrow) {
            let arrow = self.bump();
            let ret_checkpoint = self.cur;
            let return_type_tokens = if matches!(self.peek().kind, TokenKind::BraceOpen) {
                Vec::new()
            } else {
                match self.try_parse_type_expression_union() {
                    Ok(t) if !t.is_empty() => t,
                    _ => {
                        self.cur = ret_checkpoint;
                        Vec::new()
                    }
                }
            };
            (Some(arrow), return_type_tokens)
        } else {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `=>` or `{` after `)` in `function` value",
            );
            return None;
        };
        if !matches!(self.peek().kind, TokenKind::BraceOpen) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `{` after `function` value header",
            );
            return None;
        }
        let body = self.parse_block()?;
        Some(Expr::FunctionValue(FunctionValueExpr {
            function_kw,
            open_paren,
            params,
            param_type_tokens,
            param_defaults,
            param_commas,
            param_at,
            close_paren,
            arrow,
            return_type_tokens,
            body,
        }))
    }

    /// After `function`, the name may be an identifier or `include` (reserved elsewhere but valid here).
    fn parse_function_name_token(&mut self) -> Option<usize> {
        match &self.peek().kind {
            TokenKind::Ident => Some(self.bump()),
            TokenKind::Kw(Kw::Include) => Some(self.bump()),
            _ => None,
        }
    }

    fn parse_function_decl(&mut self, member_modifiers: Vec<usize>) -> Option<FunctionDecl> {
        let function_kw = self.bump();
        let name = match self.parse_function_name_token() {
            Some(n) => n,
            None => {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected function name",
                );
                return None;
            }
        };
        if !matches!(self.peek().kind, TokenKind::ParOpen) {
            self.error(
                "OPENING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `(` after function name",
            );
            return None;
        }
        let open_paren = self.bump();
        let (params, param_type_tokens, param_defaults, param_commas, param_at) =
            self.parse_param_list_inner()?;
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)`",
            );
            return None;
        }
        let close_paren = self.bump();
        let arrow_return = if matches!(self.peek().kind, TokenKind::Arrow) {
            let arrow = self.bump();
            let rt = match self.try_parse_type_expression_union() {
                Ok(t) if !t.is_empty() => t,
                Ok(_) => {
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expected return type after `=>`",
                    );
                    return None;
                }
                Err(()) => {
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expected return type after `=>`",
                    );
                    return None;
                }
            };
            Some((arrow, rt))
        } else {
            None
        };
        let body = if matches!(self.peek().kind, TokenKind::Semicolon) {
            FunctionBody::SignatureStub { semi: self.bump() }
        } else {
            FunctionBody::Block(self.parse_block()?)
        };
        Some(FunctionDecl {
            member_modifiers,
            function_kw: Some(function_kw),
            return_type_tokens: Vec::new(),
            name,
            open_paren,
            params,
            param_type_tokens,
            param_defaults,
            param_commas,
            param_at,
            close_paren,
            arrow_return,
            body,
        })
    }

    fn parse_var_decl(&mut self) -> Option<VarDecl> {
        let var_kw = self.bump();
        let mut decls = Vec::new();
        let mut commas = Vec::new();
        loop {
            let name = if matches!(self.peek().kind, TokenKind::Ident) {
                self.bump()
            } else {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected identifier after `var`",
                );
                return None;
            };
            let (eq, init) = if self.is_operator_text("=") {
                let eq = self.bump();
                let init = self.parse_expr(0)?;
                (Some(eq), Some(init))
            } else {
                (None, None)
            };
            decls.push(VarDeclarator { name, eq, init });
            if matches!(self.peek().kind, TokenKind::Comma) {
                commas.push(self.bump());
                continue;
            }
            break;
        }
        let semi = self.eat_optional_semicolon();
        Some(VarDecl {
            var_kw,
            decls,
            commas,
            semi,
        })
    }

    /// `this` / `super` `.` field then `=` / `+=` / … — not a valid `return` value, so treat as ASI.
    fn lookahead_this_member_assign_after_return(&mut self) -> bool {
        let save = self.cur;
        if !matches!(
            self.peek().kind,
            TokenKind::Kw(Kw::This | Kw::Super)
        ) {
            return false;
        }
        self.bump();
        if !matches!(self.peek().kind, TokenKind::Dot) {
            self.cur = save;
            return false;
        }
        self.bump();
        if !matches!(self.peek().kind, TokenKind::Ident) {
            self.cur = save;
            return false;
        }
        self.bump();
        let ok = matches!(self.peek().kind, TokenKind::Operator)
            && {
                let t = self.text_tok(self.cur);
                t == "=" || is_compound_assign_op(t)
            };
        self.cur = save;
        ok
    }

    fn peek_starts_new_stmt_after_naked_return(&mut self) -> bool {
        match &self.peek().kind {
            TokenKind::Kw(Kw::This) | TokenKind::Kw(Kw::Super) => {
                self.lookahead_this_member_assign_after_return()
            }
            // `return function() { ... }` is one expression; `return` newline `function name(...)` is
            // a naked return plus a named `function` decl.
            TokenKind::Kw(Kw::Function) => {
                let n = self.cur + 1;
                !(n < self.tokens.len() && matches!(self.tokens[n].kind, TokenKind::ParOpen))
            }
            // `return class['a']` / `return class.x` — `class` is an expression, not `class` Name `{`.
            TokenKind::Kw(Kw::Class) => {
                let n = self.cur + 1;
                if n < self.tokens.len() {
                    match &self.tokens[n].kind {
                        TokenKind::BracketOpen | TokenKind::Dot => false,
                        TokenKind::Operator if self.text_tok(n) == "." => false,
                        _ => true,
                    }
                } else {
                    true
                }
            }
            TokenKind::Kw(k) => matches!(
                k,
                Kw::Var
                    | Kw::If
                    | Kw::While
                    | Kw::For
                    | Kw::Return
                    | Kw::Break
                    | Kw::Continue
                    | Kw::Throw
                    | Kw::Try
                    | Kw::Do
                    | Kw::Switch
                    | Kw::Global
                    | Kw::Include
                    | Kw::Const
                    | Kw::Let
            ),
            TokenKind::Ident => {
                // Disambiguate `return cond ? a : b` from `return` newline `Type? name ...`.
                // The Java suite often flattens newlines into spaces, so relying on layout here is unsafe.
                //
                // If we see `Ident ? ... : ...` at depth 0 before the end of the statement, treat it as
                // a ternary expression start, not a typed declaration.
                let n = self.cur + 1;
                if n < self.tokens.len()
                    && matches!(self.tokens[n].kind, TokenKind::Operator)
                    && self.text_tok(n) == "?"
                {
                    let mut j = n + 1;
                    let mut par = 0u32;
                    let mut bra = 0u32;
                    let mut brc = 0u32;
                    while j < self.tokens.len() {
                        match self.tokens[j].kind {
                            TokenKind::Semicolon if par == 0 && bra == 0 && brc == 0 => break,
                            TokenKind::BraceClose if par == 0 && bra == 0 && brc == 0 => break,
                            TokenKind::Eof => break,
                            TokenKind::ParOpen => par += 1,
                            TokenKind::ParClose => par = par.saturating_sub(1),
                            TokenKind::BracketOpen => bra += 1,
                            TokenKind::BracketClose => bra = bra.saturating_sub(1),
                            TokenKind::BraceOpen => brc += 1,
                            TokenKind::BraceClose => brc = brc.saturating_sub(1),
                            TokenKind::Operator
                                if par == 0
                                    && bra == 0
                                    && brc == 0
                                    && self.text_tok(j) == ":" =>
                            {
                                return false;
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                }

                // Disambiguate `return a f(...)` (value `a`, then a call statement) from a typed decl
                // `Type name ...` after a naked return. In flattened fixtures this pattern shows up
                // as `return acc push(...)`.
                if n + 1 < self.tokens.len()
                    && matches!(self.tokens[n].kind, TokenKind::Ident)
                    && matches!(self.tokens[n + 1].kind, TokenKind::ParOpen)
                {
                    return false;
                }

                let save = self.cur;
                let save_slack = self.type_gt_slack;
                self.type_gt_slack = 0;
                let is_typed = self.try_parse_typed_var_decl().is_some();
                self.cur = save;
                self.type_gt_slack = save_slack;
                is_typed
            }
            _ => false,
        }
    }

    fn parse_return(&mut self) -> Option<ReturnStmt> {
        ptrace!(self, "parse_return");
        let return_kw = self.bump();
        let optional_question = if self.is_operator_text("?") {
            Some(self.bump())
        } else {
            None
        };
        let at_kw = self.optional_at_tok();
        let value = if matches!(
            self.peek().kind,
            TokenKind::Semicolon | TokenKind::BraceClose
        ) {
            None
        } else if self.at_eof() {
            None
        } else if self.peek_starts_new_stmt_after_naked_return() {
            None
        } else {
            ptrace!(self, "parse_return: parse value expr");
            Some(self.parse_expr(0)?)
        };
        let semi = self.eat_optional_semicolon();
        Some(ReturnStmt {
            return_kw,
            optional_question,
            at_kw,
            value,
            semi,
        })
    }

    fn parse_block(&mut self) -> Option<Block> {
        if !matches!(self.peek().kind, TokenKind::BraceOpen) {
            return None;
        }
        let open = self.bump();
        let stmts = self.parse_stmt_list();
        if !matches!(self.peek().kind, TokenKind::BraceClose) {
            self.error("END_OF_SCRIPT_UNEXPECTED", self.peek().span, "expected `}`");
            return None;
        }
        let close = self.bump();
        Some(Block { open, stmts, close })
    }

    fn parse_expr_stmt(&mut self) -> Option<(Expr, Option<usize>)> {
        let e = self.parse_expr(0)?;
        let semi = self.eat_optional_semicolon();
        Some((e, semi))
    }

    fn parse_expr(&mut self, min_bp: u8) -> Option<Expr> {
        ptrace!(self, "enter parse_expr(min_bp={min_bp})");
        let mut lhs = self.parse_prefix()?;
        ptrace!(self, "after parse_prefix");
        loop {
            if self.at_eof() {
                break;
            }
            // Java suite: many statements omit `;`. Stop the current expression if the next token
            // clearly starts a new statement at top level.
            if min_bp == 0
                && matches!(
                    self.peek().kind,
                    TokenKind::Ident
                        | TokenKind::Kw(
                            Kw::Var
                                | Kw::Return
                                | Kw::Function
                                | Kw::If
                                | Kw::While
                                | Kw::Do
                                | Kw::Switch
                                | Kw::For
                                | Kw::Break
                                | Kw::Continue
                                | Kw::Try
                                | Kw::Throw
                                | Kw::Class
                                | Kw::Global
                                | Kw::Include
                                | Kw::This
                                | Kw::Super
                        )
                )
            {
                ptrace!(self, "stop expr: next token starts stmt (implicit `;` rule)");
                break;
            }
            ptrace!(self, "loop top (min_bp={min_bp})");
            // Ternary `cond ? then : else` binds looser than `||`/`??` and associates right.
            if matches!(self.peek().kind, TokenKind::Operator) && self.text_tok(self.cur) == "?" {
                const TERNARY_L_BP: u8 = 1;
                if TERNARY_L_BP < min_bp {
                    break;
                }
                ptrace!(self, "ternary: saw '?'");
                let question = self.bump();
                let then_expr = Box::new(self.parse_expr(0)?);
                if !(matches!(self.peek().kind, TokenKind::Operator) && self.text_tok(self.cur) == ":")
                {
                    ptrace!(self, "ternary: missing ':'");
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expected `:` after `?` in ternary expression",
                    );
                    return None;
                }
                let colon = self.bump();
                ptrace!(self, "ternary: consumed ':'");
                let else_expr = Box::new(self.parse_expr(TERNARY_L_BP)?);
                lhs = Expr::Ternary {
                    cond: Box::new(lhs),
                    question,
                    then_expr,
                    colon,
                    else_expr,
                };
                continue;
            }
            if matches!(self.peek().kind, TokenKind::Kw(Kw::Not)) && self.not_in_follows() {
                let l_bp = 13;
                if l_bp < min_bp {
                    break;
                }
                ptrace!(self, "parse `not in`");
                let not_kw = self.bump();
                let in_kw = self.bump();
                let rhs = self.parse_expr(14)?;
                lhs = Expr::NotIn {
                    elem: Box::new(lhs),
                    not_kw,
                    in_kw,
                    container: Box::new(rhs),
                };
                continue;
            }
            if matches!(self.peek().kind, TokenKind::Kw(Kw::As)) {
                // Cast binds very tightly in Java (tighter than `*`/`+`), so it can appear on the
                // RHS of arithmetic without forcing parentheses.
                let l_bp = 23;
                if l_bp < min_bp {
                    break;
                }
                ptrace!(self, "parse `as` cast");
                let as_kw = self.bump();
                let (ty, type_tokens) = match self.try_parse_type_expression_union_ast() {
                    Ok(v) => v,
                    Err(()) => {
                        self.error("TYPE_EXPECTED", self.peek().span, "expected type after `as`");
                        return None;
                    }
                };
                lhs = Expr::AsCast {
                    expr: Box::new(lhs),
                    as_kw,
                    ty,
                    type_tokens,
                };
                continue;
            }
            let op_bp = match &self.peek().kind {
                TokenKind::Operator => bin_binding_power(self.text_tok(self.cur)),
                TokenKind::WordOp(WordOp::Or) => Some((1, 2)),
                TokenKind::WordOp(WordOp::And) => Some((3, 4)),
                TokenKind::WordOp(WordOp::Xor) => Some((7, 8)),
                TokenKind::WordOp(WordOp::Is) => Some((11, 12)),
                TokenKind::WordOp(WordOp::Instanceof) => Some((13, 14)),
                TokenKind::Kw(Kw::In) => Some((13, 14)),
                _ => None,
            };
            let Some((l_bp, r_bp)) = op_bp else {
                ptrace!(self, "no operator binding power; break");
                break;
            };
            if l_bp < min_bp {
                ptrace!(self, "operator too loose (l_bp={l_bp} < min_bp={min_bp}); break");
                break;
            }
            // Ternary `cond ? then : else` (right-assoc). We route it through the operator loop by
            // giving `?` a binding power in `bin_binding_power`.
            if matches!(self.peek().kind, TokenKind::Operator) && self.text_tok(self.cur) == "?" {
                ptrace!(self, "ternary(loop): saw '?' (l_bp={l_bp})");
                let question = self.bump();
                let then_expr = Box::new(self.parse_expr(0)?);
                if !(matches!(self.peek().kind, TokenKind::Operator) && self.text_tok(self.cur) == ":")
                {
                    ptrace!(self, "ternary(loop): missing ':'");
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expected `:` after `?` in ternary expression",
                    );
                    return None;
                }
                let colon = self.bump();
                let else_expr = Box::new(self.parse_expr(l_bp)?);
                lhs = Expr::Ternary {
                    cond: Box::new(lhs),
                    question,
                    then_expr,
                    colon,
                    else_expr,
                };
            } else {
                ptrace!(self, "binary op (l_bp={l_bp}, r_bp={r_bp})");
                let op_i = self.bump();
                let rhs = self.parse_expr(r_bp)?;
                lhs = Expr::Binary(Box::new(lhs), op_i, Box::new(rhs));
            }
        }
        ptrace!(self, "exit parse_expr(min_bp={min_bp})");
        self.maybe_parse_trailing_assign_expr(lhs)
    }

    /// `a += rhs` / `a >>>= rhs` in expression position (arrays, `return`, rhs of `+`, …).
    fn maybe_parse_trailing_assign_expr(&mut self, lhs: Expr) -> Option<Expr> {
        if matches!(self.peek().kind, TokenKind::Operator) {
            let op_txt = self.text_tok(self.cur);
            if is_compound_assign_op(op_txt) {
                if !self.expr_is_lvalue(&lhs) {
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expression is not a valid assignment target",
                    );
                    return None;
                }
                let op_tok = self.bump();
                let value = self.parse_expr(0)?;
                return Some(Expr::AssignExpr {
                    target: Box::new(lhs),
                    op: op_tok,
                    value: Box::new(value),
                });
            }
        }
        Some(lhs)
    }

    fn parse_prefix(&mut self) -> Option<Expr> {
        // Java suite allows multi-parameter arrow closures without parens: `x, y -> x + y`.
        // This is ambiguous in some contexts (e.g. call arguments), but at the start of an
        // expression it is unambiguous enough for the suite.
        if matches!(self.peek().kind, TokenKind::Ident) {
            // Avoid interpreting call-argument separators as lambda parameter separators, e.g.
            // `f(a, x, y -> ...)` should not treat `a, x, y -> ...` as a single lambda.
            if self.cur > 0 {
                let prev = self.tokens[self.cur - 1].kind;
                let prev_is_call_paren = if matches!(prev, TokenKind::ParOpen) && self.cur >= 2 {
                    // If the `(` is preceded by an expression-ish token, it's a call argument list.
                    // Otherwise it's a grouping paren (e.g. `return (x, y -> x + y)(1, 2)`).
                    matches!(
                        self.tokens[self.cur - 2].kind,
                        TokenKind::Ident
                            | TokenKind::ParClose
                            | TokenKind::BracketClose
                            | TokenKind::BraceClose
                            | TokenKind::String
                            | TokenKind::Number
                    )
                } else {
                    false
                };
                if prev_is_call_paren || matches!(prev, TokenKind::Comma) {
                    // fall through: do not attempt the shorthand here
                } else {
            let save = self.cur;
            let mut params = Vec::new();
            let mut commas = Vec::new();
            params.push(self.bump());
            while matches!(self.peek().kind, TokenKind::Comma) {
                commas.push(self.bump());
                if !matches!(self.peek().kind, TokenKind::Ident) {
                    self.cur = save;
                    break;
                }
                params.push(self.bump());
            }
            if self.cur != save && matches!(self.peek().kind, TokenKind::Arrow) && params.len() >= 2 {
                let arrow = self.bump();
                let body = if matches!(self.peek().kind, TokenKind::BraceOpen) {
                    ArrowFnBody::Block(self.parse_block()?)
                } else {
                    ArrowFnBody::Expr(Box::new(self.parse_expr(0)?))
                };
                return Some(Expr::ArrowFn {
                    open_paren: None,
                    params,
                    param_commas: commas,
                    close_paren: None,
                    arrow,
                    body,
                });
            }
            self.cur = save;
                }
            }
        }
        if matches!(self.peek().kind, TokenKind::Kw(Kw::New)) {
            let new_kw = self.bump();
            if !matches!(self.peek().kind, TokenKind::Ident) {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected type name after `new`",
                );
                return None;
            }
            let type_name = self.bump();
            let e = self.finish_new_expr(new_kw, type_name)?;
            return self.apply_postfixes(e);
        }
        // Nullary arrow closure: `-> expr` / `-> { ... }`
        if matches!(self.peek().kind, TokenKind::Arrow) {
            let arrow = self.bump();
            let body = if matches!(self.peek().kind, TokenKind::BraceOpen) {
                ArrowFnBody::Block(self.parse_block()?)
            } else {
                ArrowFnBody::Expr(Box::new(self.parse_expr(0)?))
            };
            return self.apply_postfixes(Expr::ArrowFn {
                open_paren: None,
                params: vec![],
                param_commas: vec![],
                close_paren: None,
                arrow,
                body,
            });
        }
        if matches!(self.peek().kind, TokenKind::Operator) {
            let t = self.text_tok(self.cur);
            if t == "@" {
                let at_kw = self.bump();
                let inner = self.parse_prefix()?;
                let e = Expr::Ref {
                    at_kw,
                    expr: Box::new(inner),
                };
                return self.apply_postfixes(e);
            }
            if t == "++" || t == "--" {
                let op1 = self.bump();
                let increment = t == "++";
                let inner = self.parse_prefix()?;
                let e = Expr::PreUpdate {
                    expr: Box::new(inner),
                    increment,
                    op1,
                    op2: op1,
                };
                return self.apply_postfixes(e);
            }
            if t == "-" || t == "!" || t == "~" {
                let op = self.bump();
                let expr = Box::new(self.parse_prefix()?);
                let e = Expr::Unary { op, expr };
                return self.apply_postfixes(e);
            }
        }
        if matches!(self.peek().kind, TokenKind::Kw(Kw::Typeof)) {
            let op = self.bump();
            let expr = Box::new(self.parse_prefix()?);
            let e = Expr::Unary { op, expr };
            return self.apply_postfixes(e);
        }
        if matches!(self.peek().kind, TokenKind::Kw(Kw::Not)) {
            let op = self.bump();
            let expr = Box::new(self.parse_prefix()?);
            let e = Expr::Unary { op, expr };
            return self.apply_postfixes(e);
        }
        // Java v1–v2 anonymous function literal: `Function(...) { ... }`
        if matches!(self.peek().kind, TokenKind::Ident)
            && matches!(self.text_tok(self.cur), "Function" | "FUNCTION")
            && self
                .tokens
                .get(self.cur + 1)
                .is_some_and(|t| matches!(t.kind, TokenKind::ParOpen))
        {
            let function_kw = self.bump();
            let open_paren = self.bump();
            let (params, param_type_tokens, param_defaults, param_commas, param_at) =
                self.parse_param_list_inner()?;
            if !matches!(self.peek().kind, TokenKind::ParClose) {
                self.error(
                    "CLOSING_PARENTHESIS_EXPECTED",
                    self.peek().span,
                    "expected `)`",
                );
                return None;
            }
            let close_paren = self.bump();
            let (arrow, return_type_tokens) = if matches!(self.peek().kind, TokenKind::BraceOpen) {
                (None, Vec::new())
            } else if matches!(self.peek().kind, TokenKind::Arrow) {
                let arrow = self.bump();
                let ret_checkpoint = self.cur;
                let return_type_tokens = if matches!(self.peek().kind, TokenKind::BraceOpen) {
                    Vec::new()
                } else {
                    match self.try_parse_type_expression_union() {
                        Ok(t) if !t.is_empty() => t,
                        _ => {
                            self.cur = ret_checkpoint;
                            Vec::new()
                        }
                    }
                };
                (Some(arrow), return_type_tokens)
            } else {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected `=>` or `{` after `)` in `Function` value",
                );
                return None;
            };
            if !matches!(self.peek().kind, TokenKind::BraceOpen) {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected `{` after `Function` value header",
                );
                return None;
            }
            let body = self.parse_block()?;
            return Some(Expr::FunctionValue(FunctionValueExpr {
                function_kw,
                open_paren,
                params,
                param_type_tokens,
                param_defaults,
                param_commas,
                param_at,
                close_paren,
                arrow,
                return_type_tokens,
                body,
            }));
        }
        if matches!(self.peek().kind, TokenKind::Ident) {
            let w = self.text_tok(self.cur);
            // `Object()` is the empty-object ctor (Java Leek), not a prefix cast of `()`.
            let object_ctor = w == "Object"
                && self
                    .tokens
                    .get(self.cur + 1)
                    .is_some_and(|t| matches!(t.kind, TokenKind::ParOpen));
            if Self::simple_type_word(w)
                && self.token_starts_expr_operand(self.cur + 1)
                && !object_ctor
            {
                let ty = self.bump();
                let inner = self.parse_expr(22)?;
                let e = Expr::PrefixCast {
                    ty,
                    expr: Box::new(inner),
                };
                return self.apply_postfixes(e);
            }
        }
        let e = self.parse_primary()?;
        if let Expr::Leaf(param_tok) = &e {
            if matches!(self.tokens[*param_tok].kind, TokenKind::Ident)
                && matches!(self.peek().kind, TokenKind::Arrow)
            {
                let arrow = self.bump();
                let body = if matches!(self.peek().kind, TokenKind::BraceOpen) {
                    ArrowFnBody::Block(self.parse_block()?)
                } else {
                    ArrowFnBody::Expr(Box::new(self.parse_expr(0)?))
                };
                return self.apply_postfixes(Expr::ArrowFn {
                    open_paren: None,
                    params: vec![*param_tok],
                    param_commas: vec![],
                    close_paren: None,
                    arrow,
                    body,
                });
            }
            // NOTE: multi-parameter arrow without parens (`x, y -> ...`) is ambiguous with call
            // arguments (`f(x, y -> ...)`). Prefer requiring parens for multi-parameter arrows.
        }
        self.apply_postfixes(e)
    }

    fn apply_postfixes(&mut self, mut e: Expr) -> Option<Expr> {
        loop {
            match self.peek().kind {
                TokenKind::ParOpen => e = self.finish_call(e)?,
                TokenKind::BracketOpen => {
                    if bracket_open_closes_leek_interval(self.tokens, self.cur) {
                        break;
                    }
                    e = self.finish_index(e)?;
                }
                TokenKind::Dot => e = self.finish_member(e)?,
                TokenKind::Operator => {
                    let t = self.text_tok(self.cur);
                    if t == "++" || t == "--" {
                        let op = self.bump();
                        let increment = t == "++";
                        e = Expr::PostUpdate {
                            expr: Box::new(e),
                            increment,
                            op1: op,
                            op2: op,
                        };
                        continue;
                    }
                    if (t == "+" || t == "-") && self.cur + 1 < self.tokens.len() {
                        let t2 = self.text_tok(self.cur + 1);
                        if t2 == t {
                            let op1 = self.bump();
                            let op2 = self.bump();
                            let increment = t == "+";
                            e = Expr::PostUpdate {
                                expr: Box::new(e),
                                increment,
                                op1,
                                op2,
                            };
                            continue;
                        }
                    }
                    break;
                }
                _ => break,
            }
        }
        Some(e)
    }

    fn finish_array_slice_after_colon(
        &mut self,
        base: Expr,
        open: usize,
        colon1: usize,
        start: Option<Box<Expr>>,
    ) -> Option<Expr> {
        let (end, colon_step, step) =
            if matches!(self.peek().kind, TokenKind::BracketClose) {
                (None, None, None)
            } else if self.is_operator_text(":") {
                let colon2 = self.bump();
                if matches!(self.peek().kind, TokenKind::BracketClose) {
                    // `[::]` / `[start::]` — omitted `end` and `step` (defaults).
                    (None, Some(colon2), None)
                } else {
                    let step = Box::new(self.parse_expr(0)?);
                    (None, Some(colon2), Some(step))
                }
            } else {
                let end_e = Box::new(self.parse_expr(0)?);
                if self.is_operator_text(":") {
                    let colon2 = self.bump();
                    if matches!(self.peek().kind, TokenKind::BracketClose) {
                        (Some(end_e), Some(colon2), None)
                    } else {
                        let step = Box::new(self.parse_expr(0)?);
                        (Some(end_e), Some(colon2), Some(step))
                    }
                } else {
                    (Some(end_e), None, None)
                }
            };
        if !matches!(self.peek().kind, TokenKind::BracketClose) {
            self.error("UNEXPECTED_TOKEN", self.peek().span, "expected `]`");
            return None;
        }
        let close = self.bump();
        Some(Expr::ArraySlice {
            base: Box::new(base),
            open,
            start,
            colon: colon1,
            end,
            colon_step,
            step,
            close,
        })
    }

    fn finish_index(&mut self, base: Expr) -> Option<Expr> {
        let open = self.bump();
        if matches!(self.peek().kind, TokenKind::BracketClose) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected index or slice inside `[`",
            );
            return None;
        }
        if self.is_operator_text(":") {
            let colon = self.bump();
            return self.finish_array_slice_after_colon(base, open, colon, None);
        }
        let first = self.parse_expr(0)?;
        if self.is_operator_text(":") {
            let colon = self.bump();
            return self.finish_array_slice_after_colon(base, open, colon, Some(Box::new(first)));
        }
        if !matches!(self.peek().kind, TokenKind::BracketClose) {
            self.error("UNEXPECTED_TOKEN", self.peek().span, "expected `]`");
            return None;
        }
        let close = self.bump();
        Some(Expr::Index {
            base: Box::new(base),
            open,
            index: Box::new(first),
            close,
        })
    }

    fn finish_member(&mut self, base: Expr) -> Option<Expr> {
        let dot = self.bump();
        let field = if matches!(self.peek().kind, TokenKind::Ident) {
            self.bump()
        } else if matches!(self.peek().kind, TokenKind::Kw(Kw::Class)) {
            self.bump()
        } else if matches!(self.peek().kind, TokenKind::Kw(Kw::Super)) {
            self.bump()
        } else {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected field name after `.`",
            );
            return None;
        };
        Some(Expr::Member {
            base: Box::new(base),
            dot,
            field,
        })
    }

    fn finish_new_expr(&mut self, new_kw: usize, type_name: usize) -> Option<Expr> {
        if !matches!(self.peek().kind, TokenKind::ParOpen) {
            // Java suite allows `new A` without parentheses for nullary constructors.
            return Some(Expr::New {
                new_kw,
                type_name,
                open: None,
                args: Vec::new(),
                arg_commas: Vec::new(),
                close: None,
            });
        }
        let open = self.bump();
        let mut args = Vec::new();
        let mut arg_commas = Vec::new();
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            loop {
                args.push(self.parse_expr(0)?);
                if matches!(self.peek().kind, TokenKind::ParClose) {
                    break;
                }
                if matches!(self.peek().kind, TokenKind::Comma) {
                    arg_commas.push(Some(self.bump()));
                    continue;
                }
                if matches!(self.peek().kind, TokenKind::String) {
                    if args.last().is_some_and(|a| self.expr_is_string_leaf(a)) {
                        arg_commas.push(None);
                        continue;
                    }
                }
                self.error("UNEXPECTED_TOKEN", self.peek().span, "expected `,` or `)`");
                return None;
            }
        }
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)`",
            );
            return None;
        }
        let close = self.bump();
        Some(Expr::New {
            new_kw,
            type_name,
            open: Some(open),
            args,
            arg_commas,
            close: Some(close),
        })
    }

    /// After leading `]` of `]..` / `]expr..` interval literals.
    fn parse_interval_leading_rbracket(&mut self, left: usize) -> Option<Expr> {
        if matches!(self.peek().kind, TokenKind::DotDot) {
            let dotdot = self.bump();
            if self.at_interval_closer() {
                let close = self.bump();
                return Some(Expr::IntervalLiteral {
                    open: left,
                    min: None,
                    dotdot,
                    max: None,
                    close,
                });
            }
            let max = self.parse_expr(0)?;
            if !self.at_interval_closer() {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected `]` or `[` after interval bound",
                );
                return None;
            }
            let close = self.bump();
            return Some(Expr::IntervalLiteral {
                open: left,
                min: None,
                dotdot,
                max: Some(Box::new(max)),
                close,
            });
        }
        let min = self.parse_expr(0)?;
        if !matches!(self.peek().kind, TokenKind::DotDot) {
            self.error(
                "INTERNAL_ERROR",
                self.peek().span,
                "interval literal missing `..` after left `]`",
            );
            return None;
        }
        let dotdot = self.bump();
        if self.at_interval_closer() {
            let close = self.bump();
            return Some(Expr::IntervalLiteral {
                open: left,
                min: Some(Box::new(min)),
                dotdot,
                max: None,
                close,
            });
        }
        let max = self.parse_expr(0)?;
        if !self.at_interval_closer() {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `]` or `[` after interval",
            );
            return None;
        }
        let close = self.bump();
        Some(Expr::IntervalLiteral {
            open: left,
            min: Some(Box::new(min)),
            dotdot,
            max: Some(Box::new(max)),
            close,
        })
    }

    /// After `[` has been consumed.
    fn parse_after_bracket(&mut self, open: usize) -> Option<Expr> {
        if matches!(self.peek().kind, TokenKind::BracketClose) {
            let close = self.bump();
            return Some(Expr::ArrayLiteral {
                open,
                elements: vec![],
                commas: vec![],
                close,
            });
        }
        if self.is_operator_text(":") {
            self.bump();
            if !matches!(self.peek().kind, TokenKind::BracketClose) {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected `]` after `[:`",
                );
                return None;
            }
            let close = self.bump();
            return Some(Expr::MapLiteral {
                open,
                entries: vec![],
                commas: vec![],
                close,
            });
        }
        if matches!(self.peek().kind, TokenKind::DotDot) {
            let dotdot = self.bump();
            if self.at_interval_closer() {
                let close = self.bump();
                return Some(Expr::IntervalLiteral {
                    open,
                    min: None,
                    dotdot,
                    max: None,
                    close,
                });
            }
            let max = self.parse_expr(0)?;
            if !self.at_interval_closer() {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected `]` or `[` after interval bound",
                );
                return None;
            }
            let close = self.bump();
            return Some(Expr::IntervalLiteral {
                open,
                min: None,
                dotdot,
                max: Some(Box::new(max)),
                close,
            });
        }

        let first = self.parse_expr(0)?;
        if self.is_operator_text(":") {
            return self.finish_map_literal(open, first);
        }
        if matches!(self.peek().kind, TokenKind::DotDot) {
            let dotdot = self.bump();
            if self.at_interval_closer() {
                let close = self.bump();
                return Some(Expr::IntervalLiteral {
                    open,
                    min: Some(Box::new(first)),
                    dotdot,
                    max: None,
                    close,
                });
            }
            let second = self.parse_expr(0)?;
            if !self.at_interval_closer() {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected `]` or `[` after interval",
                );
                return None;
            }
            let close = self.bump();
            return Some(Expr::IntervalLiteral {
                open,
                min: Some(Box::new(first)),
                dotdot,
                max: Some(Box::new(second)),
                close,
            });
        }

        let mut elements = vec![first];
        let mut commas = Vec::new();
        loop {
            if matches!(self.peek().kind, TokenKind::BracketClose) {
                break;
            }
            if matches!(self.peek().kind, TokenKind::Comma) {
                commas.push(self.bump());
                if matches!(self.peek().kind, TokenKind::BracketClose) {
                    break;
                }
            }
            elements.push(self.parse_expr(0)?);
        }
        if !matches!(self.peek().kind, TokenKind::BracketClose) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `,` or `]` in array literal",
            );
            return None;
        }
        let close = self.bump();
        Some(Expr::ArrayLiteral {
            open,
            elements,
            commas,
            close,
        })
    }

    fn finish_map_literal(&mut self, open: usize, first_key: Expr) -> Option<Expr> {
        let colon0 = self.bump();
        let v0 = self.parse_expr(0)?;
        let mut entries = vec![MapEntry {
            key: first_key,
            colon: colon0,
            value: v0,
        }];
        let mut commas = Vec::new();
        while matches!(self.peek().kind, TokenKind::Comma) {
            commas.push(self.bump());
            if matches!(self.peek().kind, TokenKind::BracketClose) {
                break;
            }
            let k = self.parse_expr(0)?;
            if !self.is_operator_text(":") {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected `:` between map key and value",
                );
                return None;
            }
            let c = self.bump();
            let v = self.parse_expr(0)?;
            entries.push(MapEntry {
                key: k,
                colon: c,
                value: v,
            });
        }
        if !matches!(self.peek().kind, TokenKind::BracketClose) {
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `]` to close map literal",
            );
            return None;
        }
        let close = self.bump();
        Some(Expr::MapLiteral {
            open,
            entries,
            commas,
            close,
        })
    }

    /// Java object literal `{` prop `:` expr (`,` …)* `}` (v2+).
    fn parse_object_literal(&mut self, open: usize) -> Option<Expr> {
        let mut properties = Vec::new();
        let mut commas = Vec::new();
        while !matches!(self.peek().kind, TokenKind::BraceClose) {
            let key_idx = match self.peek().kind {
                TokenKind::Ident | TokenKind::String | TokenKind::Number => self.bump(),
                TokenKind::Kw(Kw::True | Kw::False | Kw::Null) => self.bump(),
                _ => {
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expected property name (`ident`, string, number, or true/false/null)",
                    );
                    return None;
                }
            };
            if !self.is_operator_text(":") {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "expected `:` after property name",
                );
                return None;
            }
            let colon = self.bump();
            let value = self.parse_expr(0)?;
            properties.push(ObjectProperty {
                key_tok: key_idx,
                colon,
                value,
            });
            match self.peek().kind {
                TokenKind::Comma => commas.push(self.bump()),
                TokenKind::BraceClose => {}
                // Java-style object literal: another `key:` without an explicit comma (`{a: 1 b: 2}`).
                TokenKind::Ident
                | TokenKind::String
                | TokenKind::Number
                | TokenKind::Kw(Kw::True | Kw::False | Kw::Null) => {}
                _ => {
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expected `,` or `}` after object property",
                    );
                    return None;
                }
            }
        }
        let close = self.bump();
        Some(Expr::ObjectLiteral {
            open,
            properties,
            commas,
            close,
        })
    }

    /// One closing `>` for a set literal; `>>` / `>>>` leave slack for an outer `<...>`.
    /// Returns `Ok(None)` when slack supplied a virtual `>` (no token leaf for this level).
    fn eat_set_gt_close(&mut self) -> Result<Option<usize>, ()> {
        if self.set_gt_slack > 0 {
            self.set_gt_slack -= 1;
            return Ok(None);
        }
        if !matches!(self.peek().kind, TokenKind::Operator) {
            return Err(());
        }
        let t = self.text_tok(self.cur);
        match t {
            ">" => Ok(Some(self.bump())),
            ">>" => {
                let i = self.bump();
                self.set_gt_slack = 1;
                Ok(Some(i))
            }
            ">>>" => {
                let i = self.bump();
                self.set_gt_slack = 2;
                Ok(Some(i))
            }
            _ => Err(()),
        }
    }

    fn parse_set_literal(&mut self, open: usize) -> Option<Expr> {
        // Stop before comparison / logical ops so `>` closes the literal (not `2 > x`).
        // Stop before `>` that closes the set (comparison `>` has l_bp 13).
        // `<<` / `>>` / `>>>` are (15, 16): min_bp must exceed 16 or `2>>` is parsed as shift, eating the
        // set's closing `>>`.
        const SET_ELEM_MIN_BP: u8 = 17;
        let mut elements = Vec::new();
        let mut commas = Vec::new();
        match self.eat_set_gt_close() {
            Ok(Some(i)) => {
                return Some(Expr::SetLiteral {
                    open,
                    elements,
                    commas,
                    close: Some(i),
                });
            }
            Ok(None) => {
                self.error(
                    "UNEXPECTED_TOKEN",
                    self.peek().span,
                    "invalid set literal close",
                );
                return None;
            }
            Err(()) => {}
        }
        loop {
            elements.push(self.parse_expr(SET_ELEM_MIN_BP)?);
            match self.eat_set_gt_close() {
                Ok(Some(close_tok)) => {
                    return Some(Expr::SetLiteral {
                        open,
                        elements,
                        commas,
                        close: Some(close_tok),
                    });
                }
                Ok(None) => {
                    // Slack-only `>`: outer set may be complete (e.g. `<'a', <1, 2>> == …`), or we may
                    // need another element (`<<>>`). If `,` follows, continue; otherwise close here.
                    if !matches!(self.peek().kind, TokenKind::Comma) {
                        return Some(Expr::SetLiteral {
                            open,
                            elements,
                            commas,
                            close: None,
                        });
                    }
                }
                Err(()) => {}
            }
            if matches!(self.peek().kind, TokenKind::Comma) {
                commas.push(self.bump());
                // Java `readSet`: optional comma before `>` (trailing comma allowed).
                match self.eat_set_gt_close() {
                    Ok(Some(close_tok)) => {
                        return Some(Expr::SetLiteral {
                            open,
                            elements,
                            commas,
                            close: Some(close_tok),
                        });
                    }
                    Ok(None) => {
                        if !matches!(self.peek().kind, TokenKind::Comma) {
                            return Some(Expr::SetLiteral {
                                open,
                                elements,
                                commas,
                                close: None,
                            });
                        }
                    }
                    Err(()) => {}
                }
                continue;
            }
            self.error(
                "UNEXPECTED_TOKEN",
                self.peek().span,
                "expected `,` or `>` in set literal",
            );
            return None;
        }
    }

    fn finish_call(&mut self, callee: Expr) -> Option<Expr> {
        let open = self.bump();
        let mut args = Vec::new();
        let mut arg_commas = Vec::new();
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            loop {
                args.push(self.parse_expr(0)?);
                if matches!(self.peek().kind, TokenKind::ParClose) {
                    break;
                }
                if matches!(self.peek().kind, TokenKind::Comma) {
                    arg_commas.push(Some(self.bump()));
                    continue;
                }
                if matches!(self.peek().kind, TokenKind::String) {
                    if args.last().is_some_and(|a| self.expr_is_string_leaf(a)) {
                        arg_commas.push(None);
                        continue;
                    }
                }
                self.error("UNEXPECTED_TOKEN", self.peek().span, "expected `,` or `)`");
                return None;
            }
        }
        if !matches!(self.peek().kind, TokenKind::ParClose) {
            self.error(
                "CLOSING_PARENTHESIS_EXPECTED",
                self.peek().span,
                "expected `)`",
            );
            return None;
        }
        let close = self.bump();
        Some(Expr::Call {
            callee: Box::new(callee),
            open,
            args,
            arg_commas,
            close,
        })
    }

    /// After `(` was bumped: `(a, b) => body` / `() => body` / `(a) => body`. Returns `None` to retry as grouped expr.
    fn try_parse_arrow_fn_after_paren(&mut self, open: usize) -> Option<Expr> {
        let save = self.cur;
        let mut params = Vec::new();
        let mut commas = Vec::new();

        if matches!(self.peek().kind, TokenKind::ParClose) {
            let close = self.bump();
            if !matches!(self.peek().kind, TokenKind::Arrow) {
                self.cur = save;
                return None;
            }
            let arrow = self.bump();
            let body = if matches!(self.peek().kind, TokenKind::BraceOpen) {
                ArrowFnBody::Block(self.parse_block()?)
            } else {
                ArrowFnBody::Expr(Box::new(self.parse_expr(0)?))
            };
            return Some(Expr::ArrowFn {
                open_paren: Some(open),
                params,
                param_commas: commas,
                close_paren: Some(close),
                arrow,
                body,
            });
        }

        if !matches!(self.peek().kind, TokenKind::Ident) {
            return None;
        }
        params.push(self.bump());
        loop {
            match self.peek().kind {
                TokenKind::Comma => {
                    commas.push(self.bump());
                    if !matches!(self.peek().kind, TokenKind::Ident) {
                        self.cur = save;
                        return None;
                    }
                    params.push(self.bump());
                }
                TokenKind::ParClose => break,
                _ => {
                    self.cur = save;
                    return None;
                }
            }
        }
        let close = self.bump();
        if !matches!(self.peek().kind, TokenKind::Arrow) {
            self.cur = save;
            return None;
        }
        let arrow = self.bump();
        let body = if matches!(self.peek().kind, TokenKind::BraceOpen) {
            ArrowFnBody::Block(self.parse_block()?)
        } else {
            ArrowFnBody::Expr(Box::new(self.parse_expr(0)?))
        };
        Some(Expr::ArrowFn {
            open_paren: Some(open),
            params,
            param_commas: commas,
            close_paren: Some(close),
            arrow,
            body,
        })
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        match &self.peek().kind {
            TokenKind::Operator if self.text_tok(self.cur) == "<" => {
                let open = self.bump();
                self.parse_set_literal(open)
            }
            TokenKind::Number | TokenKind::String | TokenKind::Ident => {
                Some(Expr::Leaf(self.bump()))
            }
            TokenKind::Kw(Kw::True | Kw::False | Kw::Null | Kw::This | Kw::Super) => {
                Some(Expr::Leaf(self.bump()))
            }
            TokenKind::Kw(Kw::Class) => {
                let n = self.cur + 1;
                if n < self.tokens.len() {
                    match &self.tokens[n].kind {
                        TokenKind::BracketOpen | TokenKind::Dot => {
                            let class_kw = self.bump();
                            Some(Expr::ClassSelf { class_kw })
                        }
                        TokenKind::Operator if self.text_tok(n) == "." => {
                            let class_kw = self.bump();
                            Some(Expr::ClassSelf { class_kw })
                        }
                        _ => {
                            self.error(
                                "UNEXPECTED_TOKEN",
                                self.peek().span,
                                "expected `.` or `[` after `class` in expression context",
                            );
                            None
                        }
                    }
                } else {
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expected `.` or `[` after `class` in expression context",
                    );
                    None
                }
            }
            TokenKind::Kw(Kw::Function) => {
                let n = self.cur + 1;
                if n < self.tokens.len() && matches!(self.tokens[n].kind, TokenKind::ParOpen) {
                    self.parse_function_value_expr()
                } else {
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "expected `(` after `function` in expression",
                    );
                    None
                }
            }
            TokenKind::Lemniscate | TokenKind::Pi => Some(Expr::Leaf(self.bump())),
            TokenKind::ParOpen => {
                let open = self.bump();
                let checkpoint = self.cur;
                if let Some(arrow_fn) = self.try_parse_arrow_fn_after_paren(open) {
                    return Some(arrow_fn);
                }
                self.cur = checkpoint;
                let expr = Box::new(self.parse_expr(0)?);
                if !matches!(self.peek().kind, TokenKind::ParClose) {
                    self.error(
                        "CLOSING_PARENTHESIS_EXPECTED",
                        self.peek().span,
                        "expected `)`",
                    );
                    return None;
                }
                let close = self.bump();
                Some(Expr::Paren { open, expr, close })
            }
            TokenKind::BraceOpen => {
                let open = self.bump();
                self.parse_object_literal(open)
            }
            TokenKind::BracketClose => {
                if bracket_close_may_start_interval(self.tokens, self.cur) {
                    let left = self.bump();
                    self.parse_interval_leading_rbracket(left)
                } else {
                    self.error(
                        "UNEXPECTED_TOKEN",
                        self.peek().span,
                        "unexpected `]`",
                    );
                    None
                }
            }
            TokenKind::BracketOpen => {
                let open = self.bump();
                self.parse_after_bracket(open)
            }
            TokenKind::Arrow => {
                // `-> expr` / `-> { ... }` — zero-argument arrow (Java static field lambdas).
                let arrow = self.bump();
                let body = if matches!(self.peek().kind, TokenKind::BraceOpen) {
                    ArrowFnBody::Block(self.parse_block()?)
                } else {
                    ArrowFnBody::Expr(Box::new(self.parse_expr(0)?))
                };
                Some(Expr::ArrowFn {
                    open_paren: None,
                    params: vec![],
                    param_commas: vec![],
                    close_paren: None,
                    arrow,
                    body,
                })
            }
            _ => {
                self.error("UNEXPECTED_TOKEN", self.peek().span, "expected expression");
                None
            }
        }
    }
}

fn is_compound_assign_op(op: &str) -> bool {
    matches!(
        op,
        "=" | "+="
            | "-="
            | "*="
            | "/="
            | "%="
            | "**="
            | "\\="
            | "??="
            | "^="
            | "&="
            | "|="
            | "<<="
            | ">>="
            | ">>>="
    )
}

/// Binding powers aligned with Java `Operators.getPriority` (higher `l_bp` = tighter).
fn bin_binding_power(op: &str) -> Option<(u8, u8)> {
    match op {
        "||" | "??" => Some((1, 2)),
        "&&" => Some((3, 4)),
        "|" => Some((5, 6)),
        "^" => Some((7, 8)),
        "&" => Some((9, 10)),
        "==" | "!=" | "===" | "!==" => Some((11, 12)),
        "<" | "<=" | ">" | ">=" => Some((13, 14)),
        "<<" | ">>" | ">>>" => Some((15, 16)),
        "+" | "-" => Some((17, 18)),
        "*" | "/" | "%" | "\\" => Some((19, 20)),
        "**" => Some((21, 20)),
        _ => None,
    }
}
