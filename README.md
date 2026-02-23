# gd

**The Godot toolchain.** A fast, all-in-one CLI for formatting, linting, building, and managing Godot projects — like `cargo` for GDScript.

Built with [tree-sitter-gdscript](https://github.com/PrestonKnopp/tree-sitter-gdscript) and [tree-sitter-godot-resource](https://github.com/PrestonKnopp/tree-sitter-godot-resource) for accurate parsing, and [Rayon](https://github.com/rayon-rs/rayon) for parallel file processing.

## Features

- **Format** GDScript files with an AST-based formatter aligned to the [GDScript style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html)
- **Lint** with 84 built-in rules (17 auto-fixable), SARIF output for CI
- **Run**, **build**, **test**, and **clean** your Godot project from the terminal
- **Watch** for file changes and auto-lint/format on save
- **Manage addons** from Git or the Godot Asset Library (with lockfile and update support)
- **Generate CI/CD** configurations for GitHub Actions and GitLab CI
- **LSP server** with formatting, diagnostics, hover, go-to-definition, references, rename, completion, inlay hints, signature help, call hierarchy, find implementations, semantic tokens, workspace symbol search, scene-aware cross-referencing, and 16 refactoring commands
- **Scene management** &mdash; create scenes, add/remove/duplicate nodes, instance scenes, add sub-resources, batch-add nodes, set properties, wire connections, attach/detach scripts &mdash; plus validate `.tscn`/`.tres` files and visualize scene hierarchies
- **Debug** a running Godot game via Godot's binary debug protocol &mdash; breakpoints, stepping, variable inspection, expression evaluation, live scene tree, node inspection, game speed control, and hot-reload
- **Godot LSP proxy** &mdash; forward hover, completion, and go-to-definition to Godot's built-in LSP when the editor is running
- **3D mesh editing** *(experimental)* &mdash; procedural mesh construction from Rust-native primitives, 2D profiles, and transforms with watertight boolean operations, quad-dominant topology, multi-part assembly, materials, and batch execution
- **Analyze** your project with dependency graphs, class trees, and code statistics

## Installation

### From source

```sh
git clone https://github.com/c2lt4r/gd.git
cd gd
cargo install --path .
```

## Quick Start

```sh
# Create a new Godot project
gd new my-game

# Or create from a GitHub template
gd new my-game --from user/godot-template

cd my-game

# Format all GDScript files
gd fmt

# Lint for issues
gd lint

# Run the project
gd run
```

## Commands

| Command | Description |
|---------|-------------|
| `gd new <name>` | Create a new Godot project (templates: `default`, `2d`, `3d`, or `--from` GitHub) |
| `gd init` | Initialize gd toolchain in an existing project (detects export paths) |
| `gd fmt` | Format GDScript files |
| `gd lint` | Lint GDScript files |
| `gd run` | Run the Godot project (non-blocking, eval server enabled by default, `--bare` to disable eval, `--file-ipc` for file-based transport) |
| `gd stop` | Stop the running Godot game |
| `gd log` | View game output log via debug protocol ring buffer (`--tail N`, `--follow`, `--errors`, `--grep`, `--json`, `--clear`) |
| `gd build` | Build/export the Godot project |
| `gd check` | Check project for errors (parse, structural, semantic, `.tscn`/`.tres` validation) (`--format json`) |
| `gd clean` | Clean build artifacts |
| `gd eval` | Evaluate GDScript expressions or scripts — live against a running game or offline headless |
| `gd test` | Run GDScript tests with GUT, gdUnit4, or raw scripts (`--runner`, `--format json`) |
| `gd completions` | Generate shell completions (bash, zsh, fish, etc.) |
| `gd tree` | Show project class hierarchy (`--scene` for scene node trees) |
| `gd doc` | Generate documentation from doc comments (`--format json`, `--check`) |
| `gd watch` | Watch files and run fmt/lint on changes |
| `gd addons` | Manage project addons (install, remove, search, update, lock) |
| `gd stats` | Show project statistics (`--diff <branch>`, `--by-dir`, `--top N`) |
| `gd ci` | Generate CI/CD pipeline configuration |
| `gd daemon` | Manage the background daemon (status, stop, restart) |
| `gd debug` | Debug a running Godot game (breakpoints, stepping, eval, scene tree, inspect, time control) |
| `gd resource` | Manage `.tres` resource files (create, properties, scripts, info) |
| `gd scene` | Manage `.tscn` scene files (create, add/remove nodes, properties, connections, scripts) |
| `gd lsp` | Start the LSP server, or run one-shot queries (see below) |
| `gd deps` | Show script dependency graph (`--include-resources` for `.tscn`/`.tres`) |
| `gd env` | Show environment info (gd version, Godot version/path, OS, project root) |
| `gd man` | Generate man page |
| `gd upgrade` | Self-update to latest release |
| `gd mesh` | *(experimental)* Procedural 3D mesh editing (46 subcommands: profiles, extrude, revolve, boolean, inset, solidify, bevel, array, multi-part, materials, shading, batch, replay) |
| `gd llm` | Print AI-readable command reference (like llms.txt) |

### Formatter

```sh
# Format all .gd files in the project
gd fmt

# Check formatting without modifying files (useful in CI)
gd fmt --check

# Show a diff of what would change
gd fmt --diff

# Format specific files or directories
gd fmt scripts/ player.gd
```

### Linter

```sh
# Lint the entire project
gd lint

# Auto-fix supported issues
gd lint --fix

# Output as JSON
gd lint --format json

# Output as SARIF (for GitHub Code Scanning)
gd lint --format sarif

# Lint specific files
gd lint scripts/player.gd

# Show surrounding code context (like grep -C)
gd lint --context 3
```

### Addons

```sh
# Search the Godot Asset Library
gd addons search "dialogue"

# Install from the Asset Library (by ID or name)
gd addons install 12345
gd addons install "some-addon"

# Install from a Git URL
gd addons install https://github.com/user/addon.git

# List installed addons
gd addons list

# Remove an addon
gd addons remove my-addon

# Check for updates
gd addons update

# Apply available updates
gd addons update --apply

# Generate a lock file for reproducible installs
gd addons lock

# Install all addons from the lock file
gd addons install --locked
```

### Watch Mode

```sh
# Watch and lint on changes (default)
gd watch

# Watch with auto-formatting
gd watch --fmt

# Watch with Godot error checking
gd watch --check

# Disable lint during watch
gd watch --no-lint
```

### Dependency Graph

```sh
# Show dependency tree
gd deps

# Include .tscn/.tres resource dependencies
gd deps --include-resources

# Output as Graphviz DOT
gd deps --format dot

# Output as JSON
gd deps --format json
```

### Scene Tree

```sh
# Show scene node hierarchy for a .tscn file
gd tree --scene main.tscn

# Show all scenes in a directory
gd tree --scene .

# JSON output
gd tree --scene main.tscn --format json
```

### Scene Management

```sh
# Create a new scene
gd scene create level.tscn --root-type Node2D
gd scene create main_menu.tscn --root-type Control --root-name MainMenu

# Add nodes
gd scene add-node level.tscn --name Player --type CharacterBody2D
gd scene add-node level.tscn --name Sprite --type Sprite2D --parent Player

# Set properties on nodes
gd scene set-property level.tscn --node Player --key visible --value false

# Attach and detach scripts
gd scene attach-script level.tscn player.gd --node Player
gd scene detach-script level.tscn --node Player

# Manage signal connections
gd scene add-connection level.tscn --signal ready --from Player --to . --method _on_ready
gd scene remove-connection level.tscn --signal ready --from Player --to . --method _on_ready

# Remove a node (cascades to children, cleans up connections and orphaned resources)
gd scene remove-node level.tscn --name Player

# Preview any command without writing
gd scene create level.tscn --root-type Node2D --dry-run
```

### Resource Management

```sh
# Create a new .tres resource
gd resource create item.tres --type Resource
gd resource create theme.tres --type Theme

# Create with a script attached
gd resource create item.tres --type Resource --script item_data.gd

# Set or update a property
gd resource set-property item.tres --key cost --value 100

# Read a property
gd resource get-property item.tres --key cost

# Remove a property
gd resource remove-property item.tres --key cost

# Attach or change a script
gd resource set-script item.tres item_data.gd

# Remove a script (cleans up ext_resource and load_steps)
gd resource remove-script item.tres

# Dump resource structure as JSON
gd resource info item.tres

# Preview any command without writing
gd resource set-property item.tres --key cost --value 100 --dry-run
```

### Statistics

```sh
# Show project statistics
gd stats

# JSON output
gd stats --format json

# Per-directory breakdown
gd stats --by-dir

# Top 5 longest functions (complexity hotspots)
gd stats --top 5

# Compare current branch vs main
gd stats --diff main
```

### CI/CD Generation

```sh
# Generate GitHub Actions workflow
gd ci github

# Generate GitLab CI configuration
gd ci gitlab

# Include export stage
gd ci github --export --godot-version 4.4
```

### Daemon

A background daemon maintains persistent Godot LSP and debug protocol connections, so CLI queries are instant. It auto-starts on the first query and auto-exits after 5 minutes of inactivity. Only one daemon can run per project (enforced via file lock).

```sh
# Show daemon connectivity and state (game_running, game_pid, debug_connected, etc.)
gd daemon status

# Stop the background daemon
gd daemon stop

# Restart the daemon
gd daemon restart
```

The daemon auto-restarts when it detects a newer `gd` binary (after recompile/upgrade). If the game crashes or exits without `gd stop`, the daemon detects the dead process within seconds and clears state automatically.

**WSL support:** On WSL, `gd run` and `gd build` auto-translate paths between Linux (`/mnt/c/...`) and Windows (`C:/...`) when using a Windows Godot binary. Set the path in `gd.toml`:

```toml
[run]
godot_path = "C:/path/to/godot.exe"
```

### Debugging

Debug a running Godot game via Godot's native binary debug protocol. The background daemon maintains persistent connections, so CLI queries are instant.

```sh
# Run the project (auto-wires debug connection)
gd run

# View game output log (ring buffer via debug protocol)
gd log
gd log --tail 20
gd log --follow
gd log --errors          # errors and warnings only
gd log --grep "Player"   # filter by pattern

# Stop the running game
gd stop

# Set breakpoint by file:line
gd debug breakpoint --path res://scripts/player.gd --line 42

# Break on function entry by name (resolves to file:line automatically)
gd debug breakpoint --name apply_input

# Conditional breakpoint
gd debug breakpoint --path res://scripts/player.gd --line 42 --condition "speed > 20.0"

# Execution control
gd debug continue
gd debug next        # step over (alias: step-over)
gd debug step-in     # step into
gd debug step-out    # step out of function
gd debug pause

# Stack and variables at breakpoint
gd debug stack
gd debug vars --frame 0

# Evaluate expression while paused
gd debug eval --expr "self.speed"

# Live scene tree
gd debug scene tree

# Inspect a node by object ID (from scene tree output)
gd debug scene inspect --id 456
gd debug scene inspect --id 456 --brief   # stripped-down output for AI
gd debug scene inspect --id 456 --rich    # enrich with ClassDB docs and metadata

# Set properties on a node at runtime
gd debug set-prop --id 456 --property speed --value 100.0
gd debug set-prop-field --id 456 --property position --field x --value 5.0
gd debug set-var --name speed --value 42 --frame 0  # modify local variable at breakpoint

# Camera and scene overview
gd debug scene camera-view        # active camera + all spatial node transforms
gd debug camera screenshot        # capture game viewport (JPEG)
gd debug camera screenshot --output frame.jpg

# Game loop control
gd debug suspend             # freeze the game loop
gd debug next-frame          # advance one frame while suspended
gd debug suspend --off       # resume the game loop
gd debug time-scale --scale 0.5   # slow-mo (0.5x speed)
gd debug time-scale --scale 2.0   # fast-forward (2x speed)

# Hot-reload scripts
gd debug reload-scripts

# Start an interactive debug session (REPL)
gd debug attach

# JSON output for scripting
gd debug breakpoint --path res://scripts/player.gd --line 42 --format json
gd debug next --format json
gd debug scene tree --format json
gd debug scene inspect --id 456 --format json
```

Interactive session commands (`gd debug attach`):

| Command | Description |
|---------|-------------|
| `break <file> <line>` | Set breakpoint(s) |
| `clear <file>` | Clear breakpoints in a file |
| `wait [timeout]` | Wait for breakpoint hit |
| `continue` / `c` | Continue execution |
| `pause` / `p` | Pause execution |
| `next` / `n` | Step over |
| `step` / `s` | Step into |
| `out` / `o` | Step out of function |
| `stack` / `bt` | Show call stack |
| `vars [scope]` | Show variables (locals/members/globals) |
| `expand <ref>` | Expand nested variable |
| `eval <expr>` | Evaluate expression |
| `scene-tree` / `tree` | Show live scene tree |
| `inspect <id>` / `i` | Inspect node properties |
| `set-prop <id> <prop> <val>` | Set a node property |
| `suspend` / `resume` | Freeze/resume game loop |
| `next-frame` / `nf` | Advance one frame |
| `timescale <N>` | Set Engine.time_scale |
| `reload` | Hot-reload scripts |
| `quit` / `q` | Disconnect and exit |

### Input Automation

Simulate player input against a running game (eval server enabled by default in `gd run`):

```sh
# Click at screen coordinates or on a node
gd debug click --pos 640,360
gd debug click --node /root/Main/StartButton

# Press an input action (from InputMap)
gd debug press --action ui_accept
gd debug press --action accelerate --hold 3.0   # hold for 3 seconds

# Press a keyboard key
gd debug key --key W
gd debug key --key W --hold 5.0                 # hold W for 5 seconds (e.g. drive forward)

# Type text
gd debug type --text "Hello world"
gd debug type --text "Hello" --delay 100         # 100ms between characters

# Wait for a condition or timeout
gd debug wait --seconds 2.0

# Take a screenshot
gd debug screenshot --output frame.png
```

The `--hold` flag keeps the input pressed for the specified duration while the game continues running (physics, animations, etc.). Essential for 3D games where single-frame presses barely register.

### Game Automation

Query and control the game world by node name or path — no manual object ID wrangling:

```sh
# AI-readable game state snapshot (position, nearby nodes, scene, input actions)
gd debug describe
gd debug describe --node "Hero" --radius 1000

# Find nodes by name, class, or group
gd debug find --name "Player"
gd debug find --type "CharacterBody2D"
gd debug find --group "enemies"

# Read/write properties by node name or path
gd debug get-prop --node "Player" --property "health"
gd debug set --node "Player" --property "speed" --value "200"
gd debug set --node "Player" --property "position" --value "Vector2(100, 200)"

# Call methods on nodes
gd debug call --node "Player" --method "take_damage" --args '[10]'
gd debug call --node "/root/Main/GameManager" --method "start_wave" --args '[3]'

# Navigate via NavigationAgent (pathfinding with animations)
gd debug navigate --node "Player" --to "500,300"
gd debug navigate --node "Player" --to-node "Blacksmith" --timeout 30

# Wait for conditions
gd debug await --node "GameOver"                                  # wait for node to exist
gd debug await --node "Player" --removed                          # wait for node removal
gd debug await --node "Player" --property "health" --equals "0"   # wait for property value

# Mouse cursor control (for UI testing)
gd debug mouse-move --pos 400,300
gd debug mouse-drag --from "100,200" --to "300,400"
gd debug mouse-hover --node "MenuItem"
```

All automation commands support `--format json` for structured output.

### 3D Mesh Editing *(experimental)*

> **Note:** The mesh API is experimental and under active development. Commands, flags, and output formats may change between versions. Use it if you want to test and provide feedback, but don't depend on it for production workflows yet.

Build 3D meshes from 2D profiles, primitives, and transforms — all from the terminal. Powered by a Rust-side half-edge mesh engine (21 core modules). Designed for AI agent workflows with JSON output and batch execution.

```sh
# Initialize workspace and create a session with a cube primitive
gd mesh init
gd mesh create --name body --from cube

# Define a 2D profile and extrude into 3D
gd mesh profile --plane front --points "0,0 2,0 2,1 0,1"
gd mesh profile --shape circle --radius 0.5 --segments 16  # circle/arc profiles
gd mesh extrude --depth 5.0 --segments 4

# Revolve a profile around an axis (with end caps)
gd mesh revolve --axis z --degrees 360 --segments 12 --cap

# Shape the mesh
gd mesh taper --axis y --from-scale 1.0 --to-scale 0.3 --from 0.5 --to 1.0
gd mesh bevel --radius 0.1 --segments 2 --edges depth --profile 0.5
gd mesh subdivide --iterations 2
gd mesh loop-cut --axis y --at 2.5
gd mesh inset --factor 0.2                      # shrink faces inward
gd mesh solidify --thickness 0.1                 # add shell thickness

# Boolean operations
gd mesh boolean --mode subtract --tool hole-cutter
gd mesh boolean --mode union --tool attachment
gd mesh boolean --mode intersect --tool clip-volume

# Modifiers
gd mesh array --count 5 --offset "2,0,0"        # linear duplication
gd mesh merge-verts --distance 0.001             # remove duplicate vertices

# Multi-part assembly
gd mesh add-part --name wing --from empty
gd mesh duplicate-part --name wing --as wing-left --mirror x    # negates position
gd mesh duplicate-part --name wing --as wing-left --symmetric x  # auto-offset
gd mesh focus body           # switch to a part
gd mesh focus --all          # show all parts
gd mesh remove-part --name old-part

# Transform parts
gd mesh translate --to "0,2,0" --part wing
gd mesh translate --relative --to "0,1,0"        # offset from current
gd mesh translate --relative-to body --to "0,3,0" # offset from another part
gd mesh rotate --degrees "0,45,0" --part wing
gd mesh scale --factor "1,0.5,1" --part wing

# Materials (single part, glob, or comma list)
gd mesh material --color "#ff0000"
gd mesh material --parts "wing-*" --preset metal
gd mesh material --parts "body,canopy" --preset glass --color "#aaddff"

# Normals and shading
gd mesh fix-normals             # auto-detect outward normals (majority vote)
gd mesh fix-normals --all       # all parts at once
gd mesh flip-normals            # reverse winding
gd mesh flip-normals --caps y   # flip only axis-aligned caps
gd mesh flip-normals --all      # all parts at once
gd mesh shade-smooth            # averaged vertex normals
gd mesh shade-flat              # per-face faceted look
gd mesh auto-smooth --angle 35  # smooth below angle threshold

# Viewing and inspection
gd mesh view                    # 7 orthographic + 7 isometric screenshots
gd mesh view --zoom 2.0 --normals  # zoom in with normal debug overlay
gd mesh info --all              # part inventory with world-space AABBs
gd mesh describe                # one-shot debrief (info + composite views)
gd mesh check --margin 0.5     # detect floating/disconnected parts

# State management
gd mesh checkpoint --name before-engines
gd mesh restore --name before-engines
gd mesh snapshot output.tscn    # export to .tscn with materials and transforms

# Batch execution (JSON command array)
gd mesh batch --file commands.json
```

All mesh commands support `--format json` for structured output.

### GitHub Templates

Create projects from any GitHub repository containing a Godot project:

```sh
# From a GitHub repo (auto-detects default branch)
gd new my-game --from user/godot-template

# With a specific branch or tag
gd new my-game --from user/repo@v1.0

# Full GitHub URLs also work
gd new my-game --from https://github.com/user/repo
```

The template system automatically finds `project.godot` within the repository to determine the project root, so templates with nested directory structures work correctly.

## Lint Rules

78 built-in rules organized into 8 categories (48 default-enabled, 30 opt-in):

### Categories

| Category | Description | Rules |
|----------|-------------|-------|
| **correctness** | Definite bugs | 14 |
| **suspicious** | Likely bugs, may be intentional | 10 |
| **style** | Naming and code style | 14 |
| **complexity** | Code size and complexity metrics | 8 |
| **performance** | Godot runtime performance | 4 |
| **godot** | Godot engine best practices | 12 |
| **type_safety** | Type system strictness | 8 |
| **maintenance** | Unused code and debug artifacts | 9 |

Categories can be bulk-controlled in `gd.toml`:

```toml
[lint]
correctness = "error"      # all correctness rules → error severity
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
| `comparison-with-boolean` | style | Flag explicit `== true`/`false` comparisons | warning | yes |
| `comparison-with-itself` | correctness | Detect `x == x` self-comparisons | warning | |
| `cyclomatic-complexity` | complexity | Warn on high cyclomatic complexity | warning | |
| `deeply-nested-code` | complexity | Warn on deeply nested code blocks | warning | |
| `duplicate-delegate` | maintenance | Detect pure pass-through delegate functions | info | |
| `duplicate-function` | correctness | Detect duplicate function definitions | error | |
| `duplicate-key` | correctness | Detect duplicate dictionary keys | warning | |
| `duplicate-signal` | correctness | Detect duplicate signal declarations | error | |
| `duplicated-load` | performance | Detect duplicate load/preload calls | warning | |
| `empty-function` | style | Detect functions with only `pass` in body | warning | |
| `enum-name-collision` | correctness | Detect inner enum names that collide with a global `class_name` | error | |
| `enum-naming` | style | Enforce PascalCase/UPPER_SNAKE_CASE enums | warning | yes |
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
| `node-ready-order` | godot | Detect node access before tree is ready | warning | |
| `null-after-await` | suspicious | Warn on member access after `await` without null guard | warning | |
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
| `untyped-array-literal` | type_safety | Warn on `var x := [...]` without typed Array annotation | warning | yes |
| `unnamed-node` | godot | Detect `add_child()` with dynamically created nodes that have no `.name` set | warning | |
| `unused-parameter` | maintenance | Detect unused function parameters | warning | |
| `unused-preload` | maintenance | Detect unused preload variables | warning | |
| `unused-private-class-variable` | maintenance | Detect unused `_`-prefixed class variables | warning | |
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

## Configuration

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

### Configuration Options

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

## SARIF Output

`gd lint` supports [SARIF 2.1.0](https://sarifweb.azurewebsites.net/) output for integration with GitHub Code Scanning:

```sh
gd lint --format sarif > results.sarif
```

Example GitHub Actions step:

```yaml
- name: Lint GDScript
  run: gd lint --format sarif > results.sarif

- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

## LSP Server

`gd lsp` starts a Language Server Protocol server over stdio, providing editor integration with:

- **Diagnostics** &mdash; real-time lint warnings and errors
- **Formatting** &mdash; format documents on save
- **Code actions** &mdash; quick fixes for lint issues
- **Document symbols** &mdash; outline of classes, functions, signals, and variables
- **Hover** &mdash; type and documentation info on hover (with built-in Godot docs, cross-file resolution with origin class)
- **Go to definition** &mdash; jump to function and variable declarations (indexed cross-file lookup)
- **Find references** &mdash; find all usages across the project
- **Rename** &mdash; rename symbols across files with prepare-rename support
- **Completion** &mdash; context-aware autocomplete for symbols, builtins, and lifecycle methods
- **Inlay hints** &mdash; ghost text showing inferred types for `:=` variables and parameter names at call sites
- **Signature help** &mdash; function signature with active parameter highlight as you type
- **Call hierarchy** &mdash; incoming and outgoing calls for any function
- **Find implementations** &mdash; find all subclasses and method overrides
- **Semantic tokens** &mdash; type-aware syntax highlighting (classes, enums, functions, signals)
- **Workspace symbol search** &mdash; fuzzy search across all project symbols (Ctrl+T)
- **Godot proxy** &mdash; forwards hover, completion, and definition to Godot's built-in LSP (port 6005) when the editor is running, with `--godot-port` and `--no-godot-proxy` flags

### One-Shot CLI Queries

`gd lsp` also exposes one-shot subcommands — human-readable by default, `--format json` for structured output:

```sh
# Rename a symbol across the project (applies to disk by default)
gd lsp rename --file player.gd --line 5 --column 10 --new-name move_character

# Rename by name (project-wide search)
gd lsp rename --name old_func --new-name new_func

# Preview without writing
gd lsp rename --file player.gd --line 5 --column 10 --new-name move_character --dry-run

# Find all references to a symbol (by position)
gd lsp references --file player.gd --line 5 --column 10

# Find all references by name (project-wide search)
gd lsp references --name speed

# Go to definition
gd lsp definition --file player.gd --line 5 --column 10

# Hover information
gd lsp hover --file player.gd --line 5 --column 10

# List completions
gd lsp completions --file player.gd --line 5 --column 10

# Available code actions / quick fixes
gd lsp code-actions --file player.gd --line 5 --column 1

# Run diagnostics (same as gd lint; --format json for structured output)
gd lsp diagnostics

# List symbols in a file
gd lsp symbols --file player.gd

# Filter symbols by kind
gd lsp symbols --file player.gd --kind function,signal

# View lines from a file
gd lsp view --file player.gd --range 10-20
gd lsp view --file player.gd --start-line 15 --context 3
gd lsp view --file player.gd --format json  # structured output for AI tools

# Scene info (nodes, resources, connections from a .tscn file)
gd lsp scene-info --file main.tscn
gd lsp scene-info --file main.tscn --nodes-only

# List all scenes that reference a script
gd lsp scene-refs --file player.gd

# List signal connections targeting a script's handler functions
gd lsp signal-connections --file player.gd

# Create a new GDScript file with scaffolding
gd lsp create-file --file enemies/boss.gd --extends CharacterBody2D --class-name Boss

# Create a file with custom content (from stdin or --input-file)
echo 'extends Node2D' | gd lsp create-file --file utils/helper.gd
```

All positions are **1-based** (line 1, column 1 is the first character). Paths in output are relative to the project root with forward slashes.

### Refactoring Commands

`gd lsp` includes structural refactoring commands — human-readable by default, `--format json` for structured output, `--dry-run` to preview:

```sh
# Delete a symbol (fails if references exist, use --force to override)
gd lsp delete-symbol --file player.gd --name unused_func
gd lsp delete-symbol --file player.gd --name unused_func --force

# Delete multiple symbols at once
gd lsp bulk-delete-symbol --file player.gd --names "a,b,c"

# Move a symbol between files
gd lsp move-symbol --name helper --from utils.gd --to helpers.gd

# Move and update preload paths in callers
gd lsp move-symbol --name helper --from utils.gd --to helpers.gd --update-callers

# Extract code into a new function
gd lsp extract-method --file player.gd --start-line 10 --end-line 15 --name do_attack

# Extract symbols to a new file
gd lsp extract-class --file player.gd --symbols "speed,health,take_damage" --to stats.gd

# Inline a function at its call sites
gd lsp inline-method --file player.gd --line 5 --column 2

# Inline a pass-through delegate function
gd lsp inline-delegate --file player.gd --name attack

# Rename multiple symbols atomically
gd lsp bulk-rename --file player.gd --renames "speed:velocity,health:hp"

# Change function signature (add/remove/reorder/rename params)
gd lsp change-signature --file player.gd --name move \
  --add-param "speed: float = 1.0" --remove-param old_param --reorder "a,b,c"

# Extract an expression into a local variable
gd lsp introduce-variable --file player.gd --line 5 --column 10 --end-column 30 --name velocity

# Turn a hardcoded value into a parameter with default
gd lsp introduce-parameter --file player.gd --line 5 --column 10 --end-column 20 --name speed

# AST-aware editing (reads new content from stdin or --input-file)
echo -e '\tprint("hello")' | gd lsp replace-body --file player.gd --name _ready
echo 'func _process(delta):\n\tpass' | gd lsp insert --file player.gd --after _ready
echo 'var speed: float = 42.0' | gd lsp replace-symbol --file player.gd --name speed
echo '\t# replaced' | gd lsp edit-range --file player.gd --range 5-7

# Or use --input-file to avoid stdin pipe encoding issues (recommended on Windows)
gd lsp insert --file player.gd --after _ready --input-file /tmp/new_func.gd
```

All refactoring commands support `--dry-run` to preview changes without writing to disk.
Edit commands also support `--no-format` to skip auto-formatting, `--class` for inner class targets, and `--input-file` to read from a file instead of stdin.

### Editor Setup

**VS Code:** Download the `.vsix` from the [latest release](https://github.com/c2lt4r/gd/releases/latest), then install it with:

```sh
code --install-extension gd-gdscript-0.1.2.vsix
```

**Neovim (nvim-lspconfig):**

```lua
require('lspconfig').gdscript_gd.setup {
  cmd = { 'gd', 'lsp' },
  filetypes = { 'gdscript' },
}
```

## License

MIT
