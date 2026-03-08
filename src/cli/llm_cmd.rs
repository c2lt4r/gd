use miette::Result;

/// Print a compressed, AI-readable reference of all gd commands.
/// Designed for LLM context windows — like llms.txt for websites.
#[allow(clippy::unnecessary_wraps)]
pub fn exec() -> Result<()> {
    print!("{LLM_TXT}");
    Ok(())
}

const LLM_TXT: &str = r#"# gd — Godot CLI toolchain
# All commands support --help and --no-color. Positions are 1-based.
# Default output is human-readable text; add --format json when you need structured data.

## Project
gd new <name>                          # Create project (templates: default, 2d, 3d)
gd new <name> --from <user/repo>       # Create from GitHub template
gd init                                # Init gd in existing Godot project

## Code Quality
gd fmt [files...]                      # Format GDScript (AST-based)
gd fmt --check                         # Check formatting (CI mode, exit 1 if unformatted)
gd fmt --diff                          # Show formatting diff
gd lint [files...]                     # Lint GDScript (89 rules, 8 categories)
gd lint --fix                          # Auto-fix fixable lint issues
gd check                               # Parse + structural + semantic + .tscn/.tres validation
# Inline suppression comments (add to end of line or on line before):
#   # gd:ignore                        Suppress all warnings on this line
#   # gd:ignore[rule-name]             Suppress a specific rule
#   # gd:ignore[rule-a, rule-b]        Suppress multiple rules
#   # gd:ignore-next-line              Suppress all warnings on next line
#   # gd:ignore-next-line[rule-name]   Suppress a specific rule on next line

## Run & Build
gd run                                 # Run project (eval server enabled by default, non-blocking)
gd run --scene <path>                  # Run specific scene
gd run --bare                          # Run without eval server (disables gd eval and gd debug input commands)
gd stop                                # Stop the running game
gd log                                 # View game output log (ring buffer, via debug protocol)
gd log --tail <N>                      # Last N lines (default: 50)
gd log --follow                        # Real-time tail (like tail -f)
gd log --errors                        # Show only errors and warnings
gd log --grep <pattern>                # Filter lines matching pattern
gd log --clear                         # Clear the log buffer
gd build --preset <name>               # Export project
gd build --preset <name> --release     # Export release build
gd clean                               # Remove build artifacts
gd test run [name]                     # Run tests (auto-detects native/GUT/gdUnit4/script)
gd test run --runner native            # Run with native interpreter (no Godot needed)
gd test run --filter <pattern>         # Filter test files by pattern
gd test run --list                     # List matching tests without running
gd test run --junit <file>             # Export results to JUnit XML

## Eval
gd eval "<expr>"                       # Evaluate GDScript expression (prints result)
gd eval "var x = 1; print(x * 2)"     # Multi-statement (semicolons)
gd eval script.gd                      # Run existing .gd file via Godot -s
gd eval -                              # Read script from stdin
gd eval --native "<expr>"             # Use native interpreter (no Godot required)
gd eval --check "<expr>"               # Parse-validate before running
gd eval --timeout 10 "<expr>"          # Kill after N seconds (default: 30)
gd eval --verbose "<expr>"             # Show Godot stderr
gd eval --unsafe "<expr>"              # Skip sandbox checks (allow OS.execute etc.)

## Scene
gd scene create <path> --root-type <T>                  # Create new .tscn
gd scene add-node <scene> --name <n> --type <T>         # Add child node (default parent: root)
gd scene add-node <scene> --name <n> --type <T> --parent <path>  # Add to nested parent (e.g. UI/Panel)
gd scene add-instance <scene> <instance.tscn>           # Instance scene (auto-names from filename)
gd scene add-instance <scene> <instance.tscn> --name <n> --parent <path>
gd scene add-sub-resource <scene> --type <T>            # Add sub_resource section
gd scene add-sub-resource <scene> --type <T> --prop size=Vector3(1,1,1) --node <n> --key shape
gd scene batch-add <scene> --node N1:Type1 --node N2:Type2:Parent  # Add multiple nodes
gd scene duplicate-node <scene> --source-node <n> --name <new>     # Copy node with properties
gd scene duplicate-node <scene> --source-node <n> --name <new> --parent <path>
gd scene remove-node <scene> --name <n>                 # Remove node + descendants
gd scene set-property <scene> --node <n> --key <k> --value <v>  # Set node property
gd scene add-connection <scene> --signal <s> --from <n> --to <n> --method <m>
gd scene remove-connection <scene> --signal <s> --from <n> --to <n> --method <m>
gd scene attach-script <scene> <script>                 # Attach script to root node
gd scene attach-script <scene> <script> --node <name>   # Attach to named node
gd scene detach-script <scene>                          # Detach script from root
gd scene detach-script <scene> --node <name>            # Detach from named node
# All scene subcommands support --dry-run
# Node args accept names (Player) or paths (UI/Panel/Label) for nested nodes

## Resource (.tres)
gd resource create <path> --type <T>                    # Create new .tres
gd resource create <path> --type <T> --script <f.gd>   # Create with script attached
gd resource set-property <file> --key <k> --value <v>   # Set/update [resource] property
gd resource get-property <file> --key <k>               # Print property value to stdout
gd resource remove-property <file> --key <k>            # Delete a property
gd resource set-script <file> <script>                  # Attach/change script
gd resource remove-script <file>                        # Detach script + cleanup
gd resource info <file>                                 # Resource structure
# All write subcommands support --dry-run

## Debug (requires running game via `gd run`)
# Scene inspection (gd debug scene <cmd>)
gd debug scene tree                    # Live scene tree (node names, classes, object IDs)
gd debug scene inspect --id <N>        # Inspect node properties (all fields)
gd debug scene inspect --id <N> --brief      # AI-friendly: just name=value pairs, no Godot internals
gd debug scene inspect --id <N> --rich       # Enriched with ClassDB docs per property
gd debug scene inspect-objects --id <N> [--id <M>...]  # Inspect multiple objects at once
gd debug set-prop --id <N> --property <name> --value <val>       # Set property
gd debug set-prop --id <N> --property <name> --value <val> --screenshot  # Set + auto-screenshot
gd debug set-prop-field --id <N> --property <name> --field <f> --value <val>  # Set sub-field (e.g. position.x)
gd debug set-prop-field --id <N> --property <name> --field <f> --value <val> --screenshot
gd debug save-node --id <N> --path <file>        # Save node to file on game's filesystem

# Execution control
gd debug continue                      # Resume from breakpoint
gd debug pause                         # Pause execution
gd debug next                          # Step over (alias: step-over)
gd debug step-in                       # Step into function
gd debug step-out                      # Step out of function

# Breakpoints
gd debug breakpoint --path <res://file.gd> --line <N>               # Set breakpoint
gd debug breakpoint --name <func>                                    # By function name (auto-resolves to file:line)
gd debug breakpoint --path <res://file.gd> --line <N> --condition <expr>  # Conditional
gd debug breakpoint --path <res://file.gd> --line <N> --off         # Clear breakpoint
gd debug skip-breakpoints [--off]      # Skip/unskip all breakpoints
gd debug ignore-errors [--off]         # Ignore/stop-on error breaks

# Stack & variables (at breakpoint)
gd debug stack                         # Call stack
gd debug vars --frame <N>              # Variables for stack frame

# Evaluate (full GDScript — loops, if, var, for all work)
gd debug eval --expr <expr>            # Evaluate GDScript in game context (via eval server)
gd debug eval --expr "for i in 5: print(i)"  # Loops and control flow work
gd debug eval --expr <expr> --timeout 30     # Custom timeout (default: 10s)
gd debug eval --expr <expr> --bare     # Use Godot's Expression class (no loops/if/var, but reads breakpoint locals)

# Game loop control
gd debug suspend                       # Freeze game loop
gd debug suspend --off                 # Resume game loop
gd debug next-frame                    # Advance one physics frame
gd debug time-scale --scale <N>        # Set time scale (0.5=slow, 2.0=fast)
gd debug reload-scripts                # Hot-reload GDScript files
gd debug reload-all-scripts            # Reload all scripts (full)
gd debug mute-audio [--off]            # Mute/unmute
gd debug stop                          # Stop the running game (alias for gd stop)

# Camera & visual (gd debug camera <cmd>)
gd debug scene camera-view             # Structured spatial data: all nodes with positions/rotations
gd debug camera override [--off]       # Take/release remote camera control
gd debug camera transform-2d --transform <json-array-6-floats>
gd debug camera transform-3d --transform <json-array-12-floats> --perspective <bool> --fov <N> --near <N> --far <N>
gd debug camera screenshot             # Output PNG file path
gd debug camera screenshot --output <file>  # Copy PNG to specified path
gd debug profiler --name scripts|visual|servers [--off]  # Toggle profiler

# Live editing (gd debug live <cmd>, requires `live set-root` first)
# NOTE: live *-prop and *-call use "live edit IDs" from live set-root mapping.
# These are NOT the same as object IDs from scene tree/inspect. Two ID systems:
#   - Object IDs: from scene tree, used with inspect/set-prop/set-prop-field
#   - Live edit IDs: from live set-root, used with live node-prop/res-prop/*-call
gd debug live set-root --path </root/Main> <res://main.tscn>  # Set root for live edits
gd debug live create-node --parent <path> --class <ClassName> --name <name>
gd debug live instantiate --parent <path> --scene <res://scene.tscn> --name <name>
gd debug live remove-node --path <node-path>
gd debug live duplicate --path <node-path> --name <new-name>
gd debug live reparent --path <node-path> --new-parent <path> [--name <name>] [--pos <N>]
gd debug live node-prop --id <N> --property <name> --value <json>    # N = live edit ID
gd debug live node-call --id <N> --method <name> [--args <json-array>]
gd debug live res-prop --id <N> --property <name> --value <json>
gd debug live res-call --id <N> --method <name> [--args <json-array>]
gd debug live node-prop-res --id <N> --property <name> --res-path <res://path>
gd debug live res-prop-res --id <N> --property <name> --res-path <res://path>
gd debug live node-path --path <node-path> --id <N>     # Set node path mapping
gd debug live res-path --path <res-path> --id <N>       # Set resource path mapping
gd debug live remove-keep --path <node-path> --object-id <N>  # Remove but keep ref (uses object ID)
gd debug live restore --object-id <N> --path <node-path> [--pos <N>]  # Restore (uses object ID)

# Node automation (requires eval server — `gd run` enables by default)
gd debug describe                      # AI snapshot: player position, nearby nodes, scene, input actions
gd debug describe --node <name>        # Use specific node as reference (default: auto-detect Player)
gd debug describe --radius <N>         # Custom search radius (default: 500 for 2D, 20 for 3D)
gd debug find --name <name>            # Find node by name (recursive) or absolute path
gd debug find --type <class>           # Find all nodes of a class type
gd debug find --group <group>          # Find all nodes in a group
gd debug get-prop --node <name> --property <prop>    # Read property value by node name/path
gd debug get-prop --id <N> --property <prop>         # Read property value by object ID
gd debug call --node <name> --method <method> [--args <json-array>]   # Call method on node
gd debug call --id <N> --method <method> [--args <json-array>]        # Call method by object ID
gd debug set --node <name> --property <prop> --value <gdscript-expr>  # Set property by name/path
gd debug set --node <name> --property <prop> --value <expr> --screenshot  # Set + auto-screenshot
gd debug await --node <name>                         # Wait for node to exist (poll)
gd debug await --node <name> --removed               # Wait for node to be removed
gd debug await --node <name> --property <p> --equals <v>   # Wait for property == value
gd debug await --node <name> --property <p> --gt <v>       # Wait for property > value
gd debug await --node <name> --property <p> --lt <v>       # Wait for property < value
gd debug await --node <name> --property <p> --contains <s> # Wait for string contains
gd debug await ... --timeout <secs> --interval <ms>  # Configure polling (default: 10s / 200ms)
gd debug navigate --node <name> --to <X,Y>             # Navigate node via NavigationAgent2D (2D coords)
gd debug navigate --node <name> --to <X,Y,Z>          # Navigate node via NavigationAgent3D (3D coords)
gd debug navigate --node <name> --to-node <target>     # Navigate to another node's position
gd debug navigate ... --timeout <secs> --interval <ms> # Configure polling (default: 30s / 200ms)
gd debug mouse-move --pos <X,Y>                      # Move mouse cursor to screen coordinates
gd debug mouse-move --node <name>                    # Move mouse to node's screen position
gd debug mouse-move --node <name> --duration <secs>  # Smooth cursor move over duration
gd debug mouse-drag --from <X,Y> --to <X,Y>         # Drag cursor from A to B
gd debug mouse-drag --from-node <n> --to-node <n>   # Drag cursor between nodes
gd debug mouse-drag ... --button <left|right|middle> --duration <secs> --steps <N>
gd debug mouse-hover --node <name>                   # Hover cursor over node (triggers mouse_enter)
gd debug mouse-hover --pos <X,Y> --duration <secs>   # Hover cursor at position for duration

# File management
gd debug reload-cached --file <path> [--file <path>...]  # Reload cached files

# Node selection (gd debug select <cmd>)
gd debug select type --value <N>
gd debug select mode --value <N>
gd debug select visible [--off]
gd debug select avoid-locked [--off]
gd debug select prefer-group [--off]
gd debug select reset-cam-2d
gd debug select reset-cam-3d
gd debug select clear

## Daemon
gd daemon status                       # Show daemon state (game_running, ports, etc.)
gd daemon stop                         # Stop background daemon
gd daemon restart                      # Restart daemon

## LSP (Language Server)
gd lsp                                 # Start LSP server (for editors)

## Query (code intelligence — one-shot queries, human-readable default)
gd query hover <f> --line <L> --column <C>            # Hover info at position
gd query hover --name <sym>                            # Hover info by symbol name
gd query definition <f> --line <L> --column <C>       # Go to definition
gd query definition --name <sym>                       # Definition by symbol name
gd query references --name <sym>                       # Cross-project search by name
gd query references --name <sym> --class <cls>         # Filter to class
gd query references --name <sym> <f>                   # Filter to file
gd query references <f> --line <L> --column <C>        # References by position
gd query completions <f> --line <L> --column <C>       # Completions at position
gd query completions <f> --line <L> --column <C> --kind <kind>  # Filter by kind
gd query symbols <f>                                   # List symbols in file
gd query symbols <f> --kind <kind>                     # Filter: function,variable,class,signal,enum
gd query code-actions <f> --line <L> --column <C>      # Available code actions
gd query view <f> [--range 5-20]                       # View file lines (with optional range)
gd query scene-info <f>                                # Scene structure from .tscn
gd query scene-info <f> --nodes-only                   # Compact: nodes only
gd query scene-refs <f>                                # All scenes that reference a .gd file
gd query signal-connections <f>                        # Signal connections targeting a script's handlers
gd query find-implementations --name <method>          # All classes implementing a method
gd query find-implementations --name <method> --base <class>  # Filter by base class

## Refactor (all support --dry-run to preview, human-readable default)
# Rename
gd refactor rename <f> --line <L> --column <C> --new-name <name>   # Rename by position
gd refactor rename --name <sym> --new-name <name>                   # Rename by name (project-wide)
gd refactor bulk-rename <f> --renames "old1:new1,old2:new2"         # Rename multiple symbols atomically
gd refactor bulk-rename <f> --renames "old:new" --scope file        # Restrict to target file only

# Delete
gd refactor delete-symbol <f> --name <sym>             # Delete symbol (refuses if references exist)
gd refactor delete-symbol <f> --name <sym> --force     # Delete even with references
gd refactor delete-symbol <f> --names "a,b,c"          # Bulk delete multiple symbols
gd refactor safe-delete-file <f>                       # Check cross-file refs before deleting
gd refactor safe-delete-file <f> --force               # Actually delete the file

# Move
gd refactor move-symbol --name <sym> --from <f> --to <f>           # Move symbol between files
gd refactor move-symbol --name <sym> --from <f> --to <f> --update-callers  # Update preload/load paths
gd refactor move-file --from <f> --to <f>              # Move/rename file + update all references

# Extract
gd refactor extract-method <f> --start-line <L> --end-line <L> --name <name>
gd refactor extract-constant <f> --line <L> --column <C> --end-column <C> --name <NAME>
gd refactor extract-constant <f> --line <L> --column <C> --end-column <C> --name <NAME> --replace-all
gd refactor extract-constant <f> --line <L> --column <C> --end-column <C> --name <NAME> --class <cls>  # Into inner class
gd refactor extract-class <f> --symbols "a,b" --to <new_file>
gd refactor extract-superclass <f> --symbols "a,b" --to <new_file> [--class-name <name>]
gd refactor extract-guards <f> --name <name>           # Flatten nested ifs to early return/continue guards

# Inline
gd refactor inline-method <f> --line <L> --column <C>  # Inline single call site
gd refactor inline-method <f> --name <sym> --all       # Inline all call sites + delete function
gd refactor inline-variable <f> --line <L> --column <C>  # Replace usages with initializer, delete decl
gd refactor inline-variable <f> --name <sym>           # By name
gd refactor inline-delegate <f> --name <sym>           # Inline pass-through delegate

# Introduce
gd refactor introduce-variable <f> --line <L> --column <C> --end-column <C> --name <name>
gd refactor introduce-variable <f> --line <L> --column <C> --end-column <C> --name <name> --const --replace-all
gd refactor introduce-parameter <f> --line <L> --column <C> --end-column <C> --name <name> [--type <hint>]

# Signature & structure
gd refactor change-signature <f> --name <sym> --add-param "name: Type = default" --remove-param <name> --rename-param "old=new" --reorder "a,b,c"
gd refactor encapsulate-field <f> --name <sym>         # Add set/get accessors (inline property syntax)
gd refactor encapsulate-field <f> --name <sym> --backing-field  # Use _name + getter/setter functions
gd refactor push-down-member <f> --name <sym>          # Push member to child classes (auto-discovers)
gd refactor push-down-member <f> --name <sym> --to "child1.gd,child2.gd"
gd refactor pull-up-member <f> --name <sym>            # Pull member up to parent class
gd refactor split-declaration <f> --line <L>           # Split var x = expr into declaration + assignment
gd refactor join-declaration <f> --line <L>            # Join bare var + following assignment

# Convert
gd refactor invert-if <f> --line <L>                   # Negate condition, swap branches
gd refactor convert-node-path <f> --line <L> --column <C>  # $Path <-> get_node("Path")
gd refactor convert-onready <f> --name <sym> --to-ready    # @onready var -> _ready() assignment
gd refactor convert-onready <f> --name <sym> --to-onready  # _ready() assignment -> @onready var
gd refactor convert-signal <f.tscn> --signal <sig> --from <node> --method <m> --to-code
gd refactor convert-signal <f.tscn> --signal <sig> --from <node> --method <m> --to-scene

# Undo
gd refactor undo                                       # Undo most recent refactoring
gd refactor undo --id <N>                              # Undo specific entry
gd refactor undo --list                                # List undo-able operations

## Edit (code editing primitives — read content from stdin or --input-file)
gd edit create-file <f>                                # Create with boilerplate
gd edit create-file <f> --extends <T> --class-name <n> # Custom extends/class_name
gd edit create-file <f> --input-file <src>             # Create from file content
gd edit replace-body <f> --name <sym>                  # Replace function body (stdin)
gd edit replace-body <f> --name <sym> --input-file <src>
gd edit replace-symbol <f> --name <sym>                # Replace entire symbol (stdin)
gd edit insert <f> --after <sym>                       # Insert after symbol (stdin)
gd edit insert <f> --before <sym>                      # Insert before symbol (stdin)
gd edit edit-range <f> --range 5-20                    # Replace lines 5-20 (stdin)
gd edit edit-range <f> --start-line <L> --end-line <L> # Alternative range syntax
# All edit commands support --dry-run, --no-format, --class <inner>

## Project Analysis
gd overview                            # Project architecture overview
gd overview <path>                     # Scope to specific path
gd tree                                # Class hierarchy
gd tree --scene                        # Scene node trees from .tscn files
gd doc                                 # Generate markdown docs from ## comments
gd doc --check                         # CI: exit 1 if public methods undocumented
gd doc --stdout                        # Print to stdout
gd stats                               # Project statistics (files, LOC, functions)
gd stats --by-dir                      # Per-directory breakdown
gd stats --top <N>                     # Top-N longest functions
gd stats --diff <branch>              # Compare stats vs git branch
gd deps                                # Script dependency graph
gd deps --format dot|json|tree         # Output format
gd deps --include-resources            # Include .tscn/.tres files

## Addons
gd addons search <query>               # Search Godot Asset Library
gd addons install <name|url>           # Install addon
gd addons remove <name>                # Remove addon
gd addons list                         # List installed addons
gd addons update [--apply]             # Check for updates
gd addons lock                         # Generate lock file
gd addons install --locked             # Install from lock file

## Other
gd env                                 # Show environment info (gd version, Godot version, paths)
gd watch                               # Watch files, run fmt/lint on change
gd ci --platform github|gitlab         # Generate CI config
gd completions --shell bash|zsh|fish   # Shell completions
gd man                                 # Generate man page
gd upgrade [--check]                   # Self-update

## Config (gd.toml)
# [fmt] use_tabs, indent_size, max_line_length, blank_lines_around_functions/classes
# [lint] disabled_rules, enabled_rules, ignore_patterns
# [lint] category levels: correctness = "error", type_safety = "warning", maintenance = "off"
# [run] godot_path, extra_args
# [build] output_dir

## Patterns
# --no-color              Disable colored output (also respects NO_COLOR env)
# --format json           Available on most commands when you need structured data
# --brief                 Stripped inspect output (just name=value pairs, no Godot internals)
# --rich                  Enrich inspect with ClassDB docs (type descriptions, docs URLs)
# --off                   Toggle pattern: mute-audio --off, suspend --off, skip-breakpoints --off
# --dry-run               Preview refactoring/edit changes without applying
# --screenshot            Auto-capture screenshot after set-prop/set-prop-field (outputs PNG path)
# --check                 CI mode: exit 1 on issues (fmt --check, doc --check)
# res:// paths            Godot resource paths used in debug breakpoints and live editing
# Object IDs              From scene-tree output, used in inspect/set-prop/set-prop-field
# Live edit IDs           From live-set-root mapping, used in live-node-prop/live-res-prop (different from object IDs)
# stdin commands          replace-body, replace-symbol, insert, edit-range read content from stdin (or --input-file)
"#;
