# Appendix A — Grammar summary (*informative*)

This appendix condenses [04-syntactic-grammar.md](../04-syntactic-grammar.md) for quick reference. **Authoritative** disambiguation remains in the workspace **parser**.

```
SourceFile ::= Item*

Item ::= Stmt | Decl

Decl ::= VarDecl | FnDecl | ClassDecl | GlobalDecl

Stmt ::= ExprStmt | Block | AssignStmt | If | While | DoWhile | For | ForIn | ForInKV
       | Switch | Try | Throw | Break | Continue | Return | Empty | Include

Expr ::= Literal | Ident | Unary | Binary | Ternary | Cast | Call | New
       | ArrayLiteral | MapLiteral | ObjectLiteral | Member | Index | ArraySlice
       | FunctionLiteral | ArrowClosure | AssignExpr | PreUpdate | PostUpdate
       | This | ClassSelf | RefTo | Null | ParenExpr
```

**Notes**

- **`ClassDecl`**, **`ObjectLiteral`**, **`ClassSelf`**: language version **≥ 2** where applicable.
- **`Throw`**, **`switch`**, **`try`**: reserved / parsed per version (see appendix B).
- **Precedence** — See ch. 04; numeric table lives in the **parser** implementation.

---

*Revision: informative grammar sketch.*
