use sipha::tree::ast::{AstNode, AstNodeExt};
use sipha::tree::red::SyntaxNode;

use crate::ast::types::TypeExpr;
use crate::ast::{
    CatchClause, ClassDecl, ForeachStmt, FunctionDecl, GlobalDecl, VarDecl,
};
use crate::scope::extract::{
    extract_function_params, leek_ty_from_type_expr, try_extract_class_field,
    try_extract_class_method,
};
use crate::scope::leek_ty::LeekTy;
use crate::scope::model::{ScopeId, ScopeKind, SymbolKind};
use crate::syntax::attached_parsed_doxygen;
use crate::syntax::kinds::K;

use super::analyzer::Analyzer;
use super::spans::{
    catch_param_span, class_name_span, foreach_bind_spans, function_name_span, global_name_span,
    var_decl_name_span,
};

pub(crate) fn sync_enter(a: &mut Analyzer, node: &SyntaxNode) {
    match node.kind_as::<K>() {
        Some(K::FunctionDecl) => enter_function_decl(a, node),
        Some(K::Block) => enter_block(a, node),
        Some(K::ClassDecl) => enter_class_decl(a, node),
        Some(K::ClassMember) => enter_class_member(a, node),
        Some(K::VarDecl) => enter_var_decl(a, node),
        Some(K::GlobalDecl) => enter_global_decl(a, node),
        Some(K::ForeachStmt) => enter_foreach(a, node),
        Some(K::CatchClause) => enter_catch(a, node),
        _ => {}
    }
}

pub(crate) fn sync_leave(a: &mut Analyzer, node: &SyntaxNode) {
    match node.kind_as::<K>() {
        Some(K::Block) => leave_block(a, node),
        Some(K::FunctionDecl) => {
            a.scope_stack.pop();
        }
        Some(K::ClassMember) => {
            let cn = a.class_name_stack.last().cloned().unwrap_or_default();
            if try_extract_class_method(node, &cn).is_some() {
                a.scope_stack.pop();
            }
        }
        Some(K::ClassDecl) => {
            a.class_name_stack.pop();
            a.scope_stack.pop();
        }
        _ => {}
    }
}

fn enter_function_decl(a: &mut Analyzer, node: &SyntaxNode) {
    let outer = a.current_scope();
    let fd = FunctionDecl::cast(node.clone()).expect("fd");
    let name = fd.name().unwrap_or_default();
    let name_span = function_name_span(&fd).unwrap_or_else(|| node.text_range());
    if a.phase.is_build_scopes() {
        let doc = attached_parsed_doxygen(node);
        a.graph.declare(
            a.phase,
            outer,
            name,
            name_span,
            SymbolKind::Function,
            fd.return_type().map(|t| leek_ty_from_type_expr(&t)),
            doc,
        );
    }
    let fn_sc = a.push_child_scope(Some(outer), ScopeKind::Function);
    if a.phase.is_build_scopes() {
        for (ty, pname, pspan) in extract_function_params(&fd) {
            let dt = ty.as_ref().map(leek_ty_from_type_expr);
            a.graph
                .declare(a.phase, fn_sc, pname, pspan, SymbolKind::Parameter, dt, None);
        }
    }
}

fn enter_block(a: &mut Analyzer, node: &SyntaxNode) {
    if a.pending_class_body > 0 {
        a.pending_class_body -= 1;
        a.skip_leave_block_span = Some(node.text_range());
        return;
    }
    let p = a.current_scope();
    a.push_child_scope(Some(p), ScopeKind::Block);
}

fn enter_class_decl(a: &mut Analyzer, node: &SyntaxNode) {
    let cd = ClassDecl::cast(node.clone()).expect("cd");
    let outer = a.current_scope();
    let cname = cd.name().unwrap_or_default();
    let cspan = class_name_span(&cd).unwrap_or_else(|| node.text_range());
    if a.phase.is_build_scopes() {
        let doc = attached_parsed_doxygen(node);
        a.graph.declare(
            a.phase,
            outer,
            cname.clone(),
            cspan,
            SymbolKind::Class,
            Some(LeekTy::Class(cname.clone())),
            doc,
        );
    }
    a.class_name_stack.push(cname);
    a.push_child_scope(Some(outer), ScopeKind::Class);
    a.pending_class_body += 1;
}

fn enter_class_member(a: &mut Analyzer, node: &SyntaxNode) {
    let cn = a.class_name_stack.last().cloned().unwrap_or_default();
    if let Some(m) = try_extract_class_method(node, &cn) {
        let class_sc = a.current_scope();
        if a.phase.is_build_scopes() {
            let sk = if m.is_constructor {
                SymbolKind::Constructor
            } else {
                SymbolKind::Method
            };
            let doc = attached_parsed_doxygen(node);
            a.graph.declare(
                a.phase,
                class_sc,
                m.name,
                m.name_span,
                sk,
                None,
                doc,
            );
        }
        let msc = a.push_child_scope(Some(class_sc), ScopeKind::Method);
        if a.phase.is_build_scopes() {
            for (ty, pname, pspan) in m.params {
                let dt = ty.as_ref().map(leek_ty_from_type_expr);
                a.graph.declare(
                    a.phase,
                    msc,
                    pname,
                    pspan,
                    SymbolKind::Parameter,
                    dt,
                    None,
                );
            }
        }
    } else if a.phase.is_build_scopes() {
        if let Some((fname, fspan, fty)) = try_extract_class_field(node) {
            let class_sc = a.current_scope();
            let doc = attached_parsed_doxygen(node);
            a.graph.declare(
                a.phase,
                class_sc,
                fname,
                fspan,
                SymbolKind::Field,
                Some(fty),
                doc,
            );
        }
    }
}

fn enter_var_decl(a: &mut Analyzer, node: &SyntaxNode) {
    if !a.phase.is_build_scopes() {
        return;
    }
    let vd = VarDecl::cast(node.clone()).expect("vd");
    let sc = a.current_scope();
    if let (Some(n), Some(sp)) = (vd.first_name(), var_decl_name_span(&vd)) {
        let dt = vd
            .syntax()
            .child::<TypeExpr>()
            .map(|t| leek_ty_from_type_expr(&t));
        let doc = attached_parsed_doxygen(node);
        a.graph
            .declare(a.phase, sc, n, sp, SymbolKind::Variable, dt, doc);
    }
}

fn enter_global_decl(a: &mut Analyzer, node: &SyntaxNode) {
    if !a.phase.is_build_scopes() {
        return;
    }
    let g = GlobalDecl::cast(node.clone()).expect("g");
    let module = ScopeId(0);
    if let (Some(n), Some(sp)) = (g.first_name(), global_name_span(&g)) {
        let dt = g.type_expr().map(|t| leek_ty_from_type_expr(&t));
        let doc = attached_parsed_doxygen(node);
        a.graph.declare(a.phase, module, n, sp, SymbolKind::Global, dt, doc);
    }
}

fn enter_foreach(a: &mut Analyzer, node: &SyntaxNode) {
    if !a.phase.is_build_scopes() {
        return;
    }
    let fe = ForeachStmt::cast(node.clone()).expect("fe");
    let sc = a.current_scope();
    for (n, sp) in foreach_bind_spans(&fe) {
        a.graph.declare(a.phase, sc, n, sp, SymbolKind::Variable, None, None);
    }
}

fn enter_catch(a: &mut Analyzer, node: &SyntaxNode) {
    if !a.phase.is_build_scopes() {
        return;
    }
    let cc = CatchClause::cast(node.clone()).expect("cc");
    let sc = a.current_scope();
    if let (Some(n), Some(sp)) = (cc.param_name(), catch_param_span(&cc)) {
        let dt = cc.param_type().map(|t| leek_ty_from_type_expr(&t));
        a.graph
            .declare(a.phase, sc, n, sp, SymbolKind::Variable, dt, None);
    }
}

fn leave_block(a: &mut Analyzer, node: &SyntaxNode) {
    if a.skip_leave_block_span == Some(node.text_range()) {
        a.skip_leave_block_span = None;
        return;
    }
    a.scope_stack.pop();
}
