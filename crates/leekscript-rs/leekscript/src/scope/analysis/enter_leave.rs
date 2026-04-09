use sipha::tree::ast::{AstNode, AstNodeExt};
use sipha::tree::red::SyntaxNode;

use crate::ast::types::TypeExpr;
use crate::ast::{
    AnonFunctionExpr, CatchClause, ClassDecl, ClassMember, ForeachStmt, FunctionDecl, GlobalDecl,
    VarDecl,
};
use crate::scope::extract::{
    extract_fn_params_from_syntax, extract_function_params, leek_ty_from_type_expr,
    leek_ty_from_type_expr_with_templates, try_extract_class_field, try_extract_class_method,
};
use crate::scope::leek_ty::LeekTy;
use crate::scope::model::{ScopeId, ScopeKind, SymbolKind};
use crate::syntax::attached_parsed_doxygen;
use crate::syntax::kinds::Node;

use super::analyzer::Analyzer;
use super::spans::{
    catch_param_span, class_name_span, for_stmt_init_var_spans, foreach_bind_spans,
    function_name_span, global_name_span, lambda_param_spans, var_decl_name_span,
};

pub(crate) fn sync_enter(a: &mut Analyzer, node: &SyntaxNode) {
    match node.kind_as::<Node>() {
        Some(Node::FunctionDecl) => enter_function_decl(a, node),
        Some(Node::AnonFunctionExpr) => enter_anon_function_expr(a, node),
        Some(Node::Block) => enter_block(a, node),
        Some(Node::ClassDecl) => enter_class_decl(a, node),
        Some(Node::ClassMember) => enter_class_member(a, node),
        Some(Node::VarDecl) => enter_var_decl(a, node),
        Some(Node::GlobalDecl) => enter_global_decl(a, node),
        Some(Node::ForeachStmt) => enter_foreach(a, node),
        Some(Node::ForStmt) => enter_for_stmt(a, node),
        Some(Node::LambdaExpr) => enter_lambda_expr(a, node),
        Some(Node::CatchClause) => enter_catch(a, node),
        _ => {}
    }
}

pub(crate) fn sync_leave(a: &mut Analyzer, node: &SyntaxNode) {
    match node.kind_as::<Node>() {
        Some(Node::Block) => leave_block(a, node),
        Some(Node::FunctionDecl) => {
            let _ = a.implicit_this_receiver_stack.pop();
            a.fn_template_stack.pop();
            a.scope_stack.pop();
        }
        Some(Node::ClassMember) => {
            let cn = a.class_name_stack.last().cloned().unwrap_or_default();
            if try_extract_class_method(node, &cn).is_some() {
                let cm = ClassMember::cast(node.clone()).expect("class member");
                if !cm.is_static() && cm.has_method_body() {
                    let _ = a.implicit_this_receiver_stack.pop();
                }
                a.scope_stack.pop();
            }
        }
        Some(Node::ClassDecl) => {
            a.class_name_stack.pop();
            a.class_template_stack.pop();
            a.scope_stack.pop();
        }
        Some(Node::ForStmt) => {
            a.scope_stack.pop();
        }
        Some(Node::LambdaExpr) => {
            let _ = a.implicit_this_receiver_stack.pop();
            a.scope_stack.pop();
        }
        Some(Node::AnonFunctionExpr) => {
            let _ = a.implicit_this_receiver_stack.pop();
            a.fn_template_stack.pop();
            a.scope_stack.pop();
        }
        _ => {}
    }
}

fn enter_function_decl(a: &mut Analyzer, node: &SyntaxNode) {
    a.implicit_this_receiver_stack.push(None);
    let outer = a.current_scope();
    let fd = FunctionDecl::cast(node.clone()).expect("fd");
    let name = fd.name().unwrap_or_default();
    let name_span = function_name_span(&fd).unwrap_or_else(|| node.text_range());
    let opt_tpl = fd.template_params();
    let fn_templates = opt_tpl.as_ref().map(|p| p.names()).unwrap_or_default();
    let param_entries = extract_function_params(&fd);
    a.fn_template_stack.push(fn_templates.clone());
    if a.phase.is_build_scopes() {
        let doc = attached_parsed_doxygen(node);
        let params: Vec<LeekTy> = param_entries
            .iter()
            .map(|(opt_te, _, _)| {
                opt_te
                    .as_ref()
                    .map(|t| leek_ty_from_type_expr_with_templates(t, &fn_templates))
                    .unwrap_or(LeekTy::Unknown)
            })
            .collect();
        let ret = fd
            .return_type()
            .map(|t| leek_ty_from_type_expr_with_templates(&t, &fn_templates))
            .unwrap_or(LeekTy::Void);
        let fn_ty = LeekTy::Function {
            params,
            ret: Box::new(ret),
        };
        a.graph.declare(
            a.phase,
            outer,
            name,
            name_span,
            SymbolKind::Function,
            Some(fn_ty),
            doc,
            false,
        );
    }
    let fn_sc = a.push_child_scope(Some(outer), ScopeKind::Function);
    if a.phase.is_build_scopes() {
        if let Some(tp) = opt_tpl {
            for (tp_name, tp_span) in tp.name_spans() {
                a.graph.declare(
                    a.phase,
                    fn_sc,
                    tp_name.clone(),
                    tp_span,
                    SymbolKind::TypeParam,
                    Some(LeekTy::TypeParam(tp_name)),
                    None,
                    false,
                );
            }
        }
        for (ty, pname, pspan) in &param_entries {
            let dt = ty
                .as_ref()
                .map(|t| leek_ty_from_type_expr_with_templates(t, &fn_templates));
            a.graph.declare(
                a.phase,
                fn_sc,
                pname.clone(),
                *pspan,
                SymbolKind::Parameter,
                dt,
                None,
                false,
            );
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
    let class_templates = cd.template_params().map(|p| p.names()).unwrap_or_default();
    if a.phase.is_build_scopes() {
        let doc = attached_parsed_doxygen(node);
        a.graph.declare(
            a.phase,
            outer,
            cname.clone(),
            cspan,
            SymbolKind::Class,
            Some(LeekTy::ClassObject(cname.clone())),
            doc,
            false,
        );
    }
    a.class_name_stack.push(cname.clone());
    a.class_template_stack.push(class_templates.clone());
    let class_sc = a.push_child_scope(Some(outer), ScopeKind::Class);
    if a.phase.is_build_scopes() {
        a.graph.class_body_scope_by_name.insert(cname, class_sc);
        if let Some(tp) = cd.template_params() {
            for (tp_name, tp_span) in tp.name_spans() {
                a.graph.declare(
                    a.phase,
                    class_sc,
                    tp_name.clone(),
                    tp_span,
                    SymbolKind::TypeParam,
                    Some(LeekTy::TypeParam(tp_name)),
                    None,
                    false,
                );
            }
        }
    }
    a.pending_class_body += 1;
}

fn enter_class_member(a: &mut Analyzer, node: &SyntaxNode) {
    let cn = a.class_name_stack.last().cloned().unwrap_or_default();
    let class_templates: Vec<String> = a.class_template_stack.last().cloned().unwrap_or_default();
    if let Some(m) = try_extract_class_method(node, &cn) {
        let class_sc = a.current_scope();
        let cm = ClassMember::cast(node.clone()).expect("class member");
        if a.phase.is_build_scopes() {
            let is_static = cm.is_static() && !m.is_constructor;
            let sk = if m.is_constructor {
                SymbolKind::Constructor
            } else {
                SymbolKind::Method
            };
            let doc = attached_parsed_doxygen(node);
            let decl_ty = if m.is_constructor {
                None
            } else {
                let params: Vec<LeekTy> = m
                    .params
                    .iter()
                    .map(|(opt_te, _, _)| {
                        opt_te
                            .as_ref()
                            .map(|t| leek_ty_from_type_expr_with_templates(t, &class_templates))
                            .unwrap_or(LeekTy::Unknown)
                    })
                    .collect();
                let ret = cm
                    .leading_type_expr()
                    .map(|t| leek_ty_from_type_expr_with_templates(&t, &class_templates))
                    .unwrap_or(LeekTy::Void);
                Some(LeekTy::Function {
                    params,
                    ret: Box::new(ret),
                })
            };
            a.graph.declare(
                a.phase,
                class_sc,
                m.name,
                m.name_span,
                sk,
                decl_ty,
                doc,
                is_static,
            );
        }
        let msc = a.push_child_scope(Some(class_sc), ScopeKind::Method);
        if !cm.is_static() && cm.has_method_body() {
            a.implicit_this_receiver_stack.push(Some(cn.clone()));
        }
        if a.phase.is_build_scopes() {
            for (ty, pname, pspan) in m.params {
                let dt = ty
                    .as_ref()
                    .map(|t| leek_ty_from_type_expr_with_templates(t, &class_templates));
                a.graph.declare(
                    a.phase,
                    msc,
                    pname,
                    pspan,
                    SymbolKind::Parameter,
                    dt,
                    None,
                    false,
                );
            }
        }
    } else if a.phase.is_build_scopes() {
        if let Some((fname, fspan, fty)) = try_extract_class_field(node, &class_templates) {
            let class_sc = a.current_scope();
            let cm = ClassMember::cast(node.clone()).expect("class member");
            let is_static = cm.is_static();
            let doc = attached_parsed_doxygen(node);
            a.graph.declare(
                a.phase,
                class_sc,
                fname,
                fspan,
                SymbolKind::Field,
                Some(fty),
                doc,
                is_static,
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
        let mut visible_templates = Vec::new();
        if let Some(c) = a.class_template_stack.last() {
            visible_templates.extend(c.iter().cloned());
        }
        visible_templates.extend(a.fn_template_stack.iter().flatten().cloned());
        let dt = vd
            .syntax()
            .child::<TypeExpr>()
            .map(|t| leek_ty_from_type_expr_with_templates(&t, &visible_templates));
        let doc = attached_parsed_doxygen(node);
        a.graph
            .declare(a.phase, sc, n, sp, SymbolKind::Variable, dt, doc, false);
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
        a.graph
            .declare(a.phase, module, n, sp, SymbolKind::Global, dt, doc, false);
    }
}

fn enter_for_stmt(a: &mut Analyzer, node: &SyntaxNode) {
    let p = a.current_scope();
    let for_sc = a.push_child_scope(Some(p), ScopeKind::Block);
    if !a.phase.is_build_scopes() {
        return;
    }
    for (name, span) in for_stmt_init_var_spans(node) {
        a.graph.declare(
            a.phase,
            for_sc,
            name,
            span,
            SymbolKind::Variable,
            None,
            None,
            false,
        );
    }
}

/// `function (T x, …) => R { … }` in expression position (object/map entries, calls, etc.).
fn enter_anon_function_expr(a: &mut Analyzer, node: &SyntaxNode) {
    a.implicit_this_receiver_stack.push(None);
    let outer = a.current_scope();
    let af = AnonFunctionExpr::cast(node.clone()).expect("anon fn");
    let opt_tpl = af.template_params();
    let fn_templates = opt_tpl.as_ref().map(|p| p.names()).unwrap_or_default();
    a.fn_template_stack.push(fn_templates.clone());
    let fn_sc = a.push_child_scope(Some(outer), ScopeKind::Function);
    if !a.phase.is_build_scopes() {
        return;
    }
    if let Some(tp) = opt_tpl {
        for (tp_name, tp_span) in tp.name_spans() {
            a.graph.declare(
                a.phase,
                fn_sc,
                tp_name.clone(),
                tp_span,
                SymbolKind::TypeParam,
                Some(LeekTy::TypeParam(tp_name)),
                None,
                false,
            );
        }
    }
    for (ty, pname, pspan) in extract_fn_params_from_syntax(node) {
        let dt = ty
            .as_ref()
            .map(|t| leek_ty_from_type_expr_with_templates(t, &fn_templates));
        a.graph.declare(
            a.phase,
            fn_sc,
            pname,
            pspan,
            SymbolKind::Parameter,
            dt,
            None,
            false,
        );
    }
}

fn enter_lambda_expr(a: &mut Analyzer, node: &SyntaxNode) {
    a.implicit_this_receiver_stack.push(None);
    let p = a.current_scope();
    let lam_sc = a.push_child_scope(Some(p), ScopeKind::Block);
    if !a.phase.is_build_scopes() {
        return;
    }
    for (name, span) in lambda_param_spans(node) {
        a.graph.declare(
            a.phase,
            lam_sc,
            name,
            span,
            SymbolKind::Parameter,
            None,
            None,
            false,
        );
    }
}

fn enter_foreach(a: &mut Analyzer, node: &SyntaxNode) {
    if !a.phase.is_build_scopes() {
        return;
    }
    let fe = ForeachStmt::cast(node.clone()).expect("fe");
    let sc = a.current_scope();
    for (n, sp) in foreach_bind_spans(&fe) {
        a.graph
            .declare(a.phase, sc, n, sp, SymbolKind::Variable, None, None, false);
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
            .declare(a.phase, sc, n, sp, SymbolKind::Variable, dt, None, false);
    }
}

fn leave_block(a: &mut Analyzer, node: &SyntaxNode) {
    if a.skip_leave_block_span == Some(node.text_range()) {
        a.skip_leave_block_span = None;
        return;
    }
    a.scope_stack.pop();
}
