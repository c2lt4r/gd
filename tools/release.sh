#!/usr/bin/env bash
set -euo pipefail

# Release script for gd
# Usage:
#   ./tools/release.sh          # auto-bump patch (0.1.21 -> 0.1.22)
#   ./tools/release.sh 0.2.0    # explicit version
#   ./tools/release.sh --dry-run # preview without writing

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

DRY_RUN=false
NEW_VERSION=""

for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=true ;;
        *) NEW_VERSION="$arg" ;;
    esac
done

# ── Read current version from Cargo.toml ─────────────────────────────────────

CURRENT=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "Current version: $CURRENT"

# ── Determine new version ────────────────────────────────────────────────────

if [ -z "$NEW_VERSION" ]; then
    # Auto-bump patch: 0.1.21 -> 0.1.22
    IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"
    PATCH=$((PATCH + 1))
    NEW_VERSION="${MAJOR}.${MINOR}.${PATCH}"
fi

echo "New version:     $NEW_VERSION"
TAG="v${NEW_VERSION}"

# ── Verify changelog has an entry ────────────────────────────────────────────

if ! grep -q "## \[${NEW_VERSION}\]" CHANGELOG.md; then
    echo ""
    echo "ERROR: No CHANGELOG.md entry for [${NEW_VERSION}]."
    echo "Add an entry like:"
    echo ""
    echo "  ## [${NEW_VERSION}] - $(date +%Y-%m-%d)"
    echo "  ### Added/Changed/Fixed"
    echo "  - ..."
    echo ""
    exit 1
fi

# ── Verify working tree is clean (except expected files) ─────────────────────

DIRTY=$(git diff --name-only HEAD)
if [ -n "$DIRTY" ]; then
    echo ""
    echo "ERROR: Working tree has uncommitted changes:"
    echo "$DIRTY"
    echo "Commit or stash them first."
    exit 1
fi

if $DRY_RUN; then
    echo ""
    echo "[dry-run] Would:"
    echo "  1. Set Cargo.toml version to $NEW_VERSION (if needed)"
    echo "  2. Run cargo fmt --check, clippy, test"
    echo "  3. Commit (if needed), push, tag $TAG, push tag"
    echo "  4. Build release and install to ~/.local/bin/gd"
    exit 0
fi

# ── Update Cargo.toml version (if needed) ────────────────────────────────────

if [ "$CURRENT" = "$NEW_VERSION" ]; then
    echo ""
    echo "Cargo.toml already at $NEW_VERSION, skipping version bump."
    NEEDS_COMMIT=false
else
    sed -i "0,/^version = \".*\"/s/^version = \".*\"/version = \"${NEW_VERSION}\"/" Cargo.toml
    NEEDS_COMMIT=true
fi

# ── Run checks ───────────────────────────────────────────────────────────────

echo ""
echo "Running cargo fmt --check..."
cargo fmt -- --check

echo "Running cargo clippy..."
cargo clippy --all-targets -- -D warnings

echo "Running cargo test..."
cargo test

# ── Commit version bump if needed ────────────────────────────────────────────

if $NEEDS_COMMIT; then
    echo ""
    echo "Updating Cargo.lock..."
    cargo update --workspace

    echo "Committing version bump..."
    git add Cargo.toml Cargo.lock
    git commit -m "Bump version to ${NEW_VERSION}"
fi

# ── Tag ──────────────────────────────────────────────────────────────────────

echo ""
if git rev-parse "$TAG" >/dev/null 2>&1; then
    EXISTING=$(git rev-parse "$TAG")
    if [ "$EXISTING" = "$(git rev-parse HEAD)" ]; then
        echo "Tag $TAG already on HEAD, keeping it."
    else
        echo "Tag $TAG exists on $EXISTING, moving to HEAD..."
        git tag -d "$TAG"
        git tag "$TAG"
    fi
else
    echo "Tagging ${TAG}..."
    git tag "$TAG"
fi

# ── Push ─────────────────────────────────────────────────────────────────────

echo "Pushing..."
git push
git push origin "$TAG" --force

# ── Build release binary ─────────────────────────────────────────────────────

echo ""
echo "Building release binary..."
cargo build --release

echo "Installing to ~/.local/bin/gd..."
cp target/release/gd ~/.local/bin/gd

echo ""
gd --version
echo "Release ${TAG} complete."
