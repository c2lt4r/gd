# Godot Version Maintainability Guide

Reference for keeping `gd` compatible with Godot updates and evaluating older-version support.

**Current target: Godot 4.0+ (ClassDB generated from 4.6)**

---

## Version-Coupled Components

### Tier 1 — Regenerate on Godot Minor Release

These break silently (missing classes/methods) when Godot adds new API. Fix is mechanical.

| Component | Files | What to do |
|-----------|-------|------------|
| **ClassDB** | `src/class_db/generated.rs`, `tools/generate_class_db.py` | `godot --headless --dump-extension-api` then `python tools/generate_class_db.py extension_api.json > src/class_db/generated.rs` |
| **LSP builtins** | `src/lsp/builtins.rs` | Add docs for any new builtin types/functions |
| **Type inference builtins** | `src/core/type_inference.rs` (constructor list ~line 566) | Add new builtin constructors to the match arms |

**ClassDB is the big one.** 1024 classes, 16346 methods, 5380 enums — all generated from one JSON file. The regeneration script takes seconds. This should happen on every Godot minor release (4.7, 4.8, etc).

### Tier 2 — Check on Godot Minor Release

These are unlikely to break but should be spot-checked.

| Component | Files | Risk |
|-----------|-------|------|
| **Debug protocol variant types** | `src/debug/variant/mod.rs` | New variant type IDs if Godot adds new built-in types |
| **Lint rules** | `src/lint/rules/godot/` | Rules that check Godot-specific patterns (e.g. `look-at-before-tree` has a hardcoded method list) |
| **.tscn format** | `src/core/scene.rs`, `src/core/resource_parser.rs` | Format version bump (currently `format=3`) |
| **Eval server** | Generated GDScript in `src/build/mod.rs` | Uses `TCPServer`, `FileAccess`, `JSON.stringify` — stable in 4.x but watch for deprecations |

### Tier 3 — Stable Across Godot 4.x

These use dynamic detection or version-agnostic formats.

| Component | Why it's stable |
|-----------|----------------|
| **Godot binary detection** | Searches PATH for `godot`/`godot4`, runs `--version` dynamically |
| **project.godot parsing** | INI format, stable since Godot 3.x |
| **tree-sitter-gdscript** | GDScript syntax has been stable within 4.x; parser handles all 4.0-4.6 syntax |
| **tree-sitter-godot-resource** | .tscn/.tres format stable within 4.x |

---

## Regeneration Checklist (per Godot minor release)

```bash
# 1. Get latest extension_api.json
godot --headless --dump-extension-api
# or download from godot-cpp releases for the target version

# 2. Regenerate ClassDB
python tools/generate_class_db.py extension_api.json > src/class_db/generated.rs

# 3. Check for new builtin types (Vector5? new Variant types?)
# Compare old vs new extension_api.json builtin_class_sizes

# 4. Run tests
./tools/test.sh

# 5. Spot-check
cargo clippy --all-targets -- -D warnings
# If new Godot classes appear in type inference paths, add them
```

Time estimate: 30 minutes for a typical minor release, assuming no syntax changes.

---

## Godot 3.x Support Assessment

### What breaks completely

| Component | Why | Effort |
|-----------|-----|--------|
| **tree-sitter-gdscript** | GDScript 3.x syntax is fundamentally different: `tool` keyword instead of `@tool`, `export` instead of `@export`, `onready` instead of `@onready`, no typed arrays `Array[T]`, no lambdas, `yield` instead of `await` | Need a separate parser or a GDScript3 grammar |
| **ClassDB** | Different class set: `Spatial` not `Node3D`, `KinematicBody` not `CharacterBody3D`, no `Callable`/`Signal` built-in types, ~800 classes vs 1024 | Need separate `generated_3x.rs` from Godot 3's class docs (no `extension_api.json` — would need to scrape XML docs) |
| **Eval server** | Uses Godot 4 APIs: `TCPServer`, `JSON.stringify`, `FileAccess.open`, `PROCESS_MODE_ALWAYS` | Full rewrite of generated GDScript |
| **Debug protocol** | Different variant type IDs, missing types (`Vector4`, `Projection`, `StringName` as variant) | Version-aware codec |
| **Type inference** | Hardcoded Godot 4 constructor names and return types | Separate builtin tables |

### What works as-is or with minor changes

| Component | Status |
|-----------|--------|
| **project.godot parsing** | Works (same INI format) |
| **Binary detection** | Works (already searches for `godot`, `godot3`) |
| **Most lint rules** | ~60 of 76 rules are pure GDScript AST checks, version-agnostic — but they depend on the parser which doesn't support 3.x syntax |
| **.tscn parsing** | Format version 2 vs 3 — similar structure, different metadata (no UIDs) |

### Architecture for multi-version support

If we ever pursue this, the cleanest approach:

```
src/
  godot_version.rs          # Version detection + feature flags
  class_db/
    mod.rs                  # Shared trait/interface
    generated_4x.rs         # Godot 4.x (current)
    generated_3x.rs         # Godot 3.x (future)
  core/
    type_inference.rs        # Version-parameterized builtin tables
    type_inference_3x.rs     # 3.x-specific overrides (future)
```

The key insight: **runtime detection, not compile-time features.** A user might work on both a Godot 3.x and 4.x project. The tool should detect the project's Godot version from `project.godot` (the `config_version` field) and load the appropriate class database.

**Estimated effort for basic Godot 3.x support:**
- Parser: 2-4 weeks (integrate or build a GDScript 3 tree-sitter grammar)
- ClassDB: 1 week (scrape Godot 3 XML docs, generate Rust tables)
- Eval server: 1-2 weeks (rewrite generated GDScript)
- Debug protocol: 1 week (version-aware variant codec)
- Lint/type inference: 1-2 weeks (version-aware builtin tables)
- Testing: 1-2 weeks

**Total: ~2-3 months for core features (fmt, lint, check, run, build, lsp hover/completion/definition). Debug/eval would be the hardest.**

### Is it worth it?

Arguments for:
- Some commercial studios are on Godot 3.x (migration is expensive)
- GDScript 3.x is a simpler language — formatter and linter would be useful
- Differentiator: no other GDScript tooling supports both

Arguments against:
- Godot 3.x is in maintenance mode (3.6 is likely the last release)
- The user base is shrinking as studios migrate to 4.x
- Maintaining two parsers doubles the surface area for bugs
- Most 3.x studios that haven't migrated are likely to have mature internal tooling

**Recommendation: Don't pursue 3.x support unless there's demonstrated demand (e.g., GitHub issues from commercial users). The maintenance burden isn't justified for a shrinking user base.**

---

## Godot 5.x Preparedness

When Godot 5.0 ships (no timeline announced), expect:

1. **GDScript syntax changes** — likely minor (Godot 4 was the big syntax break). Watch for: new annotations, new built-in types, changes to typed arrays or lambdas.

2. **ClassDB changes** — regenerate `generated.rs`. Some classes may be removed/renamed (Godot 4.0 renamed dozens from 3.x).

3. **Debug protocol** — may add new variant types or change message format. Check the `servers/debugger/` source in godot-cpp.

4. **Scene format** — may bump to `format=4`. The tree-sitter grammar would need updating.

### Third-party parser risk

We depend on two community-maintained tree-sitter grammars:
- `tree-sitter-gdscript` (crate version 6.1, repo: [PrestonKnopp/tree-sitter-gdscript](https://github.com/PrestonKnopp/tree-sitter-gdscript))
- `tree-sitter-godot-resource` (crate version 0.7, repo: [PrestonKnopp/tree-sitter-godot-resource](https://github.com/PrestonKnopp/tree-sitter-godot-resource))

**If these maintainers don't update for Godot 5.x syntax/format changes, we'll need to fork and vendor them.** This is the highest-risk dependency in the project — every feature (fmt, lint, LSP, scene commands) depends on these parsers.

Vendoring plan if needed:
1. Fork the grammar repos
2. Replace crate dependencies with `path = "vendor/tree-sitter-gdscript"` in `Cargo.toml`
3. Apply syntax patches ourselves (tree-sitter grammars are typically a single `grammar.js` + generated C parser)
4. Regenerate the C parser with `tree-sitter generate` after grammar changes

This is manageable but adds ongoing maintenance. Tree-sitter grammars are well-documented and the GDScript grammar is ~1500 lines of `grammar.js`. The resource grammar is simpler (~500 lines).

**Mitigation:** Monitor the upstream repos. If they go dormant before Godot 5.x ships, proactively fork early so we're not blocked on release day.

**Prep steps we can take now:**
- Add `config_version` detection in `project.rs` (already partially there)
- Add a `--godot-version` override flag for cases where auto-detection fails
- Keep ClassDB generation scripted (already done)
- Keep variant type handling extensible (already uses match arms, easy to add)

**Estimated effort for Godot 5.x support:** 1-2 weeks once 5.0 ships (mostly ClassDB regeneration + testing).

---

## Hardcoded Godot Knowledge Locations

Quick reference for "where do I need to update?" when Godot changes:

| What | File | Line hint | Notes |
|------|------|-----------|-------|
| All classes/methods/enums | `src/class_db/generated.rs` | Entire file | Auto-generated, don't edit manually |
| Builtin constructor names | `src/core/type_inference.rs` | `is_builtin_constructor` | `Vector2`, `Vector3`, `Color`, etc. |
| Builtin function return types | `src/core/type_inference.rs` | `infer_builtin_function_type` | `abs`, `clamp`, `len`, etc. |
| Builtin type docs | `src/lsp/builtins.rs` | Top-level arrays | Hover docs for types + functions |
| Variant type IDs | `src/debug/variant/mod.rs` | `TYPE_*` constants | Must match Godot's `Variant::Type` enum |
| Tree-dependent methods | `src/class_db/mod.rs` | `is_tree_dependent_method` | `look_at`, `get_node`, etc. |
| Eval server GDScript | `src/build/mod.rs` | `EVAL_SERVER_SCRIPT` | Generated GDScript using Godot 4 APIs |
| GUT addon paths | `src/cli/test_cmd/gut.rs` | Path constants | `addons/gut/gut_cmdln.gd` |
| Scene format version | `src/core/scene.rs` | Format checks | Currently expects `format=3` |
