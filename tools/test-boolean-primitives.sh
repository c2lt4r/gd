#!/usr/bin/env bash
# Reproduce boolean primitive tests: cylinder-from-cube and box-from-cylinder.
# Run with a live Godot instance (gd run) already open.
#
# Usage: ./tools/test-boolean-primitives.sh [gd-binary]
#   gd-binary defaults to "cargo run --" for debug builds

set -euo pipefail

GD="${1:-cargo run --}"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "=== Setting up test project in $TMPDIR ==="
$GD new test-mesh --path "$TMPDIR" 2>/dev/null || $GD new test-mesh --path "$TMPDIR"
cd "$TMPDIR/test-mesh"

echo ""
echo "=== Test 1: Cylinder hole in cube ==="
echo "--- mesh init ---"
$GD mesh init --format json

echo "--- create cube ---"
$GD mesh create --from cube --format json

echo "--- add cylinder cutter ---"
$GD mesh add-part --name cutter --from cylinder --format json

echo "--- rotate cylinder 90° on Z (points along X) ---"
$GD mesh rotate --degrees 0,0,90 --part cutter --format json

echo "--- translate cylinder to cut 50% into cube (+X side) ---"
# Cylinder center at (0.5, 0, 0): extends X=0.0 to X=1.0
# Cube is X=-0.5 to +0.5 → penetrates 50%, sticks out 100%
$GD mesh translate --to 0.5,0,0 --part cutter --format json

echo "--- focus body (cube) ---"
$GD mesh focus body

echo "--- boolean subtract: cylinder from cube ---"
$GD mesh boolean --tool cutter --mode subtract --format json

echo "--- info after boolean ---"
$GD mesh info --format json

echo ""
echo "=== Test 2: Box hole in cylinder ==="
echo "--- new project ---"
cd "$TMPDIR"
$GD new test-mesh2 --path "$TMPDIR" 2>/dev/null || true
cd "$TMPDIR/test-mesh2"

echo "--- mesh init ---"
$GD mesh init --format json

echo "--- create cylinder ---"
$GD mesh create --from cylinder --format json

echo "--- add cube cutter ---"
$GD mesh add-part --name box-cutter --from cube --format json

echo "--- scale cube cutter to 0.6 wide, 0.4 tall, 1.2 deep ---"
$GD mesh scale --factor 0.6,0.4,1.2 --part box-cutter --format json

echo "--- translate box-cutter to cut 50% into cylinder (+Z side) ---"
# Cylinder radius 0.5, centered at origin
# box-cutter depth=1.2, center at (0, 0, 0.5): extends Z=-0.1 to Z=1.1
# Penetrates from Z=0.5 to Z=-0.1 (cylinder interior), sticks out Z=0.5 to Z=1.1
$GD mesh translate --to 0,0,0.5 --part box-cutter --format json

echo "--- focus body (cylinder) ---"
$GD mesh focus body

echo "--- boolean subtract: box from cylinder ---"
$GD mesh boolean --tool box-cutter --mode subtract --format json

echo "--- info after boolean ---"
$GD mesh info --format json

echo ""
echo "=== All boolean primitive tests passed ==="
