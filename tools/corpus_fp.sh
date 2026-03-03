#!/usr/bin/env bash
# corpus_fp.sh — Extract and categorize gd check false positives from the Godot corpus.
#
# Usage:
#   ./tools/corpus_fp.sh              # Full report
#   ./tools/corpus_fp.sh --json       # Machine-readable JSON
#   ./tools/corpus_fp.sh --fixable    # Only show fixable categories (not cross-file/grammar)
#   ./tools/corpus_fp.sh --diff       # Compare against saved baseline
#   ./tools/corpus_fp.sh --save       # Save current counts as baseline
#   ./tools/corpus_fp.sh --files      # List every affected file with its error messages

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CORPUS="$PROJECT_ROOT/tests/corpus/godot-4.6.1"
GD="${GD:-gd}"
BASELINE_FILE="$PROJECT_ROOT/docs/corpus-fp-baseline.json"
DIRS=("analyzer/features" "parser/features" "runtime/features")

JSON_MODE=false
FIXABLE_ONLY=false
DIFF_MODE=false
SAVE_MODE=false
FILES_MODE=false

for arg in "$@"; do
    case "$arg" in
        --json) JSON_MODE=true ;;
        --fixable) FIXABLE_ONLY=true ;;
        --diff) DIFF_MODE=true ;;
        --save) SAVE_MODE=true ;;
        --files) FILES_MODE=true ;;
        --help|-h)
            sed -n '2,/^$/p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
    esac
done

if [ ! -d "$CORPUS" ]; then
    echo "Corpus not found: $CORPUS"
    echo "Download with: cd tests/corpus && unzip godot-4.6.1.zip"
    exit 1
fi

# ── Collect all errors ─────────────────────────────────────────────────
ALL_ERRORS=$(mktemp)
trap 'rm -f "$ALL_ERRORS"' EXIT

for dir in "${DIRS[@]}"; do
    (cd "$CORPUS" && "$GD" check "$dir/" 2>&1) | sed 's/\x1b\[[0-9;]*m//g' | grep " error: " >> "$ALL_ERRORS" || true
done

TOTAL=$(wc -l < "$ALL_ERRORS")
TOTAL_FILES=$(awk -F: '{print $1}' "$ALL_ERRORS" | sort -u | wc -l)

# ── Categorize via Python for speed + accuracy ─────────────────────────
python3 - "$ALL_ERRORS" "$BASELINE_FILE" "$JSON_MODE" "$FIXABLE_ONLY" "$DIFF_MODE" "$SAVE_MODE" "$FILES_MODE" "$GD" "$CORPUS" "${DIRS[@]}" <<'PYEOF'
import sys, json, re, subprocess, os
from collections import defaultdict, Counter

errors_file = sys.argv[1]
baseline_file = sys.argv[2]
json_mode = sys.argv[3].lower() == "true"
fixable_only = sys.argv[4].lower() == "true"
diff_mode = sys.argv[5].lower() == "true"
save_mode = sys.argv[6].lower() == "true"
files_mode = sys.argv[7].lower() == "true"
gd_bin = sys.argv[8]
corpus = sys.argv[9]
dirs = sys.argv[10:]

# Known tree-sitter grammar gap files/patterns (from docs/tree-sitter-grammar-gaps.md)
GRAMMAR_GAP_PATTERNS = [
    "unicode", "abstract_methods", "nested_if", "nested_match",
    "number_separator", "match_rest", "lua_dict",
    "dictionary_lua_style",  # Lua-style dict string keys ("b" = 2)
    "class_inheritance_access",  # signal declaration in inner class
    "mixed_indentation_on_blank_lines",  # blank lines with whitespace confuse indentation parser
]

# Files where ALL errors are grammar gaps (even if the filename doesn't match a pattern)
GRAMMAR_GAP_FILES = {
    "warning_ignore_targets.gd",  # Cyrillic А in identifiers
    "warning_ignore_warnings.gd",  # Cyrillic О in identifiers
}

# Files where SOME errors are grammar-gap downstream (e.g. $%Hey parse errors, $/node division)
GRAMMAR_GAP_PARTIAL = {
    "dollar_and_percent_get_node.gd",  # $%node and $/path syntax
}

# Match pattern parse errors in match_*.gd files
MATCH_PATTERN_FILES = {"match.gd", "match_array.gd", "match_dictionary.gd"}

def categorize(filepath, msg):
    """Categorize an error message. Returns (category, subcategory)."""
    fname = os.path.basename(filepath)

    # Grammar gaps — downstream of tree-sitter parse errors
    for pat in GRAMMAR_GAP_PATTERNS:
        if pat in fname.lower():
            return ("grammar", fname)

    # Entire files where all errors are grammar gaps (e.g. Cyrillic identifiers)
    if fname in GRAMMAR_GAP_FILES:
        return ("grammar", fname)

    # Match pattern files — all parse errors are grammar gaps
    if fname in MATCH_PATTERN_FILES and "parse error" in msg:
        return ("grammar", fname)
    # match.gd also has a top-level error downstream of the parse error
    if fname in MATCH_PATTERN_FILES and "at top level" in msg:
        return ("grammar", fname)

    # dollar_and_percent_get_node.gd — $%node parse errors, $/path type mismatches,
    # and downstream identifier errors from unparsed get_node syntax
    if fname in GRAMMAR_GAP_PARTIAL:
        if "parse error" in msg:
            return ("grammar", fname)
        if "invalid operands" in msg and '"/"' in msg:
            return ("grammar", fname)
        if "not declared" in msg:
            return ("grammar", fname)

    # Cross-file identifier: "Utils", PascalCase class names from other files
    if '"Utils"' in msg and "not declared" in msg:
        return ("cross_file_ident", "Utils")

    if "not declared in the current scope" in msg:
        m = re.search(r'"([A-Z][A-Za-z0-9_]*)"', msg)
        if m:
            name = m.group(1)
            # If it looks like a class name (PascalCase, not a known builtin constant),
            # it's likely a cross-file class reference
            if not name.isupper() and name not in ("PI", "TAU", "INF", "NAN"):
                return ("cross_file_ident", name)
        return ("undefined_ident", msg)

    # Cross-file type: unknown type with dots (A.B) or PascalCase not in ClassDB
    if "unknown type" in msg or "could not find type" in msg:
        return ("cross_file_type", msg)

    # Const expression
    if "compile-time constant" in msg or "constant expression" in msg:
        return ("const_expr", msg)

    # Method not found
    if "not found in base" in msg:
        m = re.search(r'"(\w+)\(\)".*base\s+(\w+)', msg)
        sub = m.group(0) if m else msg
        return ("method_not_found", sub)

    # Type mismatches (assignment, return, argument, operator, constructor)
    if any(p in msg for p in ["cannot assign", "invalid argument", "cannot return",
                                "invalid operands", "no constructor"]):
        return ("type_mismatch", msg)

    # Indentation
    if "unexpected indentation" in msg:
        return ("indentation", msg)

    # Arg count
    if "too many arguments" in msg or "too few arguments" in msg:
        return ("arg_count", msg)

    return ("other", msg)


# Parse all errors
errors = []  # list of (filepath, line, col, category, subcategory, full_msg)
cat_counts = Counter()
cat_files = defaultdict(set)      # category -> set of files
cat_messages = defaultdict(Counter)  # category -> Counter of unique messages
file_errors = defaultdict(list)   # filepath -> list of (line, msg)

with open(errors_file) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        # Format: path/file.gd:LINE:COL error: message
        m = re.match(r'^(.+?):(\d+):(\d+)\s+error:\s+(.+)$', line)
        if not m:
            continue
        filepath, lineno, col, msg = m.group(1), int(m.group(2)), int(m.group(3)), m.group(4)
        cat, sub = categorize(filepath, msg)
        errors.append((filepath, lineno, col, cat, sub, msg))
        cat_counts[cat] += 1
        cat_files[cat].add(filepath)
        cat_messages[cat][msg] += 1
        file_errors[filepath].append((lineno, msg))

total = len(errors)
total_files = len(set(e[0] for e in errors))

ALL_CATS = ["cross_file_ident", "cross_file_type", "grammar",
            "const_expr", "method_not_found", "undefined_ident",
            "type_mismatch", "indentation", "arg_count", "other"]
UNFIXABLE = {"grammar"}
FIXABLE_CATS = [c for c in ALL_CATS if c not in UNFIXABLE]

# Per-dir counts (reuse collected data)
per_dir = {}
for d in dirs:
    per_dir[d] = sum(1 for e in errors if e[0].startswith(d.split("/")[0]))

# ── JSON / Save ────────────────────────────────────────────────────────
if json_mode or save_mode:
    data = {
        "total": total,
        "total_files": total_files,
        "categories": {c: cat_counts.get(c, 0) for c in ALL_CATS},
        "per_dir": per_dir,
    }
    if save_mode:
        with open(baseline_file, "w") as f:
            json.dump(data, f, indent=2)
        print(f"Baseline saved to {baseline_file}")
    print(json.dumps(data, indent=2))
    sys.exit(0)

# ── Diff ───────────────────────────────────────────────────────────────
if diff_mode:
    if not os.path.exists(baseline_file):
        print("No baseline found. Run with --save first.")
        sys.exit(1)
    with open(baseline_file) as f:
        baseline = json.load(f)
    print("=== Corpus FP Diff vs Baseline ===\n")
    print(f"{'Category':<25} {'Baseline':>8} {'Current':>8} {'Delta':>8}")
    print(f"{'--------':<25} {'--------':>8} {'-------':>8} {'-----':>8}")
    for cat in ALL_CATS:
        bv = baseline["categories"].get(cat, 0)
        cv = cat_counts.get(cat, 0)
        d = cv - bv
        sign = "+" if d > 0 else ""
        print(f"{cat:<25} {bv:>8} {cv:>8} {sign}{d:>8}")
    bt = baseline["total"]
    d = total - bt
    sign = "+" if d > 0 else ""
    print(f"{'TOTAL':<25} {bt:>8} {total:>8} {sign}{d:>8}")
    sys.exit(0)

# ── Files mode ─────────────────────────────────────────────────────────
if files_mode:
    for filepath in sorted(file_errors.keys()):
        errs = file_errors[filepath]
        cats = Counter()
        for _, msg in errs:
            cat, _ = categorize(filepath, msg)
            cats[cat] += 1
        cat_summary = ", ".join(f"{c}:{n}" for c, n in cats.most_common())
        print(f"\n{filepath} ({len(errs)} errors: {cat_summary})")
        for lineno, msg in sorted(errs):
            print(f"  :{lineno} {msg}")
    sys.exit(0)

# ── Human report ───────────────────────────────────────────────────────
version = subprocess.run([gd_bin, "--version"], capture_output=True, text=True).stdout.strip()
print(f"=== gd check Corpus False Positives ===")
print(f"Binary: {gd_bin} {version}")
print()

# Per-directory counts
print("── Per Directory ──")
for d in dirs:
    count = per_dir.get(d, 0)
    print(f"  {d:<25} {count:>4} errors")
print()

# Category breakdown
fixable_total = sum(cat_counts.get(c, 0) for c in FIXABLE_CATS)
unfixable_total = sum(cat_counts.get(c, 0) for c in UNFIXABLE)

print("── By Category ──")
show_cats = FIXABLE_CATS if fixable_only else ALL_CATS
for cat in show_cats:
    c = cat_counts.get(cat, 0)
    if c == 0:
        continue
    files = len(cat_files.get(cat, set()))
    tag = " [unfixable]" if cat in UNFIXABLE else ""
    print(f"  {cat:<25} {c:>4}  ({files} files){tag}")
print(f"  {'─' * 50}")
print(f"  {'TOTAL':<25} {total:>4}  (fixable: {fixable_total}, unfixable: {unfixable_total})")
print()

# Unique error messages per fixable category
print("── Fixable Error Details (top messages + files) ──")
for cat in FIXABLE_CATS:
    c = cat_counts.get(cat, 0)
    if c == 0:
        continue
    print(f"\n[{cat}] ({c} errors in {len(cat_files[cat])} files)")
    for msg, count in cat_messages[cat].most_common(8):
        print(f"  {count:>4}x  {msg[:100]}")
    print(f"  Files:")
    for fp in sorted(cat_files[cat])[:10]:
        errs_in_file = sum(1 for e in errors if e[0] == fp and e[3] == cat)
        print(f"    {fp} ({errs_in_file})")
    remaining = len(cat_files[cat]) - 10
    if remaining > 0:
        print(f"    ... and {remaining} more files")

print()
print("── Top 20 Files by Error Count ──")
file_counts = Counter(e[0] for e in errors)
for fp, count in file_counts.most_common(20):
    cats = Counter(e[3] for e in errors if e[0] == fp)
    cat_str = ", ".join(f"{c}:{n}" for c, n in cats.most_common(3))
    print(f"  {count:>4}  {fp}  ({cat_str})")
PYEOF
