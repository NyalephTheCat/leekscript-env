//! Approximates Java compile-time `Expression.getOperations()` for `ops(expr, n)` statement wrappers.

use super::leek_registry_ops::{fight_functions_registry_ops, leek_functions_registry_ops};
use leekscript_hir::{HirAssignOp, HirBinOp, HirExpr, HirForStep, HirUnaryOp};

#[inline]
fn call_registry_ops(name: &str) -> u64 {
    leek_functions_registry_ops(name).max(fight_functions_registry_ops(name))
}

fn call_callee_ops(callee: &HirExpr) -> (u64, Option<&str>) {
    match callee {
        HirExpr::Ident { name, .. } => (0, Some(name.as_str())),
        HirExpr::Member { base, field, .. } => (
            hir_java_expr_ops_budget(base).saturating_add(1),
            Some(field.as_str()),
        ),
        _ => (hir_java_expr_ops_budget(callee), None),
    }
}

/// Java `Expression.getOperations()`-style tree sum (registry + structural ops). Used after evaluation
/// in `ops(expr, n)`-shaped statement contexts.
pub(super) fn hir_java_expr_ops_budget(e: &HirExpr) -> u64 {
    match e {
        HirExpr::Integer(_)
        | HirExpr::Real(_)
        | HirExpr::String(_)
        | HirExpr::Bool(_)
        | HirExpr::Null
        | HirExpr::This
        | HirExpr::ClassSelf { .. } => 0,
        HirExpr::Ident { .. } => 0,
        HirExpr::RefTo { expr, .. } => hir_java_expr_ops_budget(expr),
        HirExpr::Unary { op, expr, .. } => {
            hir_java_expr_ops_budget(expr).saturating_add(unary_op_budget(*op))
        }
        HirExpr::Binary {
            op, left, right, ..
        } => match *op {
            // Java `LeekExpression` analyze sets AND/OR `operations = 0` (wrapper costs come from codegen paths).
            HirBinOp::LogicalAnd | HirBinOp::LogicalOr => 0,
            _ => hir_java_expr_ops_budget(left)
                .saturating_add(hir_java_expr_ops_budget(right))
                .saturating_add(binary_op_budget(*op)),
        },
        HirExpr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => hir_java_expr_ops_budget(cond)
            .saturating_add(hir_java_expr_ops_budget(then_expr))
            .saturating_add(hir_java_expr_ops_budget(else_expr))
            .saturating_add(1),
        HirExpr::Cast { expr, .. } => hir_java_expr_ops_budget(expr).saturating_add(1),
        HirExpr::Call { callee, args, .. } => {
            if args.is_empty() {
                if let HirExpr::Ident { name, .. } = callee.as_ref() {
                    if name == "getOperations" || name == "getInstructionCount" {
                        return 0;
                    }
                }
            }
            let mut s: u64 = args.iter().map(hir_java_expr_ops_budget).sum();
            let (callee_cost, reg_name) = call_callee_ops(callee);
            s = s.saturating_add(callee_cost);
            if let Some(n) = reg_name {
                s = s.saturating_add(call_registry_ops(n));
            }
            s
        }
        HirExpr::ArrayLiteral { elements, .. } => {
            elements.iter().map(hir_java_expr_ops_budget).sum()
        }
        HirExpr::MapLiteral { entries, .. } | HirExpr::ObjectLiteral { entries, .. } => entries
            .iter()
            .map(|(k, v)| hir_java_expr_ops_budget(k).saturating_add(hir_java_expr_ops_budget(v)))
            .sum(),
        HirExpr::New {
            type_name, args, ..
        } => {
            let mut s: u64 = args.iter().map(hir_java_expr_ops_budget).sum();
            if type_name == "Interval" {
                s = s.saturating_add(2);
            }
            s
        }
        HirExpr::Index { base, index, .. } => hir_java_expr_ops_budget(base)
            .saturating_add(hir_java_expr_ops_budget(index))
            .saturating_add(1),
        HirExpr::ArraySlice {
            base,
            start,
            end,
            step,
            ..
        } => {
            let mut s = hir_java_expr_ops_budget(base);
            if let Some(x) = start {
                s = s.saturating_add(hir_java_expr_ops_budget(x));
            }
            if let Some(x) = end {
                s = s.saturating_add(hir_java_expr_ops_budget(x));
            }
            if let Some(x) = step {
                s = s.saturating_add(hir_java_expr_ops_budget(x));
            }
            s.saturating_add(1)
        }
        HirExpr::Member { base, .. } => hir_java_expr_ops_budget(base).saturating_add(1),
        HirExpr::ArrowClosure { .. } | HirExpr::FunctionLiteral { .. } => 0,
        HirExpr::PostUpdate { target, .. } | HirExpr::PreUpdate { target, .. } => {
            hir_java_expr_ops_budget(target).saturating_add(1)
        }
        HirExpr::AssignExpr {
            place, op, value, ..
        } => hir_java_assign_ops(place.as_ref(), *op, value.as_ref()),
    }
}

/// `mCondition.getOperations()` **before** `getBoolean`, then `ConditionalBloc` does `mCondition.operations++`.
#[inline]
fn hir_java_cond_base_ops(cond: &HirExpr) -> u64 {
    match cond {
        HirExpr::Binary {
            op: HirBinOp::LogicalAnd | HirBinOp::LogicalOr,
            ..
        } => 0,
        _ => hir_java_expr_ops_budget(cond),
    }
}

/// Outer `ops(getBoolean(cond), n)` count for `if` / `else if` (`n` includes the `++` from `ConditionalBloc`).
#[inline]
pub(super) fn hir_java_cond_outer_charge(cond: &HirExpr) -> u64 {
    hir_java_cond_base_ops(cond).saturating_add(1)
}

/// `while` / `for` header: `ops(getBoolean(cond), mCondition.getOperations())` with **no** `ConditionalBloc` ++.
#[inline]
pub(super) fn hir_java_loop_cond_outer_charge(cond: &HirExpr) -> u64 {
    hir_java_cond_base_ops(cond)
}

/// `=` / `+=` / … — mirrors `LeekExpression` assignment `getOperations()` (place + value + assign + compound op).
pub(super) fn hir_java_assign_ops(place: &HirExpr, op: HirAssignOp, value: &HirExpr) -> u64 {
    use HirAssignOp::*;
    let p = hir_java_expr_ops_budget(place);
    let v = hir_java_expr_ops_budget(value);
    let compound = match op {
        Assign => 0,
        AddAssign => binary_op_budget(HirBinOp::Add),
        SubAssign => binary_op_budget(HirBinOp::Sub),
        MulAssign => binary_op_budget(HirBinOp::Mul),
        DivAssign => binary_op_budget(HirBinOp::Div),
        RemAssign => binary_op_budget(HirBinOp::Rem),
        PowAssign => binary_op_budget(HirBinOp::Pow),
        IntDivAssign => binary_op_budget(HirBinOp::IntDiv),
        BitAndAssign => binary_op_budget(HirBinOp::BitAnd),
        BitOrAssign => binary_op_budget(HirBinOp::BitOr),
        BitXorAssign => binary_op_budget(HirBinOp::BitXor),
        ShlAssign => binary_op_budget(HirBinOp::Shl),
        ShrAssign => binary_op_budget(HirBinOp::Shr),
        UShrAssign => binary_op_budget(HirBinOp::UShr),
    };
    p.saturating_add(v)
        .saturating_add(1)
        .saturating_add(compound)
}

pub(super) fn hir_java_for_step_ops(step: &HirForStep) -> u64 {
    match step {
        HirForStep::Expr(e) => hir_java_expr_ops_budget(e),
        HirForStep::Assign(u) => hir_java_expr_ops_budget(&u.value).saturating_add(1),
    }
}

fn unary_op_budget(op: HirUnaryOp) -> u64 {
    use HirUnaryOp::*;
    match op {
        Neg | Not | BitNot | Typeof => 1,
    }
}

fn binary_op_budget(op: HirBinOp) -> u64 {
    use HirBinOp::*;
    match op {
        Add | Sub | Eq | Ne | StrictEq | StrictNe | Lt | Le | Gt | Ge | BitAnd | BitOr | BitXor
        | Shl | Shr | UShr | NotIn | In | Instanceof => 1,
        Mul => 2,
        Div | IntDiv | Rem => 5,
        Pow => 10,
        LogicalAnd | LogicalOr | NullishCoalesce => 0,
    }
}
