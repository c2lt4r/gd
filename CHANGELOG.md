# Changelog

## [0.1.0] - 2026-02-11

### Added
- `gd new` - Create new Godot projects
- `gd init` - Initialize gd in existing projects
- `gd fmt` - Format GDScript files (--check, --diff)
- `gd lint` - Lint GDScript with 25 rules (--fix, --format json/sarif/human)
- `gd run` - Run Godot project
- `gd build` - Export Godot project
- `gd check` - Validate project without building
- `gd clean` - Remove build artifacts
- `gd test` - Run GDScript tests
- `gd completions` - Shell completion generation
- `gd tree` - Show class hierarchy
- `gd doc` - Generate documentation
- `gd watch` - Watch and auto-format/lint on changes
- `gd addons` - Manage addons from Asset Library
- `gd stats` - Project statistics
- `gd ci` - Generate CI/CD pipeline configs
- `gd lsp` - Language Server Protocol server
- `gd deps` - Script dependency graph
- LSP with 8 capabilities: diagnostics, formatting, code actions, document symbols, hover, go-to-definition, find references, rename
- Cross-file LSP support via workspace indexing
- VS Code extension in editors/vscode/
- Per-rule lint configuration in gd.toml
- Lint suppression comments (gd:ignore, gd:ignore-next-line)
- SARIF output for GitHub Code Scanning
