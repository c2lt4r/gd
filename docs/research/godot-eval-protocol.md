# Godot evaluation_return Protocol

Source: `core/debugger/remote_debugger.cpp` + `core/debugger/debugger_marshalls.cpp`

## Sending side (remote_debugger.cpp)

```cpp
Expression expression;
expression.parse(expression_str, input_names);
const Variant return_val = expression.execute(input_vals, breaked_instance->get_owner());

DebuggerMarshalls::ScriptStackVariable stvar;
stvar.name = expression_str;
stvar.value = return_val;
stvar.type = 3;  // always 3 for eval results

send_message("evaluation_return", stvar.serialize());
```

## ScriptStackVariable.serialize() (debugger_marshalls.cpp)

Produces an Array with 4 elements:
```
[name, type, value.get_type(), value_variant]
```

- `arr[0]` = String: expression name
- `arr[1]` = Int: type (always 3 for eval)
- `arr[2]` = Int: Variant type ID (e.g., 9 = Vector3, 2 = Int, 3 = Float)
- `arr[3]` = Variant: the actual result value

## Deserialization

```cpp
name = p_arr[0];
type = p_arr[1];
var_type = p_arr[2];
value = p_arr[3];
```

## Bug in our code

`parse_eval_result` was reading `args[2]` as the value — that's the TYPE ID, not the value.
The actual value is at `args[3]`. Fixed to read 4 fields.

## Eval limitations

Godot's Expression class evaluates EXPRESSIONS, not statements:
- `Vector3(1,2,3)` → works (constructor expression)
- `self.scale` → works at breakpoint (property access)
- `self.scale = Vector3(1,20,1)` → FAILS (assignment = statement)
- `node.scale.y = 20.0` → FAILS (assignment)
- `node.set('scale', ...)` → may work if `node` is in scope (method call)

Use `set-prop` / `set-prop-field` for modifying properties, not eval.
