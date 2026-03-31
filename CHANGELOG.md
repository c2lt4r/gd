# Changelog

## [0.3.24] - 2026-03-31

### Changed
- **Mutation pipeline (`MutationSet` + `commit()`)** — all `gd edit` commands now go through an atomic validate-then-persist pipeline. Mutations are validated in memory (parse-error baseline comparison) before anything touches disk. Commands no longer call `std::fs::write` directly. `WorkspaceIndex::commit_mutations()` provides the LSP path with automatic cache invalidation.
- **`--dry-run` removed from `gd edit` commands** — with the mutation pipeline, validation happens in memory before persistence. Commands either succeed and apply, or fail and write nothing. `--dry-run` is retained only on `gd refactor rename` and `change-signature` where semantic review of cross-file changes is needed.
- **Refactoring simplification** — reduced `gd refactor` from 28 subcommands to 4 (`rename`, `extract-method`, `move-file`, `change-signature`). Removed 21 commands that are agent-composable from edit primitives + queries. Removed transaction/undo machinery; validate-then-persist pipeline rejects broken mutations before writing.
- **`gd refactor rename` is now dry-run by default** — pass `--apply` to persist changes. Previously applied by default with `--dry-run` to preview.
- **`gd edit` gains `remove`, `extract`, `insert-into`** — full set of edit primitives: `insert` (before/after), `insert-into` (class body), `remove` (symbol), `extract` (move symbol to file), `replace-body`, `replace-symbol`, `create-file`.
- **`gd edit edit-range` removed** — per "symbol-or-line, never ranges" principle. Use `replace-symbol` or `insert`/`remove` instead.
- **Remove `gd-mesh` crate** — experimental half-edge mesh engine removed from workspace. Drops `earcutr`, `bincode`, `indexmap` dependencies.
- **Regenerate ClassDB from runtime-verified Godot metadata** — richer generated data: 771 operator type combinations, 154 builtin type constructors, 128 function return types, 36 annotation definitions, 48 warning codes, type conversion matrices.
- **Replace hand-coded type inference with generated lookups** — `is_builtin_type()`, `constructor_return_type()`, `builtin_member_type()`, `builtin_function_return_type()`, and `leak_str()` now delegate to generated data tables instead of maintaining manual match arms.
- **Daemon transport: TCP → Unix Domain Sockets** — daemon IPC replaced from TCP (`127.0.0.1:<random port>`) with UDS (`.godot/gd-daemon.sock`). Eliminates port management, avoids firewall/antivirus interference, removes TCP handshake overhead. Socket path is deterministic (no port discovery via state file). `port` field removed from `gd-daemon.json`. Windows support via `uds_windows` crate (AF_UNIX, Windows 10 17063+).

### Added
- **Typed AST rewriter infrastructure (`gd-core`)** — owned AST types (`OwnedExpr`, `OwnedStmt`, `OwnedFile`), bottom-up rewriter (`rewrite_file`, `rewrite_expr`, `rewrite_stmt`), and span-preserving printer (`print_file`, `print_expr`). Enables pure tree-in/tree-out code transformations — no byte-offset arithmetic or string splicing. Unchanged subtrees emit original source bytes verbatim; only rewritten nodes are printed from structure. Dirty propagation clears parent spans when children are rewritten.
- **Control flow graph infrastructure (`gd-core/cfg`)** — CFG builder, sub-body analysis, and generic dataflow framework for flow-sensitive lint rules. Handles all GDScript control flow: if/elif/else, for, while, match (with wildcard exhaustiveness), return, break, continue. Two build modes: `build()` for functions, `build_body()` for sub-bodies where break/continue exit the scope. Worklist-based dataflow solver supports forward and backward analyses with custom lattices.
- **`gd query view --symbol`** — view a symbol's full declaration including doc comments, annotations, and body. Optional `--refs` flag includes workspace references.
- **`EditOutput.diagnostics` field** — mutation commands return lint diagnostic count after edit for tooling/agent use.
- 10 new ClassDB query functions: `is_variant_type`, `builtin_member_type`, `builtin_method_return_type`, `builtin_constant_type`, `builtin_constructor_exists`, `function_return_type`, `operator_result_type`, `can_convert_type`, `annotation_def`, `godot_warning`.

### Improved
- **`missing-return` lint** — now uses CFG reachability (`can_fall_through`). No longer false-positives when dead code follows an exhaustive if/else or match with wildcard.
- **`unreachable-code` lint** — detects unreachable code after if/elif/else where all branches return, and after match with wildcard where all arms terminate. Previously only caught code after linear return/break/continue.
- **`redundant-else` lint** — uses CFG sub-body analysis for termination checks. Now detects redundant else when the if-body contains nested control flow that always terminates (e.g. inner if/else both returning).
- **`use-before-assign` lint** — rewritten on DefinitelyInitialized dataflow over the CFG. Branch-sensitive: assignment in only one branch of an if (without else) is no longer treated as definite. 1242 → 1054 lines.

## [0.3.23] - 2026-03-18

### Fixed
- **`gd query scene-info` tree rendering** — nested scene trees now display correct parent-child hierarchy. Previously, nodes at different depths were rendered at wrong indentation levels, making children appear as siblings of their parents. Rewrote flat-list renderer to build a proper recursive tree before display.
- **`gd stop` on WSL killing wrong process** — previously used `tasklist.exe` name-only filter which returned the first `godot.exe` match (often the editor). Now uses PowerShell `Get-CimInstance` to filter by `--remote-debug` command-line arg, which only the game process has. Falls back to wmic on older Windows.

## [0.3.22] - 2026-03-08

### Added
- **`gd lsp refactor extract-constant`** — extract an expression to a file-scope or class-scope `const`:
  - Lifts literals/expressions to top-level `const` declarations (unlike `introduce-variable --const` which stays local)
  - `--replace-all` replaces all matching occurrences across the entire file (crosses function boundaries)
  - `--class` targets inner classes
  - Type inference, naming warnings (`UPPER_SNAKE_CASE`), collision detection, undo support
  - Inserts after existing constants, before functions

### Fixed
- **Rename/references now update string literals in Godot reflection API calls** — `call("foo")`, `has_method("foo")`, `emit_signal("bar")`, `connect("bar", ...)`, and other reflection methods now have their string arguments included in rename and find-references results. Previously these were silently skipped, leaving stale string references that cause runtime errors.

## [0.3.21] - 2026-03-07

### Added
- **`gd-interp` crate** — GDScript interpreter built on the typed AST (Phase 1 + 2):
  - `GdValue` runtime type (17 variants: null through Callable) with GDScript-matching Display, truthiness, and type IDs
  - Expression evaluator walks `GdExpr` AST: literals, arithmetic, comparisons, boolean logic, bitwise ops, string formatting, ternary, casts, `is` checks, constructors (Vector2/3/4, Color, Rect2, NodePath), subscript, property access
  - Statement executor: var/assign/aug-assign, if/elif/else, for (arrays + range sugar), while (with 1M iteration guard), match (literal + wildcard + binding patterns), break/continue/return
  - `Interpreter` struct with function registry: user-defined function calls, recursion, default parameters, top-level var/const evaluation, enum value registration (named enums as dictionaries)
  - Mutating array methods: append, push_back/front, pop_back/front, insert, remove_at, erase, sort, reverse, clear, resize, shuffle — with in-place mutation writeback
  - Mutating dictionary methods: erase, merge (with overwrite flag), clear
  - 30+ builtin utility functions (print, abs, range, sin/cos, lerp, typeof, len, str/int/float casts, etc.)
  - 25+ String methods, Vector2/3 methods+properties, Color methods+properties
  - Static class constants: `Color.RED`, `Vector2.ZERO`, `Vector3.UP`, etc.
  - **Class system** (Phase 4): `GdObject` with properties, inner class registration, `ClassName.new()` constructor with `_init()`, `self.property` access/assignment, instance method dispatch with self-writeback, static method calls, `extends` inheritance (property + method resolution), `is` type checks for objects
  - 225 tests covering expressions, statements, builtins, function calls, mutation, classes, and end-to-end evaluation
- **Native test runner** — `gd test run` now executes `test_*` functions without Godot (Phase 3):
  - Default runner when no GUT/gdUnit4 addon detected (replaces Script runner as fallback)
  - Millisecond execution vs 2-3s Godot startup — no Godot binary required
  - Assertion builtins: `assert_eq`, `assert_ne`, `assert_true`, `assert_false`, `assert_gt`, `assert_lt`, `assert_null`, `assert_not_null`
  - Full integration: `--name` filtering, `--format json`, `--quiet`, per-test timing, cargo-style output
  - Explicit selection with `--runner native`; other runners (`gut`, `gdunit4`, `script`) still available
  - Lifecycle hooks: `before_each()`, `after_each()`, `before_all()`, `after_all()` — run setup/teardown around each test or per-file
  - **Power assertions** (Groovy/Spock-style): assertion failures decompose AST subexpressions showing intermediate values with column-aligned pointer lines — e.g. `assert_eq(a + b, 10)` shows `a` → 3, `b` → 4, `a + b` → 7
  - **Parallel file execution**: test files run concurrently via rayon with synchronized terminal output
  - **SARIF output**: `--format sarif` emits SARIF v2.1.0 for CI integration (GitHub Code Scanning, etc.)
- **`gd eval --native`** — evaluate GDScript expressions and scripts without Godot:
  - Single expressions: `gd eval --native "2 + 3 * 4"` → `14`
  - Full scripts with `func main()` / `_init()` / `_ready()` entry points
  - File input: `gd eval --native script.gd`
  - Stdin: `echo "print(42)" | gd eval --native -`
  - JSON output: `--format json` for machine-readable results

### Fixed
- **`gd check` — detect `:=` type inference failures through base-class typed variables** — when a variable is typed as a Godot base class (e.g. `VBoxContainer`) and a method call on it doesn't exist in ClassDB, `:=` can't infer the return type. Now correctly flagged as an error.
- **`gd check` — detect `:=` Variant inference from singleton method calls** — `ProjectSettings.get_setting()` and similar singleton methods returning `Variant` are now caught (previously only local variable receivers were checked).
- **`gd lint` — suppress `unused-preload` for autoload scripts** — preloaded variables in autoload files are accessed cross-file via the singleton and should not be flagged as unused.
- **`gd lint` — fix false `compiler-error` on single-file lint for `class_name` types** — when linting a single file with a relative path, `find_project_root` failed to walk upward, preventing cross-file type resolution.
- **`gd llm` — updated command reference** — corrected command namespaces, flags, and conventions to match current CLI.

### Removed
- **File-based IPC for eval server** — removed `--file-ipc` flag from `gd run` and `GD_EVAL_FILE_IPC` env var. TCP transport is now the only eval server mode (more stable, supports concurrent connections, output capture).

## [0.3.20] - 2026-03-07

### Changed
- **Vendored tree-sitter-gdscript** — local copy with 9 targeted patches for 100% clean parsing on Godot 4.6.1 test corpus (107→0 parse errors):
  - Scanner indent serialization: 16-bit little-endian (fixes overflow at 32+ indent levels)
  - Unicode identifiers: explicit CJK/Hangul ranges (workaround for tree-sitter codegen gap)
  - Number literals: trailing underscore in hex/octal/binary (`0xFF_`)
  - Get node paths: `$"path"`, `%"unique"`, complex nested paths
  - Signal annotations: `@warning_ignore` before `signal` declarations
  - Match pass body: `pass` as sole match arm body
  - Match trailing comma: comma after open-ended patterns
  - Dictionary string keys: `{"key" = val}` lua-table syntax
  - Abstract semicolon: already upstream in 6.1.0 (no patch needed)
- **`gd check` indentation tolerance** — suppresses false positive when comment at parent indent sits between control-flow colon and body
- Gitignore: `docs/` and `fuzz/` excluded from tracking

## [0.3.19] - 2026-03-04

### Changed
- **Cargo workspace split** — monolith restructured into 7 workspace members (6 library crates + 1 binary) for dramatically faster incremental builds:
  - `gd-class-db` (271k lines) — static Godot class data, compiles once and stays cached
  - `gd-core` — AST, parsing, type inference, project model
  - `gd-mesh` — half-edge mesh operations (experimental, fully independent)
  - `gd-lint` — 96 lint rules + diagnostics
  - `gd-fmt` — formatter + printer
  - `gd-lsp` — LSP server, refactoring, daemon, debug protocol
- Touching a lint rule no longer recompiles the class database (66% of code)
- All 2,748 tests pass, zero behavior changes

## [0.3.18] - 2026-03-04

### Fixed
- **`gd check` — demo-project FP reduction** (167→35 FPs, 79% reduction across godot-demo-projects corpus):
  - Path-based `extends` resolution: `signal_exists()` and `enum_member_exists()` now walk extends chains using `resolve_extends()` (handles `extends "res://..."` paths, not just class_name) — fixes 8 "finished" signal FPs
  - `@onready` non-Node check uses `extends_str()` instead of `extends_class()` — fixes 5 FPs on path-based extends
  - Vector arithmetic type inference: `Vector2 * float → Vector2` (not `float`), division preserves vector types — fixes 12 FPs
  - Enum arithmetic: enum-typed variables treated as `int` for operator checks — fixes 1 FP
  - Method chain property resolution: `basis.y.cross()` resolves `Basis.y → Vector3` before checking `cross()` — fixes 5 FPs
  - Match wildcard `_:` pattern detected directly under `pattern_section` (tree-sitter puts it without `pattern` wrapper) — fixes 4 return-path FPs
  - Class-to-class assignment allowed for all ClassDB types (GDScript defers to runtime) — fixes 7 FPs
  - Top-level statement check skips indented nodes (tree-sitter misparsing from comments at column 0) — fixes 3 FPs
  - Const expression check: comments inside arrays/dictionaries no longer break const analysis — fixes 2 FPs
  - Math utility functions (`sin`, `cos`, `tan`, `deg_to_rad`, etc.) recognized as valid const expressions — fixes 2 FPs
  - Variant inference: subscript only flagged when receiver is confirmed `Dictionary`/`Array`, not unknown types — fixes 5 FPs
  - `rpc()`/`rpc_id()` argument checks skipped (GDScript syntactic sugar, not ClassDB signature) — fixes 2 FPs
  - Param count parser handles nested parentheses in default values (e.g., `Vector3(0, 1, 0)`) — fixes 1 FP
  - Assert message accepts string format expressions (`"text %s" % val`) — fixes 1 FP
  - String format `%` with array RHS infers `String` not `int` — fixes 2 FPs

## [0.3.17] - 2026-03-03

### Fixed
- **`gd check` — cross-file identifier and type resolution** (453 FPs eliminated):
  - Relative paths (`gd check subdir/`) now resolve to absolute before project root lookup, fixing empty ProjectIndex
  - File's own `class_name` added to known identifier set
  - Const type aliases recognized: `const B = preload("file.gd")` and `const IC = InnerClass` now valid as types
  - Const and inner class names from extends chain recognized as types
  - Inner class scope tracking: consts and inner classes inherited from inner class base classes resolve in type annotations and `is`/`as` expressions
  - `InnerClassSummary` with recursive inner classes, functions, and consts for cross-file resolution
  - Dotted extends resolution: `extends "path.gd".InnerA.InnerAB` and `extends B.Inner` (const preload) now resolve `super.method()` calls correctly
  - `GdExpr::Preload` variant handled alongside `GdExpr::Call` for const preload detection
- **`gd check` — Godot 4.6.1 corpus false positive fixes (round 3)** (5 fixes across 3 files, 48→0 fixable FPs):
  - Subscript access on const literals (`[1,2,3][0]`, `{"key": 5}["key"]`) now recognized as const expressions
  - Lowercase const identifiers in same scope now resolved (e.g. `const x = other_const + 1`)
  - Comments inside argument lists no longer counted as arguments (fixed `Vector3(3, 3.5, 4, #comment)` = 4 args)
  - Top-level triple-quoted strings (docstring-style comments) no longer flagged as invalid top-level statements
  - Inner class return-path analysis no longer matches wrong function when file-level function shares the same name
- **`gd check` — Godot 4.6.1 corpus false positive fixes (round 2)** (7 fixes across 4 files):
  - Bare `new()` self-constructor calls no longer flagged as "function not found"
  - Variadic functions (`func f(...args)`) no longer flagged for too many arguments
  - Match pattern variable bindings (`var value` in match arms) now in scope for arm body and `when` guard
  - Semicolon-separated one-line statements (`func f(): var x := 1; return x`) no longer flagged as indentation errors
  - `super.method()` inside inner classes now resolves against the inner class's extends (not the file's top-level extends)
  - `is_const_expression` expanded: subscript on const values, bare function references (`absf`), `Color8()` with const args
  - `var color: Color = "red"` (named color strings) no longer flagged as type mismatch
- **`gd check` — Godot 4.6.1 corpus false positive fixes (round 1)** (12 fixes across 9 files):
  - Trailing comma in function args no longer overcounts arguments (`Vector3(3, 3.5, 4,)`)
  - `return <call>()` in void functions allowed (Godot permits side-effect returns)
  - Enums are now recognized as iterable (`for key in MyEnum:`)
  - `type_exists()` recognized as a known GDScript utility function
  - `Array()`, `Dictionary()`, `int()`, `float()`, `bool()`, `String()` recognized as valid const constructors
  - PascalCase identifiers (class/enum names) allowed as constant values
  - `@abstract` functions no longer flagged for missing return
  - `as` cast no longer treated as Variant-producing expression; `const x = 76 as float` now valid
  - `@warning_ignore` annotations now respected by `onready-with-export`, `get-node-default-without-onready`, and `native-method-override` rules
  - ClassDB enum types resolved from file's extends chain (fixes `AutoTranslateMode` on Node subclasses)

## [0.3.16] - 2026-03-03

### Added
- **`gd upgrade` — SHA256 checksum verification**: downloaded binaries are now verified against `.sha256` checksum files from GitHub Releases. Warns and proceeds gracefully for older releases without checksums.
- **`gd upgrade --skip-verify`**: flag to skip checksum verification when needed.
- **Release CI generates `.sha256` files**: each platform archive now includes a companion SHA256 checksum file in GNU coreutils format.

### Fixed
- **Windows stack overflow in `gd check`**: main thread now spawns with 8MB stack (matching Unix defaults) to prevent stack overflow from deep AST walks on Windows' default 1MB stack.

## [0.3.15] - 2026-03-02

### Improved
- **`gd check` — corpus-driven FP reduction and detection expansion** across 11 categories:
  - **Indentation check**: nested control flow (`elif`/`else`/`match` arms) no longer falsely flagged as orphaned blocks (~110 FPs eliminated)
  - **Void return detection**: void function calls used as arguments (`print(void_func())`) and void utility functions (`var x = print()`) now flagged
  - **Invalid cast**: `primitive → container` (int as Array) and `class → primitive` (Node as int) casts now detected
  - **Const assignment**: assigning to const subscripts (`CONST_ARR[0] = val`) and signals now detected
  - **Static context**: static variable initializers (`static var x = instance_var`) now checked for instance references
  - **Typed array check**: `const` declarations with typed arrays now checked for element type mismatches
  - **Augmented assignment types**: `+=`, `-=`, `*=`, `/=`, `%=` now validated for operand type compatibility
  - **Cross-file resolution**: files without `extends` now check `RefCounted` methods; inner class functions visible
  - **Enum ↔ int coercion**: enum values allowed in int/float contexts and vice versa
  - **Void return coercion**: `return void_func()` in void functions no longer flagged
  - **Const expression handling**: `Type.new()` recognized as valid constant expression
- **`gd check` — project-aware type inference**: all type-checking functions (assignment mismatch, return type mismatch, invalid operators, invalid cast, argument type/count, builtin method/property resolution) now use cross-file type inference via `ProjectIndex`, resolving user-defined base class methods, autoload types, and preloaded script types. Previously only file-local inference was used, causing ~238 false positives on real projects.
- **`gd check` — 67% FP reduction on real Godot 4 projects** (4,062 → 1,350 across 4 projects, 100% mutation parity maintained):
  - **Extends chain variable resolution**: base class variables with inferred types (`:=`) now found via new `variable_exists()` method — previously only explicit type annotations were checked
  - **Attribute subscript**: `obj.member[index]` no longer falsely flags `member` as undeclared — `attribute_subscript` tree-sitter node now recognized
  - **Variadic parameters**: `...varargs: Array` parameter syntax now correctly registers the parameter name in scope
  - **Path-based extends for identifiers**: `extends "res://path.gd"` now resolves through the project index for all identifier, method, and super-method checks (previously only class-name extends worked)
  - **ClassDB signal resolution**: signals like `visibility_changed`, `pressed`, `finished` now recognized as valid identifiers via `signal_exists()` in the extends chain
  - **Implicit RefCounted base**: files without an `extends` statement now check ClassDB for `RefCounted` (Godot's implicit default), catching constants like `NOTIFICATION_PREDELETE`
  - **Type coercion expansions**: `int ↔ float`, `int/float → bool`, `String ↔ StringName/NodePath`, `Array ↔ PackedXXXArray`, `Array ↔ Array[T]`, `Dictionary ↔ Dictionary[K,V]`, `Vector2 ↔ Vector2i`, `Rect2 ↔ Rect2i`, user class subtype covariance
  - **Override return type normalization**: ClassDB `enum::Error` prefix now stripped for comparison — `-> Error` no longer flagged as mismatching `enum::Error`
  - **Inner class type annotations**: `ClassName.InnerClass` dot notation now allowed when the base class is known
  - **Typed Dictionary iteration**: `for key in typed_dict:` no longer flagged as non-iterable
  - **GDScript keywords in identifier check**: tree-sitter sometimes emits `identifier` nodes for keywords in match bodies — now skipped

### Added
- **4 new lint rules** — loop-to-functional style rules (all opt-in, Style category, with auto-fixes):
  - `manual-array-any`: detects `for x in arr: if cond: return true` + `return false` → suggests `arr.any(func(x): return cond)`
  - `manual-array-all`: detects `for x in arr: if cond: return false` + `return true` → suggests `arr.all(func(x): return not (cond))`
  - `manual-array-filter`: detects `var result = []; for x in arr: if cond: result.append(x)` → suggests `arr.filter(func(x): return cond)`
  - `manual-array-map`: detects `var result = []; for x in arr: result.append(transform(x))` → suggests `arr.map(func(x): return transform(x))`

### Internal
- **Typed AST migration — complete across entire codebase.** All modules now use `GdFile<'_>` from the typed AST layer instead of raw tree-sitter CST for declaration-level work. This includes lint rules, `gd check`, type inference, workspace index, LSP features, and refactoring commands.
- **Eliminated `symbol_table.rs`** (~1,000 lines deleted). The per-file `SymbolTable` abstraction was fully redundant with `GdFile` and has been removed. 49 files migrated: type inference, 22 lint rules, 7 check_cmd modules, 6 LSP refactor commands, test_cmd, workspace_index, and LSP features (inlay hints, semantic tokens, signature help). The `LintRule` trait no longer takes a `symbols` parameter.
- **Typed AST enrichment** — added `name_node` fields to `GdFunc`, `GdVar`, `GdSignal`, `GdEnum`, `GdClass` for precise rename/reference locations; `else_node` to `GdIf`; `extends_node` and `class_name_node` to `GdFile`; `doc` comment fields to all declaration types; `has_static_unload` flag; `GdDecl` convenience methods (`as_func()`, `as_var()`, etc.) and `GdFile` iterator helpers (`funcs()`, `vars()`, `signals()`, `enums()`, `inner_classes()`, `extends_class()`, `extends_str()`).
- **LSP modules converted to typed AST** — hover, definition, references, implementations, document symbols, call hierarchy, completion, signature help, semantic tokens, inlay hints, and query/analysis all use `GdFile` instead of raw CST helpers. Removed CST helper functions (`find_enum`, `find_signal`, `find_inner_class`, `find_extends_class`, `matches_name`) from completion.rs.
- **Refactoring commands converted** — move_symbol, pull_up_member, push_down_member, extract_superclass, extract_class, bulk_rename, extract_method, introduce_parameter, introduce_variable, delete_symbol all use typed AST.
- **Lint rule deep conversion continued** (batches 16–21) — converted remaining rules from CST traversal to typed AST, including `duplicate_code`, `magic_number`, `naming_convention`, `nullable_current_scene`, `unused_parameter`, `parameter_shadows_field`, and 15 others. Added `name_node` fields to eliminate `child_by_field_name` calls in 12 lint rules.
- **Split `check_cmd.rs` into directory module** — the largest file in the project (6,152 lines) is now organized into 7 focused submodules (`structural`, `classdb`, `types`, `args`, `identifiers`, `builtins`, `tests`), following the established module split pattern used by `debug_cmd/`, `scene_cmd/`, and `test_cmd/`. Pure reorganization with no behavioral changes.
- **Net reduction**: ~4,800 lines deleted across the full typed AST migration (3,300 from lint rules + 1,000 from SymbolTable + 500 from CST helper removal).

## [0.3.13] - 2026-02-27

### Improved
- **`gd check` — further 8% FP reduction on real projects** (5,063 → 4,492 across 5 projects, 81.7% total reduction from v0.3.9 baseline):
  - **Signal chained access**: `obj.signal_name.connect(fn)` no longer falsely flagged as too-few-args — property chain resolved through ClassDB signals
  - **Global scope enums**: `Error`, `Corner`, `EulerOrder` and other `@GlobalScope` enum types now recognized in type annotations
  - **Typed Dictionary**: `Dictionary[String, int]` syntax now validated (each element type checked independently)
  - **Dotted type annotations**: `BaseMaterial3D.BillboardMode` and other `Class.EnumType` patterns now recognized
  - **Constructor override**: `_init()` param count no longer checked against parent — GDScript constructors have independent signatures
  - **Transform operators**: `Transform3D * Transform3D`, `Transform * Vector`, `Basis` operations now valid

## [0.3.12] - 2026-02-27

### Improved
- **`gd check` — further 25% FP reduction on real projects** (918 → 658 on godot-demo-projects, 79% total reduction from v0.3.9 baseline):
  - **Untyped function parameters**: bare parameters like `func f(a, b)` now correctly tracked as declared identifiers
  - **Path-based extends resolution**: `extends "res://base.gd"` and relative path extends now resolve through the project index for inheritance chain walking
  - **Dictionary dot-access**: `dict.key` no longer falsely flagged as "member not found" — Dictionary supports arbitrary dot-access in GDScript
  - **`@onready` on project-defined types**: classes extending project-defined types that ultimately inherit Node no longer falsely flagged
  - **`@GDScript` builtins**: added `Color8`, `is_instance_of`, `print_debug`, `convert`, `inst_to_dict`, `dict_to_inst`, and other `@GDScript` global functions

## [0.3.10] - 2026-02-26

### Improved
- **`gd check` — 72% fewer false positives on real projects** (24,540 → 6,780 across 5 open-source projects, 1,158 files) while maintaining 100% mutation parity:
  - **Singleton awareness**: recognize all 39 Godot singletons (Input, OS, Time, etc.) and skip false "non-static method on class" errors
  - **Static method detection**: regenerated ClassDB with `is_static` flag from extension_api.json; `FileAccess.open()`, `Image.create()`, etc. no longer flagged
  - **Cross-file resolution**: thread ProjectIndex to method-not-found and super-method checks; recognize methods/properties from project-defined base classes
  - **Project root fix**: `gd check <path>` now builds the project index from the target path's root, not the CWD — fixes autoload and class_name resolution
  - **UID autoloads**: handle Godot 4.4+ `uid://` paths in project.godot autoload entries
  - **Global scope constants**: replace hardcoded constant list with `@GlobalScope` ClassDB lookup (512 enum values: MOUSE_BUTTON_LEFT, KEY_ESCAPE, etc.)
  - **Extends chain constants**: recognize class enum members (CONNECT_DEFERRED, NOTIFICATION_PREDELETE, etc.) via inheritance
  - **String formatting operator**: `"hello %s" % value` no longer flagged as invalid operator
  - **Implicit type conversions**: String → StringName and String → NodePath now accepted in type checks
  - **Annotation identifiers**: `@warning_ignore` and other annotations no longer flagged as undeclared
  - **Builtin constructors**: `RID()`, `PackedVector2Array()`, etc. recognized; `Vector2(1)`, `Color("hex")` single-arg forms accepted
  - **Color constructor type check**: `Color(float)` correctly rejected while `Color(String)` and `Color(Color)` accepted

## [0.3.9] - 2026-02-26

### Added
- **`gd check` — 100% Godot compiler parity** across 115 mutation test cases (0 false negatives, 0 false positives). New checks include:
  - **Name resolution** (A1–A4): undefined identifiers, method/property not found on self/builtin types, type not found in casts/annotations
  - **Type mismatch** (B1–B6): assignment, return, argument, operator, and cast type checking with full type inference
  - **Argument count** (B4): user functions, ClassDB methods, utility functions, constructors, and builtin type methods (e.g. `Vector2.lerp`)
  - **Builtin type validation** (A2/A3): method and property existence checks for all 34 builtin types (Vector2, String, Array, etc.) using the generated API registry
- **`gd lint` now includes compiler-level checks** — like `clippy` vs `cargo check`, `gd lint` is now a superset of `gd check`, surfacing compiler-parity errors as `error`-severity diagnostics alongside the 96 lint rules

## [0.3.8] - 2026-02-26

### Improved
- **Rename** — enum member qualified references (`EnumName.MEMBER`) now found and updated cross-file, including nested patterns (`Outer.EnumName.MEMBER`)
- **Extract method** — extracted function inherits `static` when source is a static function; extraction inside inner classes places the function in the correct class with proper indentation; `break`/`continue` targeting an outer loop is rejected
- **Extract method** — parameters and return types are now inferred from the type inference engine (e.g., `func helper(speed: float) -> Vector2:`)
- **Inline variable** — `@onready` variables are rejected with a clear error; warns when inlining discards a type annotation
- **Inline method** — supports `self.method()` calls (same-file resolution) and `typed_var.method()` calls (cross-file type-based resolution with `self.` → receiver substitution); supports cross-file bare function calls via `ProjectIndex` lookup
- **Move file** — path replacement in `.gd` files is now AST-aware: only updates `preload()`/`load()`/`extends` references, preserving paths in comments and data strings
- **Move symbol** — automatically updates caller files when `--update-callers` is set: rewrites preload paths or adds new preloads with qualified reference rewriting
- **Change signature** — propagates changes to overriding methods in subclasses (transitive); parameter rename in function body is now AST-aware (skips strings and comments); warns about `.tscn` signal connections referencing the handler
- **Introduce variable** — infers type annotation from expression (e.g., `var dist: float = ...`); new `--replace-all` flag replaces all identical expressions in scope
- **Introduce parameter** — infers type annotation from expression (e.g., `func f(damage: int = 10):`)

## [0.3.7] - 2026-02-25

### Added
- **`gd refactor encapsulate-field`** — convert a plain `var` into a GDScript property with `set(value):`/`get:` accessors (inline syntax by default), or `--backing-field` for `_name` + getter/setter functions. Preserves `@export`, `@onready`, and other annotations.
- **`gd refactor extract-superclass`** — extract specified members into a new base class file and update `extends`. Supports `--class-name` for named classes, dependency warnings when moved members reference staying members.
- **`gd refactor pull-up-member`** — move a member (function, variable, signal, constant) from a child class up to its parent class. Resolves parent via `extends` (both `class_name` and `res://` path-based). Warns on self-references to child-only members.
- **`gd refactor push-down-member`** — move a member from a parent class down to child classes. Auto-discovers children via `WorkspaceIndex`, or target specific files with `--to`. Supports `--force` to skip collision errors.

## [0.3.6] - 2026-02-25

### Added
- **Unified `--name`/`--line` support across all symbol-targeting commands** — every command that identifies a symbol now accepts both `--name` and `--line` (mutually exclusive), so users and AI tools can target symbols either way:
  - `query definition`, `query hover` — added `--name` (resolves to position, then queries)
  - `refactor inline-variable`, `split-declaration`, `join-declaration` — added `--name` (resolves to line/column)
  - `refactor move-symbol`, `change-signature`, `extract-guards`, `convert-onready`, `inline-delegate` — added `--line` (resolves to name)

## [0.3.5] - 2026-02-25

### Added
- **6 new fixable style-preference lint rules** — all opt-in, all auto-fixable via `gd lint --fix` (closes #24):
  - `prefer-dollar-syntax` — warns on `get_node("X")`, fixes to `$X` / `$"X"`
  - `prefer-get-node-syntax` — warns on `$X`, fixes to `get_node("X")` (mutually exclusive with above)
  - `prefer-in-operator` — warns on `x == 1 or x == 2 or x == 3`, fixes to `x in [1, 2, 3]`
  - `prefer-ternary` — warns on simple if/else same-variable assignment, fixes to ternary
  - `prefer-is-instance` — warns on `typeof(x) == TYPE_*`, fixes to `x is Type`
  - `prefer-string-format` — warns on `str()` concatenation, fixes to `"%s" % [args]`

## [0.3.4] - 2026-02-24

### Changed
- **`bulk-delete-symbol` merged into `delete-symbol --names`** — `delete-symbol` now accepts `--names "a,b,c"` for bulk deletion. The standalone `bulk-delete-symbol` command is removed. (closes #35)

### Added
- **`bulk-rename --scope file`** — new `--scope` flag (`file` or `project`, default `project`) restricts renames to the target file only, skipping cross-file references. (closes #35)

## [0.3.3] - 2026-02-24

### Fixed
- **Rename scoping with same-name methods** — `gd refactor rename` now correctly distinguishes between static and instance methods when both share the same name. Renaming at a specific position only affects the targeted variant and its call sites. (closes #34)
- **`callable-null-check` false positive on `call_deferred`** — `node.call_deferred("method_name")` is `Object.call_deferred` (string-based), not a Callable method. No longer flagged. (closes #28)

### Added
- **`gd check` detects override signature mismatches** — new `override-signature-mismatch` lint rule catches when a child class overrides a parent method with a different number of parameters, which Godot rejects at compile time. Runs automatically as part of `gd check`. (closes #39)

## [0.3.2] - 2026-02-24

### Fixed
- **`unused-signal` false positive with `.emit` callable** — `signal_name.emit` passed as a callable reference (without parentheses) is now correctly recognized as a signal reference. Same fix applied to `signal-not-connected` rule. (closes #36)

### Added
- **`gd check` catches duplicate declarations** — `gd check` now runs `duplicate-function`, `duplicate-signal`, and `duplicate-variable` checks as part of validation. These are Godot compile errors that were previously only caught by `gd lint`. (closes #38)

## [0.3.1] - 2026-02-24

### Breaking
- **`--file` is now a positional argument across all commands** — primary file arguments changed from `--file <path>` to positional `<path>` for all `gd query`, `gd refactor`, `gd edit`, `gd mesh batch/replay`, and `gd debug` subcommands. Secondary/destination files (`--from`, `--to`, `--input-file`) remain as flags. (closes #31)

## [0.3.0] - 2026-02-24

### Breaking
- **`gd lsp` subcommands split into `gd refactor`, `gd edit`, `gd query`** — the 37 one-shot subcommands previously under `gd lsp` are now organized into semantic groups:
  - `gd refactor` — 23 commands: rename, extract-method, inline-method, change-signature, delete-symbol, move-symbol, etc. Includes merged `undo` command (`gd refactor undo --list` replaces `gd lsp undo-list`).
  - `gd edit` — 5 commands: replace-body, insert, replace-symbol, edit-range, create-file.
  - `gd query` — 11 commands: references, definition, hover, completions, symbols, view, scene-info, etc.
  - `gd lsp` now only starts the LSP server (no subcommands).
- **`gd lsp diagnostics` removed** — use `gd lint` directly (they were identical).

## [0.2.44] - 2026-02-24

### Added
- **Undo support for 10 additional commands** — `introduce-parameter`, `inline-delegate`, `bulk-delete-symbol`, `replace-body`, `insert`, `replace-symbol`, `edit-range`, `change-signature`, `bulk-rename`, and `rename` now all record undo entries. Every source-editing refactoring command is now undoable.

### Fixed
- **`gd lsp convert-signal --to-code`** — reject duplicate `.connect()` calls instead of silently creating them when the script already contains the matching connect call.

## [0.2.43] - 2026-02-24

### Added
- **`gd lsp invert-if`** — invert an if/else statement: negate the condition, swap branches. Supports elif chain rotation, De Morgan's law, comparison flipping, and boolean literal inversion.
- **`gd lsp extract-guards`** — flatten nested ifs into early return/continue guard clauses. Auto-detects exit keyword (return/continue) from function context.
- **`gd lsp split-declaration` / `join-declaration`** — split `var x = expr` into `var x` + `x = expr`, or join them back. Useful for separating declaration from initialization.
- **`gd lsp introduce-variable --const`** — introduce a constant instead of a variable. Emits `const NAME = expr` with UPPER_SNAKE_CASE naming.
- **`gd lsp convert-node-path`** — convert between `$NodePath` and `get_node("NodePath")` syntax. Auto-detects direction from cursor position, handles quoting for paths with `/`, `..`, etc.
- **`gd lsp convert-onready`** — convert between `@onready var` and explicit `_ready()` assignment. Creates `_ready()` if it doesn't exist, preserves type annotations, inserts `pass` when body becomes empty.
- **`gd lsp convert-signal`** — move signal connections between `.tscn` scene wiring and `.connect()` calls in GDScript. Cross-file operation with undo support for both files.
- **Collision warnings for rename** — `gd lsp rename` now warns when the new name collides with existing symbols in scope.
- **Post-refactoring syntax validation** — all refactoring commands now verify the output has no new parse errors compared to the original, preventing broken code from being written.

### Fixed
- **`gd lsp extract-method`** — correctly captures local variables that are assigned in the extracted range and used after it, returning them from the new function.

## [0.2.42] - 2026-02-24

### Added
- **`gd lsp inline-variable`** — inline a variable by replacing all usages with its initializer expression, then deleting the declaration. Supports `--dry-run`.
- **`gd lsp undo-list`** — list recent refactoring operations that can be undone, with timestamps and affected files.
- **`gd lsp undo`** — undo the most recent refactoring (or a specific entry by `--id`). Restores original file contents from snapshots. Supports `--dry-run`.
- **Undo support for all refactoring commands** — every file-writing refactoring now records an undo entry: extract-method, extract-class, inline-method, inline-variable, introduce-variable, delete-symbol, move-symbol, and move-file.
- **Collision warnings** — `introduce-variable` and `extract-method` now warn when the chosen name collides with a local variable, function, class, or GDScript builtin in scope.
- **Atomic rollback** — multi-file refactorings (extract-class, move-symbol, move-file) now use a write-ahead-log transaction that automatically restores all files if any step fails.

## [0.2.41] - 2026-02-23

### Added
- **Lint rule: `collapsible-if`** — detect nested `if` statements that can be collapsed into a single `if a and b:`. Default-enabled, fixable.
- **Lint rule: `needless-bool`** — detect if/else returning or assigning boolean literals, and ternary expressions like `true if x else false`. Suggests simplification to `return condition` or `x = condition`. Default-enabled, fixable.
- **Lint rule: `enum-variant-names`** — detect enum variants that all share a common prefix or suffix matching the enum name (e.g. `enum Color { COLOR_RED, COLOR_GREEN }`). Opt-in.
- **Lint rule: `manual-range-contains`** — detect manual range checks like `x >= 0 and x < 10` that can be written as `x in range(0, 10)`. Also detects the negated `or` form. Opt-in, fixable.

## [0.2.40] - 2026-02-23

### Fixed
- **Test failure messages now show assertion details** — GUT and gdUnit4 failure output now extracts the actual assertion message from JUnit XML CDATA bodies instead of just showing the generic `message` attribute (e.g. `"failed"`). Includes line numbers when available.
- **gdUnit4 stdout fallback** — when gdUnit4 fails to generate JUnit XML (e.g. addon/Godot version mismatch), per-test results are parsed from stdout with ANSI stripping. Addon-internal errors are filtered from output.
- **Compact failure display** — multi-line assertion messages (common in gdUnit4) are normalized to single-line output: `Expecting: '3' but was '2'` instead of spanning 4 lines.

## [0.2.39] - 2026-02-23

### Added
- **Unified `TestRunner` trait** — all three test runners (GUT, gdUnit4, script) now implement a common `TestRunner` trait with `RunContext`, replacing ad-hoc free functions.
- **`--name`/`--class`/`--filter` for gdUnit4** — flags now map to per-file `-a` args and `-i` (ignore) exclusion flags via tree-sitter parsing. Previously these flags only worked with GUT.
- **`--name`/`--class` for script runner** — filters at file level by parsing test content with tree-sitter; only files containing matching tests are executed.

### Fixed
- **`--no-color` now works for `gd test`** — `hprintln!` macro and all summary output in `exec_run()` now route through color-aware `cprintln!`/`ceprintln!` macros that respect `--no-color` and `NO_COLOR` env.
- **Deduplicated test output** — per-file group summary (✓/✗ per file) now only shows in `--quiet` mode where per-test results are suppressed. Previously it appeared alongside per-test results, creating two visually similar output blocks.
- **Temp test XML files moved to `.godot/`** — GUT JUnit XML and gdUnit4 report directories now write to `.godot/` (already gitignored) instead of cluttering the project root.
- **`gd lsp create-file --input-file` with `--class-name`/`--extends`** — now prepends the class header to custom content. Previously `class_name` and `extends` were silently dropped when `--input-file` or stdin was used.
- **`gd lsp delete-symbol` scoped reference search** — dangling reference check now scopes to the declaring class (via `class_name`) or source file, instead of grep-matching the symbol name globally. Previously deleting `get_current_tick` from one class reported false positives from every unrelated class with a same-name method.

## [0.2.37] - 2026-02-23

### Added
- **`gd test run` cargo-like UX** — new positional `name` argument filters test functions by name (GUT `-gunit_test_name`), `--class` filters by inner class (`-ginner_class`), `--list` lists matching tests via tree-sitter parsing without running them, `--junit <file>` exports JUnit XML (`-gjunit_xml_file`). Paths moved to `-p/--path` flag. All new flags are GUT-specific; other runners warn gracefully. Supports `--format json` for `--list`.

## [0.2.36] - 2026-02-23

### Added
- **Lint rule: `duplicate-code`** — detect structurally similar functions within the same file. Compares function bodies as normalized AST fingerprints (node kinds only, identifiers and literals stripped) using Levenshtein edit distance. Groups of 3+ similar functions reported as a single diagnostic. Opt-in rule with configurable `min_statements` (default 5) and `similarity_threshold` (default 80%).

## [0.2.35] - 2026-02-23

### Fixed
- **Refactor tools: doc comment bridging** — `declaration_full_range` now bridges up to 2 blank lines when `##` doc comments exist above a declaration. Previously, the blank line between `## Doc` and `var`/`func` caused doc comments to be left behind as orphans during move-symbol, delete-symbol, extract-class, and other refactor operations.
- **Refactor tools: section divider exclusion** — `# ===`/`# ---`/`# ~~~`/`# ***` section dividers are no longer included in symbol ranges. Previously, divider blocks adjacent to doc comments would travel with the symbol during refactoring.
- **`replace-body` signature guard** — now errors if the input's first non-empty line starts with `func ` or `static func `, which indicates the caller accidentally included the function signature. Previously this silently created a duplicate declaration.
- **`create-file` accepts positional path** — `gd lsp create-file path.gd` now works. Previously required `--file path.gd`.

## [0.2.34] - 2026-02-23

### Improved
- **`$NodePath` deep resolution** — hover and completion now follow `instance = ExtResource(...)` chains across scene boundaries. `$View/Model/Skeleton3D/Mesh` resolves through instanced sub-scenes instead of falling back to the type annotation. Hover shows the cross-scene origin (e.g. `Scene: player.tscn → soldier_model.tscn`). Depth limit of 8 prevents infinite recursion.

## [0.2.33] - 2026-02-23

### Added
- **`gd overview`: code-based signal flow** — `.connect()` calls in GDScript are now detected and shown in the Signal Flow section alongside scene-wired connections. Handles `signal.connect(handler)`, `$Node.signal.connect(handler)`, and `Autoload.signal.connect(handler)` patterns.

## [0.2.32] - 2026-02-23

### Added
- **`gd overview`** — new command that outputs a compact project architecture summary: scripts with extends/class_name, scene bindings, signal/export declarations, function signatures, signal flow grouped by scene, and autoloads. Supports `--format json` and path scoping (`gd overview src/entities/`).

## [0.2.31] - 2026-02-23

### Fixed
- **`$NodePath` hover at `$` position** — hovering on the `$` character in `$NodePath` now correctly shows node info. Previously only worked when the cursor was inside the name portion.
- **Signal handler hover** — hovering on a function declaration like `func _on_body_entered()` now shows the connected signal, source node, and scene file. Previously only showed the function signature.
- **Deep `$NodePath` fallback** — node paths that traverse instanced sub-scenes (e.g. `$View/Model/Skeleton3D/Mesh`) now show the type from the variable's type annotation when scene resolution can't follow the instance chain.

## [0.2.30] - 2026-02-23

### Added
- **Scene-aware LSP** — the LSP now indexes `.tscn` scene files alongside `.gd` scripts, enabling cross-referencing between scripts and scenes.
- **Script↔scene references** — `gd lsp references` on a script or class now includes `.tscn` locations where the script is attached to nodes.
- **Signal connection navigation** — go-to-definition on signal handler functions (e.g. `_on_body_entered`) jumps to the signal source in `.tscn`. Find-references on signal declarations includes `.tscn` connection sites.
- **`$NodePath` hover** — hovering on `$Player/Sprite2D` or `get_node("path")` shows the resolved node type, parent path, attached script, and groups from the scene tree.
- **Signal handler hover** — hovering on a signal handler function shows the connected signal name, source node, and scene file.
- **Node path completion** — autocomplete inside `$...` and `get_node("...")` suggests node paths from the scene tree with types as detail text.
- **`gd lsp scene-refs`** — list all scenes that reference a given script, with node name, type, and parent path.
- **`gd lsp signal-connections`** — list all signal connections targeting handler functions in a given script.

### Improved
- **LSP `.tscn` file tracking** — `did_open`, `did_change`, and `did_save` now update the scene index incrementally, keeping cross-references fresh as scenes are edited.

## [0.2.29] - 2026-02-23

### Improved
- **LSP hover: `preload`/`load` keywords** — hovering on `preload` or `load` now shows signature and description.
- **LSP hover: bare member class priority** — bare identifiers like `rotation` now check ClassDB properties via the file's `extends` chain before falling back to the generic first-match. Fixes `rotation` showing `float Node2D.rotation` on a Node3D script (now correctly shows `Vector3 Node3D.rotation`).

## [0.2.28] - 2026-02-23

### Improved
- **LSP hover: full untruncated docs** — method and property hover now shows complete Godot documentation including code examples and usage patterns. Previously truncated to 300 characters.
- **LSP hover: enum value docs** — hovering on engine enum constants (e.g. `CONNECT_DEFERRED`) shows the integer value and description. Hovering on an enum type name (e.g. `Viewport.MSAA`) lists all values with descriptions. Bare global constants like `OK`, `KEY_ESCAPE` also show docs.

### Fixed
- **LSP hover: bare member class resolution** — bare identifiers like `velocity` now resolve using the file's `extends` class and inheritance chain. Previously always matched the first entry (e.g. `CharacterBody2D.velocity` even in a `CharacterBody3D` script).

## [0.2.27] - 2026-02-23

### Improved
- **LSP hover/completion: embedded Godot API docs** — hover and completion now show rich documentation for the entire Godot API (992 classes, 8753 methods, 3876 properties, 488 signals, 114 utility functions). Extracted from Godot's `extension_api.json` with BBCode-to-Markdown conversion. Hand-written docs for common members are kept as priority overrides.

### Added
- **`tools/bbcode.py`** — shared BBCode-to-Markdown converter for Godot doc strings, used by both code generators.
- **ClassDB doc lookup API** — `class_doc()`, `method_doc()`, `property_doc()`, and `utility_function()` with inheritance chain walking.

## [0.2.26] - 2026-02-23

### Added
- **`gd scene add-instance`** — instance a `.tscn` scene as a child node, with auto ext_resource management and PascalCase name inference from filename.
- **`gd scene add-sub-resource`** — create `[sub_resource]` sections with `--prop key=value` pairs and optional `--node`/`--key` for automatic property assignment.
- **`gd scene batch-add`** — add multiple nodes in one command using `--node Name:Type[:Parent]` format. Handles parent-then-child ordering.
- **`gd scene duplicate-node`** — copy a node section with all properties, with optional reparenting via `--parent`.

### Fixed
- **`gd scene`: nested parent path resolution** — `--parent MarginContainer/VBoxContainer` now works correctly. `find_node()` resolves by computed path first, then bare name with ambiguity detection. All scene subcommands (`add-node`, `remove-node`, `set-property`, `attach-script`, `detach-script`) now accept full node paths.
- **`gd scene set-property`: unambiguous node matching** — when multiple nodes share the same name, the resolved node's parent attribute is now used to match the correct `[node]` section header.

## [0.2.25] - 2026-02-23

### Improved
- **Type inference: property resolution** — `infer_expression_type` now resolves property access on typed receivers. Covers value-type builtins (Vector2.x, Color.r, Rect2.position, Transform3D.origin, etc.), ClassDB properties (Node2D.position, CharacterBody2D.velocity), and cross-file user-defined class variables via `ProjectIndex`.
- **Type inference: `is`-check narrowing** — identifiers inside `if event is InputEventKey:` bodies or after `if not event is InputEventKey: return` early-exit guards are now narrowed to the specific type. Works in both the inference engine and `gd check`.
- **Type inference: `:=` initializer resolution** — `gd check` now resolves types for `:=` variables initialized from `Type.new()` constructors, `as Type` casts, and same-file functions with return type annotations. Eliminates false positives like `var target := Node3D.new(); var d := target.position`.
- **Type inference: preload/load resolution** — `preload("res://scene.tscn")` now infers `PackedScene`, `.gd` files resolve to `GDScript` (or their `class_name` with project index), images to `Texture2D`, audio to `AudioStream`, shaders to `Shader`.
- **`variant-inference` lint rule** — upgraded from per-file to project-wide inference, benefiting from cross-file property resolution and preload/load type resolution. Fewer false positives when used with typed codebases.

### Added
- **`class_db::property_type()`** — look up property types on ClassDB classes with inheritance chain walking, mirroring the existing `property_exists()`.

## [0.2.24] - 2026-02-22

### Added
- **`nullable-current-scene` lint rule** (suspicious, default-enabled) — detects `get_tree().current_scene.method()` without a null check. `current_scene` is null when scenes use `root.add_child()` instead of `change_scene_to_file()`. Also catches aliased access (`var s = get_tree().current_scene; s.method()`).
- **`untyped-array-argument` lint rule** (type_safety, default-enabled) — detects passing an untyped `Array` or wrong-element `Array[X]` to a parameter expecting `Array[T]`. Resolves parameters from same-file symbol table and cross-file `ProjectIndex`.
- **`unused-private-function` lint rule** (maintenance, opt-in) — detects functions with no cross-file callers. Excludes Godot virtual methods (`_ready`, `_process`, etc.) and engine method overrides.
- **`unused-class-signal` lint rule** (maintenance, opt-in) — detects signals with no cross-file `connect()`/`emit()` references. Also scans `.tscn` files for `[connection signal="name"]`. Skips signals starting with `_`.
- **`unused-class-variable` lint rule** (maintenance, opt-in) — detects class-scope variables with no cross-file references. Excludes `@export`, `@onready`, constants, and autoload class members.
- **`ParamSummary` in `ProjectIndex`** — function summaries now include parameter names and types for cross-file argument checking.
- **`ProjectIndex::files()` / `project_root()` / `is_autoload()` accessors** — public API for cross-file lint rules.

## [0.2.23] - 2026-02-22

### Added
- **`duplicate-variable` lint rule** (correctness, default-enabled) — detects duplicate `var` declarations at class scope. Catches cases like `replace-symbol` leaving stale declarations that Godot rejects at load time.
- **`native-method-override` signature checking** — now detects parameter type mismatches (e.g. `String` vs `NodePath`), return type mismatches (e.g. `String` vs `StringName`), and parameter count errors when overriding native methods. Previously only checked method name.
- **Method signatures in ClassDB** — `generate_class_db.py` now emits `METHOD_SIGNATURES` table with return type, required/total param counts, and param types for all engine methods.
- **`gd lsp move-file`** — move/rename a file and update all references (preload, load, ext_resource, project.godot autoloads) across the project. Supports `--dry-run` for preview.
- **`gd lsp create-file --force`** — overwrite an existing file intentionally (bypasses the existence check).

### Improved
- **Bevel spherical arc interpolation** — for circular profile (default 0.5), arc intermediates between beveled edges now use spherical interpolation on the bevel sphere instead of quadratic bezier. Eliminates the ~57% radius shortfall that caused concave pinch artifacts at corners. All cap vertices now sit at consistent distance from the original vertex.
- **Bevel corner patches** — when 3+ beveled edges meet at a vertex, cap fill now uses structured K-fold symmetric kite quads (S=2) or concentric sphere-projected rings (S≥3) instead of flat-centroid ring inset. Center vertex is projected onto the bevel sphere for smooth geometry.
- **Boolean annular quad bridging** — when a boolean cut creates a hole through a coplanar face (e.g. cylinder through cube), the annular region (outer square + inner circle) is now bridged with structured quad/tri strips instead of keeping ~112 starburst fragments. Produces clean radial edge flow suitable for bevel and subdivision.

### Fixed
- **`gd mesh create` fails on second invocation** — `queue_free()` on the old `_GdMeshHelper` node deferred deletion until end of frame, causing a name collision when the replacement node was added immediately. Godot auto-renamed the new node, making subsequent push scripts unable to find it. Fixed by using `remove_child()` before `queue_free()` to immediately free the name slot. Same fix applied to `_GdMeshGrid` overlay recreation.
- **`variant-inference`: detect `:=` with `in`/`not in` operator** — `var x := action in arr` now flags as a Variant inference error. Godot's parser treats `in`/`not in` as returning Variant even though it's always bool at runtime.
- **`gd check`: detect `:=` on unresolvable property access** — `var keycode := event.physical_keycode` (where `event` is typed as a base class like `InputEvent`) now flags as Variant inference error. Only triggers when the receiver is typed as a ClassDB class; builtin types (`Vector2.x`), `self`, and method calls are excluded.
- **`gd check`: detect `load().instantiate()` with `:=`** — `var x := load("res://scene.tscn").instantiate()` now flags as Variant inference error. `load()` returns `Resource` which has no `instantiate()` method; Godot rejects this at parse time. `preload()` is correctly excluded (Godot resolves it to `PackedScene`).
- **`gd check`: detect ClassDB Variant-return methods with `:=`** — `var meta := node.get_meta("key")` (where `node` is typed as a ClassDB class) now flags when the method returns `Variant` in ClassDB. Only triggers on typed receivers; untyped variables and `self` are excluded.
- **`look-at-before-tree`: detect `global_*` property assignments** — setting `global_position`, `global_rotation`, `global_rotation_degrees`, `global_transform`, or `global_basis` on a node before `add_child()` now triggers a warning. Previously the rule only caught method calls.
- **`gd resource set-property`: multi-line value replacement** — replacing a property with a multi-line value (arrays, dictionaries) now correctly removes all continuation lines instead of leaving orphaned fragments.
- **`gd scene set-property`: duplicate property on blank-line separator** — when a blank line separated the `[node]` header from existing properties, set-property inserted a duplicate instead of replacing. Fixed ordering so replacement always takes priority.
- **`gd scene set-property`: multi-line value replacement** — same multi-line consumption fix as resource set-property.

## [0.2.21] - 2026-02-21

### Added
- **Rust-native primitives** — cube, sphere, and cylinder are now built entirely in Rust with correct CCW winding. Godot becomes display-only; eliminates the CW/CCW winding mismatch that caused boolean normal inversions.
- **Boolean rewrite** — plane-based polygon splitting replaces the old triangle-triangle approach. Produces watertight output with quad-dominant topology. Pipeline: split → classify → T-junction repair → dissolve coplanar → quadrangulate n-gons → tag boundary edges.
- **N-gon quadrangulation** — boolean output n-gons (5+ vertices) are converted to quad-ring topology for clean bevel and subdivision behavior.
- **Coplanar edge dissolution** — fragments from plane-based splitting are merged back into larger polygons. Degenerate near-zero-area faces are unconditionally merged with neighbors.
- **Edge tagging for boolean boundaries** — boundary edges from boolean operations are tagged so `gd mesh bevel --edges tagged` can selectively bevel only the cut edges.
- **Grid-fill caps** — replaced earcut inner fan triangulation with grid-fill for quad-dominant cap topology. All generators (extrude, revolve, loft) now emit quads for side walls and multi-ring quad caps.
- **`gd mesh extrude-face`** — extrude selected faces along their normals by a given depth. Use `--where "y>0.4"` to select faces by spatial filter on centroid position.
- **`gd mesh boolean --count --spacing`** — array boolean: repeat a boolean operation N times with incremental offset. Useful for cutting repeating patterns (rail teeth, vent slits, magazine holes) in a single command.
- **`gd mesh profile --hole`** — multi-contour profiles with holes. Repeatable flag accepts hole polygons for hollow cross-sections (rings, frames). Earcut triangulates caps with holes natively.
- **`gd mesh bevel --where`** / **`--edges tagged`** — edge-selective bevel via spatial filter or boolean boundary tags.
- **`gd mesh inset --where`** — face-selective inset via spatial filter (e.g. `--where "z<-0.3"` insets only back faces).
- **`gd mesh flip-normals --where`** — flip normals on faces matching a spatial filter.
- **Spatial filter system** (`--where` flag) — shared `axis op value` expressions (e.g. `y>0.12`, `z<=-0.5`) for face centroid and edge midpoint filtering. Used by `bevel`, `inset`, `extrude-face`, and `flip-normals`.
- **Multi-ring concentric cap topology** — circle/polygon caps with >= 5 vertices now generate N concentric quad rings (auto: `max(1, n_pts/8)`, capped at 3) instead of a single fan triangulation. Eliminates pole singularity for UV unwrapping.
- **Quad-ring cap topology for revolve and loft** — consistent with extrude caps.
- **Mesh command recording & replay** — `gd mesh replay` replays recorded JSONL command sequences for reproducible builds.
- **Part groups** — `gd mesh group`, `ungroup`, `groups` for batch operations on named sets of parts.
- **Per-response mesh stats** — all mesh commands now include `_stats` in JSON output (face count, quad/tri/ngon breakdown, boundary edges).
- **Material preset persistence** — presets (glass, metal, rubber, etc.) are stored and re-applied on push.
- **Transform bake** — transforms are baked into vertex positions for correct world-space boolean operations.
- **Overlap tier detection** — `gd mesh check` detects overlapping/floating parts with configurable margin.
- **Flat shading default** — new mesh sessions default to flat (per-face) shading instead of smooth.
- **Auto-focus** — part operations automatically focus the affected part.
- **`gd mesh overlay edges/off`** — toggle edge wireframe overlay in the Godot viewport. Edges are color-classified: boundary (red), sharp (yellow, >30° dihedral), interior (gray). X-ray style (depth test disabled). Hidden automatically during `gd mesh view` screenshots.
- **Camera keyboard shortcuts** — press 1–0 in the Godot viewport to switch camera angles (Front, Side, Top, Back, Left, Bottom, and four 3/4 views).
- **`HalfEdgeMesh::classified_edges()`** — extract deduplicated edges classified by type (boundary, sharp, interior) based on dihedral angle.

### Changed
- **Boolean output is now watertight and quad-dominant** — 0 boundary edges, 88-97% quad ratio across all tested primitive combinations (cube×cube, cylinder×cube, cube×cylinder, sphere×ngon).
- **Area-weighted vertex normals** — accumulates raw (unnormalized) Newell vectors instead of unit face normals. Degenerate zero-area faces get near-zero weight, preventing fallback normals from corrupting neighbors.
- **Bevel vertex caps emit quads** instead of triangle fans.
- **Solidify and merge preserve quad topology** — no longer triangulate during shell and merge operations.
- **Mesh subcommand count: 47** (was 42) — added `extrude-face`, `replay`, `group`, `ungroup`, `overlay`.
- **Transforms bake into vertices** — `translate`, `rotate`, `scale` now modify mesh vertex positions directly in Rust instead of round-tripping through Godot GDScript. Faster and more reliable.
- **Bevel vertex caps use concentric ring fill** — caps with 5+ vertices use `build_quad_cap_3d` (same as extrude/boolean caps) instead of paired fan, producing cleaner quad topology.

### Fixed
- **Boolean winding reversal hack removed** — the unconditional `poly.reverse()` in boolean output is gone. Rust-side CCW primitives eliminate the CW/CCW mismatch.
- **Boolean degenerate face merging** — thin sliver fragments from plane splitting at tangent intersections are merged with neighbors during dissolve, preventing fallback normals.
- **Boolean dissolve fallback** — if coplanar dissolution breaks watertightness, falls back to the raw (guaranteed watertight) mesh.
- **Bevel watertightness** — strip winding derived from half-edge direction instead of heuristic.
- **Bevel concave crash** — no longer panics on concave polygon bevels.
- **Snapshot data loss** — non-destructive bake eliminates `load()` cache issues during export.
- **Hole winding** in multi-contour profiles corrected.
- **Focus desync** between Rust `MeshState` and Godot scene.
- **Checkpoint restore** reliability improved.
- **`merge-verts --all`** now works across all parts.
- **`extrude-face` corruption** on complex meshes fixed.
- **`--parts` batch material** — fixed variable scope bug that skipped parts.
- **Boolean degenerate sliver filter** — Newell's method area check rejects near-zero-area polygon fragments from plane splitting.
- **Dissolve multi-loop fallback** — when coplanar dissolution produces multiple boundary loops (hole through a face), falls back to original fragment faces instead of producing invalid polygons with holes.

## [0.2.20] - 2026-02-20

### Added
- **`--no-color` flag and `NO_COLOR` env** — disable ANSI color codes in all CLI output. Useful for piping, AI agent tools, and terminal emulators that don't render ANSI (e.g. Claude Code VS Code panel).
- **`gd lsp references --name` CSV support** — pass comma-separated symbol names (`--name "foo,bar,baz"`) to search for multiple symbols in one call.
- **Async eval support** — `gd debug eval` now handles GDScript coroutines. Expressions returning `Signal` (from `await`) are automatically awaited. Timer re-entrance during async awaits is prevented with a polling guard.
- **`gd mesh` command** — procedural 3D mesh editing for Godot via CLI. 42 subcommands for building meshes from 2D profiles, primitives, and transforms without leaving the terminal. Designed for AI agent workflows with JSON output and batch execution.
  - **Workspace**: `init` (create workspace scene), `create` (bootstrap session with camera rig and optional primitive)
  - **Geometry**: `profile` (define 2D polygon on a plane, `--copy-profile-from` for reuse, `--shape circle` for circle/arc profiles), `extrude` (with `--segments N`), `revolve` (with `--cap`, `--degrees`), `taper` (with `--from-scale`/`--to-scale`, `--from`/`--to` range, `--midpoint`), `bevel` (with `--edges all|depth|profile`, `--profile 0.0–1.0` for concave→convex), `subdivide` (edge midpoint, each triangle → 4), `loop-cut` (axis-aligned plane cut), `move-vertex`
  - **Boolean operations**: `boolean --mode subtract|union|intersect --tool <part>` with split-and-classify algorithm, vertex welding, and Moller-Trumbore ray-triangle intersection
  - **Modifiers**: `inset` (shrink faces inward by factor), `solidify` (shell thickness via offset + stitch), `merge-verts` (remove duplicate vertices within distance threshold), `array` (linear duplication with offset)
  - **Parts**: `add-part` (named sub-parts from empty or primitives), `focus` (switch active / `--all`), `remove-part`, `duplicate-part` (`--mirror x|y|z` with position negation, `--symmetric` auto-offset), `info` (`--all` with world-space AABB and transforms), `describe` (one-shot debrief with composite views)
  - **Transforms**: `translate` (`--relative`, `--relative-to <part>`), `rotate`, `scale` (`--remap` re-center)
  - **Materials**: `material` (hex/named color, `--preset glass|metal|rubber|chrome|paint|wood|matte|plastic`, `--parts` glob/comma list)
  - **Normals**: `fix-normals` (auto-detect outward via majority vote, `--all`), `flip-normals` (reverse winding, `--caps x|y|z`, `--all`)
  - **Shading**: `shade-smooth` (averaged vertex normals), `shade-flat` (per-face faceted), `auto-smooth` (smooth below angle threshold)
  - **Viewing**: `view` (7 orthographic + 7 isometric angles, `--zoom`, `--normals` debug overlay, `--focus`), `list-vertices` (`--region` bounding box filter)
  - **State**: `checkpoint` (`--name`), `restore` (`--name`), `snapshot` (export to `.tscn` with materials and transforms)
  - **Utilities**: `reference` (validate reference image), `batch` (execute JSON command array), `check` (detect floating/disconnected parts with `--margin`)
  - **Half-edge mesh engine**: 15 core modules — `half_edge`, `profile`, `extrude`, `revolve`, `bevel`, `taper`, `subdivide`, `loop_cut`, `array`, `merge`, `mirror`, `normals`, `boolean`, `inset`, `solidify`, `loft`

### Fixed
- **`fix-normals` all-inverted detection** — replaced single seed-face centroid heuristic with majority vote across all faces. Now correctly detects and fixes meshes where 100% of normals are inverted.
- **`remove-part` state desync** — now removes the part from Rust `MeshState` after the Godot node is removed. Prevents `--all` operations (auto-smooth, fix-normals, etc.) from crashing on stale part references.
- **`duplicate-part --mirror x` position negation** — now negates `transform.position` on the mirror axis, so `--mirror x` places the duplicate at the opposite X position instead of the same position.
- **`--parts` comma-separated glob patterns** — `--parts "intake-*,headlight-*,taillight-*"` now correctly matches all three patterns. Previously only matched the first glob when the string contained both commas and wildcards.

## [0.2.19] - 2026-02-19

### Added
- **`unnamed-node` lint rule** (godot, opt-in) — detects `add_child()` calls where a dynamically created node (via `.new()`) has no `.name` set, making it harder to find in the scene tree at runtime.
- **`infer-unknown-member` lint rule** (type_safety) — detects `var x := obj.member` where `obj` has a declared engine type but `member` is not a known property, method, or signal on that type. Godot's parser cannot infer the type and throws a parse error; this rule catches it at lint time.
- **`--rule` flag now force-enables opt-in rules** — `gd lint --rule <name>` activates the specified rule even if it's opt-in or disabled, instead of only post-filtering results.
- **Inline suppression docs** — `gd lint --help` and `gd man lint` now document the `# gd:ignore` / `# gd:ignore-next-line` / `# gd:ignore[rule]` suppression comment syntax. Also added to `gd llm` output for AI agent discoverability.
- **ClassDB signals table** — the generated class database now includes signal names (e.g., `value_changed`, `pressed`), used by lint rules to distinguish signals from unknown members.
- **`gd debug eval --file`** — read eval script from a file instead of `--expr`. Supports `--file -` for stdin. Preserves tabs and indentation, enabling loops/if/else blocks that can't be expressed in single-line `--expr`.

### Improved
- **Concurrent eval server** — the TCP eval server now accepts multiple simultaneous connections via a queue. One instance can install a persistent node (e.g., walk controller) while another queries game state. Node-based scripts are queued and executed next frame; RefCounted scripts execute immediately. Previously, the server only handled one eval at a time.
- **Dot-completions for builtin types** — `position.x`, `diff.length()`, `dir.normalized()` and other member accesses on inherited properties, binary expressions, and chained method calls now return correct completions. Type inference follows the full chain: inherited member → ClassDB property type → builtin member return type → progressive local variable resolution.

### Fixed
- **Multi-statement eval now returns results** — `gd eval` and `gd debug eval` with multi-statement input (semicolons or newlines) now smart-wraps the last expression with `return`. Handles var declarations, assignments, void calls, and explicit `return` without double-wrapping.
- **Game logs no longer leak into eval output** — eval output capture now uses begin/end markers so only prints from the eval script are shown, not concurrent game output (e.g. `[ClientController]` logs).
- **Eval errors now include details** — compilation and runtime errors from the eval server now include Godot's actual error messages instead of generic "Script compilation failed" or "GDScript error paused the game".
- **`gd debug screenshot` returns correct path on native Windows** — previously always converted to WSL `/mnt/c/...` path even on native Windows. Now only translates when actually running under WSL.
- **`callable-null-check` false positive on chained access** — `server.validator.is_valid()` guarding `server.validator.call()` no longer triggers a warning. The rule now recursively collects identifiers from nested attribute chains.
- **`parameter-shadows-field` false positive on static functions** — static factory methods like `static func from_box(id: int)` matching a field name no longer warn, since static methods have no `self`.
- **`replace-symbol` preserves indentation in inner classes** — replacing a symbol inside an inner class now re-indents the new content to match the original depth.
- **`result = ...` eval convention restored** — `gd debug eval --expr "result = 'hello'"` works again. Single-expression assignments to `result` are detected and wrapped with `var result` + `return result` instead of invalid `return result = ...`.
- **Eval semicolons in comments no longer corrupt `--file` scripts** — files with `;` in GDScript comments (e.g. bash loop examples) are now split on newlines, not semicolons. Newline splitting takes priority for multi-line input.
- **Breakpoint reason included in eval error messages** — when an eval script triggers a GDScript error, the `debug_enter` reason from Godot (e.g. the actual error text) is now captured and shown instead of generic "GDScript error paused the game".
- **Daemon startup race on Windows** — state file is now written immediately after port binding (before building workspace index), and the client retries for up to 5s instead of failing on first connection error.
- **`use-before-assign` false positives on Node subclasses** — members assigned in `_ready()` or `_init()` (directly or transitively through called methods) are now recognized as initialized in other methods. Previously, procedural UI code that assigned members in `_build_ui()` called from `_ready()` would produce spurious warnings.
- **`use-before-assign` false positives on null-guarded members** — bare identifier reads (`if member:`, `if not member: return`, `if x == member`) are no longer flagged as use-before-assign. Only dereference reads (`.prop`, `[idx]`) are reported. Additionally, members that are null-checked before being dereferenced are automatically suppressed.
- **`delete-symbol --line` no longer matches enclosing function** — `--line N` pointing to a statement inside a function body now correctly reports "no declaration found" instead of deleting the entire enclosing function. Only the declaration start line matches.
- **`native-method-override` no longer flags user-defined base class methods** — overriding methods from user-defined base classes (e.g., State pattern `enter()`/`exit()`) is normal polymorphism. The rule now only flags overrides of engine-native (ClassDB) methods.
- **`print-statement` no longer flags `push_error()`/`push_warning()`** — these are Godot's structured logging (shown in debugger with stack traces), not debug prints. They belong in production code.
- **`todo-comment` word boundary matching** — markers like `BUG`, `XXX`, `WARNING` now require word boundaries. "Debug trail" no longer matches `BUG`, and `"xxx-1f"` no longer matches `XXX`.
- **Formatter keeps `# gd:ignore-next-line` attached** — `gd fmt` no longer inserts blank lines between a suppression comment and the declaration it targets (functions, signals, etc.).

## [0.2.18] - 2026-02-17

### Added
- **LSP semantic index** — `WorkspaceIndex` now caches per-file `SymbolTable`, declaration index (symbol name → declaring files), and extends graph. Cross-file operations use indexed lookups instead of re-parsing all files on every request.
- **Inlay hints** (`textDocument/inlayHint`) — ghost text showing inferred types for `:=` variables and parameter names at call sites.
- **Signature help** (`textDocument/signatureHelp`) — function signature with active parameter highlight as you type inside `(`. Triggers on `(` and `,`.
- **Call hierarchy** (`callHierarchy/incomingCalls`, `callHierarchy/outgoingCalls`) — "Who calls this?" and "What does this call?" for any function.
- **Find implementations** (`textDocument/implementation`) — find all subclasses of a class and all overrides of a method via the extends graph. Also available as `gd lsp find-implementations --name <method> [--base <class>]`.
- **Semantic tokens** (`textDocument/semanticTokens/full`) — type-aware syntax highlighting: classes, enums, functions, signals, variables, parameters, and engine types get distinct token types.
- **Workspace symbol search** (`workspace/symbol`) — fuzzy search across all project symbols (Ctrl+T / Cmd+T in VS Code).

### Improved
- **Hover shows origin** — cross-file member hovers now show which class/file the symbol comes from (e.g., `*VehicleData*`). Hovering `extends ClassName` now resolves to the class definition with file origin.
- **Symbols show signatures** — `gd lsp symbols` detail field now shows actual declarations (`func(params) -> ret`, `var name: Type`, `signal name(params)`, enum members) instead of just kind names.
- **Rename includes context** — `gd lsp rename --dry-run` edits now include the source line for each change, matching the references output format.
- **Completions deduplicated** — symbols from the current file no longer appear twice (once local, once from workspace scan).
- **References pre-filter** — cross-file reference search skips files that don't contain the target identifier text before parsing, reducing unnecessary tree-sitter parses.

### Fixed
- **`safe-delete-file` no longer deletes without `--force`** — previously, unreferenced files were auto-deleted even without `--force`. Now only reports references by default; `--force` is required to actually delete.
- **Refactor commands preserve annotations** — `replace-symbol`, `delete-symbol`, `move-symbol`, and `insert` now include preceding `@rpc`, `@export`, `@onready`, and other annotations when computing a symbol's full range. Previously, annotations on their own line (e.g. `@rpc("any_peer")` above a function) were orphaned or duplicated during refactoring.
- **Windows build** — added missing `Win32_System_IO` feature to `windows-sys` dependency.

## [0.2.17] - 2026-02-17

### Added
- **`gd log` ring buffer** — game output (`print()`, `push_error()`, `push_warning()`) is now captured via the debug protocol into a 2000-entry in-memory ring buffer. View with `gd log` (default: last 50 lines), `--follow` for real-time tail, `--errors` for errors/warnings only, `--grep <pattern>` for filtering, `--json` for structured output, `--clear` to reset.
- **Eval output capture** — `gd eval` in REPL mode now captures and displays `print()` output, `push_error()` (red), and `push_warning()` (yellow) from the evaluated script. Non-void expressions return instantly; void calls poll for up to 1.5s to collect output.

### Changed
- **TCP eval IPC is non-blocking by default** — `send_eval()` (used by automation commands) does an instant drain (~2ms overhead) instead of polling for output. `send_eval_with_output()` (used by the REPL) polls for captured output on void calls.

### Removed
- **`gd run --log` flag** — replaced by `gd log` which uses the debug protocol ring buffer instead of file-based stdout/stderr piping. The ring buffer approach is more reliable (no file race conditions on WSL), captures structured output types (errors vs warnings vs log), and works across `gd stop`/restart cycles.
- **`ctrlc` dependency** — no longer needed after removing `tail_log_file`.

## [0.2.16] - 2026-02-17

### Added
- **Game automation API (phase 2)** — 10 new `gd debug` subcommands for driving a running game by node name/path:
  - `describe` — AI-readable snapshot: player position, nearby nodes with class/groups/distance, current scene, input actions. Auto-detects common player names, adjusts radius for 2D vs 3D.
  - `find` — locate nodes by `--name` (recursive), `--type` (class), or `--group`.
  - `get-prop` — read a property value by node name/path or object ID.
  - `set` — set a property using GDScript expressions (`Vector2(100, 200)`, `"Game Over"`, `200`). No object ID needed.
  - `call` — invoke a method on a node with JSON args array.
  - `navigate` — pathfind via NavigationAgent2D/3D. Sets target, polls until arrival or timeout. The game's own movement code handles physics and animations.
  - `await` — poll until a condition is met: node exists/removed, property `--equals`/`--gt`/`--lt`/`--contains`. Configurable `--timeout` and `--interval`.
  - `mouse-move` — move cursor to screen coordinates or a node's screen position (renamed from `move-to`).
  - `mouse-drag` — multi-step cursor drag between coordinates or nodes (renamed from `drag`).
  - `mouse-hover` — hover cursor over a node/position with configurable dwell time (renamed from `hover`).

### Changed
- **`gd debug eval` now uses full GDScript by default** — previously used Godot's Expression class (no loops, if, var). Now uses the file-based eval server, supporting arbitrary GDScript including loops, conditionals, and variable declarations. Add `--bare` to use the old Expression class behavior (needed for reading local variables at a breakpoint). Also adds `--timeout` flag (default 10s).
- **TCP eval IPC replaces file-based** — the eval server now uses direct TCP (4-byte length-prefixed protocol) instead of filesystem polling. Eliminates transient `ENOENT` on WSL, stale files surviving `gd stop`, and 100ms+ round-trip overhead. Port discovery via `{pid}:{port}` in the ready file. Use `gd run --file-ipc` or `GD_EVAL_FILE_IPC=1` for the old file-based transport.

### Fixed
- **Node2D screen position** — `mouse-move`/`mouse-drag`/`mouse-hover` targeting a Node2D now apply the viewport canvas transform, so coordinates are correct when a Camera2D has panned or zoomed.
- **Hold commands detect game exit** — `press/key/click --hold` and `type --delay` now poll the eval-ready file during the hold/delay instead of a blind sleep. If the game exits mid-hold, the command exits immediately with an error instead of lingering as an orphaned process.
- **Eval server crash recovery** — if a GDScript eval errors during `run()`, the eval server now clears its internal state first, so it never gets permanently stuck. Previously, a runtime error could leave the server in a state where it ignored all subsequent eval requests.
- **Navigate finish detection** — `gd debug navigate` now uses distance-based finish detection (< 25 units from target) in addition to `is_navigation_finished()`. Also detects stalled position (no movement for 1s while close). Godot's `is_navigation_finished()` uses a very tight tolerance and can return false even when the node is visually at the destination.
- **Concurrent eval safety** — stale eval file cleanup now only purges files older than 30 seconds, preventing deletion of result files from concurrent eval calls (e.g., agent running navigate polls alongside other eval commands).
- **Debug break auto-recovery** — when an eval script triggers a GDScript runtime error that pauses the game via the debugger, `gd` now detects the debug break, grabs the stack trace, sends `continue` to resume the game, and returns the error. Previously, a debug break would freeze the eval server indefinitely with a cryptic timeout message.
- **Eval server RefCounted fix** — the eval server now uses `script.new()` instead of `Node.new()` + `set_script()`, correctly handling scripts that extend RefCounted. Previously, non-Node scripts would crash the eval server with "Script inherits from RefCounted" and trigger a debug break.
- **Eval server startup cleanup** — the eval server now purges stale request/result files during initialization, preventing leftover files from a previous session (e.g., when `gd stop` couldn't delete them due to Windows file locking on WSL).
- **WSL file write retry** — eval request file writes retry once on transient `ENOENT` errors, which can occur on WSL under rapid cross-filesystem I/O (e.g., navigate polling every 200ms).
- **Void call detection via ClassDB** — `return print(...)`, `return node.set_pause(...)`, and other void-returning calls no longer trigger a `SCRIPT ERROR` that freezes the game. The eval wrapper now checks 16,346 methods from Godot's ClassDB to detect void returns and omits the `return` keyword. Builtin void functions (`print`, `push_error`, etc.) are also handled.
- **Better eval syntax errors** — `pre_check` now reports line number, column, and surrounding code for parse errors instead of a bare "Script has syntax errors" message.
- **WSL TCP partial read fix** — the eval server now uses `get_partial_data()` in a loop with a 2-second timeout instead of `get_data()`, which could return truncated scripts on WSL cross-VM TCP connections.

## [0.2.15] - 2026-02-16

### Added
- **`gd resource` command** — full CRUD API for `.tres` resource files (7 subcommands): `create`, `set-property`, `get-property`, `remove-property`, `set-script`, `remove-script`, `info`. All write commands support `--dry-run`. Create supports `--script` to attach a script at creation time.
- **StringName (`&"..."`) parse fix** — `.tscn`/`.tres` files containing `&"StringName"` literals now parse correctly. Previously tree-sitter byte offsets were misaligned when `&"` was normalized, causing mangled property values in `gd check`, `gd tree --scene`, `gd lsp scene-info`, and `gd resource info`.
- **LSP hover after dot** — `obj.method` now resolves the receiver's type and shows hover for the member on that type. Handles typed vars, `:=` inferred vars, autoloads, workspace `class_name` refs, and ClassDB inheritance chains.
- **LSP completions for local variable types** — `:=` inferred variables now resolve their type from constructor calls (`ClassName.new()`), class constants (`Vector2.ZERO`), builtin constructors (`Vector2(...)`), same-file function return types (`_get_direction() -> Vector2`), and literals.
- **LSP completions for for-loop typed iterators** — `for npc: Node2D in ...` now provides Node2D members when completing `npc.`.
- **LSP autoload completions** — `EventBus.`, `PokemonDB.` and other autoload singletons now return script-defined signals, methods, and properties plus inherited ClassDB members.
- **LSP Signal member completions** — `signal.emit()`, `.connect()`, `.disconnect()`, etc. now appear when completing after a signal name (e.g. `self.my_signal.` or `EventBus.player_moved.`).
- **LSP workspace class_name + autoload index** — `WorkspaceIndex` now indexes `class_name` declarations and `project.godot` autoloads for O(1) lookups.
- **LSP hover for engine singletons** — bare `Input`, `OS`, `Engine` etc. now show `class Input` on hover.
- **`--hold` for input commands** — `gd debug press/key/click --hold <seconds>` holds the input for a duration instead of a single-frame tap. Uses Rust-side sleep so the game processes physics frames during the hold. Essential for 3D games where single-frame presses barely register.
- **`--delay` for type command** — `gd debug type --delay <ms>` adds a delay between each character for more realistic typing.
- **Orphaned game cleanup** — when the daemon detects a binary rebuild (build_id mismatch), it now kills the orphaned game process before spawning a new daemon. Uses platform-native kill on all platforms (WSL: tasklist.exe + taskkill.exe, Linux/macOS: SIGTERM/SIGKILL, Windows: TerminateProcess).

### Fixed
- **LSP hover name collisions** — hovering on `obj.method` where `method` also exists in the current file now correctly resolves to the receiver's type instead of the local declaration.
- **LSP hover for method calls** — `obj.method()` (with parentheses) now resolves hover via tree-sitter `attribute_call` traversal, not just property access.
- **Eval ID collisions** — rapid-fire eval commands within 1ms got the same timestamp ID, causing overwrites. Now uses compound IDs (`{timestamp}-{pid}-{counter}`) with an atomic counter.
- **Per-ID eval request files** — changed from single `gd-eval-request.gd` (concurrent evals overwrite each other) to `gd-eval-request-{id}.gd` per request. GDScript server scans directory for any matching file.
- **Stale eval file cleanup** — `gd-eval-ready`, `gd-eval-request-*`, and `gd-eval-result-*` files are now cleaned up when the game exits or the daemon restarts.
- **Eval works during tree pause** — eval Timer and runner Node both set to `PROCESS_MODE_ALWAYS`, so `gd eval` works while `SceneTree.paused = true`.
- **WSL game not killed on daemon restart** — `kill_daemon()` was sending Linux `kill` to a WSL shim PID instead of using `tasklist.exe`/`taskkill.exe` to find and kill the actual Windows Godot process.
- **Stale eval timeout cascading** — purge all `gd-eval-request-*` and `gd-eval-result-*` files before each new eval to prevent the eval server from processing leftover requests from timed-out evals. Ready file now validates PID liveness.
- **`gd lsp create-file` ignoring stdin** — piped content via `cat ... | gd lsp create-file --file <path>` was ignored, creating a bare stub file instead. Now checks `is_stdin_readable()` like other refactoring commands.
- **`gd scene attach-script` path resolution** — failed with "Script file not found" when run from a subdirectory. Now resolves relative paths against both CWD and project root, and strips `res://` prefixes.

### Changed
- **Human-readable defaults for all commands** — 22 LSP subcommands (refactoring, scene-info, find-implementations, diagnostics, code-actions) and `gd resource info` now default to concise human-readable output. Use `--format json` for structured output. Saves tokens for AI agents and is easier to scan for humans.
- **Eval server enabled by default** — `gd run` now starts the eval server automatically. `gd eval` and `gd debug` input commands work without any extra flags. Use `--bare` to opt out.
- **Eval output uses file redirect** — eval-without-log mode now redirects Godot's stdout/stderr to the log file via `Stdio::from(File)` instead of pipes, preventing broken-pipe crashes on WSL when the parent process exits.

## [0.2.14] - 2026-02-16

### Added
- **Game liveness detection** — the daemon now checks every 5 seconds whether the game process is still alive. If the game crashes or exits without `gd stop`, the daemon automatically clears `game_running` and `game_pid` state instead of staying stuck forever.
- **Debug TCP disconnect callback** — when the game's debug TCP connection drops, the daemon immediately clears game state. Combined with liveness polling, this provides both instant and fallback detection.
- **Single-instance daemon (`flock`)** — the daemon now acquires an exclusive file lock (`.godot/gd-daemon.lock`) on startup. A second `gd daemon serve` for the same project exits with a clear error instead of silently competing.
- **Enriched `gd daemon status`** — response now includes `game_pid` and `debug_connected` fields alongside the existing `game_running`.
- **Platform-native process management** — `libc` (Unix) and `windows-sys` (Windows) replace shell-out to `kill`/`taskkill` for process liveness checks and game termination.
- **Faster WSL game stop** — replaced slow PowerShell `Get-CimInstance` lookup with `tasklist.exe /FI /FO CSV` for finding the Windows Godot PID.

### Changed
- **Unified game state** — `game_running` (AtomicBool) and `game_pid` (Mutex) merged into a single `game_state: Arc<Mutex<Option<GameInfo>>>`. Eliminates the class of bugs where one was set but the other wasn't.
- **Idle monitor interval** — reduced from 30s to 5s for faster crash detection and idle exit.

### Fixed
- **Daemon stuck after game crash** — the `game_running` flag stayed true forever when the game exited without `gd stop`, blocking idle timeout and confusing agents/scripts polling `gd daemon status`.
- **Stale PID accumulation** — `game_pid` was never cleared on unexpected game exit, leaving the state file pointing at a dead or reused PID.
- **`lsp_cmd.rs` Windows FFI** — replaced hand-rolled `extern "system" { fn GetFileType }` with proper `windows-sys` bindings.

## [0.2.13] - 2026-02-15

### Added
- **Context-aware dot-completions** — typing `self.`, `sprite.`, or `Vector2.` now returns only members of the receiver type instead of the full 500+ global list. Resolves `self`/`super`, typed variables (top-level and local), function parameters, engine classes, builtin types, and workspace class names.
- **ClassDB properties** — extracted 4055 properties from Godot's `extension_api.json` into the class database. New `class_properties()` walks inheritance. Dot-completions show both methods and properties.
- **Human-readable LSP query output** — `gd lsp references/definition/hover/completions/symbols` now default to `--format human` with colored, tabular output. Use `--format json` for machine-readable output.
- **Enum member hover** — hovering over enum members shows computed values (e.g. `Color.RED = 0`), handles explicit and implicit value calculation.
- **Keyword hover** — hovering on `var`, `func`, `const`, `signal`, `class`, `enum` keywords (not just the name) now shows the declaration.
- **Input automation commands** — `gd debug click/press/key/type/wait/screenshot` for game input automation via live eval (experimental).
- **Shared live eval module** — extracted `send_eval()` to `src/core/live_eval.rs` for reuse by eval and input commands.
- **Completions `--kind` filter** — filter completion results by kind (function, method, variable, property, etc.).

## [0.2.12] - 2026-02-15

### Added
- **LSP hover doc comments** — hovering over user-defined functions, variables, constants, signals, enums, and classes now shows `##` doc comments in the hover popup.
- **LSP completion doc comments** — completion items for user-defined symbols now include `##` doc comments in the documentation field.
- **Symbol table doc extraction** — `##` doc comments are parsed and stored on `FuncDecl`, `VarDecl`, `SignalDecl`, and `EnumDecl` in the symbol table, propagated to workspace index.

### Fixed
- **`replace-symbol` on `class_name` duplicated content** — replacing a symbol found via `class_name_statement` now replaces the entire file instead of just the `class_name` line, preventing old content from being appended below.
- **`naming-convention` false positive on private constants** — constants with leading underscores (e.g. `_DIALOG_BOX_SCRIPT`) are now correctly recognized as valid `UPPER_SNAKE_CASE`.
- **`gd fmt` collapsed multiline enums** — enums written across multiple lines are now preserved as multiline, matching the existing behavior for arrays and dictionaries.

## [0.2.11] - 2026-02-15

### Added
- **`gd scene create`** — create new `.tscn` scene files with `--root-type` and optional `--root-name` (defaults to PascalCase of filename).
- **`gd scene add-node`** — add nodes to a scene with `--name`, `--type`, and optional `--parent` (defaults to root).
- **`gd scene remove-node`** — remove a node and all its descendants, with cascading cleanup of connections and orphaned ext_resources.
- **`gd scene set-property`** — set or update a property on any node (pass-through Godot resource format values).
- **`gd scene add-connection`** / **`gd scene remove-connection`** — add or remove signal connections with `--signal`, `--from`, `--to`, `--method`.
- **`gd scene detach-script`** — remove a script from a node, with automatic cleanup of orphaned ext_resources and load_steps.
- All scene commands support `--dry-run` to preview changes without writing.

### Changed
- `scene_cmd.rs` split into a directory module (`scene_cmd/`) with 8 submodules for maintainability.

## [0.2.10] - 2026-02-15

### Added
- **`gd eval`** — evaluate GDScript expressions, statements, or full scripts. Supports inline (`gd eval "1+1"`), file (`gd eval script.gd`), and stdin (`cat file.gd | gd eval -`). Offline mode launches headless Godot; live mode evaluates against a running game.
- **`gd run --eval`** — start the game with an embedded eval server. Enables `gd eval` to run arbitrary GDScript against the live game (query state, mutate objects, add nodes in real-time).
- **`gd test --runner`** — explicitly select test framework (`gut`, `gdunit4`, or `script`) instead of relying on auto-detection.
- **Eval sandbox** — blocks dangerous APIs (`OS.execute`, `HTTPClient`, `Thread`, etc.) and restricts `FileAccess`/`DirAccess` to `res://` and `user://` paths. Use `--unsafe` to bypass.
- **Escape sanitization** — auto-strips invalid GDScript escape sequences (e.g. `\!` → `!`) before sending to Godot, preventing silent crashes.
- **Syntax-highlighted eval output** — scripts sent via live eval are printed with line numbers and keyword/string/number coloring.

### Fixed
- **Stdin consumed twice in `gd eval -`** — piped input was read by the live eval path, then the offline fallback tried to re-read empty stdin. Now reads once and reuses.
- **Daemon state lost after rebuild** — rebuilding `gd` auto-restarted the daemon (build ID mismatch), losing eval server tracking. Now falls back to checking the `gd-eval-ready` file directly.
- **Eval timer killed by cleanup scripts** — the polling timer was auto-named `@Timer@2` and caught by `@`-prefix cleanup. Now named `GdEvalTimer`.

## [0.2.9] - 2026-02-15

### Fixed
- **`class-definitions-order` false positives** — `@rpc` annotations and `@export_group`/`@export_category`/`@export_subgroup` are no longer miscategorized as file-level headers. Only `@tool`, `@icon`, `@static_unload` are treated as headers; export grouping annotations are treated as export vars; `@rpc` and others are skipped.
- **`gd fmt` preserves inline `# gd:ignore` comments** — trailing comments on `signal`, `var`, `const`, `extends`, `class_name`, `return`, and assignment statements are no longer moved to a separate line, which previously broke lint suppression.

## [0.2.8] - 2026-02-15

### Added
- **`gd env`** — show environment info (gd version, Godot version/path, OS, arch, WSL, project root, config path). Supports `--json`.
- **Updated `gd llm`** — command reference now covers all 30 commands including debug, scene, log, env, and lint categories.

### Fixed
- **`gd debug eval` now works without a manual breakpoint** — previously always failed because the `break` command pauses the engine without entering the GDScript debug loop. Now automatically sets a temporary breakpoint on a `_process` function, evaluates, then cleans up. Assignments, node paths, builtins, and multi-expression all work.
- **Eval text output** — fixed raw JSON leaking into display for all types; Array and Dictionary values now render as `[1, true, null]` and `{key: value}` instead of nested variant JSON.

## [0.2.7] - 2026-02-14

### Added
- **`gd scene attach-script`** — attach a GDScript file to a node in a `.tscn` scene. Auto-increments `ext_resource` ID, updates `load_steps`, supports `--node <name>` and `--dry-run`.
- **`gd run --log`** — stream Godot's stdout/stderr to the terminal for debugging print output and error backtraces
- **`gd log`** — view game output from the last `gd run`. Supports `--tail N`, `--follow` (real-time), and `--clear`.
- **Always-capture game logs** — `gd run` now always writes Godot's stdout/stderr to `.godot/gd-game.log`, even without `--log`. When `--log` is set, output is tee'd to both the terminal and the log file.

## [0.2.6] - 2026-02-14

### Added
- **New lint rule: `enum-name-collision`** (75 → 76 total) — detects when an inner enum name collides with a global `class_name` in the project, which causes Godot type resolution errors

### Fixed
- `gd check` now detects `:=` Variant inference from polymorphic builtins (`max`, `min`, `clamp`, `snapped`, `wrap`)
- Type inference engine now correctly returns Variant for polymorphic builtins (typed variants like `maxi`/`maxf` still return their specific types)

## [0.2.5] - 2026-02-14

### Added
- **Lint categories** — 8 categories for bulk rule control: `correctness`, `suspicious`, `style`, `complexity`, `performance`, `godot`, `type_safety`, `maintenance`. Each of the 76 rules belongs to exactly one category.
- **Category config in `[lint]`** — set `type_safety = "warning"` to enable all type-safety rules (including opt-in), `maintenance = "off"` to disable all maintenance rules, etc.
- **5-level severity resolution** — disabled_rules > per-rule severity > per-rule config > category level > rule default

### Changed
- `gd init` template rewritten to be minimal with category examples
- README lint section reorganized by category with bulk-control documentation

## [0.2.4] - 2026-02-14

### Added
- **Cross-file resolution engine** (`src/core/workspace_index.rs`) — Layer 3: project-wide symbol index that maps `class_name` declarations to their symbols, parses `project.godot` autoloads, and resolves `preload()` targets. Built once at lint time, shared read-only across parallel file linting.
- **3 new lint rules** (72 → 75 total):
  - `shadowed-variable-base-class` (opt-in) — local variable shadows a member of a user-defined base class
  - `static-called-on-instance` (default-on) — static method called on `self` or typed instance instead of the class
  - `missing-tool` (opt-in) — base class has `@tool` but this script does not
- **Enhanced existing rules with cross-file awareness:**
  - `return-value-discarded` — now detects user-defined non-void functions across files
  - `unsafe-void-return` — now detects user-defined void functions across files
  - `native-method-override` — now checks user-defined base class methods, not just ClassDB
- **Project-aware type inference** — `infer_expression_type_with_project()` resolves method return types from user-defined base classes before falling back to ClassDB

## [0.2.3] - 2026-02-14

### Changed
- **Remove image processing dependencies** — removed `base64`, `png`, and `jpeg-encoder` crates. Screenshot commands now return PNG file paths instead of base64-encoded data. Reduces binary size by ~240 KB.

## [0.2.2] - 2026-02-14

### Added
- **Expression type inference engine** (`src/core/type_inference.rs`) — Layer 2: infer types of any GDScript expression. Handles literals, constructors, builtin functions, self/ClassDB method calls (walks inheritance chain), operators, comparisons, casts, ternary, identifiers, subscript, and `$`/`get_node`.
- **ClassDB `method_return_type()`** — resolve return types for 16,346 engine methods with inheritance chain walking.
- **7 new lint rules** (65 → 72 total):
  - `narrowing-conversion` (opt-in, fixable) — float expression assigned to int-typed variable
  - `unsafe-void-return` (opt-in, fixable) — returning or assigning a void function call result
  - `return-value-discarded` (opt-in) — non-void call result unused as expression statement
  - `incompatible-ternary` (opt-in) — ternary branches have incompatible types
  - `standalone-ternary` (default-on) — ternary expression used as statement with result unused
  - `assert-always-true` (opt-in, fixable) — `assert(true)`, `assert(1)`, `assert("string")`
  - `assert-always-false` (opt-in, fixable) — `assert(false)`, `assert(0)`, `assert(null)`
- 4 new auto-fixes (13 → 17 fixable rules): `narrowing-conversion` wraps with `int()`, `unsafe-void-return` splits return/removes var, `assert-always-true/false` deletes assertion line
- Refactored `static-type-inference`, `variant-inference`, and `untyped-array-literal` to use centralized inference engine (same diagnostics, consolidated implementation)

## [0.2.1] - 2026-02-14

### Added
- **Symbol table** (`src/core/symbol_table.rs`) — per-file declaration-level type tracking built from tree-sitter ASTs. Extracts variables, functions, signals, enums, constants, annotations (`@tool`, `@onready`, `@export`, etc.), `class_name`, `extends`, inner classes, and type annotations.
- **6 new symbol-table-aware lint rules** (all opt-in):
  - `onready-with-export` (error) — `@onready` and `@export` on the same variable conflict at runtime
  - `enum-variable-without-default` (warning) — enum-typed variable without default will be `0`, not the first enum member
  - `redundant-static-unload` (warning) — `@static_unload` annotation with no static variables to unload
  - `get-node-default-without-onready` (error) — `$`/`get_node()` in variable default without `@onready`
  - `unused-private-class-variable` (warning) — `_`-prefixed variable declared but never referenced
  - `native-method-override` (error) — function name shadows a method inherited from an engine class
- Lint pipeline now builds symbol table once per file and passes it to all rules via `check_with_symbols()`

## [0.2.0] - 2026-02-14

### Breaking
- **Debug API grouping** — flat `gd debug <cmd>` commands reorganized into nested subcommand groups:
  - `gd debug scene tree` (was `scene-tree`), `gd debug scene inspect` (was `inspect`), `gd debug scene inspect-objects`, `gd debug scene camera-view`
  - `gd debug camera screenshot` (was `screenshot`), `gd debug camera override` (was `override-camera`), `gd debug camera transform-2d`, `gd debug camera transform-3d`
  - `gd debug live set-root` (was `live-set-root`), `gd debug live create-node`, etc.
  - `gd debug select type` (was `node-select-type`), `gd debug select clear` (was `clear-selection`), etc.
  - Top-level commands (`stop`, `continue`, `next`, `step-in`, `step-out`, `breakpoint`, `stack`, `vars`, `eval`, `set-prop`, `suspend`, etc.) remain unchanged

### Added
- **`enum-without-class-name` lint rule** (opt-in) — warns when a script defines a named enum but has no `class_name`, and a type annotation references that enum. Godot qualifies such enums as `filename.gd.EnumName`, so bare `EnumName` annotations fail to resolve.
- **`variant-inference` now detects compound expressions** — catches `:=` on binary/comparison operators (`dict["key"] == "switch"`), parenthesized expressions (`(dict["key"])`), and unary operators (`not dict["key"]`) that contain Variant-producing sub-expressions. Previously only direct subscript and method calls were detected.
- `gd check` mirrors the same Variant detection improvements (binary, parenthesized, unary)

## [0.1.31] - 2026-02-14

### Changed
- **Enforce `clippy::pedantic`** — deny-level pedantic lints across the entire codebase. Fixed 460+ violations (redundant closures, manual `is_empty`, needless borrows, `match` vs `if let`, etc.).
- **Remove DAP client** — stripped `dap_client.rs` and all DAP-specific code paths. The binary debug protocol (`godot_debug_server`) is the sole debug transport.
- **Split 7 god files into 43 submodules** — `debug_cmd` (10), `daemon` (6), `variant` (4), `test_cmd` (5), `godot_debug_server` (7), `printer` (6), `query` (5). No public API changes; all items re-exported from `mod.rs`. Every file now under 750 lines.

## [0.1.30] - 2026-02-14

### Added
- **`gd debug camera-view`** — show active camera info and all spatial node transforms in the running game. Detects cameras by engine class, script path, or node name (case-insensitive). Script classes (`res://...`) are included as spatial candidates and filtered by actual transform properties.
- **`gd debug screenshot`** — capture game viewport as JPEG (base64 via debug protocol, PNG→JPEG conversion). Supports `--output <file>` to save to disk.
- **`gd debug set-var`** — modify local variables at breakpoints (name + value + stack frame)
- **`gd debug inspect --rich`** — enrich output with ClassDB docs: class descriptions, property documentation, Godot docs URLs. Walks the full inheritance chain.
- **Property enrichment** — `inspect --rich` now resolves enum values to names, adds range metadata, and annotates type/resource hints from ClassDB

### Fixed
- **Debug server mutex deadlock** — all daemon dispatch functions now clone an `Arc<GodotDebugServer>` and release the daemon mutex before executing. Previously, long-running operations (batch inspect, accept) held the mutex for 10-30s, blocking all other debug queries and causing cascading timeouts.
- **Batch inspect reliability** — `cmd_inspect_objects` now issues individual inspect commands per object instead of a single batch send. Prevents one missing/freed object from breaking the entire batch.
- **`set-prop` with Vector3 values** — fixed `json_to_variant` catch-all that silently converted arrays/objects to Nil. Now maps JSON arrays by element count to Vector2/3/4/Transform/Basis/Projection, and JSON objects support typed wrappers like `{"Vector3": [1,2,3]}`.
- **`set-prop-field` sub-field assignment** — rewrote to use client-side inspect→modify→set instead of Godot's broken `fieldwise_assign` (which casts scalar values to the property type, zeroing sub-fields).
- **`eval` return values** — fixed 3-field vs 4-field parsing of Godot's `evaluation_return` protocol (was returning variant type ID instead of actual value).
- **Daemon kill race condition** — `kill_daemon` now polls for process exit (up to 2s) instead of a fixed 200ms sleep, preventing port conflicts on restart.
- **`accept()` interruptible** — debug server accept loop checks the `running` flag so server replacement interrupts pending accepts within ~50ms.
- **Screenshot size** — PNG→JPEG conversion (quality 80) via `png` + `jpeg-encoder` crates instead of raw PNG base64.

## [0.1.29] - 2026-02-14

### Added
- **`gd llm`** — AI-readable command reference (like llms.txt for websites). Prints the full command tree in a compressed format for LLM context windows — 204 lines covering every command, flag, and pattern.
- **`gd debug inspect --rich`** — enrich inspect output with ClassDB documentation (class descriptions, property docs, Godot docs URLs). Walks the inheritance chain (e.g. CharacterBody3D → Node3D → Node → Object).
- `src/debug/enrich.rs` — loosely coupled enrichment module (JSON in → JSON out, easy to remove)

## [0.1.28] - 2026-02-14

### Added
- **`gd stop`** — top-level command to terminate the running game (mirrors `gd run`)
  - `gd debug stop` also works as an alias
  - WSL-aware: uses PowerShell + taskkill.exe to find and kill the Windows Godot process
  - Game PID persisted in daemon state file — `gd stop` works even if daemon has died
- **`gd debug inspect --brief`** — stripped-down output for AI: just `{name: value}` pairs, no Godot internals (hint, hint_string, type_id, usage)
- **`gd debug breakpoint --name <func>`** — resolve function name to file:line automatically (searches project .gd files)
- **`gd debug breakpoint --condition <expr>`** — condition metadata stored with breakpoint (server-side enforcement coming later)
- **`gd debug next` aliased as `gd debug step-over`** — symmetry with `step-in`/`step-out`

### Changed
- `gd debug suspend --resume` → `gd debug suspend --off` — consistent with `mute-audio --off`, `skip-breakpoints --off`, etc.
- `gd debug inspect` strips `Members/` and `Constants/` prefixes from property names — property names now match what `set-prop` expects
- `gd run` now always wires `--remote-debug` silently (no user-facing port args) — enables `gd debug` without manual setup
- `gd run` output cleaned up — single status line instead of raw JSON dump

### Fixed
- `gd daemon status` now correctly shows `game_running: true` when a game is connected via binary debug protocol (was only set for DAP launches)
- `gd stop` / `gd debug stop` clears the `game_running` flag in daemon state

## [0.1.27] - 2026-02-14

### Added
- **`gd daemon`** — top-level command for daemon lifecycle management
  - `gd daemon status` — show daemon connectivity and state
  - `gd daemon stop` — stop the background daemon
  - `gd daemon restart` — restart the daemon (stop + auto-spawn on next query)
  - Moved from `gd lsp daemon-*` namespace since the daemon is no longer LSP-specific
- **WSL path translation** — `gd run` and `gd build` now work transparently from WSL with a Windows Godot binary
  - `find_godot()` resolves Windows paths in `gd.toml`/`GODOT_PATH` on WSL (auto-converts `C:\...` to `/mnt/c/...` for existence checks)
  - `--path` argument auto-converted from `/mnt/c/...` to `C:/...` when spawning a `.exe` binary
  - Daemon cache lookup: discovers Godot path from previous DAP launches
  - Clear error message on WSL when no Windows Godot binary is configured
- **Daemon auto-restart** — build_id fingerprinting (version + binary mtime) auto-kills stale daemons after recompile
- **Game exit detection** — daemon auto-clears `game_running` flag when game process exits (no more stale state)

### Changed
- `gd run` simplified — non-blocking spawn (returns immediately instead of waiting for Godot to exit), removed DAP launch logic
- WSL path utilities consolidated into `src/core/fs.rs` (removed duplicates from run_cmd, daemon, debug_cmd)

## [0.1.26] - 2026-02-13

### Added
- **Godot binary debug protocol** — full implementation of Godot's native debug protocol (port 6007), enabling direct game introspection beyond what DAP supports
  - `src/debug/variant.rs` — binary codec for all 39 Godot Variant types (Nil through PackedVector4Array)
  - `src/debug/godot_debug_server.rs` — TCP server speaking the binary protocol (length-prefixed Variant-encoded packets)
- `gd debug scene-tree` — show the running game's live scene tree (node names, classes, object IDs)
- `gd debug inspect --id <N>` — inspect a scene node's properties by object ID
- `gd debug set-prop --id <N> --property <name> --value <val>` — set a property on a scene node at runtime
- `gd debug suspend [--off]` — freeze/resume the game loop
- `gd debug next-frame` — advance one physics frame while suspended
- `gd debug time-scale --scale <N>` — set Engine.time_scale (slow-mo, fast-forward)
- `gd debug reload-scripts` — hot-reload all GDScript files in the running game
- Interactive REPL commands: `scene-tree`/`tree`, `inspect`/`i`, `set-prop`, `suspend`, `resume`, `next-frame`/`nf`, `timescale`, `reload`
- All new commands support `--format json` for scripting

## [0.1.25] - 2026-02-13

### Added
- `gd debug step-out` — step out of the current function (synthetic: repeats `next` until stack depth decreases, same technique as the official VS Code plugin)
- `gd debug --format json` on all stepping commands (`continue`, `next`, `step`, `step-out`, `pause`) — JSON output includes stack frames and full variable scopes
- `gd debug set-var --format json` — returns `{name, value, type, input}` for automation
- Client-side conditional breakpoints — `gd debug break --condition <expr>` now evaluates the expression on each hit and auto-continues when false (Godot's DAP ignores conditions natively)
- `--name` with `--file` scoping for ambiguous function names — errors with candidate list when multiple files define the same function
- Type inference for `set-var` — populates type field from value when Godot returns empty type (int, float, bool, String, constructors)

### Fixed
- `--name` now resolves to the first executable statement inside the function body (not the `func` declaration line, which Godot won't break on)
- Condition evaluator correctly parses boolean results (`"false"`, `"0"`, `"null"` → falsy; everything else → truthy)
- `==`, `!=`, `>=`, `<=` in eval expressions no longer trigger false assignment warnings
- `set-var` on local variables gives clear error: "Godot's DAP does not support setting locals"
- `set-var` and `eval` output JSON errors to stderr when `--format json` is active
- Daemon DAP recovery: failed operations set `dap_needs_reconnect` flag, next query auto-reconnects
- Daemon disconnect uses TCP shutdown to prevent stream corruption after failed operations
- String values in `set-var` correctly auto-quoted (bare words like `bike` become `"bike"`)

## [0.1.24] - 2026-02-13

### Added
- `gd debug set-var --name <var> --value <val>` — modify variable values while paused at a breakpoint

## [0.1.23] - 2026-02-13

### Added
- **Background daemon** — persistent process maintains Godot LSP and DAP connections, auto-starts on first CLI query, auto-exits after 5 min idle
  - `gd lsp daemon-status` — show daemon connectivity (Godot LSP, DAP, game state)
  - All hover, completion, and definition queries now route through the daemon for instant results
- `gd run` now launches the game via DAP when the Godot editor is open (returns immediately, falls back to direct spawn)
- `gd debug` — runtime debugging via Godot's Debug Adapter Protocol (DAP)
  - `gd debug attach` — interactive REPL session
    - `break`, `clear`, `wait` — set/clear breakpoints and wait for hits
    - `continue`, `pause`, `next`, `step` — execution control
    - `stack`, `vars`, `expand` — inspect call stack and variables
    - `eval` — evaluate expressions at breakpoints (member-access only)
  - `gd debug break --file <path> --line <N>` — one-shot: set breakpoint, wait for hit, dump stack + variables (`--format json`)
  - `gd debug break --name <func>` — break on function entry by name (resolves to file:line automatically)
  - `gd debug break --condition <expr>` — conditional breakpoints (only trigger when expression is true)
  - `gd debug eval --expr <expr>` — evaluate expression while paused at a breakpoint (`--format json`)
  - `gd debug continue/next/step/pause` — non-interactive execution control
  - `gd debug status` — show DAP connection and threads (`--format json`)
  - `gd debug stop` — terminate the running game
  - Cross-platform path resolution (WSL `/mnt/c/` to `C:/` conversion)
- Static completions now include engine methods from class_db based on `extends` clause (e.g. `extends Node2D` adds `apply_scale`, `add_child`, etc.)

### Fixed
- Godot Dictionary errors from missing `name`/`checksums` fields in DAP source objects
- Godot Dictionary errors from missing `context.triggerKind` in LSP completion requests

## [0.1.22] - 2026-02-13

### Added
- `.tscn`/`.tres` scene and resource file support:
  - `gd check` validates scene files — detects broken `res://` paths, orphaned `ext_resource` declarations, and parse errors
  - `gd deps --include-resources` includes `.tscn`/`.tres` files in the dependency graph
  - `gd tree --scene <file.tscn>` displays scene node hierarchy (text tree or `--format json`)
  - `gd tree --scene <directory>` lists all scenes in a directory
  - `gd lsp scene-info --file <scene.tscn>` returns structured JSON (nodes, ext_resources, connections)
  - `gd lsp scene-info --nodes-only` for compact node-only output
- Godot LSP proxy — forwards hover, completion, and go-to-definition to Godot's built-in LSP server (port 6005) when the editor is running
  - `gd lsp --godot-port <PORT>` to configure proxy port (default: 6005)
  - `gd lsp --no-godot-proxy` to disable proxy
  - Results are merged with static analysis (engine docs + local symbols)

### Changed
- Integration tests split into 8 domain-specific files (check, commands, deps, fmt, lint, lsp_query, lsp_refactor, scene)

## [0.1.21] - 2026-02-13

### Changed
- `gd lsp view --format json` now returns a single `content` string instead of a per-line object array (~3x fewer tokens)
- `gd lsp references` output now includes a `context` field with the trimmed source line for each reference

### Added
- `gd lsp view --range 5-20` shorthand for `--start-line 5 --end-line 20`
- `gd lsp edit-range --range 5-20` shorthand for `--start-line 5 --end-line 20`
- `gd lsp create-file --input-file` reads initial file content from a file (or stdin when piped) instead of generating boilerplate

## [0.1.20] - 2026-02-13

### Fixed
- `gd fmt` preserves multiline parenthesized expressions with comments (no longer collapses `# comment` into subsequent code)
- `gd fmt` places commas correctly before trailing comments in multiline arrays and dictionaries
- `gd fmt` properly handles line continuation (`\`) in arrays, dictionaries, binary operators, assignments, function parameters, and method chains

## [0.1.19] - 2026-02-13

### Fixed
- `gd check` no longer rejects `#region`/`#endregion` fold markers at the top level
- `gd check` no longer flags Godot enum type names (e.g. `Viewport.MSAA`) as unknown constants
- `gd fmt` preserves multiline arrays and dictionaries instead of collapsing them into single lines
- `gd fmt` correctly handles inline comments in multiline collections (trailing `# comment` no longer breaks formatting)

## [0.1.18] - 2026-02-13

### Fixed
- `untyped-array-literal` auto-fix now infers `Array[Color]`, `Array[Vector2]`, etc. from homogeneous class member/constructor expressions (not just String/int/float/bool)
- `cyclomatic-complexity` no longer penalizes guard clause patterns — `if not (A and B and C): return` no longer counts `and`/`or` in the condition

## [0.1.17] - 2026-02-13

### Added
- `use-before-assign` lint rule (opt-in) — detects method calls where the callee accesses a member variable not yet assigned at the call site
- `untyped-array-literal` is now auto-fixable — infers `Array[String]`, `Array[int]`, `Array[float]`, `Array[bool]` from homogeneous literals
- `gd lsp view` now outputs human-readable text by default (cat -n style); use `--format json` for structured output

## [0.1.16] - 2026-02-13

### Fixed
- Regenerate class database from full Godot 4.6 `extension_api.json` — 213 → 1024 classes, 5380 enum members, 16346 methods
- `gd check` no longer flags valid constants like `Mesh.PRIMITIVE_TRIANGLES`, `BaseMaterial3D.SHADING_MODE_UNSHADED`, `BoxContainer.ALIGNMENT_CENTER`, `SubViewport.UPDATE_ALWAYS`
- `constant_exists` / `enum_member_exists` / `suggest_constant` now walk the class inheritance chain

## [0.1.14] - 2026-02-13

### Added
- Godot 4.6 class database — bundled static lookup for classes, methods, enums, and constants with Levenshtein suggestions
- `gd check` semantic validation:
  - Validate `ClassName.CONSTANT` references against the Godot class database (with typo suggestions)
  - Detect `:=` resolving to `Variant` from dictionary/array subscript and `.get()`/`.values()`/`.keys()` calls
- `gd lsp create-file` — scaffold new GDScript files with `--extends`, `--class-name`, and `--dry-run`
- `gd lsp rename --name` — rename symbols by name across the project (no position needed)
- 4 new lint rules (56 total):
  - `untyped-array-literal` (default) — warns on `var x := [...]` without typed `Array[T]` annotation, infers element type from homogeneous literals
  - `variant-inference` (opt-in) — warns on `:=` inferring Variant from dict/array access patterns
  - `look-at-before-tree` (opt-in) — detects tree-dependent method calls (`look_at`, `to_global`, etc.) before `add_child()`
  - `null-after-await` (opt-in) — warns on member variable access in `_process`/`_physics_process` without null guard after `await` assignment

- `monitoring-in-signal` lint rule (default on) — detects direct `monitoring`/`monitorable` assignment in Area2D/Area3D signal callbacks, suggests `set_deferred()`
- `gd lsp view` — read lines from a GDScript file with optional `--start-line`, `--end-line`, and `--context` (JSON output for AI tools)

### Fixed
- `gd lsp edit-range` on empty files no longer errors — correctly handles insert into empty/newline-only files
- `gd lsp change-signature` no longer inserts C-style `/* */` comments at call sites — uses default value or `null` placeholder with a warning

## [0.1.13] - 2026-02-13

### Added
- gdUnit4 test framework support in `gd test` — auto-detects `addons/gdUnit4/`, invokes GdUnitCmdTool, parses JUnit XML results
  - Three-way detection priority: GUT → gdUnit4 → raw script
  - Per-test results with pass/fail status, failure messages, and timing
  - Auto-passes `--ignoreHeadlessMode` for gdUnit4 v6+ compatibility
  - Cleans up temp report directory after parsing
  - `--format json` outputs `"mode": "gdunit4"` with structured results
  - `skipped` field in test summary (only present when non-zero)

## [0.1.12] - 2026-02-12

### Added
- `gd check` structural validation — catches patterns tree-sitter accepts but Godot rejects
  - Top-level statements: flags expressions, loops, if/match at module root (only declarations allowed)
  - Indentation consistency: detects orphaned indented blocks in body nodes (e.g. code left after removing `else:`)
- Hover on standalone inherited members (`velocity`, `move_and_slide`) without `self.` prefix now shows builtin docs
- `gd lsp symbols --kind field` / `--kind property` now matches both `variable` and `field` symbol kinds

### Fixed
- VS Code extension now bundles dependencies with esbuild — fixes broken 0.1.2 vsix that was missing `vscode-languageclient`

### Changed
- VS Code extension version bumped to 0.1.3 (esbuild bundling)

## [0.1.11] - 2026-02-12

### Added
- Hover on member access (`foo.global_position`) shows builtin Godot member documentation with class, type, description, and docs link
- Hover on `self.member` resolves to the same-file declaration
- ~170 builtin member entries: Object, Node, CanvasItem, Node2D, Node3D, Control, CharacterBody2D/3D, RigidBody2D/3D, Sprite2D, Timer, AnimationPlayer, Tween, Vector2, Vector3, String, Array, Dictionary
- `--input-file <path>` flag for `insert`, `replace-body`, `replace-symbol`, `edit-range` — read content from a file instead of stdin (fixes Windows pipe encoding issues with tabs)

### Fixed
- Hover on unresolvable identifiers inside a function no longer falls through to show the enclosing function signature
- Hover on declaration keywords (`func`, `var`, `const`, `signal`, `class`, `enum`) only triggers when cursor is on the name, not on body contents

## [0.1.10] - 2026-02-12

### Added
- `gd lint --context N` — show N surrounding lines per diagnostic (text + JSON output)
- `gd lsp replace-body` — AST-aware function body replacement (reads from stdin)
- `gd lsp insert` — insert code before/after a named symbol (reads from stdin)
- `gd lsp replace-symbol` — replace entire symbol declaration (reads from stdin)
- `gd lsp edit-range` — line-range replacement fallback (reads from stdin)
- All edit commands: `--no-format` to skip auto-formatting, `--class` for inner classes, `--dry-run` to preview

## [0.1.9] - 2026-02-12

### Added
- 6 new lint rules: `parameter-shadows-field`, `god-object` (opt-in), `duplicate-delegate` (opt-in), `signal-not-connected` (opt-in), `callable-null-check`, `breakpoint-statement` (opt-in)
- `redundant-else` lint rule is now auto-fixable (removes else, dedents body)
- `todo-comment` now detects `BUG`, `DEPRECATED`, `WARNING` markers (matching Godot editor)
- Event bus heuristic for `unused-signal` — files with no functions suppress warnings
- `gd init` detects build output directory from Godot's `export_presets.cfg`
- `gd.toml` template now includes all config options with defaults (commented out)
- VS Code extension activates on workspace load (not just file open)

### Fixed
- `severity = "off"` now correctly disables default-enabled lint rules
- `self-assignment` fix now prepends `self.` instead of deleting the line
- `parameter-shadows-field` suppressed when body uses `self.<param>` (intentional DI pattern)
- `empty-function` no longer false-positives on `@abstract` methods
- `unused-parameter` skips variadic rest parameters (`...args`)
- `gd lsp references --class` now matches autoload class names

## [0.1.8] - 2026-02-12

### Added
- `gd lsp bulk-delete-symbol` - Delete multiple symbols in one pass
- `gd lsp bulk-rename` - Rename multiple symbols atomically
- `gd lsp inline-delegate` - Detect and inline pure pass-through delegate functions
- `gd lsp extract-class` - Extract symbols from one file to another
- `gd lsp move-symbol --update-callers` - Update preload paths in callers after moving
- `gd check --format json` - Structured JSON output for parse error results

### Fixed
- Windows path separators in `gd stats --by-dir` output

## [0.1.7] - 2026-02-12

### Added
- `gd addons update` - Check for and apply addon updates from the Asset Library
- `gd addons lock` / `gd addons install --locked` - Lock file for reproducible addon versions
- `gd addons install --godot-version` - Version compatibility warnings
- `gd stats --diff <branch>` - Compare project statistics between branches
- `gd stats --top N` - Show top-N longest functions (complexity hotspots)
- `gd lsp introduce-variable` - Extract expression into a local variable
- `gd lsp introduce-parameter` - Turn hardcoded value into a function parameter with default
- `gd lsp inline-method --name` - Inline by function name instead of position
- `gd lsp change-signature --rename-param` - Rename parameters in function signatures
- `gd doc --format json` - JSON output for generated documentation
- `gd doc --check` - CI mode that exits 1 if public methods lack doc comments
- `gd test --format json` - Structured JSON output with error locations
- CI download URL now auto-detects from `Cargo.toml` repository field

### Fixed
- `gd stats --diff` tests failing on macOS CI (git default branch name)

## [0.1.6] - 2026-02-12

### Added
- `gd lsp inline-method` - Inline function body at call sites with parameter substitution
- `gd lsp change-signature` - Add, remove, reorder, and rename function parameters
- `gd lsp delete-symbol --class Inner` - Delete members from inner classes
- `gd lsp delete-symbol --name EnumName.MEMBER` - Delete individual enum members
- `gd lsp move-symbol --class` - Move symbols between inner classes

## [0.1.5] - 2026-02-12

### Added
- Scope-aware LSP rename, references, and go-to-definition (local variables restricted to enclosing function)
- `gd lsp symbols --kind` - Filter symbols by type (repeatable, comma-separated)
- Lint overrides per path pattern in `gd.toml` (`[[lint.overrides]]`)
- Repeatable `--rule` flag for `gd lint` to run specific rules
- `pending()` calls now suppress lint warnings in test functions

## [0.1.4] - 2026-02-12

### Added
- One-shot LSP CLI queries: `rename`, `references`, `definition`, `hover`, `completions`, `code-actions`, `diagnostics`, `symbols`
- `gd lsp delete-symbol` - Delete symbols with reference checking
- `gd lsp move-symbol` - Move symbols between files with preload detection
- `gd lsp extract-method` - Extract code into new functions with variable capture
- Built-in Godot type and function documentation for LSP hover and completion
- `info` lint severity level
- Cross-platform path handling with `path-slash`

### Fixed
- `ignore_patterns` not working on Windows
- `unnecessary-pass` false positive with comment-only bodies

## [0.1.3] - 2026-02-11

### Added
- `gd new --from user/repo` - Create projects from GitHub templates
- VS Code extension improvements: format-on-save, status bar, restart command

### Changed
- CI workflow split into parallel lint and test jobs

## [0.1.2] - 2026-02-11

### Added
- 19 new lint rules (total: 46), including naming conventions, complexity checks, and Godot-specific patterns
- Formatter safety check: re-parse and idempotency verification after formatting
- Godot version detection for `gd new` project templates

### Fixed
- 4 formatter bugs found via stress testing against 1,247 real-world files
- 4 broken lint rules (unreachable-code, missing-return, empty-function, unused-signal)
- `gd addons` search version filter, zip extraction, git install, collision checks
- Test runner, clean defaults, CI version detection

## [0.1.1] - 2026-02-11

### Added
- `--fix` support for `unused-variable` and `unreachable-code` lint rules
- Project-wide LSP diagnostics on workspace open

### Fixed
- `empty-function` false positive on zero-param virtual stubs
- `unused-signal` not detecting `.emit()`/`.connect()`/`.disconnect()`
- False positives in `unreachable-code`, `missing-return`, `empty-function`
- `ignore_patterns` not working on Windows
- Config discovery to search from file paths, not just cwd
- LSP not respecting `gd.toml` config
- `magic-number` rule now opt-in by default

## [0.1.0] - 2026-02-11

### Added
- `gd new` - Create new Godot projects (templates: `default`, `2d`, `3d`)
- `gd init` - Initialize gd in existing projects
- `gd fmt` - Format GDScript files (`--check`, `--diff`)
- `gd lint` - Lint GDScript with 25 built-in rules (`--fix`, `--format json/sarif/text`)
- `gd run` - Run Godot project
- `gd build` - Export Godot project
- `gd check` - Validate project without building
- `gd clean` - Remove build artifacts
- `gd test` - Run GDScript tests (GUT and raw script)
- `gd completions` - Shell completion generation
- `gd tree` - Show class hierarchy
- `gd doc` - Generate documentation from `##` doc comments
- `gd watch` - Watch and auto-format/lint on changes
- `gd addons` - Manage addons from Asset Library and Git
- `gd stats` - Project statistics
- `gd ci` - Generate CI/CD configs (GitHub Actions, GitLab CI)
- `gd lsp` - Language Server Protocol server
- `gd deps` - Script dependency graph
- `gd man` - Generate man page
- `gd upgrade` - Self-update from GitHub Releases
- LSP with 9 capabilities: diagnostics, formatting, code actions, document symbols, hover, go-to-definition, find references, rename, completion
- Cross-file LSP support via workspace indexing
- VS Code extension
- Per-rule lint configuration in `gd.toml`
- Inline lint suppression (`# gd:ignore`, `# gd:ignore-next-line`, `# gd:ignore[rule]`)
- SARIF output for GitHub Code Scanning
