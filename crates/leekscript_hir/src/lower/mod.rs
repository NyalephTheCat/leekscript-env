//! Lower rowan [`SyntaxNode`] trees into [`HirFile`].

mod expr;
mod stmt;
mod util;

use crate::nodes::HirFile;
use leekscript_span::Span;
use leekscript_syntax::{LeekLanguage, LeekSyntaxKind};
use rowan::{NodeOrToken, SyntaxNode};

/// Failure while lowering a tree that was expected to be well-formed after parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirLoweringDiagnostic {
    /// Java `Error` name in `data/diagnostics/registry.yaml` (same convention as [`leekscript_run::CompileDiagnostic`]).
    pub reference: &'static str,
    pub span: Span,
    pub message: String,
}

/// Lower the grammar-shaped root (`SOURCE_FILE`) to HIR.
///
/// `language_version` must match the lexer version used to tokenize `src`.
pub fn lower_file(
    src: &str,
    root: &SyntaxNode<LeekLanguage>,
    language_version: u8,
) -> Result<HirFile, Vec<HirLoweringDiagnostic>> {
    if root.kind() != LeekSyntaxKind::SourceFile {
        return Err(vec![util::diag(
            "INTERNAL_ERROR",
            util::span_of_node(root),
            format!("expected SOURCE_FILE root, got {:?}", root.kind()),
        )]);
    }

    let ctx = util::LowerCtx {
        src,
        language_version,
    };
    let mut stmts = Vec::new();
    let mut diags = Vec::new();

    for el in util::non_trivia(root) {
        match el {
            NodeOrToken::Node(n) => match stmt::lower_stmt(&n, &ctx) {
                Ok(s) => stmts.extend(s),
                Err(d) => diags.push(d),
            },
            NodeOrToken::Token(t) => {
                diags.push(util::diag(
                    "END_OF_INSTRUCTION_EXPECTED",
                    util::span_of_range(t.text_range()),
                    "unexpected token at file scope",
                ));
            }
        }
    }

    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(HirFile::new(stmts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HirAssignOp, HirBinOp, HirExpr, HirFile, HirForStep, HirStmt, HirUnaryOp};
    use leekscript_lexer::{Lexer, LexerConfig};
    use leekscript_parser::parse_file_green;

    fn compile_hir(src: &str) -> HirFile {
        let (tok, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tok).expect("parse");
        lower_file(src, &root, 4).expect("hir")
    }

    #[test]
    fn var_and_arithmetic() {
        let hir = compile_hir("var x = 1 + 2 * 3;\n");
        let HirStmt::Var { name, init, .. } = &hir.stmts[0] else {
            panic!("expected var");
        };
        assert_eq!(name.name, "x");
        let Some(init) = init.as_ref() else {
            panic!("expected initializer");
        };
        let HirExpr::Binary { op, left, right } = init else {
            panic!("expected +");
        };
        assert_eq!(*op, HirBinOp::Add);
        assert!(matches!(left.as_ref(), HirExpr::Integer(n) if *n == 1));
        let HirExpr::Binary {
            op: op2,
            left: l,
            right: r,
        } = right.as_ref()
        else {
            panic!("expected *");
        };
        assert_eq!(*op2, HirBinOp::Mul);
        assert!(matches!(l.as_ref(), HirExpr::Integer(n) if *n == 2));
        assert!(matches!(r.as_ref(), HirExpr::Integer(n) if *n == 3));
    }

    #[test]
    fn return_void() {
        let hir = compile_hir("return;\n");
        assert!(matches!(
            hir.stmts[0],
            HirStmt::Return {
                value: None,
                if_truthy: false,
                by_ref: false,
            }
        ));
    }

    #[test]
    fn block_nested() {
        let hir = compile_hir("{ return 1; }\n");
        let HirStmt::Block(stmts) = &hir.stmts[0] else {
            panic!("expected block");
        };
        let HirStmt::Return {
            value: Some(HirExpr::Integer(n)),
            if_truthy: false,
            by_ref: false,
        } = &stmts[0]
        else {
            panic!("expected return 1");
        };
        assert_eq!(*n, 1);
    }

    #[test]
    fn if_stmt() {
        let hir = compile_hir("if (1) { return 2; } else { return 3; }\n");
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = &hir.stmts[0]
        else {
            panic!("expected if");
        };
        assert!(matches!(cond, HirExpr::Integer(n) if *n == 1));
        assert!(matches!(
            then_body.as_slice(),
            [HirStmt::Return {
                value: Some(HirExpr::Integer(n)),
                if_truthy: false,
                by_ref: false,
            }] if *n == 2
        ));
        let els = else_body.as_ref().expect("else");
        assert!(matches!(
            els.as_slice(),
            [HirStmt::Return {
                value: Some(HirExpr::Integer(n)),
                if_truthy: false,
                by_ref: false,
            }] if *n == 3
        ));
    }

    #[test]
    fn while_stmt() {
        let hir = compile_hir("while (0) { return 1; }\n");
        let HirStmt::While { cond, body } = &hir.stmts[0] else {
            panic!("expected while");
        };
        assert!(matches!(cond, HirExpr::Integer(n) if *n == 0));
        assert!(matches!(
            body.as_slice(),
            [HirStmt::Return {
                value: Some(HirExpr::Integer(n)),
                if_truthy: false,
                by_ref: false,
            }] if *n == 1
        ));
    }

    #[test]
    fn assign_stmt() {
        let hir = compile_hir("var x = 0;\nx = 3;\n");
        let HirStmt::Expr(HirExpr::AssignExpr {
            place, op, value, ..
        }) = &hir.stmts[1]
        else {
            panic!("expected expression-statement assign");
        };
        assert_eq!(*op, HirAssignOp::Assign);
        assert!(matches!(place.as_ref(), HirExpr::Ident { name, .. } if name == "x"));
        assert!(matches!(value.as_ref(), HirExpr::Integer(n) if *n == 3));
    }

    #[test]
    fn comparison_precedence() {
        let hir = compile_hir("var x = 1 + 2 == 3;\n");
        let HirStmt::Var { init, .. } = &hir.stmts[0] else {
            panic!("expected var");
        };
        let Some(init) = init.as_ref() else {
            panic!("expected initializer");
        };
        let HirExpr::Binary { op, left, right } = init else {
            panic!("expected binary at root of init");
        };
        assert_eq!(*op, HirBinOp::Eq);
        assert!(matches!(
            left.as_ref(),
            HirExpr::Binary {
                op: HirBinOp::Add,
                ..
            }
        ));
        assert!(matches!(right.as_ref(), HirExpr::Integer(n) if *n == 3));
    }

    #[test]
    fn logical_or_binds_looser_than_and() {
        let hir = compile_hir("var x = true || false && false;\n");
        let HirStmt::Var { init, .. } = &hir.stmts[0] else {
            panic!("expected var");
        };
        let Some(init) = init.as_ref() else {
            panic!("expected initializer");
        };
        let HirExpr::Binary { op, left, right } = init else {
            panic!("expected binary");
        };
        assert_eq!(*op, HirBinOp::LogicalOr);
        assert!(matches!(left.as_ref(), HirExpr::Bool(true)));
        let HirExpr::Binary {
            op: inner_op,
            left: il,
            right: ir,
        } = right.as_ref()
        else {
            panic!("expected && on rhs of ||");
        };
        assert_eq!(*inner_op, HirBinOp::LogicalAnd);
        assert!(matches!(il.as_ref(), HirExpr::Bool(false)));
        assert!(matches!(ir.as_ref(), HirExpr::Bool(false)));
    }

    #[test]
    fn unary_minus() {
        let hir = compile_hir("var x = -1;\n");
        let HirStmt::Var { init, .. } = &hir.stmts[0] else {
            panic!("expected var");
        };
        let Some(init) = init.as_ref() else {
            panic!("expected initializer");
        };
        let HirExpr::Unary { op, expr } = init else {
            panic!("expected unary");
        };
        assert_eq!(*op, HirUnaryOp::Neg);
        assert!(matches!(expr.as_ref(), HirExpr::Integer(n) if *n == 1));
    }

    #[test]
    fn for_stmt_lowering() {
        let hir = compile_hir("for (var i = 0; i < 2; i = i + 1) { var t = 0; }\n");
        let HirStmt::For {
            init,
            cond,
            update,
            body,
        } = &hir.stmts[0]
        else {
            panic!("expected for");
        };
        assert!(matches!(init.as_deref(), Some(HirStmt::Var { .. })));
        let Some(HirExpr::Binary { op, .. }) = cond else {
            panic!("expected cond");
        };
        assert_eq!(*op, HirBinOp::Lt);
        let Some(HirForStep::Assign(upd)) = update else {
            panic!("expected assign update");
        };
        assert_eq!(upd.op, HirAssignOp::Assign);
        assert_eq!(upd.name.name, "i");
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn for_stmt_postfix_update_lowering() {
        let hir = compile_hir("for (var i = 0; i < 2; i++) { }\n");
        let HirStmt::For { update, .. } = &hir.stmts[0] else {
            panic!("expected for");
        };
        let Some(HirForStep::Expr(expr)) = update else {
            panic!("expected expr update step");
        };
        let HirExpr::PostUpdate {
            target, increment, ..
        } = expr
        else {
            panic!("expected postfix ++/--");
        };
        assert!(*increment);
        assert!(matches!(
            target.as_ref(),
            HirExpr::Ident { name, .. } if name == "i"
        ));
    }

    #[test]
    fn for_stmt_prefix_update_lowering() {
        let hir = compile_hir("for (var i = 0; i < 2; ++i) { }\n");
        let HirStmt::For { update, .. } = &hir.stmts[0] else {
            panic!("expected for");
        };
        let Some(HirForStep::Expr(expr)) = update else {
            panic!("expected expr update step");
        };
        let HirExpr::PreUpdate {
            target, increment, ..
        } = expr
        else {
            panic!("expected prefix ++/--");
        };
        assert!(*increment);
        assert!(matches!(
            target.as_ref(),
            HirExpr::Ident { name, .. } if name == "i"
        ));
    }

    #[test]
    fn word_operators_lower_like_symbolic_logic() {
        let hir = compile_hir("var x = a and b or c;\n");
        let HirStmt::Var { init, .. } = &hir.stmts[0] else {
            panic!("expected var");
        };
        let Some(init) = init.as_ref() else {
            panic!("expected initializer");
        };
        let HirExpr::Binary {
            op, left, right, ..
        } = init
        else {
            panic!("expected binary");
        };
        assert_eq!(*op, HirBinOp::LogicalOr);
        assert!(matches!(
            right.as_ref(),
            HirExpr::Ident { name, .. } if name == "c"
        ));
        let HirExpr::Binary { op: inner, .. } = left.as_ref() else {
            panic!("expected (a and b) on lhs of or");
        };
        assert_eq!(*inner, HirBinOp::LogicalAnd);
    }

    #[test]
    fn keyword_not_lowers_to_unary() {
        let hir = compile_hir("var x = not true;\n");
        let HirStmt::Var { init, .. } = &hir.stmts[0] else {
            panic!("expected var");
        };
        assert!(matches!(
            init.as_ref(),
            Some(HirExpr::Unary {
                op: HirUnaryOp::Not,
                ..
            })
        ));
    }

    #[test]
    fn do_while_lowering() {
        let hir = compile_hir("do { var x = 1; } while (0);\n");
        let HirStmt::DoWhile { body, cond } = &hir.stmts[0] else {
            panic!("expected do-while");
        };
        assert!(matches!(cond, HirExpr::Integer(n) if *n == 0));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn switch_lowering() {
        let hir = compile_hir("switch (1) {\ncase 1:\n return 2;\ndefault:\n return 3;\n}\n");
        let HirStmt::Switch { discr, clauses } = &hir.stmts[0] else {
            panic!("expected switch");
        };
        assert!(matches!(discr, HirExpr::Integer(n) if *n == 1));
        assert_eq!(clauses.len(), 2);
    }

    #[test]
    fn for_in_single_stmt_body_lowering() {
        let hir = compile_hir("for (var x in [1]) return x;\n");
        let HirStmt::ForIn { body, .. } = &hir.stmts[0] else {
            panic!("expected for-in");
        };
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn for_in_lowering() {
        let hir = compile_hir("for (var x in [1, 2]) { return x; }\n");
        let HirStmt::ForIn {
            name,
            is_declaration,
            name_by_ref,
            container,
            body,
        } = &hir.stmts[0]
        else {
            panic!("expected for-in");
        };
        assert_eq!(name.name, "x");
        assert!(*is_declaration);
        assert!(!*name_by_ref);
        assert!(matches!(
            container,
            HirExpr::ArrayLiteral { elements, .. } if elements.len() == 2
        ));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn for_in_typed_lowering() {
        let hir = compile_hir("for (integer x in [1]) { return x; }\n");
        let HirStmt::ForIn {
            name,
            is_declaration,
            ..
        } = &hir.stmts[0]
        else {
            panic!("expected for-in");
        };
        assert_eq!(name.name, "x");
        assert!(*is_declaration);
    }

    #[test]
    fn for_in_key_value_lowering() {
        let hir = compile_hir("for (var i : var v in [3, 4]) { return i + v; }\n");
        let HirStmt::ForInKeyValue {
            key,
            key_is_declaration,
            key_by_ref,
            value,
            value_is_declaration,
            value_by_ref,
            container,
            body,
        } = &hir.stmts[0]
        else {
            panic!("expected for key:value in");
        };
        assert_eq!(key.name, "i");
        assert_eq!(value.name, "v");
        assert!(*key_is_declaration);
        assert!(*value_is_declaration);
        assert!(!*key_by_ref);
        assert!(!*value_by_ref);
        assert!(matches!(
            container,
            HirExpr::ArrayLiteral { elements, .. } if elements.len() == 2
        ));
        assert_eq!(body.len(), 1);
    }
}
