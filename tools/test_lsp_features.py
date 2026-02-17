#!/usr/bin/env python3
"""Test all new LSP capabilities by driving gd lsp over JSON-RPC/stdio."""

import json
import os
import subprocess
import sys
import tempfile
import shutil

# ── Helpers ──────────────────────────────────────────────────────────────────

def send(proc, method, params, req_id=None):
    """Send a JSON-RPC message to the LSP server."""
    msg = {"jsonrpc": "2.0", "method": method, "params": params}
    if req_id is not None:
        msg["id"] = req_id
    body = json.dumps(msg)
    header = f"Content-Length: {len(body)}\r\n\r\n"
    proc.stdin.write(header.encode() + body.encode())
    proc.stdin.flush()


def recv(proc):
    """Read one JSON-RPC response from the LSP server."""
    # Read headers until blank line
    content_length = 0
    while True:
        line = proc.stdout.readline().decode()
        if not line or line == "\r\n":
            break
        if line.lower().startswith("content-length:"):
            content_length = int(line.split(":")[1].strip())
    if content_length == 0:
        return None
    body = proc.stdout.read(content_length).decode()
    return json.loads(body)


def recv_response(proc, expected_id):
    """Read responses until we get the one matching expected_id."""
    for _ in range(50):  # safety limit
        msg = recv(proc)
        if msg is None:
            return None
        if msg.get("id") == expected_id:
            return msg
        # else it's a notification (diagnostics, log, etc.) — skip
    return None


# ── Test project setup ───────────────────────────────────────────────────────

def create_test_project(tmpdir):
    """Create a minimal Godot project with two GDScript files."""
    # project.godot
    with open(os.path.join(tmpdir, "project.godot"), "w") as f:
        f.write('[gd_resource type="Environment" load_steps=1 format=3]\n')
        f.write("[application]\n")
        f.write('config/name="LSP Test"\n')

    # Base class
    with open(os.path.join(tmpdir, "base.gd"), "w") as f:
        f.write("""\
class_name BaseEntity
extends Node2D

## The entity's health points.
var health: int = 100
var speed := 5.0

signal damage_taken(amount: int)

func take_damage(amount: int) -> void:
\thealth -= amount
\tdamage_taken.emit(amount)

func heal(amount: int) -> void:
\thealth += amount

func _ready() -> void:
\tvar pos := get_global_position()
\tprint(pos)
""")

    # Subclass
    with open(os.path.join(tmpdir, "player.gd"), "w") as f:
        f.write("""\
class_name Player
extends BaseEntity

var armor: int = 0

func take_damage(amount: int) -> void:
\tvar reduced := max(0, amount - armor)
\tsuper.take_damage(reduced)

func equip_armor(value: int) -> void:
\tarmor = value
\tprint("Armor equipped")

func _process(delta: float) -> void:
\tvar direction := Vector2.ZERO
\tmove_and_slide()
""")

    return tmpdir


# ── Tests ────────────────────────────────────────────────────────────────────

PASS = 0
FAIL = 0


def check(name, condition, detail=""):
    global PASS, FAIL
    if condition:
        PASS += 1
        print(f"  \033[32mPASS\033[0m {name}")
    else:
        FAIL += 1
        msg = f"  \033[31mFAIL\033[0m {name}"
        if detail:
            msg += f" — {detail}"
        print(msg)


def uri(tmpdir, filename):
    path = os.path.join(tmpdir, filename).replace("\\", "/")
    if not path.startswith("/"):
        path = "/" + path
    return f"file://{path}"


def run_tests():
    global PASS, FAIL

    # Find gd binary — prefer local build over installed
    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    cargo_target = os.path.join(project_root, "target", "debug", "gd")
    if os.path.exists(cargo_target):
        gd_bin = cargo_target
    elif shutil.which("gd"):
        gd_bin = shutil.which("gd")
    else:
        print("ERROR: 'gd' binary not found. Build with `cargo build` first.")
        sys.exit(1)

    tmpdir = tempfile.mkdtemp(prefix="gd_lsp_test_")
    try:
        create_test_project(tmpdir)
        root_uri = uri(tmpdir, "")

        print(f"Using gd: {gd_bin}")
        print(f"Test project: {tmpdir}")
        print()

        # Start LSP server
        proc = subprocess.Popen(
            [gd_bin, "lsp", "--no-godot-proxy"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        req_id = 0

        def next_id():
            nonlocal req_id
            req_id += 1
            return req_id

        # ── Initialize ───────────────────────────────────────────────────
        print("Initializing LSP server...")
        rid = next_id()
        send(proc, "initialize", {
            "processId": os.getpid(),
            "rootUri": root_uri,
            "capabilities": {},
            "workspaceFolders": [{"uri": root_uri, "name": "test"}],
        }, rid)
        init_resp = recv_response(proc, rid)
        caps = init_resp.get("result", {}).get("capabilities", {})

        send(proc, "initialized", {})

        # Check capabilities were advertised
        print("\n── Capability Registration ──")
        check("inlayHintProvider", caps.get("inlayHintProvider") is not None)
        check("signatureHelpProvider", caps.get("signatureHelpProvider") is not None)
        check("callHierarchyProvider", caps.get("callHierarchyProvider") is not None)
        check("implementationProvider", caps.get("implementationProvider") is not None)
        check("semanticTokensProvider", caps.get("semanticTokensProvider") is not None)
        check("workspaceSymbolProvider", caps.get("workspaceSymbolProvider") is not None)

        sig_opts = caps.get("signatureHelpProvider", {})
        check("signatureHelp triggers ( and ,",
              sig_opts.get("triggerCharacters") == ["(", ","],
              f"got {sig_opts.get('triggerCharacters')}")

        # ── Open documents ───────────────────────────────────────────────
        base_uri = uri(tmpdir, "base.gd")
        player_uri = uri(tmpdir, "player.gd")

        with open(os.path.join(tmpdir, "base.gd")) as f:
            base_src = f.read()
        with open(os.path.join(tmpdir, "player.gd")) as f:
            player_src = f.read()

        send(proc, "textDocument/didOpen", {
            "textDocument": {"uri": base_uri, "languageId": "gdscript", "version": 1, "text": base_src}
        })
        send(proc, "textDocument/didOpen", {
            "textDocument": {"uri": player_uri, "languageId": "gdscript", "version": 1, "text": player_src}
        })

        import time
        time.sleep(0.5)  # let diagnostics settle

        # ── 1. Inlay Hints ───────────────────────────────────────────────
        print("\n── Inlay Hints ──")
        rid = next_id()
        send(proc, "textDocument/inlayHint", {
            "textDocument": {"uri": base_uri},
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 20, "character": 0},
            },
        }, rid)
        resp = recv_response(proc, rid)
        hints = resp.get("result") or []
        check("returns hints", len(hints) > 0, f"got {len(hints)} hints")

        # Check that speed := 5.0 gets a type hint
        hint_labels = [h.get("label", "") if isinstance(h.get("label"), str)
                       else "".join(p.get("value", "") for p in h["label"])
                       if isinstance(h.get("label"), list) else ""
                       for h in hints]
        check("has float type hint for speed := 5.0",
              any("float" in l.lower() for l in hint_labels),
              f"labels: {hint_labels}")

        # ── 2. Signature Help ────────────────────────────────────────────
        print("\n── Signature Help ──")
        # Simulate typing take_damage( — cursor after the open paren
        # In base.gd, line 11: "    health -= amount" — let's use a modified buffer
        modified_base = base_src + "\nfunc test():\n\ttake_damage(\n"
        send(proc, "textDocument/didChange", {
            "textDocument": {"uri": base_uri, "version": 2},
            "contentChanges": [{"text": modified_base}],
        })
        time.sleep(0.2)

        rid = next_id()
        # Cursor at end of "take_damage(" line
        line_num = modified_base.count("\n") - 1
        send(proc, "textDocument/signatureHelp", {
            "textDocument": {"uri": base_uri},
            "position": {"line": line_num, "character": 13},
        }, rid)
        resp = recv_response(proc, rid)
        result = resp.get("result")
        check("returns signature", result is not None and len(result.get("signatures", [])) > 0,
              f"got: {json.dumps(result, indent=2)[:200] if result else 'null'}")

        if result and result.get("signatures"):
            sig = result["signatures"][0]
            check("signature contains 'take_damage'", "take_damage" in sig.get("label", ""),
                  f"label: {sig.get('label')}")
            check("has parameters", sig.get("parameters") is not None and len(sig.get("parameters", [])) > 0)

        # Restore original
        send(proc, "textDocument/didChange", {
            "textDocument": {"uri": base_uri, "version": 3},
            "contentChanges": [{"text": base_src}],
        })

        # ── 3. Call Hierarchy ────────────────────────────────────────────
        print("\n── Call Hierarchy ──")
        # Prepare on take_damage function (line 10 in base.gd: "func take_damage(amount: int) -> void:")
        rid = next_id()
        send(proc, "textDocument/prepareCallHierarchy", {
            "textDocument": {"uri": base_uri},
            "position": {"line": 10, "character": 5},  # on "take_damage"
        }, rid)
        resp = recv_response(proc, rid)
        items = resp.get("result") or []
        check("prepareCallHierarchy returns item", len(items) > 0, f"got {len(items)} items")

        if items:
            item = items[0]
            check("item name is take_damage", item.get("name") == "take_damage",
                  f"got: {item.get('name')}")

            # Outgoing calls
            rid = next_id()
            send(proc, "callHierarchy/outgoingCalls", {"item": item}, rid)
            resp = recv_response(proc, rid)
            outgoing = resp.get("result") or []
            check("outgoing calls found", len(outgoing) > 0,
                  f"got {len(outgoing)} (expect emit call)")

            # Incoming calls
            rid = next_id()
            send(proc, "callHierarchy/incomingCalls", {"item": item}, rid)
            resp = recv_response(proc, rid)
            incoming = resp.get("result") or []
            check("incoming calls found (Player.take_damage calls super)", len(incoming) > 0,
                  f"got {len(incoming)}")

        # ── 4. Go to Implementation ──────────────────────────────────────
        print("\n── Go to Implementation ──")
        # On class_name BaseEntity (line 0, col 11)
        rid = next_id()
        send(proc, "textDocument/implementation", {
            "textDocument": {"uri": base_uri},
            "position": {"line": 0, "character": 11},
        }, rid)
        resp = recv_response(proc, rid)
        result = resp.get("result")
        # Could be a single Location or array
        if isinstance(result, list):
            impl_count = len(result)
        elif isinstance(result, dict) and "uri" in result:
            impl_count = 1
        else:
            impl_count = 0
        check("finds Player as implementation of BaseEntity", impl_count > 0,
              f"got {impl_count} implementations")

        # ── 5. Semantic Tokens ───────────────────────────────────────────
        print("\n── Semantic Tokens ──")
        rid = next_id()
        send(proc, "textDocument/semanticTokens/full", {
            "textDocument": {"uri": base_uri},
        }, rid)
        resp = recv_response(proc, rid)
        result = resp.get("result") or {}
        data = result.get("data") or []
        check("returns semantic tokens", len(data) > 0, f"got {len(data)} token values")
        check("token count is multiple of 5", len(data) % 5 == 0,
              f"{len(data)} values = {len(data)//5} tokens")

        # ── 6. Workspace Symbol Search ───────────────────────────────────
        print("\n── Workspace Symbol Search ──")
        rid = next_id()
        send(proc, "workspace/symbol", {"query": "take_damage"}, rid)
        resp = recv_response(proc, rid)
        symbols = resp.get("result") or []
        check("finds take_damage symbols", len(symbols) > 0, f"got {len(symbols)} symbols")

        if symbols:
            names = [s.get("name") for s in symbols]
            check("both base and player take_damage found", names.count("take_damage") >= 2,
                  f"names: {names}")

        # Fuzzy search
        rid = next_id()
        send(proc, "workspace/symbol", {"query": "armor"}, rid)
        resp = recv_response(proc, rid)
        symbols = resp.get("result") or []
        check("fuzzy search finds armor", len(symbols) > 0, f"got {len(symbols)} symbols")

        # ── Shutdown ─────────────────────────────────────────────────────
        rid = next_id()
        send(proc, "shutdown", None, rid)
        recv_response(proc, rid)
        send(proc, "exit", None)
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()

        # ── Summary ──────────────────────────────────────────────────────
        print(f"\n{'='*50}")
        total = PASS + FAIL
        if FAIL == 0:
            print(f"\033[32mAll {total} checks passed!\033[0m")
        else:
            print(f"\033[31m{FAIL} FAILED\033[0m, {PASS} passed (of {total})")
        return FAIL == 0

    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)
        try:
            proc.kill()
        except Exception:
            pass


if __name__ == "__main__":
    ok = run_tests()
    sys.exit(0 if ok else 1)
