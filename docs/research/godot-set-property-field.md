# Godot scene:set_object_property_field Protocol

Source: `scene/debugger/scene_debugger.cpp` (Godot master)

## Message handler

```cpp
Error SceneDebugger::_msg_set_object_property_field(const Array &p_args) {
    ERR_FAIL_COND_V(p_args.size() < 4, ERR_INVALID_DATA);
    _set_object_property(p_args[0], p_args[1], p_args[2], p_args[3]);
    RuntimeNodeSelect::get_singleton()->_queue_selection_update();
    return OK;
}
```

Args: `[object_id, property_name, value, field_name]`

## _set_object_property implementation

```cpp
void SceneDebugger::_set_object_property(ObjectID p_id, const String &p_property,
        const Variant &p_value, const String &p_field) {
    Object *obj = ObjectDB::get_instance(p_id);
    if (!obj) return;

    String prop_name;
    if (p_property.begins_with("Members/")) {
        prop_name = p_property.get_slicec('/', p_property.get_slice_count("/") - 1);
    } else {
        prop_name = p_property;
    }

    Variant value = p_value;
    if (p_value.is_string() && (obj->get_static_property_type(prop_name) ==
            Variant::OBJECT || p_property == "script")) {
        value = ResourceLoader::load(p_value);
    }

    if (!p_field.is_empty()) {
        value = fieldwise_assign(obj->get(prop_name), value, p_field);
    }

    obj->set(prop_name, value);
}
```

## fieldwise_assign (core/math/math_fieldwise.cpp)

**CRITICAL**: `fieldwise_assign(target, source, field)` casts BOTH target and source to the
same type (e.g. Vector3), then copies ONLY the named field from source to target.

```cpp
#define SETUP_TYPE(m_type) \
    m_type source = p_source; \
    m_type target = p_target;

#define TRY_TRANSFER_FIELD(m_name, m_member) \
    if (p_field == m_name) \
        target.m_member = source.m_member;
```

For Vector3:
```cpp
case Variant::VECTOR3: {
    SETUP_TYPE(Vector3)
    TRY_TRANSFER_FIELD("x", x)
    else TRY_TRANSFER_FIELD("y", y)
    else TRY_TRANSFER_FIELD("z", z)
    return target;
}
```

**Bug implication**: If the value passed is `Float(7.0)`, Godot casts it to `Vector3` → `Vector3(0,0,0)`.
Then `target.y = source.y = 0`. The sub-field is zeroed instead of set to 7.0.

The value MUST be a full Vector3 (or matching type) where the target field has the desired value.
The Godot editor constructs the full modified value before sending.

## Solution

Don't rely on fieldwise_assign. Instead:
1. Inspect the object to get current property value
2. Modify the requested field client-side
3. Use `set_object_property` (without field) to set the whole value
