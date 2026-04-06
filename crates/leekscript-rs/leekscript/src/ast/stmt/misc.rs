use super::Block;
use super::TemplateParams;
use super::params::{FnParam, fn_param_children};
use crate::Span;
use crate::ast::binding_name::is_lexical_binding_name;
use crate::ast::expr::Expr;
use crate::ast::literal::LitStr;
use crate::ast::types::TypeExpr;
use crate::syntax::kinds::{Lex, Node};
use crate::syntax::syntax_el_is_trivia;
use crate::syntax::{ParsedDoxygen, attached_docstring, attached_parsed_doxygen};
use sipha::AstNode;
use sipha::prelude::*;
use sipha::tree::ast::{AstNodeExt, AstTokenExt};
use sipha::tree::red::SyntaxElement;
use sipha::types::IntoSyntaxKind;

/// Empty statement: a single `;`.
#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::EmptyStmt)]
pub struct EmptyStmt(SyntaxNode);

/// Parse error placeholder (recovery mode); empty CST node at the error offset.
#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ErrorStmt)]
pub struct ErrorStmt(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::GlobalDecl)]
pub struct GlobalDecl(SyntaxNode);

impl GlobalDecl {
    /// Leading Doxygen doc comment (`/** … */`, `/// …`) on this declaration, if any.
    #[must_use]
    pub fn docstring(&self) -> Option<String> {
        attached_docstring(self.syntax())
    }

    #[must_use]
    pub fn parsed_docstring(&self) -> Option<ParsedDoxygen> {
        attached_parsed_doxygen(self.syntax())
    }

    /// Optional type in `T name` form (`global integer x` — not `global x: integer`).
    pub fn type_expr(&self) -> Option<TypeExpr> {
        self.syntax().child::<TypeExpr>()
    }

    pub fn first_name(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == Lex::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }
}

/// One member inside a class body (`Node::ClassMember`): field, method, or constructor.
#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ClassMember)]
pub struct ClassMember(SyntaxNode);

impl ClassMember {
    /// Leading Doxygen doc comment (`/** … */`, `/// …`) on this declaration, if any.
    #[must_use]
    pub fn docstring(&self) -> Option<String> {
        attached_docstring(self.syntax())
    }

    /// `constructor () { }` / `m () { }` — has a `{ … }` body.
    #[must_use]
    pub fn has_method_body(&self) -> bool {
        self.syntax()
            .child_nodes()
            .any(|n| n.kind_as::<Node>() == Some(Node::Block))
    }

    #[must_use]
    pub fn is_constructor(&self) -> bool {
        self.syntax()
            .descendant_tokens()
            .iter()
            .any(|t| t.kind_as::<Lex>() == Some(Lex::ConstructorKw))
    }

    /// Parameters inside `( … )` for methods and constructors.
    pub fn fn_params(&self) -> impl Iterator<Item = FnParam> + '_ {
        fn_param_children(self.syntax())
    }

    /// Leading type when spelled (`integer n`, method return type before name, …).
    #[must_use]
    pub fn leading_type_expr(&self) -> Option<TypeExpr> {
        self.syntax().child::<TypeExpr>()
    }

    /// `static` field or method (leading modifier before the member name / type).
    #[must_use]
    pub fn is_static(&self) -> bool {
        self.syntax()
            .descendant_tokens()
            .into_iter()
            .any(|t| t.kind_as::<Lex>() == Some(Lex::StaticKw))
    }

    /// Constructor uses `class_name`; methods use the `ident` before `(`.
    pub fn method_name_and_span(&self, class_name: &str) -> Option<(String, Span)> {
        if self.is_constructor() {
            let span = self
                .syntax()
                .descendant_tokens()
                .iter()
                .find(|t| t.kind_as::<Lex>() == Some(Lex::ConstructorKw))
                .map(|t| t.text_range())
                .unwrap_or_else(|| Span::new(0, 0));
            return Some((class_name.to_string(), span));
        }
        method_name_ident_before_params(self.syntax())
    }
}

fn method_name_ident_before_params(
    syntax: &sipha::tree::red::SyntaxNode,
) -> Option<(String, Span)> {
    let children: Vec<SyntaxElement> = syntax
        .children()
        .filter(|e| !syntax_el_is_trivia(e))
        .collect();
    let lparen_idx = children.iter().position(|e| {
        e.as_token()
            .is_some_and(|t| t.kind() == Lex::LParen.into_syntax_kind())
    })?;
    let mut out = None;
    for el in &children[..lparen_idx] {
        if let Some(t) = el.as_token() {
            if let Some(k) = t.kind_as::<Lex>() {
                if is_lexical_binding_name(k) {
                    out = Some((t.text().to_string(), t.text_range()));
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ClassDecl)]
pub struct ClassDecl(SyntaxNode);

impl ClassDecl {
    /// Leading Doxygen doc comment (`/** … */`, `/// …`) on this declaration, if any.
    #[must_use]
    pub fn docstring(&self) -> Option<String> {
        attached_docstring(self.syntax())
    }

    #[must_use]
    pub fn parsed_docstring(&self) -> Option<ParsedDoxygen> {
        attached_parsed_doxygen(self.syntax())
    }

    /// Class name (`class` … `{`); stops before `extends` if present.
    pub fn name(&self) -> Option<String> {
        let ext = Lex::ExtendsKw.into_syntax_kind();
        let id = Lex::Ident.into_syntax_kind();
        for t in self.syntax().child_tokens() {
            let k = t.kind();
            if k == ext {
                break;
            }
            if k == id {
                return Some(t.text().to_string());
            }
        }
        None
    }

    /// Template parameters `class C<T> { }` / `class C<T> extends …` when present (experimental).
    #[must_use]
    pub fn template_params(&self) -> Option<TemplateParams> {
        self.syntax().child::<TemplateParams>()
    }

    /// Superclass name after `extends`, if any.
    pub fn extends(&self) -> Option<String> {
        let ext = Lex::ExtendsKw.into_syntax_kind();
        let mut after_extends = false;
        for t in self.syntax().child_tokens() {
            let k = t.kind();
            if k == ext {
                after_extends = true;
                continue;
            }
            if after_extends {
                if let Some(lx) = t.kind_as::<Lex>() {
                    if matches!(
                        lx,
                        Lex::Ident
                            | Lex::ArrayKw
                            | Lex::MapKw
                            | Lex::ObjectKw
                            | Lex::SetTypeKw
                            | Lex::FunctionTypeKw
                            | Lex::IntervalKw
                            | Lex::ClassTypeKw
                            | Lex::StringTypeKw
                            | Lex::IntegerKw
                            | Lex::RealKw
                            | Lex::BooleanKw
                            | Lex::AnyKw
                            | Lex::VoidKw
                    ) {
                        return Some(t.text().to_string());
                    }
                }
            }
        }
        None
    }

    pub fn body(&self) -> Option<Block> {
        self.syntax().child::<Block>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ConstDecl)]
pub struct ConstDecl(SyntaxNode);

impl ConstDecl {
    /// Leading Doxygen doc comment (`/** … */`, `/// …`) on this declaration, if any.
    #[must_use]
    pub fn docstring(&self) -> Option<String> {
        attached_docstring(self.syntax())
    }

    #[must_use]
    pub fn parsed_docstring(&self) -> Option<ParsedDoxygen> {
        attached_parsed_doxygen(self.syntax())
    }

    pub fn first_name(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == Lex::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ElseStmt)]
pub struct ElseStmt(SyntaxNode);

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::TryStmt)]
pub struct TryStmt(SyntaxNode);

impl TryStmt {
    pub fn try_block(&self) -> Option<Block> {
        self.syntax().child::<Block>()
    }

    pub fn catch_clauses(&self) -> impl Iterator<Item = CatchClause> + '_ {
        AstNodeExt::children::<CatchClause>(self.syntax())
    }

    /// `finally { ... }` when present (second direct `Block` after the `try` body).
    pub fn finally_block(&self) -> Option<Block> {
        let mut it = AstNodeExt::children::<Block>(self.syntax());
        let _try_body = it.next()?;
        it.next()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::CatchClause)]
pub struct CatchClause(SyntaxNode);

impl CatchClause {
    /// `T` in `catch (T name) { … }` (type before the binding).
    pub fn param_type(&self) -> Option<TypeExpr> {
        self.syntax().child::<TypeExpr>()
    }

    pub fn param_name(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == Lex::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }

    pub fn block(&self) -> Option<Block> {
        self.syntax().child::<Block>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ThrowStmt)]
pub struct ThrowStmt(SyntaxNode);

impl ThrowStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ImportStmt)]
pub struct ImportStmt(SyntaxNode);

impl ImportStmt {
    pub fn string_path(&self) -> Option<LitStr> {
        self.syntax().token_ast::<LitStr>()
    }

    /// `import foo.bar` segments when not using a string literal.
    pub fn name_segments(&self) -> Option<Vec<String>> {
        if self.string_path().is_some() {
            return None;
        }
        let segs: Vec<_> = self
            .syntax()
            .child_tokens()
            .filter(|t| t.kind() == Lex::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
            .collect();
        if segs.is_empty() { None } else { Some(segs) }
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::ExportStmt)]
pub struct ExportStmt(SyntaxNode);

impl ExportStmt {
    pub fn block(&self) -> Option<Block> {
        self.syntax().child::<Block>()
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::GotoStmt)]
pub struct GotoStmt(SyntaxNode);

impl GotoStmt {
    pub fn label(&self) -> Option<String> {
        self.syntax()
            .child_tokens()
            .find(|t| t.kind() == Lex::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::PackageStmt)]
pub struct PackageStmt(SyntaxNode);

impl PackageStmt {
    pub fn segments(&self) -> impl Iterator<Item = String> + '_ {
        self.syntax()
            .child_tokens()
            .filter(|t| t.kind() == Lex::Ident.into_syntax_kind())
            .map(|t| t.text().to_string())
    }

    /// `a.b.c` as declared after `package`.
    pub fn qualified_name(&self) -> String {
        self.segments().collect::<Vec<_>>().join(".")
    }
}

#[derive(Debug, Clone, AstNode)]
#[ast(kind = Node::MatchStmt)]
pub struct MatchStmt(SyntaxNode);

impl MatchStmt {
    pub fn scrutinee(&self) -> Option<Expr> {
        self.syntax().child::<Expr>()
    }
}
