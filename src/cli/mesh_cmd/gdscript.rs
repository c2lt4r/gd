/// GDScript array literal — 8 distinct medium-saturation colors for auto-assignment.
const PALETTE: &str = "[Color(0.69,0.69,0.69),Color(0.88,0.44,0.31),\
     Color(0.31,0.63,0.88),Color(0.38,0.75,0.38),\
     Color(0.88,0.75,0.31),Color(0.63,0.38,0.75),\
     Color(0.31,0.75,0.69),Color(0.88,0.50,0.56)]";

/// Generate the GDScript for `mesh create`.
#[allow(clippy::too_many_lines)]
pub fn generate_create(name: &str, primitive: &str) -> String {
    // Primitives are built in Rust and pushed via generate_push_script.
    // The GDScript only creates the scene node (no Godot mesh assignment).
    let primitive_size = match primitive {
        "cube" => "[1, 1, 1]",
        "sphere" | "cylinder" => "[2, 2, 2]",
        _ => "[0, 0, 0]",
    };

    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar old = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif old: old.queue_free()\n\
         \tvar helper = Node3D.new()\n\
         \thelper.name = \"_GdMeshHelper\"\n\
         \troot.add_child(helper)\n\
         \tvar rig = Node3D.new()\n\
         \trig.name = \"_CameraRig\"\n\
         \thelper.add_child(rig)\n\
         \tvar cam_front = Camera3D.new()\n\
         \tcam_front.name = \"Front\"\n\
         \tcam_front.projection = Camera3D.PROJECTION_ORTHOGONAL\n\
         \tcam_front.size = 10\n\
         \tcam_front.position = Vector3(0, 0, -20)\n\
         \trig.add_child(cam_front)\n\
         \tcam_front.look_at(Vector3.ZERO)\n\
         \tvar cam_side = Camera3D.new()\n\
         \tcam_side.name = \"Side\"\n\
         \tcam_side.projection = Camera3D.PROJECTION_ORTHOGONAL\n\
         \tcam_side.size = 10\n\
         \tcam_side.position = Vector3(20, 0, 0)\n\
         \trig.add_child(cam_side)\n\
         \tcam_side.look_at(Vector3.ZERO)\n\
         \tvar cam_top = Camera3D.new()\n\
         \tcam_top.name = \"Top\"\n\
         \tcam_top.projection = Camera3D.PROJECTION_ORTHOGONAL\n\
         \tcam_top.size = 10\n\
         \tcam_top.position = Vector3(0, 20, 0)\n\
         \trig.add_child(cam_top)\n\
         \tcam_top.look_at(Vector3.ZERO, Vector3.FORWARD)\n\
         \tvar cam_back = Camera3D.new()\n\
         \tcam_back.name = \"Back\"\n\
         \tcam_back.projection = Camera3D.PROJECTION_ORTHOGONAL\n\
         \tcam_back.size = 10\n\
         \tcam_back.position = Vector3(0, 0, 20)\n\
         \trig.add_child(cam_back)\n\
         \tcam_back.look_at(Vector3.ZERO)\n\
         \tvar cam_left = Camera3D.new()\n\
         \tcam_left.name = \"Left\"\n\
         \tcam_left.projection = Camera3D.PROJECTION_ORTHOGONAL\n\
         \tcam_left.size = 10\n\
         \tcam_left.position = Vector3(-20, 0, 0)\n\
         \trig.add_child(cam_left)\n\
         \tcam_left.look_at(Vector3.ZERO)\n\
         \tvar cam_bottom = Camera3D.new()\n\
         \tcam_bottom.name = \"Bottom\"\n\
         \tcam_bottom.projection = Camera3D.PROJECTION_ORTHOGONAL\n\
         \tcam_bottom.size = 10\n\
         \tcam_bottom.position = Vector3(0, -20, 0)\n\
         \trig.add_child(cam_bottom)\n\
         \tcam_bottom.look_at(Vector3.ZERO, Vector3.FORWARD)\n\
         \tvar cam_fr = Camera3D.new()\n\
         \tcam_fr.name = \"FrontRight\"\n\
         \tcam_fr.fov = 27\n\
         \tcam_fr.position = Vector3(14, 0, -14)\n\
         \trig.add_child(cam_fr)\n\
         \tcam_fr.look_at(Vector3.ZERO)\n\
         \tvar cam_fl = Camera3D.new()\n\
         \tcam_fl.name = \"FrontLeft\"\n\
         \tcam_fl.fov = 27\n\
         \tcam_fl.position = Vector3(-14, 0, -14)\n\
         \trig.add_child(cam_fl)\n\
         \tcam_fl.look_at(Vector3.ZERO)\n\
         \tvar cam_br = Camera3D.new()\n\
         \tcam_br.name = \"BackRight\"\n\
         \tcam_br.fov = 27\n\
         \tcam_br.position = Vector3(14, 0, 14)\n\
         \trig.add_child(cam_br)\n\
         \tcam_br.look_at(Vector3.ZERO)\n\
         \tvar cam_bl = Camera3D.new()\n\
         \tcam_bl.name = \"BackLeft\"\n\
         \tcam_bl.fov = 27\n\
         \tcam_bl.position = Vector3(-14, 0, 14)\n\
         \trig.add_child(cam_bl)\n\
         \tcam_bl.look_at(Vector3.ZERO)\n\
         \tvar cam_hfr = Camera3D.new()\n\
         \tcam_hfr.name = \"HighFrontRight\"\n\
         \tcam_hfr.fov = 27\n\
         \tcam_hfr.position = Vector3(10, 14, -10)\n\
         \trig.add_child(cam_hfr)\n\
         \tcam_hfr.look_at(Vector3.ZERO)\n\
         \tvar cam_hfl = Camera3D.new()\n\
         \tcam_hfl.name = \"HighFrontLeft\"\n\
         \tcam_hfl.fov = 27\n\
         \tcam_hfl.position = Vector3(-10, 14, -10)\n\
         \trig.add_child(cam_hfl)\n\
         \tcam_hfl.look_at(Vector3.ZERO)\n\
         \tvar cam_hbr = Camera3D.new()\n\
         \tcam_hbr.name = \"HighBackRight\"\n\
         \tcam_hbr.fov = 27\n\
         \tcam_hbr.position = Vector3(10, 14, 10)\n\
         \trig.add_child(cam_hbr)\n\
         \tcam_hbr.look_at(Vector3.ZERO)\n\
         \tvar cam_hbl = Camera3D.new()\n\
         \tcam_hbl.name = \"HighBackLeft\"\n\
         \tcam_hbl.fov = 27\n\
         \tcam_hbl.position = Vector3(-10, 14, 10)\n\
         \trig.add_child(cam_hbl)\n\
         \tcam_hbl.look_at(Vector3.ZERO)\n\
         \tvar key_light = DirectionalLight3D.new()\n\
         \tkey_light.name = \"_KeyLight\"\n\
         \tkey_light.rotation_degrees = Vector3(-45, -45, 0)\n\
         \tkey_light.light_energy = 1.0\n\
         \tkey_light.shadow_enabled = true\n\
         \thelper.add_child(key_light)\n\
         \tvar fill_light = DirectionalLight3D.new()\n\
         \tfill_light.name = \"_FillLight\"\n\
         \tfill_light.rotation_degrees = Vector3(0, 45, 0)\n\
         \tfill_light.light_energy = 0.5\n\
         \tfill_light.light_color = Color(0.9, 0.92, 0.95)\n\
         \thelper.add_child(fill_light)\n\
         \tvar rim_light = DirectionalLight3D.new()\n\
         \trim_light.name = \"_RimLight\"\n\
         \trim_light.rotation_degrees = Vector3(-70, 180, 0)\n\
         \trim_light.light_energy = 1.0\n\
         \trim_light.shadow_enabled = true\n\
         \thelper.add_child(rim_light)\n\
         \tvar top_light = DirectionalLight3D.new()\n\
         \ttop_light.name = \"_TopLight\"\n\
         \ttop_light.rotation_degrees = Vector3(-90, 0, 0)\n\
         \ttop_light.light_energy = 0.2\n\
         \thelper.add_child(top_light)\n\
         \tvar cyc = CSGPolygon3D.new()\n\
         \tcyc.name = \"_Cyclorama\"\n\
         \tcyc.mode = CSGPolygon3D.MODE_SPIN\n\
         \tcyc.polygon = PackedVector2Array([Vector2(0,0),Vector2(2000,0),Vector2(2000,50),Vector2(2050,100),Vector2(2050,1000),Vector2(0,1000)])\n\
         \tcyc.spin_degrees = 360.0\n\
         \tcyc.spin_sides = 48\n\
         \tvar cyc_mat = StandardMaterial3D.new()\n\
         \tcyc_mat.albedo_color = Color.html(\"2A2A2A\")\n\
         \tcyc_mat.roughness = 1.0\n\
         \tcyc_mat.metallic_specular = 0.2\n\
         \tcyc.material = cyc_mat\n\
         \thelper.add_child(cyc)\n\
         \tvar probe = ReflectionProbe.new()\n\
         \tprobe.name = \"_ReflectionProbe\"\n\
         \tprobe.size = Vector3(200, 100, 200)\n\
         \tprobe.intensity = 1.0\n\
         \tprobe.max_distance = 200.0\n\
         \thelper.add_child(probe)\n\
         \tvar env_res = Environment.new()\n\
         \tenv_res.background_mode = 1\n\
         \tenv_res.background_color = Color.html(\"0A0A0A\")\n\
         \tenv_res.ambient_light_source = 1\n\
         \tenv_res.ambient_light_color = Color(0.3, 0.3, 0.3)\n\
         \tenv_res.tonemap_mode = 2\n\
         \tenv_res.ssao_enabled = true\n\
         \tenv_res.ssao_radius = 0.3\n\
         \tenv_res.ssao_intensity = 2.0\n\
         \tenv_res.glow_enabled = true\n\
         \tenv_res.glow_intensity = 0.6\n\
         \tenv_res.glow_bloom = 0.1\n\
         \tvar world_env = WorldEnvironment.new()\n\
         \tworld_env.name = \"_MeshEnv\"\n\
         \tworld_env.environment = env_res\n\
         \thelper.add_child(world_env)\n\
         \tvar hud_layer = CanvasLayer.new()\n\
         \thud_layer.name = \"_HudLayer\"\n\
         \thud_layer.layer = 100\n\
         \thelper.add_child(hud_layer)\n\
         \tvar hud_label = Label.new()\n\
         \thud_label.name = \"_HudLabel\"\n\
         \thud_label.text = \"gd mesh create\"\n\
         \thud_label.add_theme_font_size_override(\"font_size\", 16)\n\
         \thud_label.add_theme_color_override(\"font_color\", Color(0.6, 0.8, 1.0, 0.7))\n\
         \thud_label.position = Vector2(10, 10)\n\
         \thud_layer.add_child(hud_label)\n\
         \tvar _af_src = \"extends Node\\n\\nvar _lh := 0.0\\n\\nfunc _process(_d):\\n\"\n\
         \t_af_src += \"\\tvar h := get_parent()\\n\"\n\
         \t_af_src += \"\\tvar combined := AABB()\\n\"\n\
         \t_af_src += \"\\tvar first := true\\n\"\n\
         \t_af_src += \"\\tfor ch in h.get_children():\\n\"\n\
         \t_af_src += \"\\t\\tif ch is MeshInstance3D and not ch.name.begins_with('_') and ch.visible and ch.mesh and ch.mesh.get_surface_count() > 0:\\n\"\n\
         \t_af_src += \"\\t\\t\\tvar ab: AABB = ch.transform * ch.mesh.get_aabb()\\n\"\n\
         \t_af_src += \"\\t\\t\\tif first:\\n\"\n\
         \t_af_src += \"\\t\\t\\t\\tcombined = ab\\n\"\n\
         \t_af_src += \"\\t\\t\\t\\tfirst = false\\n\"\n\
         \t_af_src += \"\\t\\t\\telse:\\n\"\n\
         \t_af_src += \"\\t\\t\\t\\tcombined = combined.merge(ab)\\n\"\n\
         \t_af_src += \"\\tif first: return\\n\"\n\
         \t_af_src += \"\\tvar hv: float = combined.position.x + combined.position.y * 7.0 + combined.size.x * 13.0 + combined.size.y * 17.0 + combined.size.z * 23.0\\n\"\n\
         \t_af_src += \"\\tif abs(hv - _lh) < 0.001: return\\n\"\n\
         \t_af_src += \"\\t_lh = hv\\n\"\n\
         \t_af_src += \"\\tvar center := combined.get_center()\\n\"\n\
         \t_af_src += \"\\tvar dims: Vector3 = combined.size\\n\"\n\
         \t_af_src += \"\\tvar sz: float = max(max(dims.x, dims.y), dims.z) * 1.5\\n\"\n\
         \t_af_src += \"\\tif sz < 0.5: sz = 0.5\\n\"\n\
         \t_af_src += \"\\tvar rg := h.get_node_or_null('_CameraRig')\\n\"\n\
         \t_af_src += \"\\tif not rg: return\\n\"\n\
         \t_af_src += \"\\trg.position = center\\n\"\n\
         \t_af_src += \"\\tfor cam in rg.get_children():\\n\"\n\
         \t_af_src += \"\\t\\tif cam is Camera3D:\\n\"\n\
         \t_af_src += \"\\t\\t\\tif cam.projection == Camera3D.PROJECTION_ORTHOGONAL:\\n\"\n\
         \t_af_src += \"\\t\\t\\t\\tcam.size = sz\\n\"\n\
         \t_af_src += \"\\t\\t\\telse:\\n\"\n\
         \t_af_src += \"\\t\\t\\t\\tvar hf: float = deg_to_rad(cam.fov * 0.5)\\n\"\n\
         \t_af_src += \"\\t\\t\\t\\tvar dt: float = (sz * 0.5) / tan(hf)\\n\"\n\
         \t_af_src += \"\\t\\t\\t\\tif dt < 1.0: dt = 1.0\\n\"\n\
         \t_af_src += \"\\t\\t\\t\\tcam.position = cam.position.normalized() * dt\\n\"\n\
         \t_af_src += \"\\t\\t\\tif cam.name == 'Top' or cam.name == 'Bottom':\\n\"\n\
         \t_af_src += \"\\t\\t\\t\\tcam.look_at(center, Vector3.FORWARD)\\n\"\n\
         \t_af_src += \"\\t\\t\\telse:\\n\"\n\
         \t_af_src += \"\\t\\t\\t\\tcam.look_at(center)\\n\"\n\
         \tvar _af_scr = GDScript.new()\n\
         \t_af_scr.source_code = _af_src\n\
         \t_af_scr.reload()\n\
         \tvar _af_node = Node.new()\n\
         \t_af_node.name = \"_AutoFocus\"\n\
         \t_af_node.set_script(_af_scr)\n\
         \thelper.add_child(_af_node)\n\
         \tvar _ih_src = \"extends Node\\n\\nfunc _input(event):\\n\"\n\
         \t_ih_src += \"\\tif not (event is InputEventKey and event.pressed and not event.echo): return\\n\"\n\
         \t_ih_src += \"\\tvar rig = get_parent().get_node_or_null('_CameraRig')\\n\"\n\
         \t_ih_src += \"\\tif not rig: return\\n\"\n\
         \t_ih_src += \"\\tvar cam_map = {{KEY_1:'Front', KEY_2:'Side', KEY_3:'Top', KEY_4:'Back', KEY_5:'Left', KEY_6:'Bottom', KEY_7:'HighFrontRight', KEY_8:'HighFrontLeft', KEY_9:'HighBackRight', KEY_0:'HighBackLeft'}}\\n\"\n\
         \t_ih_src += \"\\tvar cam_name = cam_map.get(event.keycode, '')\\n\"\n\
         \t_ih_src += \"\\tif cam_name == '': return\\n\"\n\
         \t_ih_src += \"\\tvar cam = rig.get_node_or_null(cam_name)\\n\"\n\
         \t_ih_src += \"\\tif cam: cam.current = true\\n\"\n\
         \tvar _ih_scr = GDScript.new()\n\
         \t_ih_scr.source_code = _ih_src\n\
         \t_ih_scr.reload()\n\
         \tvar _ih_node = Node.new()\n\
         \t_ih_node.name = \"_InputHandler\"\n\
         \t_ih_node.set_script(_ih_scr)\n\
         \thelper.add_child(_ih_node)\n\
         \tvar mesh_inst = MeshInstance3D.new()\n\
         \tmesh_inst.name = \"{name}\"\n\
         \thelper.add_child(mesh_inst)\n\
         \thelper.set_meta(\"active_mesh\", \"{name}\")\n\
         \thelper.set_meta(\"mesh_parts\", [\"{name}\"])\n\
         \thelper.set_meta(\"profile_points\", [])\n\
         \thelper.set_meta(\"profile_plane\", \"\")\n\
         \tvar _palette = {PALETTE}\n\
         \tvar _color = _palette[0]\n\
         \tmesh_inst.set_meta(\"part_color\", _color)\n\
         \tvar _mat = StandardMaterial3D.new()\n\
         \t_mat.albedo_color = _color\n\
         \tmesh_inst.material_override = _mat\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = \"{name}\"\n\
         \td[\"primitive\"] = \"{primitive}\"\n\
         \td[\"default_size\"] = {primitive_size}\n\
         \td[\"vertex_count\"] = 0\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh profile`.
///
/// Sets explicit normals based on the plane to avoid winding-order ambiguity.
pub fn generate_profile(points: &[(f64, f64)], plane: &str) -> String {
    let points_str = points
        .iter()
        .map(|(x, y)| format!("[{x}, {y}]"))
        .collect::<Vec<_>>()
        .join(", ");

    let mapping = match plane {
        "front" => "var v = Vector3(p[0], p[1], 0)",
        "side" => "var v = Vector3(0, p[1], p[0])",
        _ => "var v = Vector3(p[0], 0, p[1])", // top
    };

    let normal = match plane {
        "front" => "Vector3(0, 0, 1)",
        "side" => "Vector3(1, 0, 0)",
        _ => "Vector3(0, 1, 0)", // top
    };

    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar points = [{points_str}]\n\
         \thelper.set_meta(\"profile_points\", points)\n\
         \thelper.set_meta(\"profile_plane\", \"{plane}\")\n\
         \tvar mesh_name = helper.get_meta(\"active_mesh\")\n\
         \tvar mesh_inst = helper.get_node_or_null(mesh_name)\n\
         \tif mesh_inst == null: return \"ERROR: mesh node not found\"\n\
         \tvar pts3d = []\n\
         \tfor p in points:\n\
         \t\t{mapping}\n\
         \t\tpts3d.append(v)\n\
         \tvar pts2d = PackedVector2Array()\n\
         \tfor p in points:\n\
         \t\tpts2d.append(Vector2(p[0], p[1]))\n\
         \tvar indices = Geometry2D.triangulate_polygon(pts2d)\n\
         \tif indices.size() == 0: return \"ERROR: could not triangulate polygon\"\n\
         \tvar face_n = {normal}\n\
         \tvar tri_n = (pts3d[indices[1]] - pts3d[indices[0]]).cross(pts3d[indices[2]] - pts3d[indices[0]])\n\
         \tvar flip = tri_n.dot(face_n) < 0\n\
         \tvar st = SurfaceTool.new()\n\
         \tst.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \tif flip:\n\
         \t\tfor ti in range(0, indices.size(), 3):\n\
         \t\t\tst.set_normal(face_n)\n\
         \t\t\tst.add_vertex(pts3d[indices[ti + 2]])\n\
         \t\t\tst.set_normal(face_n)\n\
         \t\t\tst.add_vertex(pts3d[indices[ti + 1]])\n\
         \t\t\tst.set_normal(face_n)\n\
         \t\t\tst.add_vertex(pts3d[indices[ti]])\n\
         \telse:\n\
         \t\tfor i in indices:\n\
         \t\t\tst.set_normal(face_n)\n\
         \t\t\tst.add_vertex(pts3d[i])\n\
         \tmesh_inst.mesh = st.commit()\n\
         \tvar mat = StandardMaterial3D.new()\n\
         \tmat.cull_mode = BaseMaterial3D.CULL_DISABLED\n\
         \tif mesh_inst.has_meta(\"part_color\"):\n\
         \t\tmat.albedo_color = mesh_inst.get_meta(\"part_color\")\n\
         \tmesh_inst.material_override = mat\n\
         \tmesh_inst.set_meta(\"_profile_points\", points)\n\
         \tmesh_inst.set_meta(\"_profile_plane\", \"{plane}\")\n\
         \tvar d = {{}}\n\
         \td[\"plane\"] = \"{plane}\"\n\
         \td[\"point_count\"] = points.size()\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript to switch to a named camera.
pub fn generate_switch_camera(view_name: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar rig = helper.get_node_or_null(\"_CameraRig\")\n\
         \tif rig == null: return \"ERROR: camera rig not found\"\n\
         \tvar cam = rig.get_node_or_null(\"{view_name}\")\n\
         \tif cam == null: return \"ERROR: camera '{view_name}' not found\"\n\
         \tcam.current = true\n\
         \treturn \"ok\"\n"
    )
}

/// Generate the GDScript to restore original camera (deactivate all mesh cameras).
pub fn generate_restore_camera() -> String {
    "extends Node\n\
     \n\
     func run():\n\
     \tvar root = get_tree().get_root()\n\
     \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
     \tif helper == null: return \"ok\"\n\
     \tvar rig = helper.get_node_or_null(\"_CameraRig\")\n\
     \tif rig == null: return \"ok\"\n\
     \tfor cam in rig.get_children():\n\
     \t\tif cam is Camera3D:\n\
     \t\t\tcam.current = false\n\
     \treturn \"ok\"\n"
        .to_string()
}

/// Generate the GDScript to capture a screenshot via viewport.
///
/// Includes active camera name for debugging camera switch issues.
pub fn generate_capture_screenshot(view: &str, capture_id: u64) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar cam = get_viewport().get_camera_3d()\n\
         \tvar cam_name = cam.name if cam else \"none\"\n\
         \tvar img = get_viewport().get_texture().get_image()\n\
         \tvar path = OS.get_user_data_dir() + \"/gd_mesh_{view}_{capture_id}.png\"\n\
         \timg.save_png(path)\n\
         \tvar d = {{}}\n\
         \td[\"path\"] = path\n\
         \td[\"width\"] = img.get_width()\n\
         \td[\"height\"] = img.get_height()\n\
         \td[\"active_camera\"] = cam_name\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh snapshot`.
///
/// Iterates all `MeshInstance3D` children under the helper (skipping `_`-prefixed
/// internal nodes). Each mesh is saved as a separate `.tres` resource. If there
/// is only one part, the scene root is a single `MeshInstance3D` (backwards
/// compatible). For multiple parts, a `Node3D` root wraps them.
pub fn generate_snapshot(tscn_path: &str) -> String {
    let base_path = tscn_path.replace(".tscn", "");
    format!(
        "extends Node\n\
         \n\
         func _bake_mesh(mi: MeshInstance3D) -> ArrayMesh:\n\
         \tif not mi.mesh or mi.mesh.get_surface_count() == 0: return null\n\
         \tvar arrays = mi.mesh.surface_get_arrays(0)\n\
         \tvar verts = arrays[Mesh.ARRAY_VERTEX]\n\
         \tvar norms = arrays[Mesh.ARRAY_NORMAL]\n\
         \tvar t = mi.transform\n\
         \tif t != Transform3D():\n\
         \t\tfor i in verts.size():\n\
         \t\t\tverts[i] = t * verts[i]\n\
         \t\tif norms:\n\
         \t\t\tfor i in norms.size():\n\
         \t\t\t\tnorms[i] = (t.basis * norms[i]).normalized()\n\
         \t\tarrays[Mesh.ARRAY_VERTEX] = verts\n\
         \t\tif norms: arrays[Mesh.ARRAY_NORMAL] = norms\n\
         \tvar baked = ArrayMesh.new()\n\
         \tbaked.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)\n\
         \treturn baked\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar mesh_children = []\n\
         \tfor child in helper.get_children():\n\
         \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\tif child.mesh and child.mesh.get_surface_count() > 0:\n\
         \t\t\t\tmesh_children.append(child)\n\
         \tif mesh_children.size() == 0: return \"ERROR: no mesh data in any part\"\n\
         \tvar baked_meshes = {{}}\n\
         \tfor mi in mesh_children:\n\
         \t\tvar baked = _bake_mesh(mi)\n\
         \t\tif baked: baked_meshes[mi.name] = baked\n\
         \tvar resources = []\n\
         \tvar materials = {{}}\n\
         \tfor mi in mesh_children:\n\
         \t\tif not baked_meshes.has(mi.name): continue\n\
         \t\tvar res_path = \"{base_path}_\" + mi.name + \".tres\"\n\
         \t\tvar err = ResourceSaver.save(baked_meshes[mi.name], res_path)\n\
         \t\tif err != OK: return \"ERROR: failed to save mesh '\" + mi.name + \"': \" + str(err)\n\
         \t\tresources.append({{\"name\": mi.name, \"resource\": res_path, \"mesh\": baked_meshes[mi.name]}})\n\
         \t\tif mi.material_override:\n\
         \t\t\tvar mat_path = \"{base_path}_\" + mi.name + \"_mat.tres\"\n\
         \t\t\tvar merr = ResourceSaver.save(mi.material_override, mat_path)\n\
         \t\t\tif merr == OK:\n\
         \t\t\t\tmaterials[mi.name] = mat_path\n\
         \tvar scene_root\n\
         \tif resources.size() == 1:\n\
         \t\tvar node = MeshInstance3D.new()\n\
         \t\tnode.name = resources[0][\"name\"]\n\
         \t\tnode.mesh = resources[0][\"mesh\"]\n\
         \t\tif materials.has(node.name):\n\
         \t\t\tnode.material_override = load(materials[node.name])\n\
         \t\tscene_root = node\n\
         \telse:\n\
         \t\tscene_root = Node3D.new()\n\
         \t\tscene_root.name = \"MeshRoot\"\n\
         \t\tfor r in resources:\n\
         \t\t\tvar node = MeshInstance3D.new()\n\
         \t\t\tnode.name = r[\"name\"]\n\
         \t\t\tnode.mesh = r[\"mesh\"]\n\
         \t\t\tif materials.has(r[\"name\"]):\n\
         \t\t\t\tnode.material_override = load(materials[r[\"name\"]])\n\
         \t\t\tscene_root.add_child(node)\n\
         \t\t\tnode.owner = scene_root\n\
         \tvar scene = PackedScene.new()\n\
         \tscene.pack(scene_root)\n\
         \tvar err = ResourceSaver.save(scene, \"{tscn_path}\")\n\
         \tscene_root.queue_free()\n\
         \tif err != OK: return \"ERROR: failed to save scene: \" + str(err)\n\
         \tvar d = {{}}\n\
         \td[\"path\"] = \"{tscn_path}\"\n\
         \td[\"parts\"] = []\n\
         \tfor r in resources:\n\
         \t\td[\"parts\"].append({{\"name\": r[\"name\"], \"resource\": r[\"resource\"]}})\n\
         \td[\"part_count\"] = resources.size()\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh list-vertices`.
///
/// Returns vertex positions as a JSON array. Optionally filtered to a bounding box.
pub fn generate_list_vertices(region: Option<&super::BoundingBox>) -> String {
    let filter = if let Some(((x1, y1, z1), (x2, y2, z2))) = region {
        // Compute min/max for each axis
        let (min_x, max_x) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
        let (min_y, max_y) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
        let (min_z, max_z) = if z1 < z2 { (z1, z2) } else { (z2, z1) };
        format!(
            "\tvar min_b = Vector3({min_x}, {min_y}, {min_z})\n\
             \tvar max_b = Vector3({max_x}, {max_y}, {max_z})\n\
             \tfor i in verts.size():\n\
             \t\tvar v = verts[i]\n\
             \t\tif v.x >= min_b.x and v.x <= max_b.x and v.y >= min_b.y and v.y <= max_b.y and v.z >= min_b.z and v.z <= max_b.z:\n\
             \t\t\tresult.append({{\"index\": i, \"position\": [v.x, v.y, v.z]}})\n"
        )
    } else {
        "\tfor i in verts.size():\n\
         \t\tvar v = verts[i]\n\
         \t\tresult.append({\"index\": i, \"position\": [v.x, v.y, v.z]})\n"
            .to_string()
    };
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar mesh_name = helper.get_meta(\"active_mesh\")\n\
         \tvar mesh_inst = helper.get_node_or_null(mesh_name)\n\
         \tif mesh_inst == null: return \"ERROR: mesh node not found\"\n\
         \tif mesh_inst.mesh == null: return \"ERROR: no mesh data\"\n\
         \tvar arrays = mesh_inst.mesh.surface_get_arrays(0)\n\
         \tvar verts = arrays[Mesh.ARRAY_VERTEX]\n\
         \tvar result = []\n\
         {filter}\
         \tvar d = {{}}\n\
         \td[\"name\"] = mesh_name\n\
         \td[\"total_vertices\"] = verts.size()\n\
         \td[\"returned\"] = result.size()\n\
         \td[\"vertices\"] = result\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh check`.
///
/// Detects floating/disconnected parts by comparing world-space AABBs.
/// A part is "floating" if its expanded AABB doesn't overlap with any other part's AABB.
pub fn generate_check(margin: f64, max_overlap: f64) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar parts = []\n\
         \tfor child in helper.get_children():\n\
         \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\tvar aabb = AABB()\n\
         \t\t\tif child.mesh and child.mesh.get_surface_count() > 0:\n\
         \t\t\t\taabb = child.get_aabb()\n\
         \t\t\t\taabb.position += child.position\n\
         \t\t\tparts.append({{\"name\": String(child.name), \"aabb\": aabb}})\n\
         \tvar margin = {margin}\n\
         \tvar max_overlap = {max_overlap}\n\
         \tvar floating = []\n\
         \tvar connected = []\n\
         \tfor i in parts.size():\n\
         \t\tvar p = parts[i]\n\
         \t\tvar expanded = p.aabb.grow(margin)\n\
         \t\tvar has_neighbor = false\n\
         \t\tfor j in parts.size():\n\
         \t\t\tif i == j: continue\n\
         \t\t\tif expanded.intersects(parts[j].aabb):\n\
         \t\t\t\thas_neighbor = true\n\
         \t\t\t\tbreak\n\
         \t\tif has_neighbor:\n\
         \t\t\tconnected.append(p.name)\n\
         \t\telse:\n\
         \t\t\tfloating.append(p.name)\n\
         \tvar clipping = []\n\
         \tvar embedded = []\n\
         \tfor i in parts.size():\n\
         \t\tfor j in range(i + 1, parts.size()):\n\
         \t\t\tif not parts[i].aabb.intersects(parts[j].aabb): continue\n\
         \t\t\tvar overlap = parts[i].aabb.intersection(parts[j].aabb)\n\
         \t\t\tvar ov = overlap.size.x * overlap.size.y * overlap.size.z\n\
         \t\t\tvar vi = parts[i].aabb.size.x * parts[i].aabb.size.y * parts[i].aabb.size.z\n\
         \t\t\tvar vj = parts[j].aabb.size.x * parts[j].aabb.size.y * parts[j].aabb.size.z\n\
         \t\t\tvar smaller = min(vi, vj)\n\
         \t\t\tif smaller <= 0: continue\n\
         \t\t\tvar pct = (ov / smaller) * 100.0\n\
         \t\t\tif pct > 50.0:\n\
         \t\t\t\tembedded.append({{\"part_a\": parts[i].name, \"part_b\": parts[j].name, \"overlap_percent\": pct}})\n\
         \t\t\telif pct > max_overlap:\n\
         \t\t\t\tclipping.append({{\"part_a\": parts[i].name, \"part_b\": parts[j].name, \"overlap_percent\": pct}})\n\
         \tvar d = {{}}\n\
         \td[\"total_parts\"] = parts.size()\n\
         \td[\"floating\"] = floating\n\
         \td[\"connected\"] = connected\n\
         \td[\"clipping\"] = clipping\n\
         \td[\"embedded\"] = embedded\n\
         \td[\"ok\"] = floating.size() == 0 and clipping.size() == 0 and embedded.size() == 0\n\
         \td[\"margin\"] = margin\n\
         \td[\"max_overlap\"] = max_overlap\n\
         \treturn JSON.stringify(d)\n"
    )
}

#[cfg(test)]
pub fn generate_translate(part: Option<&str>, x: f64, y: f64, z: f64, relative: bool) -> String {
    let target = part.map_or(
        String::from("\tvar name = helper.get_meta(\"active_mesh\")\n"),
        |p| format!("\tvar name = \"{p}\"\n"),
    );
    let position_line = if relative {
        format!("\ttarget.position += Vector3({x}, {y}, {z})\n")
    } else {
        format!("\ttarget.position = Vector3({x}, {y}, {z})\n")
    };
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         {target}\
         \tvar target = helper.get_node_or_null(str(name))\n\
         \tif target == null: return \"ERROR: part '\" + str(name) + \"' not found\"\n\
         {position_line}\
         \tvar d = {{}}\n\
         \td[\"name\"] = name\n\
         \td[\"position\"] = [target.position.x, target.position.y, target.position.z]\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh translate --relative-to`.
///
/// Positions the target part at `ref_part_center + offset`.
#[cfg(test)]
pub fn generate_translate_relative_to(
    part: Option<&str>,
    ref_part: &str,
    x: f64,
    y: f64,
    z: f64,
) -> String {
    let target = part.map_or(
        String::from("\tvar name = helper.get_meta(\"active_mesh\")\n"),
        |p| format!("\tvar name = \"{p}\"\n"),
    );
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         {target}\
         \tvar target = helper.get_node_or_null(str(name))\n\
         \tif target == null: return \"ERROR: part '\" + str(name) + \"' not found\"\n\
         \tvar ref_part = helper.get_node_or_null(\"{ref_part}\")\n\
         \tif ref_part == null: return \"ERROR: reference part '{ref_part}' not found\"\n\
         \tvar ref_aabb = ref_part.transform * ref_part.mesh.get_aabb() if ref_part.mesh else AABB()\n\
         \tvar ref_center = ref_aabb.get_center()\n\
         \ttarget.position = ref_center + Vector3({x}, {y}, {z})\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = name\n\
         \td[\"relative_to\"] = \"{ref_part}\"\n\
         \td[\"ref_center\"] = [snapped(ref_center.x, 0.01), snapped(ref_center.y, 0.01), snapped(ref_center.z, 0.01)]\n\
         \td[\"position\"] = [snapped(target.position.x, 0.01), snapped(target.position.y, 0.01), snapped(target.position.z, 0.01)]\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh rotate`.
#[cfg(test)]
pub fn generate_rotate(part: Option<&str>, rx: f64, ry: f64, rz: f64) -> String {
    let target = part.map_or(
        String::from("\tvar name = helper.get_meta(\"active_mesh\")\n"),
        |p| format!("\tvar name = \"{p}\"\n"),
    );
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         {target}\
         \tvar target = helper.get_node_or_null(str(name))\n\
         \tif target == null: return \"ERROR: part '\" + str(name) + \"' not found\"\n\
         \ttarget.rotation_degrees = Vector3({rx}, {ry}, {rz})\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = name\n\
         \td[\"rotation\"] = [{rx}, {ry}, {rz}]\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh scale`.
#[cfg(test)]
pub fn generate_scale(part: Option<&str>, sx: f64, sy: f64, sz: f64, remap: bool) -> String {
    let target = part.map_or(
        String::from("\tvar name = helper.get_meta(\"active_mesh\")\n"),
        |p| format!("\tvar name = \"{p}\"\n"),
    );
    let remap_code = if remap {
        "\tvar aabb_before = target.transform * target.mesh.get_aabb() if target.mesh else AABB()\n\
         \tvar center_before = aabb_before.get_center()\n"
    } else {
        ""
    };
    let remap_after = if remap {
        "\tif target.mesh:\n\
         \t\tvar aabb_after = target.transform * target.mesh.get_aabb()\n\
         \t\tvar center_after = aabb_after.get_center()\n\
         \t\ttarget.position += center_before - center_after\n"
    } else {
        ""
    };
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         {target}\
         \tvar target = helper.get_node_or_null(str(name))\n\
         \tif target == null: return \"ERROR: part '\" + str(name) + \"' not found\"\n\
         {remap_code}\
         \ttarget.scale = Vector3({sx}, {sy}, {sz})\n\
         {remap_after}\
         \tvar d = {{}}\n\
         \td[\"name\"] = name\n\
         \td[\"scale\"] = [{sx}, {sy}, {sz}]\n\
         \td[\"remap\"] = {remap}\n\
         \tvar pos = target.position\n\
         \td[\"position\"] = [snapped(pos.x, 0.01), snapped(pos.y, 0.01), snapped(pos.z, 0.01)]\n\
         \treturn JSON.stringify(d)\n",
        remap = if remap { "true" } else { "false" },
    )
}

/// Generate the GDScript for `mesh remove-part`.
pub fn generate_remove_part(name: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar target = helper.get_node_or_null(\"{name}\")\n\
         \tif target == null: return \"ERROR: part '{name}' not found\"\n\
         \ttarget.queue_free()\n\
         \tvar parts = helper.get_meta(\"mesh_parts\", [])\n\
         \tvar new_parts = []\n\
         \tfor p in parts:\n\
         \t\tif p != \"{name}\":\n\
         \t\t\tnew_parts.append(p)\n\
         \thelper.set_meta(\"mesh_parts\", new_parts)\n\
         \tvar active = helper.get_meta(\"active_mesh\", \"\")\n\
         \tif active == \"{name}\":\n\
         \t\tif new_parts.size() > 0:\n\
         \t\t\thelper.set_meta(\"active_mesh\", new_parts[0])\n\
         \t\telse:\n\
         \t\t\thelper.set_meta(\"active_mesh\", \"\")\n\
         \tvar d = {{}}\n\
         \td[\"removed\"] = \"{name}\"\n\
         \td[\"active\"] = helper.get_meta(\"active_mesh\", \"\")\n\
         \td[\"part_count\"] = new_parts.size()\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh info --all`.
///
/// Includes per-part transforms (position, rotation, scale) so the agent can
/// verify transforms after applying them.
pub fn generate_info_all() -> String {
    "extends Node\n\
     \n\
     func run():\n\
     \tvar root = get_tree().get_root()\n\
     \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
     \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
     \tvar active = helper.get_meta(\"active_mesh\", \"\")\n\
     \tvar part_names = helper.get_meta(\"mesh_parts\", [])\n\
     \tvar parts = []\n\
     \tvar total_vc = 0\n\
     \tvar total_fc = 0\n\
     \tfor pname in part_names:\n\
     \t\tvar mi = helper.get_node_or_null(str(pname))\n\
     \t\tvar pd = {}\n\
     \t\tpd[\"name\"] = pname\n\
     \t\tpd[\"visible\"] = mi.visible if mi else false\n\
     \t\tif mi:\n\
     \t\t\tpd[\"position\"] = [mi.position.x, mi.position.y, mi.position.z]\n\
     \t\t\tpd[\"rotation\"] = [mi.rotation_degrees.x, mi.rotation_degrees.y, mi.rotation_degrees.z]\n\
     \t\t\tpd[\"scale\"] = [mi.scale.x, mi.scale.y, mi.scale.z]\n\
     \t\tif mi and mi.mesh and mi.mesh.get_surface_count() > 0:\n\
     \t\t\tvar verts = mi.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX]\n\
     \t\t\tpd[\"vertex_count\"] = verts.size()\n\
     \t\t\tpd[\"face_count\"] = verts.size() / 3\n\
     \t\t\ttotal_vc += verts.size()\n\
     \t\t\ttotal_fc += verts.size() / 3\n\
     \t\t\tvar waabb = mi.transform * mi.mesh.get_aabb()\n\
     \t\t\tvar wend = waabb.position + waabb.size\n\
     \t\t\tpd[\"aabb_min\"] = [snapped(waabb.position.x, 0.01), snapped(waabb.position.y, 0.01), snapped(waabb.position.z, 0.01)]\n\
     \t\t\tpd[\"aabb_max\"] = [snapped(wend.x, 0.01), snapped(wend.y, 0.01), snapped(wend.z, 0.01)]\n\
     \t\telse:\n\
     \t\t\tpd[\"vertex_count\"] = 0\n\
     \t\t\tpd[\"face_count\"] = 0\n\
     \t\tparts.append(pd)\n\
     \tvar d = {}\n\
     \td[\"active\"] = active\n\
     \td[\"part_count\"] = part_names.size()\n\
     \td[\"total_vertex_count\"] = total_vc\n\
     \td[\"total_face_count\"] = total_fc\n\
     \td[\"parts\"] = parts\n\
     \treturn JSON.stringify(d)\n"
        .to_string()
}

/// Generate the GDScript for `mesh duplicate-part --mirror`.
///
/// Duplicates the source part, then mirrors all mesh vertices across the given axis
/// and reverses triangle winding to fix normals (flipping one axis inverts handedness).
/// Also mirrors the transform position on the same axis.
#[allow(clippy::too_many_lines)]
pub fn generate_mirror_part(src: &str, dst: &str, axis: &str, symmetric: bool) -> String {
    // Which component to negate: x=0, y=1, z=2
    let axis_idx = match axis {
        "x" => "0",
        "y" => "1",
        _ => "2",
    };
    // When --symmetric is used and source position on mirror axis is near 0,
    // auto-offset by AABB extent so the mirrored part doesn't overlap.
    let symmetric_offset = if symmetric {
        "\tif abs(pos[axis_idx]) < 0.01 and src.mesh and src.mesh.get_surface_count() > 0:\n\
         \t\tvar src_aabb = src.get_aabb()\n\
         \t\tvar half = src_aabb.size[axis_idx] * 0.5 + abs(src_aabb.get_center()[axis_idx])\n\
         \t\tpos[axis_idx] = -(half + 0.1)\n"
    } else {
        ""
    };
    format!(
        "extends Node\n\
         \n\
         func _retarget(helper, center, sz):\n\
         \tvar rig = helper.get_node(\"_CameraRig\")\n\
         \trig.position = center\n\
         \tfor cam in rig.get_children():\n\
         \t\tif cam is Camera3D:\n\
         \t\t\tif cam.projection == Camera3D.PROJECTION_ORTHOGONAL:\n\
         \t\t\t\tcam.size = sz\n\
         \t\t\telse:\n\
         \t\t\t\tvar half_fov = deg_to_rad(cam.fov * 0.5)\n\
         \t\t\t\tvar dist = (sz * 0.5) / tan(half_fov)\n\
         \t\t\t\tif dist < 1.0: dist = 1.0\n\
         \t\t\t\tcam.position = cam.position.normalized() * dist\n\
         \t\t\tif cam.name == \"Top\" or cam.name == \"Bottom\":\n\
         \t\t\t\tcam.look_at(center, Vector3.FORWARD)\n\
         \t\t\telse:\n\
         \t\t\t\tcam.look_at(center)\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar src = helper.get_node_or_null(\"{src}\")\n\
         \tif src == null: return \"ERROR: source part '{src}' not found\"\n\
         \tvar parts = helper.get_meta(\"mesh_parts\", [])\n\
         \tfor p in parts:\n\
         \t\tif p == \"{dst}\": return \"ERROR: part '{dst}' already exists\"\n\
         \tvar mi = MeshInstance3D.new()\n\
         \tmi.name = \"{dst}\"\n\
         \tmi.transform = src.transform\n\
         \tvar axis_idx = {axis_idx}\n\
         \tvar pos = mi.position\n\
         \tpos[axis_idx] = -pos[axis_idx]\n\
         {symmetric_offset}\
         \tmi.position = pos\n\
         \tvar rot = mi.rotation_degrees\n\
         \tfor i in 3:\n\
         \t\tif i != axis_idx: rot[i] = -rot[i]\n\
         \tmi.rotation_degrees = rot\n\
         \thelper.add_child(mi)\n\
         \tparts.append(\"{dst}\")\n\
         \thelper.set_meta(\"mesh_parts\", parts)\n\
         \thelper.set_meta(\"active_mesh\", \"{dst}\")\n\
         \thelper.set_meta(\"profile_points\", [])\n\
         \thelper.set_meta(\"profile_plane\", \"\")\n\
         \tfor child in helper.get_children():\n\
         \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\tchild.visible = (child.name == \"{dst}\")\n\
         \tvar aabb = mi.get_aabb() if mi.mesh else AABB(Vector3.ZERO, Vector3(2, 2, 2))\n\
         \tvar center = aabb.get_center()\n\
         \tvar dims = aabb.size\n\
         \tvar sz = max(max(dims.x, dims.y), dims.z) * 1.5\n\
         \tif sz < 2.0: sz = 2.0\n\
         \t_retarget(helper, center, sz)\n\
         \tvar _palette = {PALETTE}\n\
         \tvar _color = _palette[(parts.size() - 1) % _palette.size()]\n\
         \tmi.set_meta(\"part_color\", _color)\n\
         \tvar _mat = StandardMaterial3D.new()\n\
         \t_mat.albedo_color = _color\n\
         \tmi.material_override = _mat\n\
         \tvar vc = 0\n\
         \tif mi.mesh and mi.mesh.get_surface_count() > 0:\n\
         \t\tvc = mi.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = \"{dst}\"\n\
         \td[\"source\"] = \"{src}\"\n\
         \td[\"mirror\"] = \"{axis}\"\n\
         \td[\"position\"] = [mi.position.x, mi.position.y, mi.position.z]\n\
         \td[\"part_count\"] = parts.size()\n\
         \td[\"vertex_count\"] = vc\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh material --color`.
///
/// Parses hex color or named color, creates `StandardMaterial3D`, sets `albedo_color`,
/// and stores the color in `part_color` metadata so other commands can restore it.
pub fn generate_material(part: Option<&str>, color: &str) -> String {
    let target = part.map_or(
        String::from("\tvar mesh_name = helper.get_meta(\"active_mesh\")\n"),
        |p| format!("\tvar mesh_name = \"{p}\"\n"),
    );
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         {target}\
         \tvar mesh_inst = helper.get_node_or_null(mesh_name)\n\
         \tif mesh_inst == null: return \"ERROR: part '\" + str(mesh_name) + \"' not found\"\n\
         \tvar hex = \"{color}\"\n\
         \tvar color = Color.html(hex)\n\
         \tvar mat = StandardMaterial3D.new()\n\
         \tmat.albedo_color = color\n\
         \tmesh_inst.material_override = mat\n\
         \tmesh_inst.set_meta(\"part_color\", color)\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = mesh_name\n\
         \td[\"color\"] = hex\n\
         \td[\"rgb\"] = [snapped(color.r, 0.01), snapped(color.g, 0.01), snapped(color.b, 0.01)]\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh material --parts`.
///
/// Applies a color to all parts matching a pattern (glob with `*`/`?`, or comma-separated names).
pub fn generate_material_multi(pattern: &str, color: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar pattern = \"{pattern}\"\n\
         \tvar hex = \"{color}\"\n\
         \tvar color_val = Color.html(hex)\n\
         \tvar applied = []\n\
         \tvar skipped = []\n\
         \tif pattern.contains(\"*\") or pattern.contains(\"?\"):\n\
         \t\tfor child in helper.get_children():\n\
         \t\t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\t\tif String(child.name).match(pattern):\n\
         \t\t\t\t\tvar mat = StandardMaterial3D.new()\n\
         \t\t\t\t\tmat.albedo_color = color_val\n\
         \t\t\t\t\tchild.material_override = mat\n\
         \t\t\t\t\tchild.set_meta(\"part_color\", color_val)\n\
         \t\t\t\t\tapplied.append(String(child.name))\n\
         \telse:\n\
         \t\tfor n in pattern.split(\",\"):\n\
         \t\t\tvar name = n.strip_edges()\n\
         \t\t\tif name.is_empty(): continue\n\
         \t\t\tvar mi = helper.get_node_or_null(NodePath(name))\n\
         \t\t\tif mi:\n\
         \t\t\t\tvar mat = StandardMaterial3D.new()\n\
         \t\t\t\tmat.albedo_color = color_val\n\
         \t\t\t\tmi.material_override = mat\n\
         \t\t\t\tmi.set_meta(\"part_color\", color_val)\n\
         \t\t\t\tapplied.append(name)\n\
         \t\t\telse:\n\
         \t\t\t\tskipped.append(name)\n\
         \tvar d = {{}}\n\
         \td[\"pattern\"] = pattern\n\
         \td[\"color\"] = hex\n\
         \td[\"applied\"] = applied\n\
         \td[\"count\"] = applied.size()\n\
         \tif skipped.size() > 0: d[\"skipped\"] = skipped\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh material --parts --preset`.
pub fn generate_material_preset_multi(pattern: &str, preset: &str, color: Option<&str>) -> String {
    let default_color = match preset {
        "glass" => "Color(0.8, 0.9, 1.0)",
        "metal" => "Color(0.7, 0.7, 0.7)",
        "chrome" => "Color(0.95, 0.95, 0.95)",
        "rubber" => "Color(0.15, 0.15, 0.15)",
        "paint" => "Color(0.8, 0.1, 0.1)",
        "wood" => "Color(0.55, 0.35, 0.2)",
        "matte" => "Color(0.5, 0.5, 0.5)",
        _ => "Color(0.9, 0.9, 0.9)", // plastic
    };
    let color_line = if let Some(hex) = color {
        format!("\tvar base_color = Color.html(\"{hex}\")\n")
    } else {
        format!("\tvar base_color = {default_color}\n")
    };
    // Helper function that sets PBR props — called at both nesting levels
    let props_fn = match preset {
        "glass" => {
            "\tmat.metallic = 0.0\n\tmat.roughness = 0.05\n\tmat.transparency = 1\n\tmat.albedo_color.a = 0.3\n"
        }
        "metal" => "\tmat.metallic = 0.9\n\tmat.roughness = 0.3\n",
        "chrome" => "\tmat.metallic = 1.0\n\tmat.roughness = 0.05\n\tmat.specular = 1.0\n",
        "rubber" => "\tmat.metallic = 0.0\n\tmat.roughness = 0.95\n",
        "paint" => "\tmat.metallic = 0.1\n\tmat.roughness = 0.4\n",
        "wood" => "\tmat.metallic = 0.0\n\tmat.roughness = 0.7\n",
        "matte" => "\tmat.metallic = 0.0\n\tmat.roughness = 1.0\n",
        _ => "\tmat.metallic = 0.0\n\tmat.roughness = 0.4\n", // plastic
    };
    format!(
        "extends Node\n\
         \n\
         func _apply_preset(target, base_color):\n\
         \tvar mat = StandardMaterial3D.new()\n\
         \tmat.albedo_color = base_color\n\
         {props_fn}\
         \ttarget.material_override = mat\n\
         \ttarget.set_meta(\"part_color\", base_color)\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar pattern = \"{pattern}\"\n\
         {color_line}\
         \tvar applied = []\n\
         \tvar skipped = []\n\
         \tif pattern.contains(\"*\") or pattern.contains(\"?\"):\n\
         \t\tfor child in helper.get_children():\n\
         \t\t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\t\tif String(child.name).match(pattern):\n\
         \t\t\t\t\t_apply_preset(child, base_color)\n\
         \t\t\t\t\tapplied.append(String(child.name))\n\
         \telse:\n\
         \t\tfor n in pattern.split(\",\"):\n\
         \t\t\tvar name = n.strip_edges()\n\
         \t\t\tif name.is_empty(): continue\n\
         \t\t\tvar mi = helper.get_node_or_null(NodePath(name))\n\
         \t\t\tif mi:\n\
         \t\t\t\t_apply_preset(mi, base_color)\n\
         \t\t\t\tapplied.append(name)\n\
         \t\t\telse:\n\
         \t\t\t\tskipped.append(name)\n\
         \tvar d = {{}}\n\
         \td[\"pattern\"] = pattern\n\
         \td[\"preset\"] = \"{preset}\"\n\
         \td[\"applied\"] = applied\n\
         \td[\"count\"] = applied.size()\n\
         \tif skipped.size() > 0: d[\"skipped\"] = skipped\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh material --preset`.
///
/// Creates a `StandardMaterial3D` with PBR properties appropriate for the preset
/// (metallic, roughness, transparency, specular, etc.).
pub fn generate_material_preset(part: Option<&str>, preset: &str, color: Option<&str>) -> String {
    let target = part.map_or(
        String::from("\tvar mesh_name = helper.get_meta(\"active_mesh\")\n"),
        |p| format!("\tvar mesh_name = \"{p}\"\n"),
    );
    // PBR properties per preset: (metallic, roughness, transparency, alpha, specular, color_default)
    let props = match preset {
        "glass" => {
            "\tmat.metallic = 0.0\n\
                     \tmat.roughness = 0.05\n\
                     \tmat.transparency = 1\n\
                     \tmat.albedo_color.a = 0.3\n\
                     \tmat.specular = 0.5\n\
                     \tmat.refraction_enabled = true\n\
                     \tmat.refraction_scale = 0.02\n"
        }
        "metal" => {
            "\tmat.metallic = 0.9\n\
                     \tmat.roughness = 0.3\n\
                     \tmat.specular = 0.8\n"
        }
        "chrome" => {
            "\tmat.metallic = 1.0\n\
                      \tmat.roughness = 0.05\n\
                      \tmat.specular = 1.0\n"
        }
        "rubber" => {
            "\tmat.metallic = 0.0\n\
                      \tmat.roughness = 0.95\n\
                      \tmat.specular = 0.1\n"
        }
        "paint" => {
            "\tmat.metallic = 0.1\n\
                     \tmat.roughness = 0.4\n\
                     \tmat.specular = 0.5\n"
        }
        "wood" => {
            "\tmat.metallic = 0.0\n\
                    \tmat.roughness = 0.7\n\
                    \tmat.specular = 0.2\n"
        }
        "matte" => {
            "\tmat.metallic = 0.0\n\
                     \tmat.roughness = 1.0\n\
                     \tmat.specular = 0.0\n"
        }
        // plastic
        _ => {
            "\tmat.metallic = 0.0\n\
              \tmat.roughness = 0.4\n\
              \tmat.specular = 0.5\n"
        }
    };
    let default_color = match preset {
        "glass" => "Color(0.8, 0.9, 1.0)",
        "metal" => "Color(0.7, 0.7, 0.75)",
        "chrome" => "Color(0.95, 0.95, 0.97)",
        "rubber" => "Color(0.15, 0.15, 0.15)",
        "paint" => "Color(0.8, 0.1, 0.1)",
        "wood" => "Color(0.55, 0.35, 0.2)",
        "matte" => "Color(0.5, 0.5, 0.5)",
        _ => "Color(0.9, 0.9, 0.9)", // plastic
    };
    let color_line = if let Some(hex) = color {
        format!("\tmat.albedo_color = Color.html(\"{hex}\")\n")
    } else {
        format!("\tmat.albedo_color = {default_color}\n")
    };
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         {target}\
         \tvar mesh_inst = helper.get_node_or_null(mesh_name)\n\
         \tif mesh_inst == null: return \"ERROR: part '\" + str(mesh_name) + \"' not found\"\n\
         \tvar mat = StandardMaterial3D.new()\n\
         {color_line}\
         {props}\
         \tmesh_inst.material_override = mat\n\
         \tvar c = mat.albedo_color\n\
         \tmesh_inst.set_meta(\"part_color\", c)\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = mesh_name\n\
         \td[\"preset\"] = \"{preset}\"\n\
         \td[\"rgb\"] = [snapped(c.r, 0.01), snapped(c.g, 0.01), snapped(c.b, 0.01)]\n\
         \td[\"metallic\"] = mat.metallic\n\
         \td[\"roughness\"] = mat.roughness\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript to apply a face orientation debug overlay.
///
/// Blue = front-facing (correct outward winding), Red = back-facing (inverted).
/// Uses the GPU's `FRONT_FACING` built-in which matches culling behaviour exactly.
pub fn generate_normal_debug() -> String {
    "extends Node\n\
     \n\
     func run():\n\
     \tvar root = get_tree().get_root()\n\
     \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
     \tif helper == null: return \"ERROR: no mesh session\"\n\
     \tvar shader = Shader.new()\n\
     \tshader.code = \"shader_type spatial;\\nrender_mode unshaded, cull_disabled;\\n\\nvoid fragment() {\\n\\tALBEDO = FRONT_FACING ? vec3(0.2, 0.4, 1.0) : vec3(1.0, 0.2, 0.2);\\n}\\n\"\n\
     \tvar mat = ShaderMaterial.new()\n\
     \tmat.shader = shader\n\
     \tvar count = 0\n\
     \tfor child in helper.get_children():\n\
     \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
     \t\t\tif child.material_override and not (child.material_override is ShaderMaterial):\n\
     \t\t\t\tchild.set_meta(\"_saved_material\", child.material_override.duplicate())\n\
     \t\t\tchild.material_override = mat\n\
     \t\t\tcount += 1\n\
     \tvar d = {}\n\
     \td[\"mode\"] = \"normal_debug\"\n\
     \td[\"parts_affected\"] = count\n\
     \treturn JSON.stringify(d)\n"
        .to_string()
}

/// Generate the GDScript to remove the face orientation debug overlay.
///
/// Only acts on parts that currently have a `ShaderMaterial` (i.e. the debug overlay).
/// Restores the original material from `_saved_material` metadata if available,
/// otherwise falls back to `part_color` metadata.
pub fn generate_normal_debug_clear() -> String {
    "extends Node\n\
     \n\
     func run():\n\
     \tvar root = get_tree().get_root()\n\
     \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
     \tif helper == null: return \"ok\"\n\
     \tfor child in helper.get_children():\n\
     \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
     \t\t\tif child.material_override is ShaderMaterial:\n\
     \t\t\t\tif child.has_meta(\"_saved_material\"):\n\
     \t\t\t\t\tchild.material_override = child.get_meta(\"_saved_material\")\n\
     \t\t\t\t\tchild.remove_meta(\"_saved_material\")\n\
     \t\t\t\telif child.has_meta(\"part_color\"):\n\
     \t\t\t\t\tvar mat = StandardMaterial3D.new()\n\
     \t\t\t\t\tmat.albedo_color = child.get_meta(\"part_color\")\n\
     \t\t\t\t\tchild.material_override = mat\n\
     \t\t\t\telse:\n\
     \t\t\t\t\tchild.material_override = null\n\
     \treturn \"ok\"\n"
        .to_string()
}

/// Generate the GDScript to auto-fit all cameras to the combined AABB of
/// visible `MeshInstance3D` children. Returns the computed camera size as JSON
/// so the view command can report accurate bounds.
pub fn generate_autofit_cameras(zoom: f64) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session\"\n\
         \tvar rig = helper.get_node_or_null(\"_CameraRig\")\n\
         \tif rig == null: return \"ERROR: camera rig not found\"\n\
         \tvar combined = AABB()\n\
         \tvar first = true\n\
         \tvar visible_count = 0\n\
         \tvar total_count = 0\n\
         \tfor child in helper.get_children():\n\
         \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\ttotal_count += 1\n\
         \t\t\tif child.visible and child.mesh:\n\
         \t\t\t\tvisible_count += 1\n\
         \t\t\t\tvar ab = child.transform * child.mesh.get_aabb()\n\
         \t\t\t\tif first:\n\
         \t\t\t\t\tcombined = ab\n\
         \t\t\t\t\tfirst = false\n\
         \t\t\t\telse:\n\
         \t\t\t\t\tcombined = combined.merge(ab)\n\
         \tif first:\n\
         \t\tcombined = AABB(Vector3(-5, -5, -5), Vector3(10, 10, 10))\n\
         \tvar center = combined.get_center()\n\
         \tvar dims = combined.size\n\
         \tvar sz = max(max(dims.x, dims.y), dims.z) * 1.5\n\
         \tif sz < 2.0: sz = 2.0\n\
         \tsz /= {zoom}\n\
         \trig.position = center\n\
         \tfor cam in rig.get_children():\n\
         \t\tif cam is Camera3D:\n\
         \t\t\tif cam.projection == Camera3D.PROJECTION_ORTHOGONAL:\n\
         \t\t\t\tcam.size = sz\n\
         \t\t\telse:\n\
         \t\t\t\tvar half_fov = deg_to_rad(cam.fov * 0.5)\n\
         \t\t\t\tvar dist = (sz * 0.5) / tan(half_fov)\n\
         \t\t\t\tif dist < 1.0: dist = 1.0\n\
         \t\t\t\tcam.position = cam.position.normalized() * dist\n\
         \t\t\tif cam.name == \"Top\" or cam.name == \"Bottom\":\n\
         \t\t\t\tcam.look_at(center, Vector3.FORWARD)\n\
         \t\t\telse:\n\
         \t\t\t\tcam.look_at(center)\n\
         \tvar d = {{}}\n\
         \td[\"camera_size\"] = sz\n\
         \td[\"center\"] = [center.x, center.y, center.z]\n\
         \td[\"aabb_size\"] = [dims.x, dims.y, dims.z]\n\
         \td[\"visible_parts\"] = visible_count\n\
         \td[\"total_parts\"] = total_count\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript to add a coordinate grid overlay.
pub fn generate_grid(plane: &str, size: f64) -> String {
    let int_size = size as i32;
    let grid_code = match plane {
        "front" => format!(
            "\tfor i in range(-{int_size}, {int_size} + 1):\n\
             \t\tvar fi = float(i)\n\
             \t\tst.add_vertex(Vector3(fi, -{size}, 0))\n\
             \t\tst.add_vertex(Vector3(fi, {size}, 0))\n\
             \t\tst.add_vertex(Vector3(-{size}, fi, 0))\n\
             \t\tst.add_vertex(Vector3({size}, fi, 0))\n"
        ),
        "side" => format!(
            "\tfor i in range(-{int_size}, {int_size} + 1):\n\
             \t\tvar fi = float(i)\n\
             \t\tst.add_vertex(Vector3(0, fi, -{size}))\n\
             \t\tst.add_vertex(Vector3(0, fi, {size}))\n\
             \t\tst.add_vertex(Vector3(0, -{size}, fi))\n\
             \t\tst.add_vertex(Vector3(0, {size}, fi))\n"
        ),
        _ => format!(
            // top
            "\tfor i in range(-{int_size}, {int_size} + 1):\n\
             \t\tvar fi = float(i)\n\
             \t\tst.add_vertex(Vector3(fi, 0, -{size}))\n\
             \t\tst.add_vertex(Vector3(fi, 0, {size}))\n\
             \t\tst.add_vertex(Vector3(-{size}, 0, fi))\n\
             \t\tst.add_vertex(Vector3({size}, 0, fi))\n"
        ),
    };

    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session\"\n\
         \tvar old = helper.get_node_or_null(\"_GdMeshGrid\")\n\
         \tif old: old.queue_free()\n\
         \tvar grid = MeshInstance3D.new()\n\
         \tgrid.name = \"_GdMeshGrid\"\n\
         \tvar st = SurfaceTool.new()\n\
         \tst.begin(Mesh.PRIMITIVE_LINES)\n\
         {grid_code}\
         \tgrid.mesh = st.commit()\n\
         \tvar mat = StandardMaterial3D.new()\n\
         \tmat.albedo_color = Color(0.5, 0.5, 0.5, 0.3)\n\
         \tmat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA\n\
         \tmat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED\n\
         \tgrid.material_override = mat\n\
         \thelper.add_child(grid)\n\
         \treturn \"ok\"\n"
    )
}

/// Generate the GDScript to remove the grid overlay.
pub fn generate_remove_grid() -> String {
    "extends Node\n\
     \n\
     func run():\n\
     \tvar root = get_tree().get_root()\n\
     \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
     \tif helper == null: return \"ok\"\n\
     \tvar grid = helper.get_node_or_null(\"_GdMeshGrid\")\n\
     \tif grid: grid.queue_free()\n\
     \treturn \"ok\"\n"
        .to_string()
}

/// Generate the GDScript for `mesh duplicate-part`.
///
/// Clones the source part's mesh and transform to a new named part. Sets the
/// new part as active, hides others, retargets cameras.
pub fn generate_duplicate_part(src: &str, dst: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func _retarget(helper, center, sz):\n\
         \tvar rig = helper.get_node(\"_CameraRig\")\n\
         \trig.position = center\n\
         \tfor cam in rig.get_children():\n\
         \t\tif cam is Camera3D:\n\
         \t\t\tif cam.projection == Camera3D.PROJECTION_ORTHOGONAL:\n\
         \t\t\t\tcam.size = sz\n\
         \t\t\telse:\n\
         \t\t\t\tvar half_fov = deg_to_rad(cam.fov * 0.5)\n\
         \t\t\t\tvar dist = (sz * 0.5) / tan(half_fov)\n\
         \t\t\t\tif dist < 1.0: dist = 1.0\n\
         \t\t\t\tcam.position = cam.position.normalized() * dist\n\
         \t\t\tif cam.name == \"Top\" or cam.name == \"Bottom\":\n\
         \t\t\t\tcam.look_at(center, Vector3.FORWARD)\n\
         \t\t\telse:\n\
         \t\t\t\tcam.look_at(center)\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar src = helper.get_node_or_null(\"{src}\")\n\
         \tif src == null: return \"ERROR: source part '{src}' not found\"\n\
         \tvar parts = helper.get_meta(\"mesh_parts\", [])\n\
         \tfor p in parts:\n\
         \t\tif p == \"{dst}\": return \"ERROR: part '{dst}' already exists\"\n\
         \tvar mi = MeshInstance3D.new()\n\
         \tmi.name = \"{dst}\"\n\
         \tif src.mesh:\n\
         \t\tmi.mesh = src.mesh.duplicate()\n\
         \tmi.transform = src.transform\n\
         \thelper.add_child(mi)\n\
         \tparts.append(\"{dst}\")\n\
         \thelper.set_meta(\"mesh_parts\", parts)\n\
         \thelper.set_meta(\"active_mesh\", \"{dst}\")\n\
         \thelper.set_meta(\"profile_points\", [])\n\
         \thelper.set_meta(\"profile_plane\", \"\")\n\
         \tfor child in helper.get_children():\n\
         \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\tchild.visible = (child.name == \"{dst}\")\n\
         \tvar aabb = mi.get_aabb() if mi.mesh else AABB(Vector3.ZERO, Vector3(2, 2, 2))\n\
         \tvar center = aabb.get_center()\n\
         \tvar dims = aabb.size\n\
         \tvar sz = max(max(dims.x, dims.y), dims.z) * 1.5\n\
         \tif sz < 2.0: sz = 2.0\n\
         \t_retarget(helper, center, sz)\n\
         \tvar _palette = {PALETTE}\n\
         \tvar _color = _palette[(parts.size() - 1) % _palette.size()]\n\
         \tmi.set_meta(\"part_color\", _color)\n\
         \tvar _mat = StandardMaterial3D.new()\n\
         \t_mat.albedo_color = _color\n\
         \tmi.material_override = _mat\n\
         \tvar vc = 0\n\
         \tif mi.mesh and mi.mesh.get_surface_count() > 0:\n\
         \t\tvc = mi.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = \"{dst}\"\n\
         \td[\"source\"] = \"{src}\"\n\
         \td[\"part_count\"] = parts.size()\n\
         \td[\"vertex_count\"] = vc\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh add-part`.
///
/// Primitives are built in Rust and pushed via `generate_push_script`.
/// The GDScript only creates the scene node (no Godot mesh assignment).
pub fn generate_add_part(name: &str, _primitive: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func _retarget(helper, center, sz):\n\
         \tvar rig = helper.get_node(\"_CameraRig\")\n\
         \trig.position = center\n\
         \tfor cam in rig.get_children():\n\
         \t\tif cam is Camera3D:\n\
         \t\t\tcam.size = sz\n\
         \t\t\tif cam.name == \"Top\":\n\
         \t\t\t\tcam.look_at(center, Vector3.FORWARD)\n\
         \t\t\telse:\n\
         \t\t\t\tcam.look_at(center)\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar parts = helper.get_meta(\"mesh_parts\", [])\n\
         \tfor p in parts:\n\
         \t\tif p == \"{name}\": return \"ERROR: part '{name}' already exists\"\n\
         \tvar mesh_inst = MeshInstance3D.new()\n\
         \tmesh_inst.name = \"{name}\"\n\
         \thelper.add_child(mesh_inst)\n\
         \tparts.append(\"{name}\")\n\
         \thelper.set_meta(\"mesh_parts\", parts)\n\
         \thelper.set_meta(\"active_mesh\", \"{name}\")\n\
         \thelper.set_meta(\"profile_points\", [])\n\
         \thelper.set_meta(\"profile_plane\", \"\")\n\
         \tfor child in helper.get_children():\n\
         \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\tchild.visible = (child.name == \"{name}\")\n\
         \tvar aabb = AABB(Vector3.ZERO, Vector3(2, 2, 2))\n\
         \tvar center = aabb.get_center()\n\
         \tvar dims = aabb.size\n\
         \tvar sz = max(max(dims.x, dims.y), dims.z) * 1.5\n\
         \tif sz < 2.0: sz = 2.0\n\
         \t_retarget(helper, center, sz)\n\
         \tvar _palette = {PALETTE}\n\
         \tvar _color = _palette[(parts.size() - 1) % _palette.size()]\n\
         \tmesh_inst.set_meta(\"part_color\", _color)\n\
         \tvar _mat = StandardMaterial3D.new()\n\
         \t_mat.albedo_color = _color\n\
         \tmesh_inst.material_override = _mat\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = \"{name}\"\n\
         \td[\"part_count\"] = parts.size()\n\
         \td[\"vertex_count\"] = 0\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh focus <name>`.
pub fn generate_focus(name: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func _retarget(helper, center, sz):\n\
         \tvar rig = helper.get_node(\"_CameraRig\")\n\
         \trig.position = center\n\
         \tfor cam in rig.get_children():\n\
         \t\tif cam is Camera3D:\n\
         \t\t\tcam.size = sz\n\
         \t\t\tif cam.name == \"Top\":\n\
         \t\t\t\tcam.look_at(center, Vector3.FORWARD)\n\
         \t\t\telse:\n\
         \t\t\t\tcam.look_at(center)\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar target = helper.get_node_or_null(\"{name}\")\n\
         \tif target == null: return \"ERROR: part '{name}' not found\"\n\
         \thelper.set_meta(\"active_mesh\", \"{name}\")\n\
         \thelper.set_meta(\"profile_points\", [])\n\
         \thelper.set_meta(\"profile_plane\", \"\")\n\
         \tfor child in helper.get_children():\n\
         \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\tchild.visible = (child.name == \"{name}\")\n\
         \tvar aabb = target.get_aabb() if target.mesh else AABB(Vector3.ZERO, Vector3(2, 2, 2))\n\
         \tvar center = aabb.get_center()\n\
         \tvar dims = aabb.size\n\
         \tvar sz = max(max(dims.x, dims.y), dims.z) * 1.5\n\
         \tif sz < 2.0: sz = 2.0\n\
         \t_retarget(helper, center, sz)\n\
         \tvar vc = 0\n\
         \tif target.mesh and target.mesh.get_surface_count() > 0:\n\
         \t\tvc = target.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar d = {{}}\n\
         \td[\"active\"] = \"{name}\"\n\
         \td[\"vertex_count\"] = vc\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh overlay edges`.
///
/// Creates an `_EdgeOverlay` `MeshInstance3D` with three surfaces:
/// boundary (red), sharp (yellow), interior (gray, semi-transparent).
/// Uses `FLAG_DISABLE_DEPTH_TEST` for x-ray visibility.
pub fn generate_edge_overlay(data: &super::overlay::EdgeOverlayData) -> String {
    use std::fmt::Write;

    // Build surface blocks for each edge class
    let mut surfaces = String::new();
    let classes: [(&[(usize, usize)], &str); 3] = [
        (&data.boundary, "Color(1, 0.2, 0.2)"),
        (&data.sharp, "Color(1, 0.85, 0.2)"),
        (&data.interior, "Color(0.5, 0.5, 0.5, 0.4)"),
    ];

    for (idx, (edges, color)) in classes.iter().enumerate() {
        if edges.is_empty() {
            continue;
        }
        let _ = writeln!(surfaces, "\tvar st{idx} = SurfaceTool.new()");
        let _ = writeln!(surfaces, "\tst{idx}.begin(Mesh.PRIMITIVE_LINES)");
        for &(a, b) in *edges {
            let pa = data.positions[a];
            let pb = data.positions[b];
            let _ = writeln!(
                surfaces,
                "\tst{idx}.add_vertex(Vector3({}, {}, {}))",
                pa[0], pa[1], pa[2]
            );
            let _ = writeln!(
                surfaces,
                "\tst{idx}.add_vertex(Vector3({}, {}, {}))",
                pb[0], pb[1], pb[2]
            );
        }
        let _ = writeln!(surfaces, "\tvar mat{idx} = StandardMaterial3D.new()");
        let _ = writeln!(surfaces, "\tmat{idx}.albedo_color = {color}");
        let _ = writeln!(
            surfaces,
            "\tmat{idx}.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED"
        );
        let _ = writeln!(surfaces, "\tmat{idx}.no_depth_test = true");
        let _ = writeln!(
            surfaces,
            "\tmat{idx}.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA"
        );
        let _ = writeln!(surfaces, "\tst{idx}.set_material(mat{idx})");
        let _ = writeln!(surfaces, "\tst{idx}.commit(amesh)");
    }

    let mut script = String::from(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session\"\n\
         \tvar old = helper.get_node_or_null(\"_EdgeOverlay\")\n\
         \tif old: old.queue_free()\n\
         \tvar amesh = ArrayMesh.new()\n",
    );
    script.push_str(&surfaces);
    script.push_str(
        "\tvar overlay = MeshInstance3D.new()\n\
         \toverlay.name = \"_EdgeOverlay\"\n\
         \toverlay.mesh = amesh\n\
         \thelper.add_child(overlay)\n\
         \treturn \"ok\"\n",
    );
    script
}

/// Generate the GDScript to remove the edge overlay.
pub fn generate_remove_edge_overlay() -> String {
    "extends Node\n\
     \n\
     func run():\n\
     \tvar root = get_tree().get_root()\n\
     \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
     \tif helper == null: return \"ok\"\n\
     \tvar overlay = helper.get_node_or_null(\"_EdgeOverlay\")\n\
     \tif overlay: overlay.queue_free()\n\
     \treturn \"ok\"\n"
        .to_string()
}

/// Generate the GDScript for `mesh focus --all`.
pub fn generate_focus_all() -> String {
    "extends Node\n\
     \n\
     func _retarget(helper, center, sz):\n\
     \tvar rig = helper.get_node(\"_CameraRig\")\n\
     \trig.position = center\n\
     \tfor cam in rig.get_children():\n\
     \t\tif cam is Camera3D:\n\
     \t\t\tcam.size = sz\n\
     \t\t\tif cam.name == \"Top\":\n\
     \t\t\t\tcam.look_at(center, Vector3.FORWARD)\n\
     \t\t\telse:\n\
     \t\t\t\tcam.look_at(center)\n\
     \n\
     func run():\n\
     \tvar root = get_tree().get_root()\n\
     \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
     \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
     \tvar combined = AABB()\n\
     \tvar first = true\n\
     \tfor child in helper.get_children():\n\
     \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
     \t\t\tchild.visible = true\n\
     \t\t\tif child.mesh:\n\
     \t\t\t\tvar ab = child.get_aabb()\n\
     \t\t\t\tif first:\n\
     \t\t\t\t\tcombined = ab\n\
     \t\t\t\t\tfirst = false\n\
     \t\t\t\telse:\n\
     \t\t\t\t\tcombined = combined.merge(ab)\n\
     \tif first:\n\
     \t\tcombined = AABB(Vector3.ZERO, Vector3(2, 2, 2))\n\
     \tvar center = combined.get_center()\n\
     \tvar dims = combined.size\n\
     \tvar sz = max(max(dims.x, dims.y), dims.z) * 1.5\n\
     \tif sz < 2.0: sz = 2.0\n\
     \t_retarget(helper, center, sz)\n\
     \tvar active = helper.get_meta(\"active_mesh\", \"\")\n\
     \tvar parts = helper.get_meta(\"mesh_parts\", [])\n\
     \tvar d = {}\n\
     \td[\"active\"] = active\n\
     \td[\"part_count\"] = parts.size()\n\
     \td[\"visible\"] = \"all\"\n\
     \treturn JSON.stringify(d)\n"
        .to_string()
}
