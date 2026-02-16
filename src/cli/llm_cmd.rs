use miette::Result;

/// Print a compressed, AI-readable reference of all gd commands.
/// Designed for LLM context windows — like llms.txt for websites.
#[allow(clippy::unnecessary_wraps)]
pub fn exec() -> Result<()> {
    print!("{LLM_TXT}");
    Ok(())
}

const LLM_TXT: &str = r#"# gd — Godot CLI toolchain
# All commands support --help. Many support --format json for structured output.
# AI hint: prefer --brief on inspect, --format json on most commands.

## Project
gd new <name>                          # Create project (templates: default, 2d, 3d)
gd new <name> --from <user/repo>       # Create from GitHub template
gd init                                # Init gd in existing Godot project

## Code Quality
gd fmt [files...]                      # Format GDScript (AST-based)
gd fmt --check                         # Check formatting (CI mode, exit 1 if unformatted)
gd fmt --diff                          # Show formatting diff
gd lint [files...]                     # Lint GDScript (76 rules, 8 categories)
gd lint --fix                          # Auto-fix fixable lint issues
gd lint --format json|text|sarif       # Output format
gd check                               # Parse + structural + semantic + .tscn/.tres validation
gd check --format json                 # Machine-readable diagnostics

## Run & Build
gd run                                 # Run project (eval server enabled by default, non-blocking, captures to .godot/gd-game.log)
gd run --scene <path>                  # Run specific scene
gd run --log                           # Also stream Godot's stdout/stderr to terminal
gd run --bare                          # Run without eval server (disables `gd eval` and `gd debug` input commands)
gd stop                                # Stop the running game
gd log                                 # View game output from last run (cat -n style)
gd log --tail <N>                      # Last N lines
gd log --follow                        # Real-time tail (like tail -f)
gd log --clear                         # Truncate log file
gd build --preset <name>               # Export project
gd build --preset <name> --release     # Export release build
gd clean                               # Remove build artifacts
gd test                                # Run tests (auto-detects GUT/gdUnit4/script)
gd test --format json                  # Machine-readable test results

## Eval
gd eval "<expr>"                       # Evaluate GDScript expression (prints result)
gd eval "var x = 1; print(x * 2)"     # Multi-statement (semicolons)
gd eval script.gd                      # Run existing .gd file via Godot -s
gd eval -                              # Read script from stdin
gd eval --check "<expr>"               # Parse-validate before running
gd eval --timeout 10 "<expr>"          # Kill after N seconds (default: 30)
gd eval --verbose "<expr>"             # Show Godot stderr
gd eval --format json "<expr>"         # Machine-readable output (stdout, stderr, exit_code, errors)

## Scene
gd scene create <path> --root-type <T>                  # Create new .tscn
gd scene add-node <scene> --name <n> --type <T>         # Add child node (default parent: root)
gd scene remove-node <scene> --name <n>                 # Remove node + descendants
gd scene set-property <scene> --node <n> --key <k> --value <v>  # Set node property
gd scene add-connection <scene> --signal <s> --from <n> --to <n> --method <m>
gd scene remove-connection <scene> --signal <s> --from <n> --to <n> --method <m>
gd scene attach-script <scene> <script>                 # Attach script to root node
gd scene attach-script <scene> <script> --node <name>   # Attach to named node
gd scene detach-script <scene>                          # Detach script from root
gd scene detach-script <scene> --node <name>            # Detach from named node
# All scene subcommands support --dry-run

## Resource (.tres)
gd resource create <path> --type <T>                    # Create new .tres
gd resource create <path> --type <T> --script <f.gd>   # Create with script attached
gd resource set-property <file> --key <k> --value <v>   # Set/update [resource] property
gd resource get-property <file> --key <k>               # Print property value to stdout
gd resource remove-property <file> --key <k>            # Delete a property
gd resource set-script <file> <script>                  # Attach/change script
gd resource remove-script <file>                        # Detach script + cleanup
gd resource info <file>                                 # Resource structure (human-readable)
gd resource info <file> --format json                   # JSON dump of resource structure
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
gd debug set-prop-field --id <N> --property <name> --field <f> --value <val> --screenshot  # Set + auto-screenshot
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

# Evaluate expressions (auto-breaks into _process, no manual breakpoint needed)
gd debug eval --expr <expr>            # Evaluate GDScript expression in game context
gd debug eval --expr <expr> --format json  # JSON result with type info
# Rewrites: $Node → get_node(), x = val → set(), x += val → set(), multi; expr → [array]

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
gd debug scene camera-view --format json  # JSON: camera info + every spatial node's transform
gd debug camera override [--off]       # Take/release remote camera control
gd debug camera transform-2d --transform <json-array-6-floats>
gd debug camera transform-3d --transform <json-array-12-floats> --perspective <bool> --fov <N> --near <N> --far <N>
gd debug camera screenshot             # Output PNG file path
gd debug camera screenshot --format json  # JSON with width, height, format, path fields
gd debug camera screenshot --output <file>  # Copy PNG to specified path
gd debug profiler --name scripts|visual|servers [--off]  # Toggle profiler

# Live editing (gd debug live <cmd>, requires `live set-root` first)
# NOTE: live *-prop and *-call use "live edit IDs" from live set-root mapping.
# These are NOT the same as object IDs from scene tree/inspect. Two ID systems:
#   - Object IDs: from scene tree, used with inspect/set-prop/set-prop-field
#   - Live edit IDs: from live set-root, used with live node-prop/res-prop/*-call
gd debug live set-root --path </root/Main> --file <res://main.tscn>  # Set root for live edits
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

# One-shot queries (human-readable default, add --format json for structured output)
gd lsp hover --file <f> --line <L> --column <C>
gd lsp definition --file <f> --line <L> --column <C>
gd lsp references --name <sym>                    # Cross-project search by name
gd lsp references --name <sym> --class <cls>      # Filter to class
gd lsp references --name <sym> --file <f>         # Filter to file
gd lsp completions --file <f> --line <L> --column <C>
gd lsp rename --name <sym> --new-name <name>                       # Rename by name (project-wide)
gd lsp rename --file <f> --line <L> --column <C> --new-name <name> # Rename by position
gd lsp diagnostics [files...]
gd lsp symbols --file <f>                         # List symbols
gd lsp symbols --file <f> --kind <kind>           # Filter: function,variable,class,signal,enum
gd lsp code-actions --file <f> --line <L> --column <C>
gd lsp view --file <f> [--start <L> --end <L>]   # View file lines
gd lsp scene-info --file <f>                      # Scene structure from .tscn

# Refactoring (human-readable default, --format json for structured output, --dry-run to preview)
gd lsp delete-symbol --file <f> --name <sym>       # Also: --line <L>, --force
gd lsp move-symbol --name <sym> --from <f> --to <f> [--update-callers]
gd lsp extract-method --file <f> --start-line <L> --end-line <L> --name <name>
gd lsp inline-method --file <f> --name <sym>
gd lsp change-signature --file <f> --name <sym> --add-param "name: Type = default" --remove-param <name> --rename-param "old=new" --reorder "a,b,c"
gd lsp introduce-variable --file <f> --line <L> --column <C> --end-column <C> --name <name>
gd lsp introduce-parameter --file <f> --line <L> --column <C> --end-column <C> --name <name>
gd lsp bulk-delete-symbol --file <f> --names "a,b,c"
gd lsp bulk-rename --file <f> --renames "old1:new1,old2:new2"
gd lsp inline-delegate --file <f> --name <sym>
gd lsp extract-class --file <f> --symbols "a,b" --to <new_file>
gd lsp replace-body --file <f> --name <sym>        # Reads new body from stdin
gd lsp replace-symbol --file <f> --name <sym>      # Reads replacement from stdin
gd lsp insert --file <f> --after <sym>             # Insert after symbol (reads from stdin)
gd lsp insert --file <f> --before <sym>            # Insert before symbol (reads from stdin)
gd lsp edit-range --file <f> --range 5-20          # Replace lines 5-20 (reads from stdin)
gd lsp edit-range --file <f> --start-line <L> --end-line <L>  # Alternative range syntax
gd lsp create-file --file <f>                      # Create with boilerplate
gd lsp safe-delete-file --file <f>                 # Check for cross-file refs first
gd lsp find-implementations --name <method>        # Find all classes implementing a method

## Project Analysis
gd tree                                # Class hierarchy
gd tree --scene                        # Scene node trees from .tscn files
gd doc                                 # Generate markdown docs from ## comments
gd doc --format json                   # JSON doc output
gd doc --check                         # CI: exit 1 if public methods undocumented
gd doc --stdout                        # Print to stdout
gd stats                               # Project statistics (files, LOC, functions)
gd stats --format json                 # Machine-readable stats
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
gd env --json                          # Machine-readable environment info
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
# --format json           Available on most commands for machine-readable output
# --brief                 AI-preferred: stripped inspect output (just name=value pairs)
# --rich                  Enrich inspect with ClassDB docs (type descriptions, docs URLs)
# --off                   Toggle pattern: mute-audio --off, suspend --off, skip-breakpoints --off
# --dry-run               Preview refactoring changes without applying
# --screenshot             Auto-capture screenshot after set-prop/set-prop-field (outputs PNG path)
# --check                 CI mode: exit 1 on issues (fmt --check, doc --check)
# res:// paths            Godot resource paths used in debug breakpoints and live editing
# Object IDs              From scene-tree output, used in inspect/set-prop/set-prop-field
# Live edit IDs           From live-set-root mapping, used in live-node-prop/live-res-prop (different from object IDs)
# stdin commands          replace-body, replace-symbol, insert, edit-range read content from stdin (or --input-file)
"#;
