//! [`LeekDoc`] visitation, typed offset lookup, text splice, and structured transform.

use leekscript::ast::{Stmt, VarDecl};
use leekscript::visit::{Visitor, WalkOptions, typed_at_offset};
use leekscript::{LeekDoc, TransformResult, Transformer, Version};
use sipha::tree::ast::AstNode;
use sipha::tree::ast::AstNodeExt;
use sipha::tree::red::{SyntaxNode, SyntaxToken};
use std::ops::ControlFlow;

#[test]
fn walk_counts_nodes_and_tokens() {
    let doc = LeekDoc::parse("let a = 1; let b = 2;", Version::VNext).expect("parse");
    struct Count {
        nodes: usize,
        tokens: usize,
    }
    impl Visitor for Count {
        fn enter_node(&mut self, _node: &SyntaxNode) -> ControlFlow<(), ()> {
            self.nodes += 1;
            ControlFlow::Continue(())
        }
        fn visit_token(&mut self, _token: &SyntaxToken) -> ControlFlow<(), ()> {
            self.tokens += 1;
            ControlFlow::Continue(())
        }
    }
    let mut v = Count {
        nodes: 0,
        tokens: 0,
    };
    let _ = doc.walk(&mut v, &WalkOptions::default());
    assert!(v.nodes > 3, "nodes: {}", v.nodes);
    assert!(v.tokens > 8, "tokens: {}", v.tokens);
}

#[test]
fn typed_at_offset_finds_var_decl() {
    let doc = LeekDoc::parse("let renamed = 0;", Version::VNext).expect("parse");
    let offset = doc.source_str().find("renamed").expect("renamed") as u32;
    let v: VarDecl = doc.typed_at_offset(offset).expect("VarDecl");
    assert_eq!(v.first_name(), Some("renamed".into()));
}

#[test]
fn typed_at_offset_free_function_matches() {
    let doc = LeekDoc::parse("let x = 1;", Version::VNext).expect("parse");
    let offset = doc.source_str().find('x').expect("x") as u32;
    let a = typed_at_offset::<VarDecl>(doc.root_syntax(), offset);
    let b = doc.typed_at_offset::<VarDecl>(offset);
    assert!(a.is_some() && b.is_some());
}

#[test]
fn replace_span_reparses() {
    let mut doc = LeekDoc::parse("let old = 1;", Version::VNext).expect("parse");
    let src = doc.source_str();
    let start = src.find("old").expect("old") as u32;
    let end = start + "old".len() as u32;
    doc.replace_span(leekscript::Span::new(start, end), "new_name", Version::VNext)
        .expect("replace");
    assert!(doc.source_str().contains("new_name"));
    let root = doc.root_ast().expect("root");
    let stmts: Vec<Stmt> = AstNodeExt::children::<Stmt>(root.syntax()).collect();
    let v = stmts[0].as_var_decl().expect("var");
    assert_eq!(v.first_name(), Some("new_name".into()));
}

#[test]
fn transform_noop_preserves_meaning() {
    struct Noop;
    impl Transformer for Noop {
        fn transform_node(&mut self, _node: &SyntaxNode) -> TransformResult {
            None
        }
    }
    let mut doc = LeekDoc::parse("let x = 1;", Version::VNext).expect("parse");
    doc.apply_transform(&mut Noop);
    let root = doc.root_ast().expect("root");
    let stmts: Vec<Stmt> = AstNodeExt::children::<Stmt>(root.syntax()).collect();
    let v = stmts[0].as_var_decl().expect("var");
    assert_eq!(v.first_name(), Some("x".into()));
}
