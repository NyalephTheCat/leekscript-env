//! Interpreter errors and `throw` vs hard-error distinction.

use super::value::Value;

/// Failure while executing HIR (undefined name, bad operand types, etc.).
///
/// [`reference`](Self::reference) matches a Java `Error` enum name in `data/diagnostics/registry.yaml`
/// so [`leekscript_diagnostics::Registry::code_for_reference`]
/// can resolve a stable `E####` code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpretError {
    /// Java `Error` enum name (e.g. `VARIABLE_NOT_EXISTS`).
    pub reference: &'static str,
    pub message: String,
}

impl InterpretError {
    #[must_use]
    pub fn variable_not_exists(name: &str) -> Self {
        Self {
            reference: "VARIABLE_NOT_EXISTS",
            message: format!("undefined variable `{name}`"),
        }
    }

    #[must_use]
    pub fn division_by_zero() -> Self {
        Self {
            reference: "DIVISION_BY_ZERO",
            message: "division by zero".into(),
        }
    }

    #[must_use]
    pub fn remainder_by_zero() -> Self {
        Self {
            reference: "DIVISION_BY_ZERO",
            message: "remainder by zero".into(),
        }
    }

    #[must_use]
    pub fn this_not_allowed_here() -> Self {
        Self {
            reference: "THIS_NOT_ALLOWED_HERE",
            message: "`this` is not allowed in this execution context".into(),
        }
    }

    #[must_use]
    pub fn class_self_not_allowed_here() -> Self {
        Self {
            reference: "THIS_NOT_ALLOWED_HERE",
            message: "`class` is not allowed in this execution context".into(),
        }
    }

    #[must_use]
    pub fn super_not_available_parent() -> Self {
        Self {
            reference: "SUPER_NOT_AVAILABLE_PARENT",
            message: "no superclass available for `super` call".into(),
        }
    }

    #[must_use]
    pub fn class_static_member_does_not_exist(class_name: &str, field: &str) -> Self {
        Self {
            reference: "CLASS_STATIC_MEMBER_DOES_NOT_EXIST",
            message: format!("static member `{field}` does not exist on class `{class_name}`"),
        }
    }

    #[must_use]
    pub fn cant_assign_value() -> Self {
        Self {
            reference: "CANT_ASSIGN_VALUE",
            message: "can't assign a value here".into(),
        }
    }

    #[must_use]
    pub fn cannot_redefine_function(name: &str) -> Self {
        Self {
            reference: "CANNOT_REDEFINE_FUNCTION",
            message: format!("cannot redefine function `{name}`"),
        }
    }

    #[must_use]
    pub fn incompatible_type() -> Self {
        Self {
            reference: "INCOMPATIBLE_TYPE",
            message: "incompatible type".into(),
        }
    }

    #[must_use]
    pub fn impossible_cast() -> Self {
        Self {
            reference: "IMPOSSIBLE_CAST",
            message: "impossible cast".into(),
        }
    }

    #[must_use]
    pub fn class_member_does_not_exist(class_name: &str, field: &str) -> Self {
        Self {
            reference: "CLASS_MEMBER_DOES_NOT_EXIST",
            message: format!("member `{field}` does not exist on class `{class_name}`"),
        }
    }

    #[must_use]
    pub fn wrong_operand_types_binary() -> Self {
        Self {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "binary operation requires numeric operands".into(),
        }
    }

    #[must_use]
    pub fn wrong_operand_types_compare() -> Self {
        Self {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "comparison requires compatible operand types".into(),
        }
    }

    #[must_use]
    pub fn wrong_unary_operand() -> Self {
        Self {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "unary `-` requires a number operand".into(),
        }
    }

    #[must_use]
    pub fn not_callable() -> Self {
        Self {
            reference: "NOT_CALLABLE",
            message: "value is not callable".into(),
        }
    }

    #[must_use]
    pub fn function_not_available() -> Self {
        Self {
            reference: "FUNCTION_NOT_AVAILABLE",
            message: "this function is not available for this language version".into(),
        }
    }

    /// Legacy builtin removed in Leek v4 — Java `Error.REMOVED_FUNCTION_REPLACEMENT`.
    #[must_use]
    pub fn removed_function_replacement() -> Self {
        Self {
            reference: "REMOVED_FUNCTION_REPLACEMENT",
            message: "this function was removed; use the replacement".into(),
        }
    }

    #[must_use]
    pub fn map_duplicated_key() -> Self {
        Self {
            reference: "MAP_DUPLICATED_KEY",
            message: "duplicate key in map literal".into(),
        }
    }

    #[must_use]
    pub fn invalid_parameter_count(expected: usize, got: usize) -> Self {
        Self {
            reference: "INVALID_PARAMETER_COUNT",
            message: format!("expected {expected} arguments, got {got}"),
        }
    }

    #[must_use]
    pub fn randint_empty_range() -> Self {
        Self {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "randInt requires min(a, b) < max(a, b) (half-open range [lo, hi))".into(),
        }
    }

    #[must_use]
    pub fn break_out_of_loop() -> Self {
        Self {
            reference: "BREAK_OUT_OF_LOOP",
            message: "`break` is not inside a loop".into(),
        }
    }

    #[must_use]
    pub fn continue_out_of_loop() -> Self {
        Self {
            reference: "CONTINUE_OUT_OF_LOOP",
            message: "`continue` is not inside a loop".into(),
        }
    }

    #[must_use]
    pub fn not_iterable() -> Self {
        Self {
            reference: "NOT_ITERABLE",
            message: crate::interpret_reference_display_message("NOT_ITERABLE")
                .expect("NOT_ITERABLE must have a display message")
                .into(),
        }
    }

    #[must_use]
    pub fn cannot_iterate_unbounded_interval() -> Self {
        Self {
            reference: "CANNOT_ITERATE_UNBOUNDED_INTERVAL",
            message: "cannot iterate an unbounded interval".into(),
        }
    }

    #[must_use]
    pub fn invalid_constructor(name: &str, detail: &str) -> Self {
        Self {
            reference: "WRONG_ARGUMENT_TYPE",
            message: format!("invalid `{name}` constructor: {detail}"),
        }
    }

    #[must_use]
    pub fn uncaught_throw() -> Self {
        Self {
            reference: "UNCAUGHT_THROW",
            message: "uncaught exception".into(),
        }
    }

    #[must_use]
    pub fn array_index_out_of_bounds() -> Self {
        Self {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "array index out of bounds".into(),
        }
    }

    #[must_use]
    pub fn array_out_of_bound_strict() -> Self {
        Self {
            reference: "ARRAY_OUT_OF_BOUND",
            message: "array index out of bounds".into(),
        }
    }

    #[must_use]
    pub fn not_indexable() -> Self {
        Self {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "value is not indexable".into(),
        }
    }

    #[must_use]
    pub fn member_requires_instance() -> Self {
        Self {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "`.` access requires an instance value".into(),
        }
    }

    #[must_use]
    pub fn private_field() -> Self {
        Self {
            reference: "PRIVATE_FIELD",
            message: "private field is not accessible from this context".into(),
        }
    }

    #[must_use]
    pub fn protected_field() -> Self {
        Self {
            reference: "PROTECTED_FIELD",
            message: "protected field is not accessible from this context".into(),
        }
    }

    #[must_use]
    pub fn protected_method() -> Self {
        Self {
            reference: "PROTECTED_METHOD",
            message: "protected method is not accessible from this context".into(),
        }
    }

    #[must_use]
    pub fn private_method() -> Self {
        Self {
            reference: "PRIVATE_METHOD",
            message: "private method is not accessible from this context".into(),
        }
    }

    #[must_use]
    pub fn protected_constructor() -> Self {
        Self {
            reference: "PROTECTED_CONSTRUCTOR",
            message: "protected constructor is not accessible from this context".into(),
        }
    }

    #[must_use]
    pub fn private_constructor() -> Self {
        Self {
            reference: "PRIVATE_CONSTRUCTOR",
            message: "private constructor is not accessible from this context".into(),
        }
    }

    #[must_use]
    pub fn protected_static_method() -> Self {
        Self {
            reference: "PROTECTED_STATIC_METHOD",
            message: "protected static method is not accessible from this context".into(),
        }
    }

    #[must_use]
    pub fn private_static_method() -> Self {
        Self {
            reference: "PRIVATE_STATIC_METHOD",
            message: "private static method is not accessible from this context".into(),
        }
    }

    #[must_use]
    pub fn invalid_assign_target() -> Self {
        Self {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "invalid assignment target".into(),
        }
    }

    #[must_use]
    pub fn in_operator_requires_container() -> Self {
        Self {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "`in` requires an array, map, or set on the right-hand side".into(),
        }
    }

    #[must_use]
    pub fn assignment_incompatible_type() -> Self {
        Self {
            reference: "ASSIGNMENT_INCOMPATIBLE_TYPE",
            message: "assignment incompatible type".into(),
        }
    }

    #[must_use]
    pub fn cannot_assign_final_field() -> Self {
        Self {
            reference: "CANNOT_ASSIGN_FINAL_FIELD",
            message: "cannot assign to a final field".into(),
        }
    }

    #[must_use]
    pub fn too_much_operations() -> Self {
        Self {
            reference: "TOO_MUCH_OPERATIONS",
            message: "too much operations".into(),
        }
    }

    #[must_use]
    pub fn out_of_memory() -> Self {
        Self {
            reference: "OUT_OF_MEMORY",
            message: "out of memory".into(),
        }
    }
}

impl std::fmt::Display for InterpretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for InterpretError {}

/// [`eval_expr`](crate::interp::expr::eval_expr) and friends signal `throw` separately from hard interpreter errors.
#[derive(Debug)]
pub enum ExecAbort {
    Error(InterpretError),
    Throw(Option<Value>),
}

impl From<InterpretError> for ExecAbort {
    fn from(e: InterpretError) -> Self {
        ExecAbort::Error(e)
    }
}
