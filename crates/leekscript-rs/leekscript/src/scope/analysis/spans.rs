//! Name / binding span helpers for AST nodes used during scope construction.

use crate::Span;
use crate::ast::binding_name::function_decl_name_token;
use crate::ast::{CatchClause, ClassDecl, ForeachStmt, FunctionDecl, GlobalDecl, VarDecl};
use crate::syntax::kinds::Lex;
use sipha::tree::ast::AstNode;
use sipha::tree::red::SyntaxNode;
use sipha::tree::red::SyntaxToken;

pub(crate) fn function_name_span(fd: &FunctionDecl) -> Option<Span> {
    function_decl_name_token(fd.syntax()).map(|t| t.text_range())
}

pub(crate) fn class_name_span(cd: &ClassDecl) -> Option<Span> {
    cd.syntax()
        .child_tokens()
        .take_while(|t| t.kind_as::<Lex>() != Some(Lex::ExtendsKw))
        .find(|t| t.kind_as::<Lex>() == Some(Lex::Ident))
        .map(|t| t.text_range())
}

pub(crate) fn var_decl_name_span(vd: &VarDecl) -> Option<Span> {
    vd.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<Lex>() == Some(Lex::Ident))
        .map(|t| t.text_range())
}

pub(crate) fn global_name_span(g: &GlobalDecl) -> Option<Span> {
    g.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<Lex>() == Some(Lex::Ident))
        .map(|t| t.text_range())
}

pub(crate) fn catch_param_span(cc: &CatchClause) -> Option<Span> {
    cc.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<Lex>() == Some(Lex::Ident))
        .map(|t| t.text_range())
}

pub(crate) fn foreach_bind_spans(fe: &ForeachStmt) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    let mut after_for = false;
    for t in fe.syntax().child_tokens() {
        match t.kind_as::<Lex>() {
            Some(Lex::ForKw) => after_for = true,
            Some(Lex::InKw) => break,
            Some(Lex::Ident) if after_for => out.push((t.text().to_string(), t.text_range())),
            _ => {}
        }
    }
    out
}

/// Tokens inside the `(` `)` header of `for ( … )`, excluding the outer parentheses.
fn for_stmt_header_inner_tokens(syntax: &SyntaxNode) -> Vec<SyntaxToken> {
    let mut depth = 0i32;
    let mut header = Vec::new();
    for t in syntax.descendant_tokens() {
        if t.is_trivia() {
            continue;
        }
        match t.kind_as::<Lex>() {
            Some(Lex::LParen) => {
                if depth >= 1 {
                    header.push(t.clone());
                }
                depth += 1;
            }
            Some(Lex::RParen) => {
                if depth >= 2 {
                    header.push(t.clone());
                }
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {
                if depth >= 1 {
                    header.push(t.clone());
                }
            }
        }
    }
    header
}

/// First header clause (up to the first `;` at nesting depth 0) inside `header_inner`.
fn for_stmt_init_clause_tokens(header_inner: &[SyntaxToken]) -> Vec<SyntaxToken> {
    let mut d = 0i32;
    let mut out = Vec::new();
    for t in header_inner {
        if t.is_trivia() {
            continue;
        }
        match t.kind_as::<Lex>() {
            Some(Lex::LParen) | Some(Lex::LBracket) | Some(Lex::LBrace) => {
                d += 1;
                out.push(t.clone());
            }
            Some(Lex::RParen) | Some(Lex::RBracket) | Some(Lex::RBrace) => {
                d -= 1;
                out.push(t.clone());
            }
            Some(Lex::Semi) if d == 0 => break,
            _ => out.push(t.clone()),
        }
    }
    out
}

/// `for (var x = 0; …)` / `for (integer i = 0; …)` — variable bound in the whole `for` statement.
pub(crate) fn for_stmt_init_var_spans(syntax: &SyntaxNode) -> Vec<(String, Span)> {
    let header = for_stmt_header_inner_tokens(syntax);
    let init = for_stmt_init_clause_tokens(&header);
    let mut last_ident: Option<(String, Span)> = None;
    let mut d = 0i32;
    let mut i = 0usize;
    while i < init.len() {
        let t = &init[i];
        if t.is_trivia() {
            i += 1;
            continue;
        }
        match t.kind_as::<Lex>() {
            Some(Lex::Semi) if d == 0 => {
                return vec![];
            }
            Some(Lex::Eq) if d == 0 => {
                let out = last_ident.clone();
                i += 1;
                while i < init.len() {
                    let t2 = &init[i];
                    match t2.kind_as::<Lex>() {
                        Some(Lex::LParen) | Some(Lex::LBracket) | Some(Lex::LBrace) => d += 1,
                        Some(Lex::RParen) | Some(Lex::RBracket) | Some(Lex::RBrace) => d -= 1,
                        Some(Lex::Semi) if d == 0 => break,
                        _ => {}
                    }
                    i += 1;
                }
                return out.into_iter().collect();
            }
            Some(Lex::LParen) | Some(Lex::LBracket) | Some(Lex::LBrace) => d += 1,
            Some(Lex::RParen) | Some(Lex::RBracket) | Some(Lex::RBrace) => d -= 1,
            Some(Lex::Ident) if d == 0 => {
                last_ident = Some((t.text().to_string(), t.text_range()));
            }
            _ => {}
        }
        i += 1;
    }
    vec![]
}

fn last_ident_outside_generics(tokens: &[SyntaxToken]) -> Option<(String, Span)> {
    let mut angle = 0i32;
    let mut last = None;
    for t in tokens {
        if t.is_trivia() {
            continue;
        }
        match t.kind_as::<Lex>() {
            Some(Lex::Lt) => angle += 1,
            Some(Lex::Gt) => angle = (angle - 1).max(0),
            Some(Lex::Ident) if angle == 0 => {
                last = Some((t.text().to_string(), t.text_range()));
            }
            _ => {}
        }
    }
    last
}

fn split_comma_top_level(inner: &[SyntaxToken]) -> Vec<&[SyntaxToken]> {
    let mut angle = 0i32;
    let mut depth = 0i32;
    let mut start = 0usize;
    let mut out = Vec::new();
    for (i, t) in inner.iter().enumerate() {
        if t.is_trivia() {
            continue;
        }
        match t.kind_as::<Lex>() {
            Some(Lex::LParen) | Some(Lex::LBracket) | Some(Lex::LBrace) => depth += 1,
            Some(Lex::RParen) | Some(Lex::RBracket) | Some(Lex::RBrace) => depth -= 1,
            Some(Lex::Lt) => angle += 1,
            Some(Lex::Gt) => angle = (angle - 1).max(0),
            Some(Lex::Comma) if depth == 0 && angle == 0 => {
                out.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

/// Parameters for `x => …`, `integer x => …`, `(a, b) => …`, `(Map<K,V> m, integer n) => …`.
pub(crate) fn lambda_param_spans(syntax: &SyntaxNode) -> Vec<(String, Span)> {
    let tokens: Vec<_> = syntax
        .descendant_tokens()
        .into_iter()
        .filter(|t| !t.is_trivia())
        .take_while(|t| t.kind_as::<Lex>() != Some(Lex::Arrow))
        .collect();
    if tokens.is_empty() {
        return vec![];
    }
    let has_paren_params = tokens.iter().any(|t| t.kind_as::<Lex>() == Some(Lex::LParen));
    if !has_paren_params {
        return last_ident_outside_generics(&tokens).into_iter().collect();
    }
    let Some(start) = tokens
        .iter()
        .position(|t| t.kind_as::<Lex>() == Some(Lex::LParen))
    else {
        return vec![];
    };
    let mut depth = 0i32;
    let mut end = None;
    for (i, t) in tokens.iter().enumerate().skip(start) {
        match t.kind_as::<Lex>() {
            Some(Lex::LParen) => depth += 1,
            Some(Lex::RParen) => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end) = end else {
        return vec![];
    };
    let inner = &tokens[start + 1..end];
    split_comma_top_level(inner)
        .into_iter()
        .filter_map(|seg| last_ident_outside_generics(seg))
        .collect()
}
