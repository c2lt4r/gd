# Godot Binary Debug Protocol — Complete Reference

**Source**: Godot Engine `master` branch (`godotengine/godot`)
**Wire format**: `[4-byte LE uint32 payload_size][Variant-encoded Array]`
**Message structure**: `Array[0]` = String command, `Array[1]` = Array of arguments
**Default port**: 6007 (TCP), configurable via `--remote-debug`
**Buffer limits**: 8 MiB per message, 144 Hz poll rate, background thread
**Connection**: TCP with retry (6 attempts: 1ms, 10ms, 100ms, 1000ms, 1000ms, 1000ms)

---

## Message Routing

`RemoteDebugger` routes incoming commands by splitting on the first `:` character:

- **No colon** (e.g. `"step"`) — routed to `_core_capture` in `RemoteDebugger`
- **Has colon** (e.g. `"scene:request_scene_tree"`) — prefix before `:` is the capture name, remainder is the sub-command. Routed via `EngineDebugger::capture_parse(capture_name, sub_command, args)`
- **Special**: `"profiler:<name>"` routes to `_profiler_capture` which enables/disables the named profiler

### Registered Captures

| Capture Name | Registered By | Source File |
|---|---|---|
| `core` | `RemoteDebugger` constructor | `core/debugger/remote_debugger.cpp` |
| `profiler` | `RemoteDebugger` constructor | `core/debugger/remote_debugger.cpp` |
| `scene` | `SceneDebugger::initialize()` | `scene/debugger/scene_debugger.cpp` |
| `servers` | `ServersDebugger::initialize()` | `servers/debugger/servers_debugger.cpp` |
| `multiplayer` | `MultiplayerDebugger::initialize()` | `modules/multiplayer/multiplayer_debugger.cpp` |

### Registered Profilers

| Profiler Name | Class | Source |
|---|---|---|
| `performance` | `PerformanceProfiler` | `core/debugger/remote_debugger.cpp` |
| `servers` | `ServersProfiler` | `servers/debugger/servers_debugger.cpp` |
| `visual` | `VisualProfiler` | `servers/debugger/servers_debugger.cpp` |
| `multiplayer:bandwidth` | `BandwidthProfiler` | `modules/multiplayer/multiplayer_debugger.cpp` |
| `multiplayer:rpc` | `RPCProfiler` | `modules/multiplayer/multiplayer_debugger.cpp` |
| `multiplayer:replication` | `ReplicationProfiler` | `modules/multiplayer/multiplayer_debugger.cpp` |

---

## 1. Core Commands (no prefix)

Source: `core/debugger/remote_debugger.cpp`

Commands are handled in two contexts: the blocking `debug()` loop (when stopped at a breakpoint) and `_core_capture()` (when the game is running).

### Commands available ONLY inside the `debug()` loop

These require the game to be stopped at a breakpoint. Sending them while the game is running has no effect.

| Command | Wire String | Arguments | Behavior |
|---|---|---|---|
| Step Into | `"step"` | none | Sets depth=-1, lines_left=1. Breaks at next statement in any function. |
| Step Over | `"next"` | none | Sets depth=0, lines_left=1. Breaks at next statement at same or shallower call depth. |
| Step Out | `"out"` | none | Sets depth=1, lines_left=1. Breaks when returning to caller. |
| Continue | `"continue"` | none | Sets depth=-1, lines_left=-1. Runs until next breakpoint or error. |
| Break (while stopped) | `"break"` | none | No-op. Prints error "already broke". |
| Get Stack Dump | `"get_stack_dump"` | none | Responds with `"stack_dump"` message containing serialized `ScriptStackDump`. |
| Get Stack Frame Vars | `"get_stack_frame_vars"` | `[0]`: int frame_level | Responds with `"stack_frame_vars"` header then N x `"stack_frame_var"` messages. |
| Evaluate Expression | `"evaluate"` | `[0]`: String expression, `[1]`: int frame_index | Evaluates via `Expression::execute()` in the given stack frame context. Responds with `"evaluation_return"`. |

### Commands available via `_core_capture()` (game running)

These can be sent at any time, whether or not the game is at a breakpoint.

| Command | Wire String | Arguments | Behavior |
|---|---|---|---|
| Break | `"break"` | none | Forces debug break. Calls `script_debugger->debug(break_language)`. Game enters `debug()` loop. |
| Reload Scripts | `"reload_scripts"` | `[0..N]`: Array of script paths | Queues scripts for reload (processed at next idle poll). |
| Reload All Scripts | `"reload_all_scripts"` | none | Sets flag to reload all scripts on next poll. |
| Set Breakpoint | `"breakpoint"` | `[0]`: String script_path, `[1]`: int line, `[2]`: bool set | If `set=true`, inserts breakpoint via `insert_breakpoint(line, source)`. If `false`, removes it. |
| Skip Breakpoints | `"set_skip_breakpoints"` | `[0]`: bool skip | Toggles whether all breakpoints are globally skipped. |
| Ignore Error Breaks | `"set_ignore_error_breaks"` | `[0]`: bool ignore | Toggles whether error conditions trigger debug breaks. |

**Note**: `breakpoint`, `set_skip_breakpoints`, `set_ignore_error_breaks`, `reload_scripts`, and `reload_all_scripts` are also available inside the `debug()` loop (they appear in both contexts).

### Breakpoint internals

- **Storage**: `HashMap<int, HashSet<StringName>> breakpoints` — key=line, value=set of source paths
- **Checking**: GDScript VM's `OPCODE_LINE` calls `is_breakpoint(current_line, source)` where `source` = script's `get_path()` (returns `res://` path)
- **Path matching**: `StringName` comparison — case-sensitive, no normalization. The path sent in `breakpoint` must exactly match what the VM uses.
- **No confirmation**: There is no "breakpoint verified" response. The client cannot know whether Godot accepted the breakpoint.

### Evaluate internals

Pipeline in `remote_debugger.cpp` when `"evaluate"` is received:

1. Gets `ScriptInstance` at breakpoint via `debug_get_stack_level_instance(frame)`
2. Collects **local variables** from VM stack (`debug_get_stack_level_locals`)
3. Collects **GDScript globals** (`debug_get_globals`)
4. Collects **engine singletons** (exposed ClassDB types + autoloads)
5. Collects **user global classes** (class_name scripts from ScriptServer)
6. Runs `Expression.parse(expr, input_names)` then `Expression.execute(input_vals, breaked_instance->get_owner())`
7. Sends `"evaluation_return"` with result variant

**Key details**:
- Member variables (`self.x`) are NOT in `input_names`. They resolve through `Expression.execute()`'s `p_base` parameter (`breaked_instance->get_owner()`). Unresolved identifiers become `self.identifier` lookups.
- The `"break"` command pauses the engine but does NOT provide GDScript script context. `get_stack_level_instance()` returns null, so evaluate returns null. Only breakpoints that fire during GDScript VM execution provide `breaked_instance`.
- **Error gap**: RemoteDebugger does NOT check `Expression.has_execute_failed()` or send `Expression.get_error_text()`. Client receives null for both "expression evaluated to null" and "parse/execute failed".

### Expression class capabilities

`Expression` (`core/math/expression.cpp`) is a standalone evaluator, completely separate from GDScript. It has its own parser.

**Supported**:
- Arithmetic: `(2 + 4) * 16`, `2 ** 3`
- Comparison: `x > 5`, `x == 10`, `x in [1,2,3]`
- Boolean: `true && false`, `not x`, `a or b`
- Bitwise: `x & 0xFF`, `x << 2`
- Constructors: `Vector2(1, 2)`, `Color(1, 0, 0)`, `Transform3D()`
- Property access: `position.x`, `global_position.y`
- Method calls: `get_node("Player").get_name()`, `queue_free()`
- Built-in functions: `sin(x)`, `lerp(a, b, t)`, `sqrt()`, `abs()` (~114 functions)
- Collections: `[1, 2, 3]`, `{"key": value}`
- Indexing: `arr[0]`, `dict["key"]`
- Singletons: `Input.is_action_pressed("jump")`, `OS.get_ticks_msec()`
- Mutation via methods: `set("speed", 100)`, `set_indexed("position:x", 42)`

**Not supported** (Expression class limitations):
- Assignment (`x = 5`) — `=` is not in the grammar
- Variable declaration (`var x`) — not an expression
- Control flow (`if/for/while/match`) — not expressions
- GDScript sugar (`$Path`, `%Name`) — GDScript-only syntax
- Ternary (`x if c else y`) — Godot bug #84131, #97713 (always evaluates true branch)
- Lambda/closures — not in Expression grammar
- `await`, `yield`, `super()`, `preload()` — GDScript-only
- `as` type casting, `?.` null-safe access — not supported
- Multi-line code — single expression only

---

## 2. Scene Commands (`scene:` prefix)

Source: `scene/debugger/scene_debugger.cpp`

### Scene Tree & Inspection

| Command | Wire String | Arguments | Response |
|---|---|---|---|
| Request Scene Tree | `"scene:request_scene_tree"` | none | Sends `"scene:scene_tree"` with serialized depth-first tree from root |
| Inspect Objects | `"scene:inspect_objects"` | `[0]`: Array of ObjectIDs, `[1]`: bool update_selection | Sends `"scene:inspect_objects"` with serialized object data. If `update_selection=true`, also sends `"remote_objects_selected"` or `"remote_nothing_selected"` |
| Inspect Object (**deprecated**) | `"scene:inspect_object"` | `[0]`: ObjectID | Legacy single-object inspect. Sends `"scene:inspect_object"`. Replaced by `"scene:inspect_objects"` (plural). |
| Clear Selection | `"scene:clear_selection"` | none | Clears runtime node selection |
| Save Node | `"scene:save_node"` | `[0]`: ObjectID, `[1]`: String file_path | Duplicates node, packs as PackedScene, saves to path. Sends `"filesystem:update_file"` |

### Execution Control

| Command | Wire String | Arguments | Behavior |
|---|---|---|---|
| Suspend/Resume | `"scene:suspend_changed"` | `[0]`: bool suspended | Pauses/resumes the scene tree processing. Unlike `"break"`, this freezes the game loop + physics rather than stopping at a debug point. |
| Next Frame | `"scene:next_frame"` | none | Advances one physics frame while the game is suspended |
| Speed Changed | `"scene:speed_changed"` | `[0]`: double time_scale | Sets `Engine.time_scale` |

### Audio

| Command | Wire String | Arguments | Behavior |
|---|---|---|---|
| Mute Audio | `"scene:debug_mute_audio"` | `[0]`: bool mute | Mutes/unmutes all audio buses |

### Camera Override

| Command | Wire String | Arguments | Behavior |
|---|---|---|---|
| Override Cameras | `"scene:override_cameras"` | `[0]`: bool enable, `[1]`: bool from_editor | Enables/disables editor camera override. Must be enabled before transform commands work. |
| Transform Camera 2D | `"scene:transform_camera_2d"` | `[0]`: Transform2D | Sets 2D camera transform in override mode |
| Transform Camera 3D | `"scene:transform_camera_3d"` | `[0]`: Transform3D, `[1]`: bool is_perspective, `[2]`: float size_or_fov, `[3]`: float near, `[4]`: float far | Sets 3D camera transform and projection in override mode |

### Property Manipulation

| Command | Wire String | Arguments | Behavior |
|---|---|---|---|
| Set Property | `"scene:set_object_property"` | `[0]`: ObjectID, `[1]`: String property, `[2]`: Variant value | Sets a property on a remote object |
| Set Property Field | `"scene:set_object_property_field"` | `[0]`: ObjectID, `[1]`: String property, `[2]`: Variant value, `[3]`: String field | Sets a sub-field of a composite property (e.g. `position.x`) |

### Screenshot

| Command | Wire String | Arguments | Response |
|---|---|---|---|
| Request Screenshot | `"scene:rq_screenshot"` | `[0]`: int request_id | Captures viewport, saves PNG. Responds with `"game_view:get_screenshot"` containing `[request_id, width, height, file_path]` |

### Setup

| Command | Wire String | Arguments | Behavior |
|---|---|---|---|
| Setup Scene | `"scene:setup_scene"` | `[0]`: Shortcut (serialized quit key) | Configures debug shortcuts for embedded game view |
| Setup Embedded Shortcuts | `"scene:setup_embedded_shortcuts"` | `[0]`: Dictionary of shortcuts | Configures embedded view keyboard shortcuts |
| Reload Cached Files | `"scene:reload_cached_files"` | `[0]`: PackedStringArray of file paths | Hot-reloads modified resource files at runtime |

### Live Editing

Live editing modifies the running game's scene tree in real time. It uses a separate integer ID system from the ObjectID system used by inspection commands.

**Setup**: `live_set_root` must be called first to establish the root context. Then `live_node_path` / `live_res_path` map node/resource paths to integer IDs. All subsequent `live_*` commands use these integer IDs.

| Command | Wire String | Arguments |
|---|---|---|
| Set Live Root | `"scene:live_set_root"` | `[0]`: NodePath scene_path, `[1]`: String scene_filename |
| Cache Node Path | `"scene:live_node_path"` | `[0]`: NodePath path, `[1]`: int node_id |
| Cache Resource Path | `"scene:live_res_path"` | `[0]`: String resource_path, `[1]`: int resource_id |
| Set Node Property (value) | `"scene:live_node_prop"` | `[0]`: int node_id, `[1]`: String property, `[2]`: Variant value |
| Set Node Property (resource) | `"scene:live_node_prop_res"` | `[0]`: int node_id, `[1]`: String property, `[2]`: String resource_path |
| Set Resource Property (value) | `"scene:live_res_prop"` | `[0]`: int resource_id, `[1]`: String property, `[2]`: Variant value |
| Set Resource Property (resource) | `"scene:live_res_prop_res"` | `[0]`: int resource_id, `[1]`: String property, `[2]`: String resource_path |
| Call Node Method | `"scene:live_node_call"` | `[0]`: int node_id, `[1]`: String method, `[2+]`: Variant args... |
| Call Resource Method | `"scene:live_res_call"` | `[0]`: int resource_id, `[1]`: String method, `[2+]`: Variant args... |
| Create Node | `"scene:live_create_node"` | `[0]`: NodePath parent, `[1]`: String class_type, `[2]`: String name |
| Instantiate Node | `"scene:live_instantiate_node"` | `[0]`: NodePath parent, `[1]`: String scene_path, `[2]`: String name |
| Remove Node | `"scene:live_remove_node"` | `[0]`: NodePath path |
| Remove and Keep Node | `"scene:live_remove_and_keep_node"` | `[0]`: NodePath path, `[1]`: ObjectID keep_id |
| Restore Node | `"scene:live_restore_node"` | `[0]`: ObjectID, `[1]`: NodePath path, `[2]`: int position_index |
| Duplicate Node | `"scene:live_duplicate_node"` | `[0]`: NodePath path, `[1]`: String new_name |
| Reparent Node | `"scene:live_reparent_node"` | `[0]`: NodePath source, `[1]`: NodePath dest, `[2]`: String new_name, `[3]`: int position (-1 = append) |

### Runtime Node Selection

These control interactive node selection in the game viewport (used by the editor's embedded game view).

| Command | Wire String | Arguments |
|---|---|---|
| Setup Selection | `"scene:runtime_node_select_setup"` | `[0]`: Dictionary config |
| Set Selection Type | `"scene:runtime_node_select_set_type"` | `[0]`: int NodeType enum |
| Set Selection Mode | `"scene:runtime_node_select_set_mode"` | `[0]`: int SelectMode enum |
| Set Selection Visible | `"scene:runtime_node_select_set_visible"` | `[0]`: bool visible |
| Avoid Locked Nodes | `"scene:runtime_node_select_set_avoid_locked"` | `[0]`: bool avoid |
| Prefer Group Selection | `"scene:runtime_node_select_set_prefer_group"` | `[0]`: bool prefer |
| Reset 2D Camera | `"scene:runtime_node_select_reset_camera_2d"` | none |
| Reset 3D Camera | `"scene:runtime_node_select_reset_camera_3d"` | none |

---

## 3. Servers Commands (`servers:` prefix)

Source: `servers/debugger/servers_debugger.cpp`

| Command | Wire String | Arguments | Response |
|---|---|---|---|
| Memory Usage | `"servers:memory"` | none | Sends `"servers:memory_usage"` with serialized ResourceUsage (textures, meshes, VRAM bytes, types, formats) |
| Force Draw | `"servers:draw"` | none | Synchronizes RenderingServer, forces redraw with delta calculation. Sends `"servers:drawn"` when complete. |
| Foreground | `"servers:foreground"` | none | Moves game window to foreground, resets draw timer, skips next profiler frame |

---

## 4. Profiler Commands (`profiler:` prefix)

Source: `core/debugger/remote_debugger.cpp` (`_profiler_capture`)

The `profiler:` capture enables/disables profilers by name. The wire command is `"profiler:<profiler_name>"`.

| Wire String | Profiler Class | Arguments | Data Stream |
|---|---|---|---|
| `"profiler:performance"` | `PerformanceProfiler` | `[0]`: bool enable | `"performance:profile_frame"` every ~1s |
| `"profiler:servers"` | `ServersProfiler` | `[0]`: bool enable, `[1]`: (optional) Array options | `"servers:profile_frame"` per frame |
| `"profiler:visual"` | `VisualProfiler` | `[0]`: bool enable | `"visual:profile_frame"` per frame |
| `"profiler:multiplayer:bandwidth"` | `BandwidthProfiler` | `[0]`: bool enable | `"multiplayer:bandwidth"` per tick |
| `"profiler:multiplayer:rpc"` | `RPCProfiler` | `[0]`: bool enable | `"multiplayer:rpc"` per tick |
| `"profiler:multiplayer:replication"` | `ReplicationProfiler` | `[0]`: bool enable | `"multiplayer:syncs"` per tick |

---

## 5. Multiplayer Commands (`multiplayer:` prefix)

Source: `modules/multiplayer/multiplayer_debugger.cpp`

| Command | Wire String | Arguments | Response |
|---|---|---|---|
| Cache | `"multiplayer:cache"` | Array of ObjectIDs | Sends `"multiplayer:cache"` with `[ObjectID, class_name, path]` tuples |

---

## 6. Outbound Messages (Game to Client)

These are messages sent FROM the running game TO the debugger client via `send_message()`. The client does not request these — they arrive asynchronously or as responses to inbound commands.

### Core Messages

| Message | Data | When Sent |
|---|---|---|
| `"set_pid"` | `[process_id]` | Immediately on connection (constructor) |
| `"debug_enter"` | `[can_continue, error_string, has_stack, thread_id]` | When entering debug break (breakpoint hit, `"break"` command, or error) |
| `"debug_exit"` | `[]` | When leaving debug break (step/continue) |
| `"stack_dump"` | Serialized `ScriptStackDump` (array of frames with file, line, function) | Response to `"get_stack_dump"` |
| `"stack_frame_vars"` | `[num_locals, num_members, num_globals]` | Header before `"stack_frame_var"` messages |
| `"stack_frame_var"` | Serialized `ScriptStackVariable` (name, value, type, var_type) | One per variable, follows `"stack_frame_vars"` header |
| `"evaluation_return"` | Serialized `ScriptStackVariable` with result | Response to `"evaluate"` |
| `"output"` | `[strings_array, types_array]` | Buffered print output. Types: LOG=0, ERROR=1, LOG_RICH=2 |
| `"error"` | Serialized `OutputError` | When error/warning occurs during execution |

### Performance Messages

| Message | Data | When Sent |
|---|---|---|
| `"performance:profile_names"` | `[custom_monitor_names, custom_monitor_types]` | When performance monitor set changes |
| `"performance:profile_frame"` | Array of monitor values | Every ~1 second when performance profiler enabled |

### Scene Messages

| Message | Data | When Sent |
|---|---|---|
| `"scene:scene_tree"` | Serialized depth-first tree | Response to `"scene:request_scene_tree"` |
| `"scene:inspect_objects"` | Serialized object property data | Response to `"scene:inspect_objects"` |
| `"scene:inspect_object"` | Serialized single object data | Legacy response (deprecated) |
| `"remote_objects_selected"` | Array of serialized objects | When runtime selection changes (in-game click) |
| `"remote_nothing_selected"` | `[]` | When in-game selection cleared |
| `"remote_selection_invalidated"` | Array of invalid ObjectIDs | When previously selected objects are freed |
| `"show_selection_limit_warning"` | `[]` | When too many items selected in runtime picker |
| `"request_quit"` | `[]` | When game requests quit (e.g. embedded shortcut) |
| `"request_embed_next_frame"` | `[]` | When embedded next-frame shortcut pressed |
| `"request_embed_suspend_toggle"` | `[]` | When embedded suspend shortcut pressed |
| `"filesystem:update_file"` | `[file_path]` | After `"scene:save_node"` completes |
| `"game_view:get_screenshot"` | `[request_id, width, height, file_path]` | Response to `"scene:rq_screenshot"` |

### Servers Messages

| Message | Data | When Sent |
|---|---|---|
| `"servers:memory_usage"` | Serialized ResourceUsage | Response to `"servers:memory"` |
| `"servers:drawn"` | `[]` | After forced draw completes |
| `"servers:function_signature"` | Function signature data | During profiler initialization |
| `"servers:profile_frame"` | Serialized frame timing data | Per-frame when servers profiler active |
| `"servers:profile_total"` | Accumulated profiling totals | When servers profiler disabled |
| `"visual:hardware_info"` | `[processor_name, video_adapter_name]` | When visual profiler first enabled |
| `"visual:profile_frame"` | Serialized `VisualProfilerFrame` (frame_number, areas with CPU/GPU ms) | Per-frame when visual profiler active |

### Multiplayer Messages

| Message | Data | When Sent |
|---|---|---|
| `"multiplayer:cache"` | `[ObjectID, class, path]` tuples | Response to `"multiplayer:cache"` command |
| `"multiplayer:bandwidth"` | `[incoming_bytes, outgoing_bytes]` | Per-tick when bandwidth profiler active |
| `"multiplayer:rpc"` | Serialized RPCFrame | Per-tick when RPC profiler active |
| `"multiplayer:syncs"` | Serialized ReplicationFrame | Per-tick when replication profiler active |

---

## 7. Wire Format — Variant Types

All data on the wire is encoded using Godot's binary variant format.

| Type ID | Type | Encoding |
|---|---|---|
| 0 | Nil | (empty) |
| 1 | Bool | 4-byte int (0 or 1) |
| 2 | Int | i32 or i64 (with `ENCODE_FLAG_64`) |
| 3 | Float | f32 or f64 (with `ENCODE_FLAG_64`) |
| 4 | String | 4-byte len + UTF-8 + pad to 4-byte alignment |
| 5 | Vector2 | 2x f64 |
| 6 | Vector2i | 2x i32 |
| 7 | Rect2 | 4x f64 |
| 8 | Rect2i | 4x i32 |
| 9 | Vector3 | 3x f64 |
| 10 | Vector3i | 3x i32 |
| 11 | Transform2D | 6x f64 |
| 12 | Vector4 | 4x f64 |
| 13 | Vector4i | 4x i32 |
| 14 | Plane | 4x f64 |
| 15 | Quaternion | 4x f64 |
| 16 | AABB | 6x f64 |
| 17 | Basis | 9x f64 |
| 18 | Transform3D | 12x f64 |
| 19 | Projection | 16x f64 |
| 20 | Color | 4x f32 (RGBA) |
| 21 | StringName | same as String |
| 22 | NodePath | special: name count + subname count + flag + names |
| 23 | RID | u64 |
| 24 | Object | class_name + properties (or just ID with `ENCODE_FLAG_OBJECT_AS_ID`) |
| 25 | Callable | (not fully serializable) |
| 26 | Signal | object_id + signal_name |
| 27 | Dictionary | 4-byte count + key/value pairs |
| 28 | Array | 4-byte count + elements |
| 29-38 | PackedArrays | Byte, Int32, Int64, Float32, Float64, String, Vector2, Vector3, Color, Vector4 |

**Encoding flags** (OR'd into type ID):
- `ENCODE_FLAG_64` (1 << 16): 64-bit int/float encoding
- `ENCODE_FLAG_OBJECT_AS_ID` (1 << 16): Send object as ID only (mutually exclusive with above, same bit but different type contexts)

### Serialization Structures

From `core/debugger/debugger_marshalls.h`:

- **ScriptStackVariable**: `{name: String, value: Variant, type: int, var_type: int}` — serialized with 1 MiB max size
- **ScriptStackDump**: `{frames: List<StackInfo>}` — each frame has `file`, `line`, `function`
- **OutputError**: `{hr, min, sec, msec: int, source_file: String, source_func: String, source_line: int, error: String, error_descr: String, warning: bool, callstack: Vector<StackInfo>}`

---

## 8. Deprecated & Legacy Commands

| Command | Status | Replacement |
|---|---|---|
| `"scene:inspect_object"` (singular) | Deprecated in handler registration | `"scene:inspect_objects"` (plural, accepts array of ObjectIDs) |
| `"scene:click_ctrl"` | Legacy — appears in editor's inbound parser but no corresponding `send_message` in engine | Likely removed |
| `"window:title"` | Registered as inbound handler on editor side but no sender in engine source | Possibly removed or reserved |

---

## 9. Command Count Summary

| Subsystem | Inbound Commands | Outbound Messages |
|---|---|---|
| Core (no prefix) | 12 | 9 |
| Scene (`scene:`) | 39 | 12 |
| Servers (`servers:`) | 3 | 6 |
| Profiler (`profiler:`) | 6 (one per profiler) | 0 (profilers send via own channels) |
| Multiplayer (`multiplayer:`) | 1 | 4 |
| **Total** | **61 inbound** | **31 outbound** |

---

## Godot Source References

- `core/debugger/remote_debugger.cpp` — `debug()` loop, `_core_capture`, evaluate, breakpoints, stack dump
- `core/debugger/remote_debugger.h` — message queue, profiler registration
- `core/debugger/engine_debugger.cpp` — capture routing, profiler management
- `core/debugger/script_debugger.h` — `is_breakpoint()`, `insert_breakpoint()`, HashMap storage
- `core/debugger/debugger_marshalls.h` — ScriptStackVariable, ScriptStackDump, OutputError serialization
- `core/math/expression.cpp` — Expression class (standalone evaluator used by `evaluate`)
- `scene/debugger/scene_debugger.cpp` — `scene:*` command handlers, live editing, camera override
- `servers/debugger/servers_debugger.cpp` — `servers:*` handlers, ServersProfiler, VisualProfiler
- `modules/multiplayer/multiplayer_debugger.cpp` — `multiplayer:*` handlers, bandwidth/RPC/replication profilers
- `modules/gdscript/gdscript_vm.cpp` — `OPCODE_LINE`, breakpoint checking during bytecode execution
- `modules/gdscript/gdscript.cpp` — `_get_debug_path()` returns the source path used in breakpoint matching
