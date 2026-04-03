//! Name / binding span helpers for AST nodes used during scope construction.

use crate::Span;
use crate::ast::{CatchClause, ClassDecl, ForeachStmt, FunctionDecl, GlobalDecl, VarDecl};
use crate::syntax::kinds::K;
use sipha::tree::ast::AstNode;

pub(crate) fn function_name_span(fd: &FunctionDecl) -> Option<Span> {
    fd.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<K>() == Some(K::Ident))
        .map(|t| t.text_range())
}

pub(crate) fn class_name_span(cd: &ClassDecl) -> Option<Span> {
    cd.syntax()
        .child_tokens()
        .take_while(|t| t.kind_as::<K>() != Some(K::ExtendsKw))
        .find(|t| t.kind_as::<K>() == Some(K::Ident))
        .map(|t| t.text_range())
}

pub(crate) fn var_decl_name_span(vd: &VarDecl) -> Option<Span> {
    vd.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<K>() == Some(K::Ident))
        .map(|t| t.text_range())
}

pub(crate) fn global_name_span(g: &GlobalDecl) -> Option<Span> {
    g.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<K>() == Some(K::Ident))
        .map(|t| t.text_range())
}

pub(crate) fn catch_param_span(cc: &CatchClause) -> Option<Span> {
    cc.syntax()
        .child_tokens()
        .find(|t| t.kind_as::<K>() == Some(K::Ident))
        .map(|t| t.text_range())
}

pub(crate) fn foreach_bind_spans(fe: &ForeachStmt) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    let mut after_for = false;
    for t in fe.syntax().child_tokens() {
        match t.kind_as::<K>() {
            Some(K::ForKw) => after_for = true,
            Some(K::InKw) => break,
            Some(K::Ident) if after_for => out.push((t.text().to_string(), t.text_range())),
            _ => {}
        }
    }
    out
}
