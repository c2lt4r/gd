# Command Reference

| Command | Description |
|---------|-------------|
| `gd new <name>` | Create a new Godot project (templates: `default`, `2d`, `3d`, or `--from` GitHub) |
| `gd init` | Initialize gd toolchain in an existing project (detects export paths) |
| `gd fmt` | Format GDScript files |
| `gd lint` | Lint GDScript files |
| `gd run` | Run the Godot project (non-blocking, eval server enabled by default, `--bare` to disable eval) |
| `gd stop` | Stop the running Godot game |
| `gd log` | View game output log via debug protocol ring buffer (`--tail N`, `--follow`, `--errors`, `--grep`, `--json`, `--clear`) |
| `gd build` | Build/export the Godot project |
| `gd check` | Check project for errors with 100% Godot compiler parity (parse, structural, semantic, `.tscn`/`.tres` validation) (`--format json`) |
| `gd parse` | Parse GDScript files and report syntax errors only (no semantic analysis) (`--format json`) |
| `gd clean` | Clean build artifacts |
| `gd eval` | Evaluate GDScript expressions or scripts — live against a running game or offline headless |
| `gd test` | Run GDScript tests with GUT, gdUnit4, or raw scripts (`--list`, `--name`, `--class`, `--junit`, `--filter`, `--runner`, `--format json`) |
| `gd completions` | Generate shell completions (bash, zsh, fish, etc.) |
| `gd tree` | Show project class hierarchy (`--scene` for scene node trees) |
| `gd doc` | Generate documentation from doc comments (`--format json`, `--check`) |
| `gd watch` | Watch files and run fmt/lint on changes |
| `gd addons` | Manage project addons (install, remove, search, update, lock) |
| `gd overview` | Show project architecture overview (scripts, scenes, signal flow, autoloads) (`--format json`) |
| `gd stats` | Show project statistics (`--diff <branch>`, `--by-dir`, `--top N`) |
| `gd ci` | Generate CI/CD pipeline configuration |
| `gd daemon` | Manage the background daemon (status, stop, restart) |
| `gd debug` | Debug a running Godot game (breakpoints, stepping, eval, scene tree, inspect, time control) |
| `gd resource` | Manage `.tres` resource files (create, properties, scripts, info) |
| `gd scene` | Manage `.tscn` scene files (create, add/remove nodes, properties, connections, scripts) |
| `gd lsp` | Start the LSP server |
| `gd refactor` | Refactoring operations (rename, extract, inline, change-signature, undo, etc.) |
| `gd edit` | Code editing primitives (replace, insert, remove, extract, create-file) |
| `gd query` | Code intelligence queries (references, hover, definition, symbols, completions, etc.) |
| `gd ssr` | Structural search and replace for GDScript (`$placeholder` patterns, type constraints, `--dry-run`, `--format json`) |
| `gd deps` | Show script dependency graph (`--include-resources` for `.tscn`/`.tres`) |
| `gd env` | Show environment info (gd version, Godot version/path, OS, project root) |
| `gd man` | Generate man page |
| `gd upgrade` | Self-update to latest release |
| `gd llm` | Print AI-readable command reference (like llms.txt) |

## Formatter

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

## Linter

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

## Addons

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

## Watch Mode

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

## Dependency Graph

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

## Scene Tree

```sh
# Show scene node hierarchy for a .tscn file
gd tree --scene main.tscn

# Show all scenes in a directory
gd tree --scene .

# JSON output
gd tree --scene main.tscn --format json
```

## Scene Management

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

## Resource Management

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

## Statistics

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

## CI/CD Generation

```sh
# Generate GitHub Actions workflow
gd ci github

# Generate GitLab CI configuration
gd ci gitlab

# Include export stage
gd ci github --export --godot-version 4.4
```

See [CI/CD Integration](ci.md) for SARIF output and GitHub Code Scanning setup.

## Daemon

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

## Debugging

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

## Input Automation

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

## Game Automation

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

## Code Queries

`gd query` provides one-shot code intelligence queries — human-readable by default, `--format json` for structured output:

```sh
# Find all references to a symbol (by position)
gd query references player.gd --line 5 --column 10

# Find all references by name (project-wide search)
gd query references --name speed

# Go to definition (by position or by name)
gd query definition player.gd --line 5 --column 10
gd query definition player.gd --name my_func

# Hover information (by position or by name)
gd query hover player.gd --line 5 --column 10
gd query hover player.gd --name speed

# List completions
gd query completions player.gd --line 5 --column 10

# Available code actions / quick fixes
gd query code-actions player.gd --line 5 --column 1

# List symbols in a file
gd query symbols player.gd

# Filter symbols by kind
gd query symbols player.gd --kind function,signal

# View lines from a file
gd query view player.gd --range 10-20
gd query view player.gd --start-line 15 --context 3
gd query view player.gd --format json  # structured output for AI tools

# Scene info (nodes, resources, connections from a .tscn file)
gd query scene-info main.tscn
gd query scene-info main.tscn --nodes-only

# List all scenes that reference a script
gd query scene-refs player.gd

# List signal connections targeting a script's handler functions
gd query signal-connections player.gd
```

All positions are **1-based** (line 1, column 1 is the first character). Paths in output are relative to the project root with forward slashes.

## Refactoring

`gd refactor` provides structural refactoring commands — human-readable by default, `--format json` for structured output, `--dry-run` to preview:

```sh
# Rename a symbol across the project (applies to disk by default)
gd refactor rename player.gd --line 5 --column 10 --new-name move_character

# Rename by name (project-wide search)
gd refactor rename --name old_func --new-name new_func

# Preview without writing
gd refactor rename player.gd --line 5 --column 10 --new-name move_character --dry-run

# Delete a symbol (fails if references exist, use --force to override)
gd refactor delete-symbol player.gd --name unused_func
gd refactor delete-symbol player.gd --name unused_func --force

# Delete multiple symbols at once
gd refactor delete-symbol player.gd --names "a,b,c"

# Move a symbol between files (by name or by line)
gd refactor move-symbol --name helper --from utils.gd --to helpers.gd
gd refactor move-symbol --line 5 --from utils.gd --to helpers.gd

# Move and update preload paths in callers
gd refactor move-symbol --name helper --from utils.gd --to helpers.gd --update-callers

# Extract code into a new function
gd refactor extract-method player.gd --start-line 10 --end-line 15 --name do_attack

# Extract symbols to a new file
gd refactor extract-class player.gd --symbols "speed,health,take_damage" --to stats.gd

# Inline a function (same-file, cross-file, or self.method() / obj.method())
gd refactor inline-method player.gd --line 5 --column 2

# Inline a pass-through delegate function (by name or by line)
gd refactor inline-delegate player.gd --name attack
gd refactor inline-delegate player.gd --line 5

# Rename multiple symbols atomically
gd refactor bulk-rename player.gd --renames "speed:velocity,health:hp"

# Rename symbols in a single file only (skip cross-file references)
gd refactor bulk-rename player.gd --renames "speed:velocity" --scope file

# Change function signature (by name or by line)
gd refactor change-signature player.gd --name move \
  --add-param "speed: float = 1.0" --remove-param old_param --reorder "a,b,c"
gd refactor change-signature player.gd --line 12 --add-param "speed: float = 1.0"

# Inline a variable (by position or by name)
gd refactor inline-variable player.gd --line 5 --column 10
gd refactor inline-variable player.gd --name speed

# Extract an expression into a local variable (type inferred automatically)
gd refactor introduce-variable player.gd --line 5 --column 10 --end-column 30 --name velocity

# Replace all identical expressions in scope
gd refactor introduce-variable player.gd --line 5 --column 10 --end-column 30 --name velocity --replace-all

# Turn a hardcoded value into a parameter with default
gd refactor introduce-parameter player.gd --line 5 --column 10 --end-column 20 --name speed

# Invert an if/else: negate condition, swap branches
gd refactor invert-if player.gd --line 5

# Flatten nested ifs to early return/continue guard clauses (by name or by line)
gd refactor extract-guards player.gd --name _process
gd refactor extract-guards player.gd --line 5

# Split/join variable declaration and assignment (by line or by name)
gd refactor split-declaration player.gd --line 3
gd refactor split-declaration player.gd --name health
gd refactor join-declaration player.gd --line 3
gd refactor join-declaration player.gd --name health

# Convert between $NodePath and get_node() syntax
gd refactor convert-node-path player.gd --line 5 --column 10

# Convert between @onready var and _ready() assignment (by name or by line)
gd refactor convert-onready player.gd --name sprite --to-ready
gd refactor convert-onready player.gd --line 3 --to-onready

# Convert signal connections between scene wiring and code
gd refactor convert-signal player.tscn --signal pressed --from Button --method _on_btn --to-code
gd refactor convert-signal player.tscn --signal pressed --from Button --method _on_btn --to-scene

# Encapsulate a field with property accessors (inline syntax)
gd refactor encapsulate-field player.gd --name health

# Encapsulate with backing field pattern (_health + getter/setter)
gd refactor encapsulate-field player.gd --name health --backing-field

# Extract members into a new superclass
gd refactor extract-superclass entity.gd --symbols "health,take_damage" --to base_entity.gd --class-name BaseEntity

# Pull a member up from child to parent class
gd refactor pull-up-member player.gd --name score

# Push a member down from parent to all children
gd refactor push-down-member entity.gd --name get_speed

# Push down to specific children only
gd refactor push-down-member entity.gd --name get_speed --to player.gd,enemy.gd

# Safely delete a file (checks for references first)
gd refactor safe-delete-file unused.gd

# Move a file and update all references
gd refactor move-file --from utils.gd --to lib/utils.gd
```

All refactoring commands support `--dry-run` to preview changes without writing to disk.

### Undo

Every refactoring command records an undo entry. If a refactoring produces unexpected results, revert it:

```sh
# List recent refactoring operations
gd refactor undo --list

# Undo the most recent refactoring
gd refactor undo

# Undo a specific entry by ID
gd refactor undo --id 3

# Preview what would be restored
gd refactor undo --dry-run
```

Multi-file refactorings (extract-class, move-symbol, move-file) use atomic transactions — if any step fails, all files are automatically restored to their original state.

## Code Editing

`gd edit` provides AST-aware editing primitives for AI agent workflows — reads new content from stdin or `--input-file`:

```sh
# Replace a declaration by name
echo 'var speed: float = 42.0' | gd edit replace player.gd --name speed

# Replace only a function body (keep signature)
echo -e '\tprint("hello")' | gd edit replace player.gd --name _ready --body

# Replace by line range
echo 'var x = 10' | gd edit replace player.gd --line 5-7

# Insert after a symbol
echo 'func _process(delta):\n\tpass' | gd edit insert player.gd --name _ready --after

# Insert into a function body (as first statement)
echo -e '\tprint("start")' | gd edit insert player.gd --name _ready --into

# Insert into a class body (as last member)
echo -e '\tfunc foo():\n\t\tpass' | gd edit insert player.gd --name Inner --into-end

# Create a new GDScript file with scaffolding
gd edit create-file enemies/boss.gd --extends CharacterBody2D --class-name Boss

# Use --input-file to avoid stdin pipe encoding issues (recommended on Windows)
gd edit insert player.gd --name _ready --after --input-file /tmp/new_func.gd
```

Edit commands support `--dry-run` to preview, `--no-format` to skip auto-formatting, `--class` for inner class targets, and `--input-file` to read from a file instead of stdin.

## Structural Search & Replace

```sh
# Search: find all additions in the project
gd ssr '$a + $b'

# Search with count only
gd ssr '$a + $b' -c

# Replace: swap operands (dry-run preview)
gd ssr '$a + $b' -r '$b + $a' -n

# Replace: apply changes
gd ssr '$a + $b' -r '$b + $a'

# Replace deprecated API pattern
gd ssr '$node.get_child($i).name' -r '$node.get_child($i).get_name()'

# Variadic: rename function calls with any number of arguments
gd ssr 'print($$args)' -r 'log($$args)'

# Type-constrained: match only when receiver is a Node (or subclass)
gd ssr '$recv:Node.remove_child($c)' -r '$recv.remove_child($c); $c.queue_free()'

# Duck-typing: match calls on types that have a specific method
gd ssr '$obj:{has_method("process")}.process($d)'

# Restrict to specific files
gd ssr '$a + $b' -f player.gd -f enemy.gd

# JSON output for tooling
gd ssr '$a + $b' --format json

# Statement patterns (var declarations, assignments, return)
gd ssr 'var $name = $value'
gd ssr 'return $expr'
gd ssr '$target += $value'
```

Patterns are GDScript with `$`-prefixed placeholders. `$name` matches any single expression, `$$name` matches zero or more arguments (variadic, call-position only), and `$name:Type` adds a type constraint. Repeated placeholders (`$a + $a`) require structurally identical matches. Respects `ignore_patterns` from `gd.toml`.
