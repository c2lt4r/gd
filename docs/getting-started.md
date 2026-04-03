# Getting Started

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

## Creating from Templates

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

## Editor Setup

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
