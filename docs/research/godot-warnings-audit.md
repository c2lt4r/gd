# Godot GDScript Warnings vs gd Lint Rules — Audit

Source: `modules/gdscript/gdscript_warning.h` (Godot master, 2026-02-14)

## Godot Warnings (43 active + 3 deprecated)

Excluding the 3 deprecated warnings (`PROPERTY_USED_AS_FUNCTION`, `CONSTANT_USED_AS_FUNCTION`, `FUNCTION_USED_AS_PROPERTY`) which are never produced.

### Matched — We cover these

| Godot Warning | Default | gd Rule | Notes |
|---------------|---------|---------|-------|
| `UNUSED_VARIABLE` | WARN | `unused-variable` | Match. We also auto-fix. |
| `UNUSED_PARAMETER` | WARN | `unused-parameter` | Match (opt-in in gd). |
| `UNUSED_SIGNAL` | WARN | `unused-signal` | Match. |
| `SHADOWED_VARIABLE` | WARN | `shadowed-variable` | Partial — we detect current-class shadowing but not base-class (`SHADOWED_VARIABLE_BASE_CLASS`). |
| `UNREACHABLE_CODE` | WARN | `unreachable-code` | Match. We also auto-fix. |
| `STANDALONE_EXPRESSION` | WARN | `standalone-expression` | Match. |
| `UNTYPED_DECLARATION` | IGNORE | `missing-type-hint` | Match. Both off by default. |
| `INFERRED_DECLARATION` | IGNORE | `static-type-inference` | Match. Godot warns on implicit `:=` inference. |
| `INTEGER_DIVISION` | WARN | `integer-division` | Match. |
| `INFERENCE_ON_VARIANT` | ERROR | `variant-inference` | **Severity mismatch**: Godot treats this as ERROR, we have it as opt-in WARNING. Consider promoting. |
| `GET_NODE_DEFAULT_WITHOUT_ONREADY` | ERROR | `node-ready-order` | Partial overlap — we detect node access before ready, Godot specifically flags `get_node()` / `$` in default values without `@onready`. |

### Gaps — Godot warns, we don't

| Godot Warning | Default | Feasibility | Notes |
|---------------|---------|-------------|-------|
| `UNASSIGNED_VARIABLE` | WARN | Layer 1 | Variable used before any assignment. Needs control flow analysis. Our `use-before-assign` is similar but limited to member variables accessed via method calls. |
| `UNASSIGNED_VARIABLE_OP_ASSIGN` | WARN | Layer 1 | `x += 1` without prior `x = ...`. Same as above but for compound assignment. |
| `UNUSED_LOCAL_CONSTANT` | WARN | Easy | Local `const` declared but never referenced. Same logic as `unused-variable` but for constants. |
| `UNUSED_PRIVATE_CLASS_VARIABLE` | WARN | Medium | Class-level `var _foo` with `_` prefix never used in the class. Cross-method analysis needed. |
| `SHADOWED_VARIABLE_BASE_CLASS` | WARN | Layer 1 | Local shadows a base class member. Needs extends-chain resolution + ClassDB. |
| `SHADOWED_GLOBAL_IDENTIFIER` | WARN | Medium | Variable name collides with a global class/function name (e.g., `var Node = ...`). Needs a list of global identifiers. |
| `UNREACHABLE_PATTERN` | WARN | Medium | Match pattern after wildcard/bind. AST walk of match statements. |
| `STANDALONE_TERNARY` | WARN | Easy | Ternary expression result discarded. Extension of `standalone-expression`. |
| `INCOMPATIBLE_TERNARY` | WARN | Layer 2 | Ternary branches return incompatible types. Needs type inference. |
| `UNSAFE_PROPERTY_ACCESS` | IGNORE | Layer 2 | Property not found on inferred type. Full type inference needed. |
| `UNSAFE_METHOD_ACCESS` | IGNORE | Layer 2 | Method not found on inferred type. Full type inference needed. |
| `UNSAFE_CAST` | IGNORE | Layer 2 | Casting Variant to non-Variant. Type system needed. |
| `UNSAFE_CALL_ARGUMENT` | IGNORE | Layer 2 | Argument is supertype of required type. Full type checking needed. |
| `UNSAFE_VOID_RETURN` | WARN | Layer 2 | Returning a call to a possibly-void function. Type system needed. |
| `RETURN_VALUE_DISCARDED` | IGNORE | Medium | Function call result ignored. Needs function return type knowledge (ClassDB has this for engine methods). |
| `STATIC_CALLED_ON_INSTANCE` | WARN | Layer 1 | `self.my_static()` instead of `MyClass.my_static()`. Needs to detect `@static` annotation. |
| `MISSING_TOOL` | WARN | Easy | Base class has `@tool`, this script doesn't. Check extends chain for `@tool` annotation. |
| `REDUNDANT_STATIC_UNLOAD` | WARN | Easy | `@static_unload` but no static variables. Check annotations + variable declarations. |
| `REDUNDANT_AWAIT` | WARN | Layer 2 | `await` on non-coroutine, non-signal expression. Needs to know if callee is coroutine. |
| `MISSING_AWAIT` | IGNORE | Layer 2 | Coroutine call without `await`. Same — needs coroutine detection. |
| `ASSERT_ALWAYS_TRUE` | WARN | Medium | `assert(true)`, `assert(1)`. Detect constant expressions in assert. |
| `ASSERT_ALWAYS_FALSE` | WARN | Medium | `assert(false)`, `assert(0)`. Same. |
| `NARROWING_CONVERSION` | WARN | Layer 2 | Float assigned to int variable. Needs type inference. |
| `INT_AS_ENUM_WITHOUT_CAST` | WARN | Layer 1 | Integer used where enum expected. Needs declared-type tracking. |
| `INT_AS_ENUM_WITHOUT_MATCH` | WARN | Layer 1 | Integer doesn't match any enum member. Needs enum value tracking. |
| `ENUM_VARIABLE_WITHOUT_DEFAULT` | WARN | Easy | `var x: MyEnum` with no `= MyEnum.VALUE`. Check variable declarations with enum types. |
| `EMPTY_FILE` | WARN | Easy | Script file has no content. Trivial. |
| `DEPRECATED_KEYWORD` | WARN | Medium | Obsolete keywords (Godot 3→4 migration). Need a list of deprecated keywords per version. |
| `CONFUSABLE_IDENTIFIER` | WARN | Medium | Homoglyph/confusable Unicode characters. Needs Unicode confusable table. |
| `CONFUSABLE_LOCAL_DECLARATION` | WARN | Medium | Parent block declares same identifier below. Forward-looking scope analysis. |
| `CONFUSABLE_LOCAL_USAGE` | WARN | Medium | Identifier will be shadowed below in same block. Same. |
| `CONFUSABLE_CAPTURE_REASSIGNMENT` | WARN | Medium | Reassigning captured lambda variable doesn't affect outer scope. Lambda analysis. |
| `NATIVE_METHOD_OVERRIDE` | ERROR | Layer 1 | Script overrides native method (won't be called by engine). Needs ClassDB method lookup. |
| `ONREADY_WITH_EXPORT` | ERROR | Easy | `@onready` + `@export` on same variable — onready overrides export. Check annotations. |

## gd-only rules — No Godot equivalent

These are rules we have that Godot's analyzer doesn't warn about:

| gd Rule | Description |
|---------|-------------|
| `await-in-ready` | Godot allows it, we flag it as a potential issue |
| `callable-null-check` | `.call()` without `.is_valid()` guard |
| `comparison-with-boolean` | `== true` / `== false` (style) |
| `comparison-with-itself` | `x == x` (logic error) |
| `cyclomatic-complexity` | Complexity metric (code quality) |
| `deeply-nested-code` | Nesting depth (code quality) |
| `duplicate-function` | Godot errors on this, we catch it at lint time |
| `duplicate-key` | Duplicate dict keys |
| `duplicate-signal` | Duplicate signal declarations |
| `duplicated-load` | Same `preload()`/`load()` called twice |
| `empty-function` | Function with only `pass` |
| `enum-naming` | PascalCase/UPPER_SNAKE enforcement |
| `enum-without-class-name` | Enum type annotation without `class_name` |
| `float-comparison` | `==` on floats |
| `get-node-in-process` | `get_node()` in `_process()` (performance) |
| `long-function` | Function length threshold |
| `loop-variable-name` | snake_case loop variables |
| `magic-number` | Unexplained numeric literals |
| `missing-return` | Missing return in typed function |
| `monitoring-in-signal` | Direct `monitoring` assignment in signal callback |
| `naming-convention` | PascalCase/snake_case enforcement |
| `node-ready-order` | Broader than Godot's `GET_NODE_DEFAULT_WITHOUT_ONREADY` |
| `null-after-await` | Member access after `await` without null guard |
| `parameter-naming` | snake_case parameter names |
| `parameter-shadows-field` | Parameter name shadows class field |
| `physics-in-process` | Physics calls in `_process()` instead of `_physics_process()` |
| `preload-type-hint` | Untyped preload assignment |
| `private-method-access` | Calling `_private()` externally |
| `redundant-else` | Else after return |
| `return-type-mismatch` | Void/non-void return mismatch |
| `self-assignment` | `x = x` |
| `signal-name-convention` | Signal naming style |
| `too-many-parameters` | Parameter count threshold |
| `unnecessary-pass` | `pass` in non-empty body |
| `untyped-array` | Untyped Array annotations |
| `untyped-array-literal` | `var x := [...]` without `Array[T]` |
| `unused-preload` | Unused preload variable |
| `use-before-assign` | Member access via method call before assignment |
| Various opt-in quality rules | `breakpoint-statement`, `class-definitions-order`, `duplicate-delegate`, `god-object`, `look-at-before-tree`, `max-file-lines`, `max-line-length`, `max-public-methods`, `print-statement`, `signal-not-connected`, `todo-comment` |

## Priority Implementation Recommendations

### Easy wins (no type system needed)

1. **`STANDALONE_TERNARY`** — extend `standalone-expression` to detect ternary
2. **`EMPTY_FILE`** — trivial AST check
3. **`ONREADY_WITH_EXPORT`** — check annotation pairs on variables
4. **`MISSING_TOOL`** — check `@tool` on base class extends chain
5. **`REDUNDANT_STATIC_UNLOAD`** — check `@static_unload` + no static vars
6. **`ENUM_VARIABLE_WITHOUT_DEFAULT`** — check typed enum variables for missing defaults
7. **`UNUSED_LOCAL_CONSTANT`** — mirror `unused-variable` for `const`

### Medium effort (pattern matching, no full type system)

8. **`ASSERT_ALWAYS_TRUE/FALSE`** — detect constant assert expressions
9. **`RETURN_VALUE_DISCARDED`** — use ClassDB return types for engine method calls
10. **`UNREACHABLE_PATTERN`** — analyze match statement patterns
11. **`SHADOWED_GLOBAL_IDENTIFIER`** — compare against ClassDB class/function names

### Requires Layer 1 symbol table

12. **`UNASSIGNED_VARIABLE`** — control flow analysis for local variable initialization
13. **`SHADOWED_VARIABLE_BASE_CLASS`** — extends chain + ClassDB member lookup
14. **`NATIVE_METHOD_OVERRIDE`** — script overrides engine virtual method
15. **`STATIC_CALLED_ON_INSTANCE`** — detect `@static` + instance access
16. **`INT_AS_ENUM_WITHOUT_CAST/MATCH`** — enum type tracking

### Severity review

- **`variant-inference`**: Godot treats `INFERENCE_ON_VARIANT` as **ERROR**. We have it as opt-in warning. Should we promote to default-enabled warning or even error?
