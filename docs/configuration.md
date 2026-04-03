# Configuration

Configure `gd` via a `gd.toml` file in your project root. The toolchain searches upward from the current directory.

```toml
[fmt]
use_tabs = true
indent_size = 4
max_line_length = 100
blank_lines_around_functions = 2
blank_lines_around_classes = 2
trailing_newline = true

[lint]
ignore_patterns = ["addons/**"]

# Category-level controls: "off" | "info" | "warning" | "error"
correctness = "error"
type_safety = "warning"    # enables all type-safety rules incl. opt-in
maintenance = "off"        # disables all maintenance rules

# Per-rule overrides (take precedence over category)
[lint.rules.naming-convention]
severity = "error"

[lint.rules.print-statement]
severity = "warning"       # re-enable despite maintenance = "off"

[build]
output_dir = "build"

[run]
# godot_path = "/usr/bin/godot"
extra_args = []
```

## Configuration Options

**`[fmt]`**

| Option | Default | Description |
|--------|---------|-------------|
| `use_tabs` | `true` | Use tabs instead of spaces for indentation |
| `indent_size` | `4` | Number of spaces per indent level (when not using tabs) |
| `max_line_length` | `100` | Maximum line length before warnings |
| `blank_lines_around_functions` | `2` | Blank lines around top-level functions |
| `blank_lines_around_classes` | `2` | Blank lines around inner class definitions |
| `trailing_newline` | `true` | Ensure file ends with exactly one newline |

**`[lint]`**

| Option | Default | Description |
|--------|---------|-------------|
| `disabled_rules` | `[]` | List of rule names to disable |
| `max_function_length` | `50` | Max lines before `long-function` warns |
| `max_function_params` | `5` | Max parameters before `too-many-parameters` warns |
| `max_cyclomatic_complexity` | `10` | Max complexity before `cyclomatic-complexity` warns |
| `max_nesting_depth` | `4` | Max depth before `deeply-nested-code` warns |
| `max_line_length` | `120` | Max line length before `max-line-length` warns |
| `max_file_lines` | `500` | Max file lines before `max-file-lines` warns |
| `max_public_methods` | `20` | Max public methods before `max-public-methods` warns |
| `max_god_object_functions` | `20` | Max functions before `god-object` warns |
| `max_god_object_members` | `15` | Max member variables before `god-object` warns |
| `max_god_object_lines` | `500` | Max lines before `god-object` warns |
| `ignore_patterns` | `[]` | Glob patterns for files to skip |
| `correctness` | (none) | Category level: `"off"`, `"info"`, `"warning"`, `"error"` |
| `suspicious` | (none) | Category level for likely-bug rules |
| `style` | (none) | Category level for naming/style rules |
| `complexity` | (none) | Category level for complexity metric rules |
| `performance` | (none) | Category level for performance rules |
| `godot` | (none) | Category level for Godot best-practice rules |
| `type_safety` | (none) | Category level for type-system rules |
| `maintenance` | (none) | Category level for unused-code/debug rules |

**`[lint.rules.<name>]`** — per-rule overrides:

| Option | Values | Description |
|--------|--------|-------------|
| `severity` | `"info"`, `"warning"`, `"error"`, `"off"` | Override severity or disable a rule |

Resolution order (highest wins): `disabled_rules` > per-rule severity > category level > rule default.

**`[build]`**

| Option | Default | Description |
|--------|---------|-------------|
| `output_dir` | `"build"` | Directory for export output |

**`[run]`**

| Option | Default | Description |
|--------|---------|-------------|
| `godot_path` | (auto) | Path to the Godot binary; uses `PATH` if unset |
| `extra_args` | `[]` | Additional arguments passed to Godot |

## Lint Rules

89 built-in rules organized into 8 categories (52 default-enabled, 37 opt-in):

### Categories

| Category | Description | Rules |
|----------|-------------|-------|
| **correctness** | Definite bugs | 15 |
| **suspicious** | Likely bugs, may be intentional | 11 |
| **style** | Naming and code style | 14 |
| **complexity** | Code size and complexity metrics | 8 |
| **performance** | Godot runtime performance | 4 |
| **godot** | Godot engine best practices | 11 |
| **type_safety** | Type system strictness | 9 |
| **maintenance** | Unused code and debug artifacts | 13 |

Categories can be bulk-controlled in `gd.toml`:

```toml
[lint]
correctness = "error"      # all correctness rules -> error severity
type_safety = "warning"    # enable all type safety rules (incl. opt-in)
maintenance = "off"        # disable all maintenance rules

# Per-rule overrides still take precedence
[lint.rules.print-statement]
severity = "warning"       # re-enable despite maintenance = "off"
```

### All Rules

| Rule | Category | Description | Severity | Fixable |
|------|----------|-------------|----------|---------|
| `assert-always-false` | correctness | Detect `assert(false)`, `assert(0)`, `assert(null)` | warning | yes |
| `assert-always-true` | correctness | Detect `assert(true)`, `assert(1)`, `assert("string")` | warning | yes |
| `await-in-ready` | godot | Detect `await` in `_ready()` | warning | |
| `breakpoint-statement` | maintenance | Detect leftover `breakpoint` statements | info | |
| `callable-null-check` | godot | Warn on `.call()` without `.is_valid()` guard | warning | |
| `class-definitions-order` | style | Enforce canonical member ordering | warning | |
| `collapsible-if` | style | Detect nested `if` that can be collapsed with `and` | warning | yes |
| `comparison-with-boolean` | style | Flag explicit `== true`/`false` comparisons | warning | yes |
| `comparison-with-itself` | correctness | Detect `x == x` self-comparisons | warning | |
| `cyclomatic-complexity` | complexity | Warn on high cyclomatic complexity | warning | |
| `deeply-nested-code` | complexity | Warn on deeply nested code blocks | warning | |
| `duplicate-code` | maintenance | Detect structurally similar functions in the same file | warning | |
| `duplicate-delegate` | maintenance | Detect pure pass-through delegate functions | info | |
| `duplicate-function` | correctness | Detect duplicate function definitions | error | |
| `duplicate-key` | correctness | Detect duplicate dictionary keys | warning | |
| `duplicate-signal` | correctness | Detect duplicate signal declarations | error | |
| `duplicate-variable` | correctness | Detect duplicate variable declarations | error | |
| `duplicated-load` | performance | Detect duplicate load/preload calls | warning | |
| `empty-function` | style | Detect functions with only `pass` in body | warning | |
| `enum-name-collision` | correctness | Detect inner enum names that collide with a global `class_name` | error | |
| `enum-naming` | style | Enforce PascalCase/UPPER_SNAKE_CASE enums | warning | yes |
| `enum-variant-names` | style | Detect enum variants sharing prefix/suffix with enum name | warning | |
| `enum-variable-without-default` | godot | Warn on enum-typed variables without a default value | warning | |
| `enum-without-class-name` | godot | Warn on enum type annotations in scripts without `class_name` | warning | |
| `float-comparison` | suspicious | Warn on float equality comparisons | warning | yes |
| `get-node-default-without-onready` | correctness | Detect `$`/`get_node()` default without `@onready` | error | |
| `get-node-in-process` | performance | Detect `get_node()` in `_process()` | warning | |
| `god-object` | complexity | Warn on classes with too many functions/members/lines | warning | |
| `incompatible-ternary` | suspicious | Detect ternary branches with incompatible types | warning | |
| `infer-unknown-member` | type_safety | Detect `:=` inference from unknown engine class members | warning | |
| `integer-division` | suspicious | Warn on integer literal division truncation | warning | |
| `long-function` | complexity | Warn on functions exceeding line threshold | warning | |
| `look-at-before-tree` | godot | Detect tree-dependent calls and `global_*` assignments before `add_child()` | warning | |
| `loop-variable-name` | style | Enforce snake_case loop variables | warning | yes |
| `magic-number` | type_safety | Flag unexplained numeric literals | warning | |
| `manual-range-contains` | suspicious | Detect manual range checks replaceable with `in range()` | info | yes |
| `max-file-lines` | complexity | Enforce maximum file length | warning | |
| `max-line-length` | complexity | Enforce maximum line length | warning | |
| `max-public-methods` | complexity | Enforce maximum public methods per class | warning | |
| `missing-return` | correctness | Detect missing return in typed functions | warning | |
| `missing-tool` | godot | Detect missing `@tool` when base class has it | warning | |
| `missing-type-hint` | type_safety | Warn on missing type annotations | warning | |
| `monitoring-in-signal` | godot | Detect direct `monitoring`/`monitorable` assignment in Area signal callbacks | warning | |
| `naming-convention` | style | Enforce snake_case/PascalCase naming | warning | yes |
| `narrowing-conversion` | suspicious | Detect float-to-int narrowing conversions | warning | yes |
| `native-method-override` | suspicious | Detect overriding native engine methods | error | |
| `needless-bool` | style | Detect if/else or ternary returning boolean literals | warning | yes |
| `node-ready-order` | godot | Detect node access before tree is ready | warning | |
| `null-after-await` | suspicious | Warn on member access after `await` without null guard | warning | |
| `nullable-current-scene` | suspicious | Detect `get_tree().current_scene` access without null check | warning | |
| `onready-with-export` | correctness | Detect `@onready` combined with `@export` | error | |
| `parameter-naming` | style | Enforce snake_case parameters | warning | yes |
| `parameter-shadows-field` | style | Warn when parameter name shadows a class field | warning | |
| `physics-in-process` | performance | Detect physics calls in `_process()` | warning | |
| `preload-type-hint` | performance | Warn on untyped preload/load assignments | warning | |
| `print-statement` | maintenance | Detect debug print calls | info | |
| `private-method-access` | type_safety | Warn on calling private methods externally | warning | |
| `redundant-else` | style | Detect unnecessary else after return | warning | yes |
| `redundant-static-unload` | godot | Detect `@static_unload` without any `static var` | warning | |
| `return-type-mismatch` | correctness | Detect void/non-void return mismatches | warning | |
| `return-value-discarded` | suspicious | Detect discarded non-void function return values | info | |
| `self-assignment` | correctness | Detect `x = x` assignments | warning | yes |
| `shadowed-variable` | style | Detect variable shadowing in inner scopes | warning | |
| `shadowed-variable-base-class` | style | Detect local variables shadowing base class members | warning | |
| `signal-name-convention` | style | Warn on signals with `on_` prefix | warning | yes |
| `signal-not-connected` | godot | Detect signals emitted but never connected | info | |
| `standalone-expression` | style | Detect side-effect-free expressions | warning | |
| `standalone-ternary` | suspicious | Detect ternary used as statement (result unused) | warning | |
| `static-called-on-instance` | suspicious | Detect static methods called on instances | warning | |
| `static-type-inference` | type_safety | Suggest explicit type annotations | warning | |
| `todo-comment` | maintenance | Detect TODO/FIXME/HACK comments | info | |
| `too-many-parameters` | complexity | Warn on functions with too many parameters | warning | |
| `unnecessary-pass` | style | Detect `pass` in non-empty function bodies | warning | yes |
| `unreachable-code` | correctness | Detect code after return/break/continue | warning | yes |
| `unsafe-void-return` | suspicious | Detect returning or assigning void call results | warning | yes |
| `untyped-array` | type_safety | Suggest typed array annotations | warning | |
| `untyped-array-argument` | type_safety | Warn on passing untyped `Array` to parameter expecting `Array[T]` | warning | |
| `untyped-array-literal` | type_safety | Warn on `var x := [...]` without typed Array annotation | warning | yes |
| `unnamed-node` | godot | Detect `add_child()` with dynamically created nodes that have no `.name` set | warning | |
| `unused-class-signal` | maintenance | Detect signals with no cross-file connections or emissions | warning | |
| `unused-class-variable` | maintenance | Detect class variables with no cross-file references | warning | |
| `unused-parameter` | maintenance | Detect unused function parameters | warning | |
| `unused-preload` | maintenance | Detect unused preload variables | warning | |
| `unused-private-class-variable` | maintenance | Detect unused `_`-prefixed class variables | warning | |
| `unused-private-function` | maintenance | Detect `_`-prefixed functions with no cross-file callers | warning | |
| `unused-signal` | maintenance | Detect signals that are never emitted | warning | |
| `unused-variable` | maintenance | Detect unused local variables | warning | yes |
| `use-before-assign` | correctness | Detect method calls accessing uninitialized members | warning | |
| `variant-inference` | type_safety | Warn on `:=` inferring Variant from dict/array access | warning | |

### Inline Suppression

Suppress lint warnings with comments:

```gdscript
# Suppress all rules on this line
var x = 42  # gd:ignore

# Suppress all rules on the next line
# gd:ignore-next-line
var y = 42

# Suppress specific rules
var z = 42  # gd:ignore[magic-number]

# Suppress specific rules on next line
# gd:ignore-next-line[naming-convention, magic-number]
var MyVar = 100
```
