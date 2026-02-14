# Changelog

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
  - `gd tree --scene <file.tscn>` displays scene node hierarchy (human tree or `--format json`)
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
