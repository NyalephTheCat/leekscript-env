//! Simplified LeekScript types for semantic analysis (aligned loosely with leekscript-java `Type`).

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
    /// Named class / user type (`Foo`).
    Class(String),
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
            (Self::Null, Self::Nullable(_)) => true,
            (Self::Null, Self::Any) => true,
            (Self::Integer, Self::Real) => true,
            (Self::Real, Self::Integer) => true,
            (Self::Union(parts), exp) => parts.iter().any(|p| Self::is_assignable_to(p, exp)),
            (val, Self::Union(parts)) => parts.iter().any(|p| Self::is_assignable_to(val, p)),
            (Self::Nullable(inner), exp) => {
                Self::is_assignable_to(inner, exp)
                    || matches!(exp, Self::Nullable(e) if Self::is_assignable_to(inner, e))
            }
            (val, Self::Nullable(inner)) => {
                matches!(val, Self::Null) || Self::is_assignable_to(val, inner)
            }
            (Self::Array(e), Self::Array(exp)) => Self::is_assignable_to(e, exp),
            (Self::Map(ek, ev), Self::Map(xk, xv)) => {
                Self::is_assignable_to(ek, xk) && Self::is_assignable_to(ev, xv)
            }
            (Self::Interval(vi), Self::Interval(ei)) => {
                Self::is_interval_element(vi)
                    && Self::is_interval_element(ei)
                    && Self::is_assignable_to(vi, ei)
            }
            (Self::Class(a), Self::Class(b)) => a == b,
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
            other => Some(other.clone()),
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
}
