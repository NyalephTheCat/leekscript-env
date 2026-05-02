# Appendix C — Diagnostic codes mapping

**Normative policy**; the **registry table** below is a **generated snapshot** of the **bundled diagnostic registry** (stable **`E####`** ↔ **`reference`** / toolchain id). Human workflow and override rules: [diagnostics-registry.md](../../reference/diagnostics-registry.md). After changing the registry dataset, re-run **`python3 scripts/gen_spec_appendices.py`** from the repository root.

## Registry snapshot (`E####` ↔ identifier ↔ band)

| Code | `reference` / `id` | Band | Primary spec (*informative*) |
|------|---------------------|------|------------------------------|
| E0101 | `INVALID_CHAR` | lexical | [03-lexical-grammar.md](../03-lexical-grammar.md) |
| E0102 | `INVALID_NUMBER` | lexical | [03-lexical-grammar.md](../03-lexical-grammar.md) |
| E0103 | `MULTIPLE_NUMERIC_SEPARATORS` | lexical | [03-lexical-grammar.md](../03-lexical-grammar.md) |
| E0104 | `STRING_NOT_CLOSED` | lexical | [03-lexical-grammar.md](../03-lexical-grammar.md) |
| E0200 | `OPENING_PARENTHESIS_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0201 | `OPENING_CURLY_BRACKET_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0202 | `PARENTHESIS_EXPECTED_AFTER_PARAMETERS` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0203 | `OPEN_BLOC_REMAINING` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0204 | `NO_BLOC_TO_CLOSE` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0205 | `END_OF_SCRIPT_UNEXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0206 | `END_OF_INSTRUCTION_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0207 | `BREAK_OUT_OF_LOOP` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0208 | `CONTINUE_OUT_OF_LOOP` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0209 | `CLOSING_PARENTHESIS_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0210 | `CLOSING_SQUARE_BRACKET_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0211 | `FUNCTION_ONLY_IN_MAIN_BLOCK` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0212 | `KEYWORD_IN_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0213 | `WHILE_EXPECTED_AFTER_DO` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0214 | `NO_IF_BLOCK` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0215 | `GLOBAL_ONLY_IN_MAIN_BLOCK` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0216 | `VAR_NAME_EXPECTED_AFTER_GLOBAL` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0217 | `PARENTHESIS_EXPECTED_AFTER_FUNCTION` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0218 | `END_OF_CLASS_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0219 | `ARROW_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0220 | `CLOSING_CHEVRON_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0221 | `COMMA_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0222 | `DOT_DOT_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0223 | `DEFAULT_ARGUMENT_NOT_END` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0224 | `CASE_OR_DEFAULT_EXPECTED` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0225 | `COLON_EXPECTED_AFTER_CASE` | parse | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| E0400 | `OPERATOR_UNEXPECTED` | expr | [08-expressions.md](../08-expressions.md) |
| E0401 | `VALUE_EXPECTED` | expr | [08-expressions.md](../08-expressions.md) |
| E0402 | `CANT_ADD_INSTRUCTION_AFTER_BREAK` | expr | [08-expressions.md](../08-expressions.md) |
| E0403 | `UNCOMPLETE_EXPRESSION` | expr | [08-expressions.md](../08-expressions.md) |
| E0404 | `INVALID_OPERATOR` | expr | [08-expressions.md](../08-expressions.md) |
| E0405 | `UNKNOWN_OPERATOR` | expr | [08-expressions.md](../08-expressions.md) |
| E0600 | `INCLUDE_ONLY_IN_MAIN_BLOCK` | include | [09-statements-and-control-flow.md](../09-statements-and-control-flow.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E0601 | `AI_NAME_EXPECTED` | include | [09-statements-and-control-flow.md](../09-statements-and-control-flow.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E0602 | `AI_NOT_EXISTING` | include | [09-statements-and-control-flow.md](../09-statements-and-control-flow.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E0603 | `NO_AI_EQUIPPED` | include | [09-statements-and-control-flow.md](../09-statements-and-control-flow.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E0604 | `INVALID_AI` | include | [09-statements-and-control-flow.md](../09-statements-and-control-flow.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E0605 | `CANNOT_LOAD_AI` | include | [09-statements-and-control-flow.md](../09-statements-and-control-flow.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E1000 | `VARIABLE_NAME_EXPECTED` | names | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E1001 | `VARIABLE_NAME_UNAVAILABLE` | names | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E1002 | `VARIABLE_NOT_EXISTS` | names | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E1003 | `KEYWORD_UNEXPECTED` | names | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E1004 | `VAR_NAME_EXPECTED` | names | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E1005 | `SIMPLE_ARRAY` | names | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E1006 | `ASSOCIATIVE_ARRAY` | names | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E1007 | `UNKNOWN_VARIABLE_OR_FUNCTION` | names | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E1200 | `FUNCTION_NAME_UNAVAILABLE` | user_fn_decl | [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| E1201 | `PARAMETER_NAME_UNAVAILABLE` | user_fn_decl | [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| E1202 | `PARAMETER_NAME_EXPECTED` | user_fn_decl | [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| E1203 | `FUNCTION_NAME_EXPECTED` | user_fn_decl | [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| E1204 | `CANNOT_REDEFINE_FUNCTION` | user_fn_decl | [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| E1205 | `DUPLICATED_ARGUMENT` | user_fn_decl | [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| E2000 | `FUNCTION_NOT_EXISTS` | builtin_api | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| E2001 | `INVALID_PARAMETER_COUNT` | builtin_api | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| E2002 | `UNKNOWN_FUNCTION` | builtin_api | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| E2003 | `REMOVED_FUNCTION` | builtin_api | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| E2004 | `FUNCTION_NOT_AVAILABLE` | builtin_api | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| E2005 | `REMOVED_FUNCTION_REPLACEMENT` | builtin_api | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| E2200 | `WRONG_ARGUMENT_TYPE` | call_shape | [08-expressions.md](../08-expressions.md), [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| E2201 | `NOT_CALLABLE` | call_shape | [08-expressions.md](../08-expressions.md), [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| E2202 | `MAY_NOT_BE_CALLABLE` | call_shape | [08-expressions.md](../08-expressions.md), [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| E3000 | `ASSIGN_SAME_VARIABLE` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3001 | `COMPARISON_ALWAYS_FALSE` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3002 | `COMPARISON_ALWAYS_TRUE` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3003 | `ASSIGNMENT_INCOMPATIBLE_TYPE` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3004 | `TYPE_EXPECTED` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3005 | `IMPOSSIBLE_CAST` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3006 | `INCOMPATIBLE_TYPE` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3007 | `DANGEROUS_CONVERSION` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3008 | `DANGEROUS_CONVERSION_VARIABLE` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3009 | `IMPOSSIBLE_CAST_VALUES` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3010 | `FIELD_MAY_NOT_EXIST` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3011 | `USELESS_NON_NULL_ASSERTION` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3012 | `OVERRIDDEN_METHOD_DIFFERENT_TYPE` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3013 | `USELESS_CAST` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3014 | `UNARY_OPERATOR_INCOMPATIBLE_TYPE` | types | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3100 | `CANT_ASSIGN_VALUE` | types_assign | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3101 | `CANNOT_ASSIGN_FINAL_FIELD` | types_assign | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3102 | `CANNOT_ASSIGN_FINAL_VALUE` | types_assign | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| E3600 | `NOT_ITERABLE` | collections | [08-expressions.md](../08-expressions.md), [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| E3601 | `MAY_NOT_BE_ITERABLE` | collections | [08-expressions.md](../08-expressions.md), [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| E3602 | `MAY_NOT_BE_INDEXABLE` | collections | [08-expressions.md](../08-expressions.md), [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| E3603 | `NOT_INDEXABLE` | collections | [08-expressions.md](../08-expressions.md), [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| E3604 | `INTERVAL` | collections | [08-expressions.md](../08-expressions.md), [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| E3605 | `OPERATOR_IN_ON_INVALID_CONTAINER` | collections | [08-expressions.md](../08-expressions.md), [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| E3606 | `CANNOT_ITERATE_UNBOUNDED_INTERVAL` | collections | [08-expressions.md](../08-expressions.md), [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| E3607 | `INTERVAL_INFINITE_CLOSED` | collections | [08-expressions.md](../08-expressions.md), [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| E4000 | `CONSTRUCTOR_ALREADY_EXISTS` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4001 | `FIELD_ALREADY_EXISTS` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4002 | `NO_SUCH_CLASS` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4003 | `CLASS_MEMBER_DOES_NOT_EXIST` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4004 | `CLASS_STATIC_MEMBER_DOES_NOT_EXIST` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4005 | `EXTENDS_LOOP` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4006 | `DUPLICATED_METHOD` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4007 | `UNKNOWN_METHOD` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4008 | `UNKNOWN_STATIC_METHOD` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4009 | `STRING_METHOD_MUST_RETURN_STRING` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4010 | `UNKNOWN_FIELD` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4011 | `UNKNOWN_CONSTRUCTOR` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4012 | `RESERVED_FIELD` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4013 | `DUPLICATED_CONSTRUCTOR` | oop_struct | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4300 | `PRIVATE_FIELD` | visibility | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4301 | `PROTECTED_FIELD` | visibility | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4302 | `PRIVATE_STATIC_FIELD` | visibility | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4303 | `PROTECTED_STATIC_FIELD` | visibility | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4304 | `PRIVATE_METHOD` | visibility | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4305 | `PROTECTED_METHOD` | visibility | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4306 | `PRIVATE_CONSTRUCTOR` | visibility | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4307 | `PROTECTED_CONSTRUCTOR` | visibility | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4308 | `PRIVATE_STATIC_METHOD` | visibility | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4309 | `PROTECTED_STATIC_METHOD` | visibility | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| E4600 | `THIS_NOT_ALLOWED_HERE` | this_super | [05-names-and-scoping.md](../05-names-and-scoping.md), [08-expressions.md](../08-expressions.md) |
| E4601 | `KEYWORD_MUST_BE_IN_CLASS` | this_super | [05-names-and-scoping.md](../05-names-and-scoping.md), [08-expressions.md](../08-expressions.md) |
| E4602 | `SUPER_NOT_AVAILABLE_PARENT` | this_super | [05-names-and-scoping.md](../05-names-and-scoping.md), [08-expressions.md](../08-expressions.md) |
| E4603 | `INSTANCEOF_MUST_BE_CLASS` | this_super | [05-names-and-scoping.md](../05-names-and-scoping.md), [08-expressions.md](../08-expressions.md) |
| E5000 | `DIVISION_BY_ZERO` | runtime_val | [07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5001 | `CAN_NOT_EXECUTE_VALUE` | runtime_val | [07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5002 | `CAN_NOT_EXECUTE_WITH_ARGUMENTS` | runtime_val | [07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5003 | `VALUE_IS_NOT_AN_ARRAY` | runtime_val | [07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5004 | `ARRAY_OUT_OF_BOUND` | runtime_val | [07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5005 | `MAP_DUPLICATED_KEY` | runtime_val | [07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5006 | `UNKNOWN_ERROR` | runtime_val | [07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5007 | `INVALID_VALUE` | runtime_val | [07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5300 | `MODIFICATION_DURING_ITERATION` | runtime_iter | [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| E5301 | `ENTITY_DIED` | runtime_iter | [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| E5600 | `CODE_TOO_LARGE` | limits | [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5601 | `CODE_TOO_LARGE_FUNCTION` | limits | [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5602 | `STACKOVERFLOW` | limits | [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5603 | `TOO_MUCH_OPERATIONS` | limits | [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5604 | `ARRAY_TOO_LARGE` | limits | [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5605 | `OUT_OF_MEMORY` | limits | [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5606 | `TOO_MUCH_ERRORS` | limits | [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E5900 | `COMPILE_JAVA` | platform | [13-interpreter-behavior.md](../13-interpreter-behavior.md), [operations docs](../../operations/) |
| E5901 | `AI_DISABLED` | platform | [13-interpreter-behavior.md](../13-interpreter-behavior.md), [operations docs](../../operations/) |
| E5902 | `AI_INTERRUPTED` | platform | [13-interpreter-behavior.md](../13-interpreter-behavior.md), [operations docs](../../operations/) |
| E5903 | `AI_TIMEOUT` | platform | [13-interpreter-behavior.md](../13-interpreter-behavior.md), [operations docs](../../operations/) |
| E5904 | `TRANSPILE_TO_JAVA` | platform | [13-interpreter-behavior.md](../13-interpreter-behavior.md), [operations docs](../../operations/) |
| E5905 | `CANNOT_WRITE_AI` | platform | [13-interpreter-behavior.md](../13-interpreter-behavior.md), [operations docs](../../operations/) |
| E5906 | `HELP_PAGE_LINK` | platform | [13-interpreter-behavior.md](../13-interpreter-behavior.md), [operations docs](../../operations/) |
| E7001 | `invalid_leek_toml` | config | [leek-toml.md](../../reference/leek-toml.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E7002 | `UNCAUGHT_THROW` | runtime_val | [07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| E7201 | `unknown_leek_directive` | directives | [12-directives-and-pragmas.md](../12-directives-and-pragmas.md) |
| E7202 | `leek_directive_invalid_value` | directives | [12-directives-and-pragmas.md](../12-directives-and-pragmas.md) |
| E8000 | `REFERENCE_DEPRECATED` | deprecated | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| E8001 | `DEPRECATED_FUNCTION` | deprecated | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| E8002 | `TRIPLE_EQUALS_DEPRECATED` | deprecated | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| E8800 | `UNUSED_VARIABLE` | lint | [diagnostics-registry.md](../../reference/diagnostics-registry.md) (*tooling*) |
| E9000 | `INTERNAL_ERROR` | ice | [13-interpreter-behavior.md](../13-interpreter-behavior.md) (*internal*) |

## Band → spec guide (*informative*)

| Band | Typical spec chapters |
|------|------------------------|
| `builtin_api` | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| `call_shape` | [08-expressions.md](../08-expressions.md), [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| `collections` | [08-expressions.md](../08-expressions.md), [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| `config` | [leek-toml.md](../../reference/leek-toml.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| `deprecated` | [11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md) |
| `directives` | [12-directives-and-pragmas.md](../12-directives-and-pragmas.md) |
| `expr` | [08-expressions.md](../08-expressions.md) |
| `ice` | [13-interpreter-behavior.md](../13-interpreter-behavior.md) (*internal*) |
| `include` | [09-statements-and-control-flow.md](../09-statements-and-control-flow.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| `lexical` | [03-lexical-grammar.md](../03-lexical-grammar.md) |
| `limits` | [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| `lint` | [diagnostics-registry.md](../../reference/diagnostics-registry.md) (*tooling*) |
| `names` | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| `oop_struct` | [05-names-and-scoping.md](../05-names-and-scoping.md) |
| `parse` | [04-syntactic-grammar.md](../04-syntactic-grammar.md) |
| `platform` | [13-interpreter-behavior.md](../13-interpreter-behavior.md), [operations docs](../../operations/) |
| `runtime_iter` | [09-statements-and-control-flow.md](../09-statements-and-control-flow.md) |
| `runtime_val` | [07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md) |
| `this_super` | [05-names-and-scoping.md](../05-names-and-scoping.md), [08-expressions.md](../08-expressions.md) |
| `types` | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| `types_assign` | [06-types-and-subtyping.md](../06-types-and-subtyping.md) |
| `user_fn_decl` | [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md) |
| `visibility` | [05-names-and-scoping.md](../05-names-and-scoping.md) |

## Static compilation phases

Phases in the **compilation API**: **Directives**, **Lexer**, **Parser**, **HIR**, **Resolve**, **Types**. Bands above map loosely to these phases (e.g. `lexical` → Lexer, `parse` → Parser, `names` → Resolve).

## Interpreter (`InterpretError`)

Stable **`reference`** strings include the interpreter’s **published emit list** and additional variants on **`InterpretError`** (e.g. **`TOO_MUCH_OPERATIONS`**, **`OUT_OF_MEMORY`**). Rows with bands **`runtime_val`**, **`runtime_iter`**, **`limits`**, **`collections`** often correspond to dynamic errors.

## PR checklist

When normative spec text introduces a **new** error condition, contributors **MUST**:

1. Add or reuse a row in the **diagnostic registry dataset**.
2. Emit that **`reference`** / id from the implementation.
3. Re-run **`python3 scripts/gen_spec_appendices.py`** and commit updated appendix C.
4. Add or extend a test cited from [E-conformance-tests-index.md](E-conformance-tests-index.md).

---

*Revision: includes generated registry table; maintain via `gen_spec_appendices`.*
