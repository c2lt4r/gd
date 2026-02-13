# Changelog

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
- `gd lint --context N` — show N surrounding lines per diagnostic (human + JSON output)
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
- `gd lint` - Lint GDScript with 25 built-in rules (`--fix`, `--format json/sarif/human`)
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
