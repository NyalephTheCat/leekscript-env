//! Simplified LeekScript types for semantic analysis (aligned loosely with leekscript-java `Type`).

use std::fmt;

/// Inferred or declared type for expressions and symbols.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LeekTy {
    /// Not yet known or unsupported construct.
    Unknown,
    Void,
    Null,
    Any,
    Boolean,
    Integer,
    Real,
    String,
    /// Instance of a user class (`Foo` in type position / `Foo x` / `new Foo`).
    Class(String),
    /// Class object / metaclass value (`Foo` as an expression — static members, `x.class`).
    ClassObject(String),
    /// Template type parameter (`T` from `function f<T>(…)` / `class C<T>`).
    TypeParam(String),
    /// `Array<T>` / `Set<T>` element type `T` (same representation for analysis).
    Array(Box<LeekTy>),
    /// `Map<K, V>` (value type used for `m[k]`; key tracked separately when needed).
    Map(Box<LeekTy>, Box<LeekTy>),
    /// Interval bounds type (`Interval<integer>` / `Interval<real>`). Only `integer` and `real`
    /// are valid element types; anything else is normalized to `Unknown` by `LeekTy::interval_inner`.
    Interval(Box<LeekTy>),
    /// Nullable / optional wrapper (`T?`).
    Nullable(Box<LeekTy>),
    /// Union `T | U` (flattened list).
    Union(Vec<LeekTy>),
    /// Function type: `Function<P0, P1, …, R>` or `Function<P0, … => R>` in source — parameter
    /// types in order, then return type (last `TypeExpr` inside `<…>`).
    Function {
        params: Vec<LeekTy>,
        ret: Box<LeekTy>,
    },
}

impl LeekTy {
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Integer | Self::Real)
    }

    /// `integer` / `real` only — valid `Interval<T>` parameter `T`.
    #[must_use]
    pub fn is_interval_element(ty: &Self) -> bool {
        matches!(ty, Self::Integer | Self::Real)
    }

    /// Clamp interval type argument to supported element types.
    #[must_use]
    pub fn interval_inner(inner: Self) -> Self {
        if Self::is_interval_element(&inner) {
            inner
        } else {
            Self::Unknown
        }
    }

    /// Classify a numeric literal token text (`1`, `1.0`, `1e2`, …).
    #[must_use]
    pub fn from_number_literal_text(s: &str) -> Self {
        if s.contains('.') || s.contains('e') || s.contains('E') {
            Self::Real
        } else {
            Self::Integer
        }
    }

    /// Java-style numeric widening: `integer` ↔ `real` in arithmetic.
    #[must_use]
    pub fn unify_binary_numeric(a: &Self, b: &Self) -> Self {
        match (a, b) {
            (Self::Real, _) | (_, Self::Real) => Self::Real,
            (Self::Integer, Self::Integer) => Self::Integer,
            _ => Self::Unknown,
        }
    }

    /// Whether `value` can be used where `expected` is required (assignment, return, call arg).
    #[must_use]
    pub fn is_assignable_to(value: &Self, expected: &Self) -> bool {
        if matches!(expected, Self::Unknown | Self::Any) {
            return true;
        }
        if matches!(value, Self::Unknown) {
            return true;
        }
        if matches!(expected, Self::Any) {
            return true;
        }
        match (value, expected) {
            (a, b) if a == b => true,
            (val, Self::TypeParam(exp)) => match val {
                Self::TypeParam(v) => v == exp,
                _ => true,
            },
            (Self::Null, Self::Nullable(_)) => true,
            (Self::Null, Self::Any) => true,
            (Self::Integer, Self::Real) => true,
            (Self::Real, Self::Integer) => true,
            // A value of union type may be any branch at runtime; the target must accept every branch.
            (Self::Union(parts), exp) => parts.iter().all(|p| Self::is_assignable_to(p, exp)),
            (val, Self::Union(parts)) => parts.iter().any(|p| Self::is_assignable_to(val, p)),
            (Self::Nullable(inner), exp) => {
                Self::is_assignable_to(inner, exp)
                    || matches!(exp, Self::Nullable(e) if Self::is_assignable_to(inner, e))
            }
            (val, Self::Nullable(inner)) => {
                matches!(val, Self::Null) || Self::is_assignable_to(val, inner)
            }
            // Signature stubs use `Array<U>` / `Map<T, U>`; treat unknown template args as compatible.
            (Self::Array(e), Self::Array(exp)) => {
                matches!(**e, Self::TypeParam(_)) || Self::is_assignable_to(e, exp)
            }
            (Self::Map(ek, ev), Self::Map(xk, xv)) => {
                (matches!(**ek, Self::TypeParam(_)) || Self::is_assignable_to(ek, xk))
                    && (matches!(**ev, Self::TypeParam(_)) || Self::is_assignable_to(ev, xv))
            }
            (Self::Interval(vi), Self::Interval(ei)) => {
                Self::is_interval_element(vi)
                    && Self::is_interval_element(ei)
                    && Self::is_assignable_to(vi, ei)
            }
            (Self::Class(a), Self::Class(b)) => a == b,
            (Self::ClassObject(a), Self::ClassObject(b)) => a == b,
            (
                Self::Function {
                    params: vp,
                    ret: vr,
                },
                Self::Function {
                    params: ep,
                    ret: er,
                },
            ) => {
                if vp.len() != ep.len() {
                    return false;
                }
                ep.iter()
                    .zip(vp.iter())
                    .all(|(e_param, v_param)| Self::is_assignable_to(e_param, v_param))
                    && Self::is_assignable_to(vr, er)
            }
            _ => false,
        }
    }

    /// Best-effort join of two inferred types (ternary branches, array elements, map entry types).
    #[must_use]
    pub fn unify_inference(lhs: &Self, rhs: &Self) -> Self {
        if lhs == rhs {
            return lhs.clone();
        }
        match (lhs, rhs) {
            (Self::Unknown, o) | (o, Self::Unknown) => o.clone(),
            (Self::TypeParam(_), o) | (o, Self::TypeParam(_)) => o.clone(),
            (Self::Integer, Self::Real) | (Self::Real, Self::Integer) => Self::Real,
            _ => Self::Unknown,
        }
    }

    /// Join types from `cond ? a : b` (then/else arms): same as [`Self::unify_inference`] for unknown
    /// / type params / numeric widening; otherwise a [`Self::Union`] of the two arms.
    #[must_use]
    pub fn ternary_inference(lhs: &Self, rhs: &Self) -> Self {
        if lhs == rhs {
            return lhs.clone();
        }
        match (lhs, rhs) {
            (Self::Unknown, o) | (o, Self::Unknown) => o.clone(),
            (Self::TypeParam(_), o) | (o, Self::TypeParam(_)) => o.clone(),
            (a, b) if a.is_numeric() && b.is_numeric() => Self::unify_binary_numeric(a, b),
            (a, b) => Self::Union(vec![a.clone(), b.clone()]),
        }
    }

    /// Apply numeric / null coercion for binary operators (`+`, `-`, …).
    #[must_use]
    pub fn coerce_binary_op(lhs: &Self, rhs: &Self) -> Self {
        if lhs.is_numeric() && rhs.is_numeric() {
            return Self::unify_binary_numeric(lhs, rhs);
        }
        if matches!((lhs, rhs), (Self::String, _) | (_, Self::String)) {
            return Self::String;
        }
        Self::Unknown
    }

    /// Fold `null` into [`Self::Nullable`]: `T | null`, `null | T`, and `A | B | null` become
    /// `T?`, `T?`, and `(A | B)?` respectively (recursive through type constructors).
    #[must_use]
    pub fn normalize_null_in_union(self) -> Self {
        match self {
            Self::Union(parts) => {
                let parts: Vec<Self> = parts
                    .into_iter()
                    .map(Self::normalize_null_in_union)
                    .collect();
                let mut non_null = Vec::new();
                let mut saw_null = false;
                for p in parts {
                    if matches!(p, Self::Null) {
                        saw_null = true;
                    } else {
                        non_null.push(p);
                    }
                }
                if saw_null && !non_null.is_empty() {
                    let inner = if non_null.len() == 1 {
                        non_null.into_iter().next().expect("non_empty")
                    } else {
                        Self::Union(non_null)
                    };
                    Self::Nullable(Box::new(inner))
                } else if saw_null && non_null.is_empty() {
                    Self::Null
                } else if non_null.len() == 1 {
                    non_null.into_iter().next().expect("one")
                } else {
                    Self::Union(non_null)
                }
            }
            Self::Nullable(inner) => match (*inner).clone().normalize_null_in_union() {
                Self::Nullable(i) => Self::Nullable(i),
                o => Self::Nullable(Box::new(o)),
            },
            Self::Array(el) => Self::Array(Box::new((*el).clone().normalize_null_in_union())),
            Self::Map(k, v) => Self::Map(
                Box::new((*k).clone().normalize_null_in_union()),
                Box::new((*v).clone().normalize_null_in_union()),
            ),
            Self::Interval(inner) => {
                Self::Interval(Box::new((*inner).clone().normalize_null_in_union()))
            }
            Self::Function { params, ret } => Self::Function {
                params: params
                    .into_iter()
                    .map(Self::normalize_null_in_union)
                    .collect(),
                ret: Box::new((*ret).clone().normalize_null_in_union()),
            },
            other => other,
        }
    }

    /// When `expr instanceof ClassName` is **false**: remove that class arm from unions, or map a lone
    /// `Class(class_name)` to [`Self::Unknown`]. Returns [`None`] if this type is unchanged (no
    /// matching class branch, or type params / unsupported shapes).
    #[must_use]
    pub fn exclude_instanceof_class(&self, excluded: &str) -> Option<Self> {
        match self {
            Self::Union(parts) => {
                let filtered: Vec<Self> = parts
                    .iter()
                    .filter(|p| !matches!(p, Self::Class(c) if c == excluded))
                    .cloned()
                    .collect();
                if filtered.len() == parts.len() {
                    None
                } else {
                    Some(match filtered.len() {
                        0 => Self::Unknown,
                        1 => filtered[0].clone(),
                        _ => Self::Union(filtered),
                    })
                }
            }
            Self::Nullable(inner) => match inner.exclude_instanceof_class(excluded) {
                Some(i) => {
                    if matches!(i, Self::Unknown) {
                        Some(Self::Unknown)
                    } else {
                        Some(Self::Nullable(Box::new(i)))
                    }
                }
                None => None,
            },
            Self::Class(c) if c == excluded => Some(Self::Unknown),
            Self::Class(_) | Self::TypeParam(_) => None,
            _ => None,
        }
    }

    /// For `x != null` style narrowing: `T?` → `T`, `T | null` → `T`, plain `T` unchanged as `Some(T)`.
    #[must_use]
    pub fn non_null_variant(&self) -> Option<Self> {
        match self {
            Self::Nullable(inner) => Some((**inner).clone()),
            Self::Union(parts) => {
                let filtered: Vec<Self> = parts
                    .iter()
                    .filter(|p| !matches!(p, Self::Null))
                    .cloned()
                    .collect();
                match filtered.len() {
                    0 => None,
                    1 => Some(filtered[0].clone()),
                    _ => Some(Self::Union(filtered)),
                }
            }
            Self::Null => None,
            Self::TypeParam(_) => Some(self.clone()),
            other => Some(other.clone()),
        }
    }
}

fn type_needs_parens_in_union(ty: &LeekTy) -> bool {
    matches!(ty, LeekTy::Union(_) | LeekTy::Function { .. })
}

fn type_needs_parens_after_nullable(ty: &LeekTy) -> bool {
    matches!(ty, LeekTy::Union(_) | LeekTy::Function { .. })
}

impl fmt::Display for LeekTy {
    /// Source-like spelling for diagnostics (not guaranteed to round-trip parse).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => f.write_str("?"),
            Self::Void => f.write_str("void"),
            Self::Null => f.write_str("null"),
            Self::Any => f.write_str("any"),
            Self::Boolean => f.write_str("boolean"),
            Self::Integer => f.write_str("integer"),
            Self::Real => f.write_str("real"),
            Self::String => f.write_str("string"),
            Self::Class(name) => f.write_str(name),
            Self::ClassObject(name) => write!(f, "Class<{name}>"),
            Self::TypeParam(name) => f.write_str(name),
            Self::Array(el) => write!(f, "Array<{el}>"),
            Self::Map(k, v) => write!(f, "Map<{k}, {v}>"),
            Self::Interval(inner) => write!(f, "Interval<{inner}>"),
            Self::Nullable(inner) => {
                if type_needs_parens_after_nullable(inner) {
                    write!(f, "({inner})?")
                } else {
                    write!(f, "{inner}?")
                }
            }
            Self::Union(parts) => {
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" | ")?;
                    }
                    if type_needs_parens_in_union(p) {
                        write!(f, "({p})")?;
                    } else {
                        write!(f, "{p}")?;
                    }
                }
                Ok(())
            }
            Self::Function { params, ret } => {
                f.write_str("Function<")?;
                if params.is_empty() {
                    write!(f, "{ret}")?;
                } else {
                    for (i, p) in params.iter().enumerate() {
                        if i > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{p}")?;
                    }
                    write!(f, " => {ret}")?;
                }
                f.write_str(">")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LeekTy;

    #[test]
    fn function_type_contravariant_params_covariant_ret() {
        // `(real) -> void` may be used where `(integer) -> void` is required:
        // callers pass integer, parameter accepts real.
        let value = LeekTy::Function {
            params: vec![LeekTy::Real],
            ret: Box::new(LeekTy::Void),
        };
        let expected = LeekTy::Function {
            params: vec![LeekTy::Integer],
            ret: Box::new(LeekTy::Void),
        };
        assert!(LeekTy::is_assignable_to(&value, &expected));

        let nullary = LeekTy::Function {
            params: vec![],
            ret: Box::new(LeekTy::Void),
        };
        let unary = LeekTy::Function {
            params: vec![LeekTy::Integer],
            ret: Box::new(LeekTy::Void),
        };
        assert!(!LeekTy::is_assignable_to(&nullary, &unary));

        let returns_int = LeekTy::Function {
            params: vec![],
            ret: Box::new(LeekTy::Integer),
        };
        let needs_real_ret = LeekTy::Function {
            params: vec![],
            ret: Box::new(LeekTy::Real),
        };
        assert!(LeekTy::is_assignable_to(&returns_int, &needs_real_ret));
    }

    #[test]
    fn interval_assignable_only_for_numeric_element() {
        let i = LeekTy::Interval(Box::new(LeekTy::Integer));
        let r = LeekTy::Interval(Box::new(LeekTy::Real));
        assert!(LeekTy::is_assignable_to(&i, &r));
        assert!(LeekTy::is_assignable_to(&r, &i));

        let bad = LeekTy::Interval(Box::new(LeekTy::Unknown));
        assert!(!LeekTy::is_assignable_to(&i, &bad));
    }

    #[test]
    fn type_param_assignability() {
        let t = LeekTy::TypeParam("T".into());
        assert!(LeekTy::is_assignable_to(&LeekTy::Integer, &t));
        assert!(LeekTy::is_assignable_to(&t, &t));
        assert!(!LeekTy::is_assignable_to(&t, &LeekTy::Integer));
        assert!(!LeekTy::is_assignable_to(
            &LeekTy::TypeParam("U".into()),
            &t
        ));
    }

    #[test]
    fn union_value_assignable_only_if_all_branches_match_expected() {
        let mixed = LeekTy::Union(vec![LeekTy::Integer, LeekTy::String]);
        assert!(!LeekTy::is_assignable_to(&mixed, &LeekTy::Real));
        let nums = LeekTy::Union(vec![LeekTy::Integer, LeekTy::Real]);
        assert!(LeekTy::is_assignable_to(&nums, &LeekTy::Real));
    }

    #[test]
    fn ternary_inference_unions_distinct_non_numeric_types() {
        assert_eq!(
            LeekTy::ternary_inference(&LeekTy::Integer, &LeekTy::String),
            LeekTy::Union(vec![LeekTy::Integer, LeekTy::String])
        );
        assert_eq!(
            LeekTy::ternary_inference(&LeekTy::Integer, &LeekTy::Real),
            LeekTy::Real
        );
    }

    #[test]
    fn display_spellings_for_diagnostics() {
        assert_eq!(LeekTy::Integer.to_string(), "integer");
        assert_eq!(
            LeekTy::Array(Box::new(LeekTy::String)).to_string(),
            "Array<string>"
        );
        assert_eq!(
            LeekTy::Nullable(Box::new(LeekTy::Union(vec![
                LeekTy::Integer,
                LeekTy::String,
            ])))
            .to_string(),
            "(integer | string)?"
        );
    }

    #[test]
    fn exclude_instanceof_class_removes_union_arm() {
        let u = LeekTy::Union(vec![
            LeekTy::Class("GameState".into()),
            LeekTy::Class("Consequences".into()),
        ]);
        assert_eq!(
            u.exclude_instanceof_class("GameState"),
            Some(LeekTy::Class("Consequences".into()))
        );
        assert_eq!(u.exclude_instanceof_class("Other"), None);
        assert_eq!(
            LeekTy::Class("GameState".into()).exclude_instanceof_class("GameState"),
            Some(LeekTy::Unknown)
        );
    }

    #[test]
    fn non_null_variant_strips_nullable_and_union_null() {
        assert_eq!(
            LeekTy::Nullable(Box::new(LeekTy::String)).non_null_variant(),
            Some(LeekTy::String)
        );
        assert_eq!(
            LeekTy::Union(vec![LeekTy::Integer, LeekTy::Null]).non_null_variant(),
            Some(LeekTy::Integer)
        );
        assert_eq!(LeekTy::Null.non_null_variant(), None);
    }

    #[test]
    fn normalize_null_in_union_folds_null_branch_to_nullable() {
        assert_eq!(
            LeekTy::Union(vec![LeekTy::Integer, LeekTy::Null]).normalize_null_in_union(),
            LeekTy::Nullable(Box::new(LeekTy::Integer))
        );
        assert_eq!(
            LeekTy::Union(vec![LeekTy::Null, LeekTy::String]).normalize_null_in_union(),
            LeekTy::Nullable(Box::new(LeekTy::String))
        );
        assert_eq!(
            LeekTy::Union(vec![LeekTy::Integer, LeekTy::String, LeekTy::Null,])
                .normalize_null_in_union(),
            LeekTy::Nullable(Box::new(LeekTy::Union(vec![
                LeekTy::Integer,
                LeekTy::String,
            ])))
        );
        assert_eq!(
            LeekTy::Union(vec![LeekTy::Integer, LeekTy::Null])
                .normalize_null_in_union()
                .to_string(),
            "integer?"
        );
    }
}
