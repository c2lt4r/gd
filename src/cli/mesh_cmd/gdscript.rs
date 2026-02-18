/// Generate the GDScript for `mesh create`.
#[allow(clippy::too_many_lines)]
pub fn generate_create(name: &str, primitive: &str) -> String {
    let primitive_line = match primitive {
        "cube" => "\tmesh_inst.mesh = BoxMesh.new()\n",
        "sphere" => "\tmesh_inst.mesh = SphereMesh.new()\n",
        "cylinder" => "\tmesh_inst.mesh = CylinderMesh.new()\n",
        _ => "",
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
         \tcam_front.position = Vector3(0, 0, 20)\n\
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
         \tcam_back.position = Vector3(0, 0, -20)\n\
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
         \tvar cam_iso = Camera3D.new()\n\
         \tcam_iso.name = \"Iso\"\n\
         \tcam_iso.projection = Camera3D.PROJECTION_ORTHOGONAL\n\
         \tcam_iso.size = 10\n\
         \tcam_iso.position = Vector3(12, 12, 12)\n\
         \trig.add_child(cam_iso)\n\
         \tcam_iso.look_at(Vector3.ZERO)\n\
         \tvar key_light = DirectionalLight3D.new()\n\
         \tkey_light.name = \"_KeyLight\"\n\
         \tkey_light.rotation_degrees = Vector3(-45, -30, 0)\n\
         \tkey_light.light_energy = 0.8\n\
         \thelper.add_child(key_light)\n\
         \tvar fill_light = DirectionalLight3D.new()\n\
         \tfill_light.name = \"_FillLight\"\n\
         \tfill_light.rotation_degrees = Vector3(-30, 150, 0)\n\
         \tfill_light.light_energy = 0.4\n\
         \thelper.add_child(fill_light)\n\
         \tvar rim_light = DirectionalLight3D.new()\n\
         \trim_light.name = \"_RimLight\"\n\
         \trim_light.rotation_degrees = Vector3(15, 90, 0)\n\
         \trim_light.light_energy = 0.3\n\
         \thelper.add_child(rim_light)\n\
         \tvar env_res = Environment.new()\n\
         \tenv_res.background_mode = 1\n\
         \tenv_res.background_color = Color(0.15, 0.15, 0.15)\n\
         \tenv_res.ambient_light_source = 1\n\
         \tenv_res.ambient_light_color = Color(0.3, 0.3, 0.3)\n\
         \tvar world_env = WorldEnvironment.new()\n\
         \tworld_env.name = \"_MeshEnv\"\n\
         \tworld_env.environment = env_res\n\
         \thelper.add_child(world_env)\n\
         \tvar mesh_inst = MeshInstance3D.new()\n\
         \tmesh_inst.name = \"{name}\"\n\
         {primitive_line}\
         \thelper.add_child(mesh_inst)\n\
         \thelper.set_meta(\"active_mesh\", \"{name}\")\n\
         \thelper.set_meta(\"mesh_parts\", [\"{name}\"])\n\
         \thelper.set_meta(\"profile_points\", [])\n\
         \thelper.set_meta(\"profile_plane\", \"\")\n\
         \tvar vc = 0\n\
         \tif mesh_inst.mesh and mesh_inst.mesh.get_surface_count() > 0:\n\
         \t\tvc = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = \"{name}\"\n\
         \td[\"primitive\"] = \"{primitive}\"\n\
         \td[\"cameras\"] = [\"Front\", \"Back\", \"Side\", \"Left\", \"Top\", \"Bottom\", \"Iso\"]\n\
         \td[\"vertex_count\"] = vc\n\
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
         \tmesh_inst.material_override = mat\n\
         \tvar d = {{}}\n\
         \td[\"plane\"] = \"{plane}\"\n\
         \td[\"point_count\"] = points.size()\n\
         \td[\"points\"] = points\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh extrude`.
///
/// Emits raw geometry (caps + side walls) without manual normals, then uses a
/// per-triangle centroid check to fix any inverted winding before generating
/// normals from the corrected winding order.
#[allow(clippy::too_many_lines)]
pub fn generate_extrude(depth: f64) -> String {
    let half = depth / 2.0;
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar points = helper.get_meta(\"profile_points\")\n\
         \tvar plane = helper.get_meta(\"profile_plane\")\n\
         \tif points.size() < 3: return \"ERROR: no profile — run 'gd mesh profile' first\"\n\
         \tvar mesh_name = helper.get_meta(\"active_mesh\")\n\
         \tvar mesh_inst = helper.get_node_or_null(mesh_name)\n\
         \tif mesh_inst == null: return \"ERROR: mesh node not found\"\n\
         \tvar half = {half}\n\
         \tvar front = []\n\
         \tvar back = []\n\
         \tfor p in points:\n\
         \t\tvar fv\n\
         \t\tvar bv\n\
         \t\tif plane == \"front\":\n\
         \t\t\tfv = Vector3(p[0], p[1], half)\n\
         \t\t\tbv = Vector3(p[0], p[1], -half)\n\
         \t\telif plane == \"side\":\n\
         \t\t\tfv = Vector3(half, p[1], p[0])\n\
         \t\t\tbv = Vector3(-half, p[1], p[0])\n\
         \t\telse:\n\
         \t\t\tfv = Vector3(p[0], half, p[1])\n\
         \t\t\tbv = Vector3(p[0], -half, p[1])\n\
         \t\tfront.append(fv)\n\
         \t\tback.append(bv)\n\
         \tvar pts2d = PackedVector2Array()\n\
         \tfor p in points:\n\
         \t\tpts2d.append(Vector2(p[0], p[1]))\n\
         \tvar indices = Geometry2D.triangulate_polygon(pts2d)\n\
         \tif indices.size() == 0: return \"ERROR: could not triangulate polygon\"\n\
         \tvar st = SurfaceTool.new()\n\
         \tst.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \tfor i in indices:\n\
         \t\tst.add_vertex(front[i])\n\
         \tfor i in indices:\n\
         \t\tst.add_vertex(back[i])\n\
         \tvar n_pts = front.size()\n\
         \tfor i in n_pts:\n\
         \t\tvar j = (i + 1) % n_pts\n\
         \t\tst.add_vertex(front[i])\n\
         \t\tst.add_vertex(front[j])\n\
         \t\tst.add_vertex(back[i])\n\
         \t\tst.add_vertex(front[j])\n\
         \t\tst.add_vertex(back[j])\n\
         \t\tst.add_vertex(back[i])\n\
         \tst.generate_normals()\n\
         \tvar mesh = st.commit()\n\
         \tvar arrays = mesh.surface_get_arrays(0)\n\
         \tvar verts = arrays[Mesh.ARRAY_VERTEX]\n\
         \tvar centroid = Vector3.ZERO\n\
         \tfor v in verts:\n\
         \t\tcentroid += v\n\
         \tcentroid /= verts.size()\n\
         \tvar need_fix = false\n\
         \tfor ti in range(0, verts.size(), 3):\n\
         \t\tvar a = verts[ti]\n\
         \t\tvar b = verts[ti + 1]\n\
         \t\tvar c = verts[ti + 2]\n\
         \t\tvar face_n = (b - a).cross(c - a)\n\
         \t\tvar tri_center = (a + b + c) / 3.0\n\
         \t\tif face_n.dot(tri_center - centroid) < 0:\n\
         \t\t\tverts[ti + 1] = c\n\
         \t\t\tverts[ti + 2] = b\n\
         \t\t\tneed_fix = true\n\
         \tif need_fix:\n\
         \t\tvar st2 = SurfaceTool.new()\n\
         \t\tst2.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \t\tfor v in verts:\n\
         \t\t\tst2.add_vertex(v)\n\
         \t\tst2.generate_normals()\n\
         \t\tmesh_inst.mesh = st2.commit()\n\
         \telse:\n\
         \t\tmesh_inst.mesh = mesh\n\
         \tmesh_inst.material_override = null\n\
         \tvar vc = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar fc = vc / 3\n\
         \tvar d = {{}}\n\
         \td[\"depth\"] = {depth}\n\
         \td[\"plane\"] = plane\n\
         \td[\"vertex_count\"] = vc\n\
         \td[\"face_count\"] = fc\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh revolve`.
///
/// Uses per-triangle centroid check to fix winding, then generates normals.
pub fn generate_revolve(axis: &str, angle: f64, segments: u32) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar points = helper.get_meta(\"profile_points\")\n\
         \tvar plane = helper.get_meta(\"profile_plane\")\n\
         \tif points.size() < 3: return \"ERROR: no profile — run 'gd mesh profile' first\"\n\
         \tvar mesh_name = helper.get_meta(\"active_mesh\")\n\
         \tvar mesh_inst = helper.get_node_or_null(mesh_name)\n\
         \tif mesh_inst == null: return \"ERROR: mesh node not found\"\n\
         \tvar axis = \"{axis}\"\n\
         \tvar angle_deg = {angle}\n\
         \tvar segments = {segments}\n\
         \tvar angle_rad = deg_to_rad(angle_deg)\n\
         \tvar step = angle_rad / segments\n\
         \tvar profile = []\n\
         \tfor p in points:\n\
         \t\tvar v\n\
         \t\tif plane == \"front\":\n\
         \t\t\tv = Vector3(p[0], p[1], 0)\n\
         \t\telif plane == \"side\":\n\
         \t\t\tv = Vector3(0, p[1], p[0])\n\
         \t\telse:\n\
         \t\t\tv = Vector3(p[0], 0, p[1])\n\
         \t\tprofile.append(v)\n\
         \tvar rings = []\n\
         \tfor s in segments + 1:\n\
         \t\tvar a = s * step\n\
         \t\tvar ring = []\n\
         \t\tfor pt in profile:\n\
         \t\t\tring.append(_rotate(pt, axis, a))\n\
         \t\trings.append(ring)\n\
         \tvar st = SurfaceTool.new()\n\
         \tst.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \tvar pn = profile.size()\n\
         \tfor s in segments:\n\
         \t\tfor i in pn:\n\
         \t\t\tvar j = (i + 1) % pn\n\
         \t\t\tst.add_vertex(rings[s][i])\n\
         \t\t\tst.add_vertex(rings[s][j])\n\
         \t\t\tst.add_vertex(rings[s + 1][j])\n\
         \t\t\tst.add_vertex(rings[s][i])\n\
         \t\t\tst.add_vertex(rings[s + 1][j])\n\
         \t\t\tst.add_vertex(rings[s + 1][i])\n\
         \tst.generate_normals()\n\
         \tvar mesh = st.commit()\n\
         \tvar arrays = mesh.surface_get_arrays(0)\n\
         \tvar verts = arrays[Mesh.ARRAY_VERTEX]\n\
         \tvar centroid = Vector3.ZERO\n\
         \tfor v in verts:\n\
         \t\tcentroid += v\n\
         \tcentroid /= verts.size()\n\
         \tvar need_fix = false\n\
         \tfor ti in range(0, verts.size(), 3):\n\
         \t\tvar a = verts[ti]\n\
         \t\tvar b = verts[ti + 1]\n\
         \t\tvar c = verts[ti + 2]\n\
         \t\tvar face_n = (b - a).cross(c - a)\n\
         \t\tvar tri_center = (a + b + c) / 3.0\n\
         \t\tif face_n.dot(tri_center - centroid) < 0:\n\
         \t\t\tverts[ti + 1] = c\n\
         \t\t\tverts[ti + 2] = b\n\
         \t\t\tneed_fix = true\n\
         \tif need_fix:\n\
         \t\tvar st2 = SurfaceTool.new()\n\
         \t\tst2.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \t\tfor v in verts:\n\
         \t\t\tst2.add_vertex(v)\n\
         \t\tst2.generate_normals()\n\
         \t\tmesh_inst.mesh = st2.commit()\n\
         \telse:\n\
         \t\tmesh_inst.mesh = mesh\n\
         \tmesh_inst.material_override = null\n\
         \tvar vc = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar d = {{}}\n\
         \td[\"axis\"] = axis\n\
         \td[\"angle\"] = angle_deg\n\
         \td[\"segments\"] = segments\n\
         \td[\"vertex_count\"] = vc\n\
         \treturn JSON.stringify(d)\n\
         \n\
         func _rotate(pt, ax, a):\n\
         \tvar c = cos(a)\n\
         \tvar s = sin(a)\n\
         \tif ax == \"x\":\n\
         \t\treturn Vector3(pt.x, pt.y * c - pt.z * s, pt.y * s + pt.z * c)\n\
         \telif ax == \"y\":\n\
         \t\treturn Vector3(pt.x * c + pt.z * s, pt.y, -pt.x * s + pt.z * c)\n\
         \telse:\n\
         \t\treturn Vector3(pt.x * c - pt.y * s, pt.x * s + pt.y * c, pt.z)\n"
    )
}

/// Generate the GDScript for `mesh move-vertex`.
pub fn generate_move_vertex(index: u32, dx: f64, dy: f64, dz: f64) -> String {
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
         \tvar idx = {index}\n\
         \tif idx < 0 or idx >= verts.size():\n\
         \t\treturn \"ERROR: vertex index %d out of range (0..%d)\" % [idx, verts.size() - 1]\n\
         \tvar old = verts[idx]\n\
         \tverts[idx] = old + Vector3({dx}, {dy}, {dz})\n\
         \tvar am = ArrayMesh.new()\n\
         \tarrays[Mesh.ARRAY_VERTEX] = verts\n\
         \tam.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)\n\
         \tmesh_inst.mesh = am\n\
         \tvar d = {{}}\n\
         \td[\"index\"] = idx\n\
         \td[\"old_position\"] = [old.x, old.y, old.z]\n\
         \td[\"new_position\"] = [verts[idx].x, verts[idx].y, verts[idx].z]\n\
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
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar mesh_children = []\n\
         \tfor child in helper.get_children():\n\
         \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\tif child.mesh:\n\
         \t\t\t\tmesh_children.append(child)\n\
         \tif mesh_children.size() == 0: return \"ERROR: no mesh data in any part\"\n\
         \tvar resources = []\n\
         \tvar transforms = {{}}\n\
         \tfor mi in mesh_children:\n\
         \t\tvar res_path = \"{base_path}_\" + mi.name + \".tres\"\n\
         \t\tvar err = ResourceSaver.save(mi.mesh, res_path)\n\
         \t\tif err != OK: return \"ERROR: failed to save mesh '\" + mi.name + \"': \" + str(err)\n\
         \t\tresources.append({{\"name\": mi.name, \"resource\": res_path}})\n\
         \t\ttransforms[mi.name] = mi.transform\n\
         \tvar scene_root\n\
         \tif mesh_children.size() == 1:\n\
         \t\tvar node = MeshInstance3D.new()\n\
         \t\tnode.name = mesh_children[0].name\n\
         \t\tnode.mesh = load(resources[0][\"resource\"])\n\
         \t\tnode.transform = transforms[node.name]\n\
         \t\tscene_root = node\n\
         \telse:\n\
         \t\tscene_root = Node3D.new()\n\
         \t\tscene_root.name = \"MeshRoot\"\n\
         \t\tfor r in resources:\n\
         \t\t\tvar node = MeshInstance3D.new()\n\
         \t\t\tnode.name = r[\"name\"]\n\
         \t\t\tnode.mesh = load(r[\"resource\"])\n\
         \t\t\tnode.transform = transforms[r[\"name\"]]\n\
         \t\t\tscene_root.add_child(node)\n\
         \t\t\tnode.owner = scene_root\n\
         \tvar scene = PackedScene.new()\n\
         \tscene.pack(scene_root)\n\
         \tvar err = ResourceSaver.save(scene, \"{tscn_path}\")\n\
         \tscene_root.queue_free()\n\
         \tif err != OK: return \"ERROR: failed to save scene: \" + str(err)\n\
         \tvar d = {{}}\n\
         \td[\"path\"] = \"{tscn_path}\"\n\
         \td[\"parts\"] = resources\n\
         \td[\"part_count\"] = mesh_children.size()\n\
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

/// Generate the GDScript for `mesh taper`.
///
/// Scales vertices along the taper axis between `start_scale` and `end_scale`.
/// Vertices at the min extent of the axis get `start_scale`, at the max extent
/// get `end_scale`, with linear interpolation between.
pub fn generate_taper(axis: &str, start_scale: f64, end_scale: f64) -> String {
    let (axis_component, other1, other2) = match axis {
        "x" => ("x", "y", "z"),
        "z" => ("z", "x", "y"),
        _ => ("y", "x", "z"), // y default
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
         \tvar axis_min = INF\n\
         \tvar axis_max = -INF\n\
         \tfor v in verts:\n\
         \t\tif v.{axis_component} < axis_min: axis_min = v.{axis_component}\n\
         \t\tif v.{axis_component} > axis_max: axis_max = v.{axis_component}\n\
         \tvar axis_range = axis_max - axis_min\n\
         \tif axis_range < 0.0001: return \"ERROR: mesh has no extent along {axis} axis\"\n\
         \tvar start_s = {start_scale}\n\
         \tvar end_s = {end_scale}\n\
         \tvar center_{other1} = 0.0\n\
         \tvar center_{other2} = 0.0\n\
         \tfor v in verts:\n\
         \t\tcenter_{other1} += v.{other1}\n\
         \t\tcenter_{other2} += v.{other2}\n\
         \tcenter_{other1} /= verts.size()\n\
         \tcenter_{other2} /= verts.size()\n\
         \tfor i in verts.size():\n\
         \t\tvar v = verts[i]\n\
         \t\tvar t = (v.{axis_component} - axis_min) / axis_range\n\
         \t\tvar s = start_s + (end_s - start_s) * t\n\
         \t\tv.{other1} = center_{other1} + (v.{other1} - center_{other1}) * s\n\
         \t\tv.{other2} = center_{other2} + (v.{other2} - center_{other2}) * s\n\
         \t\tverts[i] = v\n\
         \tarrays[Mesh.ARRAY_VERTEX] = verts\n\
         \tarrays[Mesh.ARRAY_NORMAL] = null\n\
         \tvar st = SurfaceTool.new()\n\
         \tst.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \tfor v in verts:\n\
         \t\tst.add_vertex(v)\n\
         \tst.generate_normals()\n\
         \tvar mesh = st.commit()\n\
         \tvar norms = mesh.surface_get_arrays(0)[Mesh.ARRAY_NORMAL]\n\
         \tvar centroid = Vector3.ZERO\n\
         \tfor v in verts:\n\
         \t\tcentroid += v\n\
         \tcentroid /= verts.size()\n\
         \tvar need_fix = false\n\
         \tfor ti in range(0, verts.size(), 3):\n\
         \t\tvar a = verts[ti]\n\
         \t\tvar b = verts[ti + 1]\n\
         \t\tvar c = verts[ti + 2]\n\
         \t\tvar face_n = (b - a).cross(c - a)\n\
         \t\tvar tri_center = (a + b + c) / 3.0\n\
         \t\tif face_n.dot(tri_center - centroid) < 0:\n\
         \t\t\tverts[ti + 1] = c\n\
         \t\t\tverts[ti + 2] = b\n\
         \t\t\tneed_fix = true\n\
         \tif need_fix:\n\
         \t\tvar st2 = SurfaceTool.new()\n\
         \t\tst2.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \t\tfor v in verts:\n\
         \t\t\tst2.add_vertex(v)\n\
         \t\tst2.generate_normals()\n\
         \t\tmesh_inst.mesh = st2.commit()\n\
         \telse:\n\
         \t\tmesh_inst.mesh = mesh\n\
         \tmesh_inst.material_override = null\n\
         \tvar vc = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar d = {{}}\n\
         \td[\"axis\"] = \"{axis}\"\n\
         \td[\"start_scale\"] = start_s\n\
         \td[\"end_scale\"] = end_s\n\
         \td[\"vertex_count\"] = vc\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh bevel`.
///
/// Chamfers all sharp edges by inserting new faces. Works by detecting edges
/// shared by exactly two triangles with a dihedral angle above a threshold,
/// then cutting corners by offsetting vertices along edge normals.
#[allow(clippy::too_many_lines)]
pub fn generate_bevel(radius: f64, segments: u32) -> String {
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
         \tvar radius = {radius}\n\
         \tvar segments = {segments}\n\
         \tvar edge_map = {{}}\n\
         \tfor ti in range(0, verts.size(), 3):\n\
         \t\tvar tri_n = (verts[ti+1] - verts[ti]).cross(verts[ti+2] - verts[ti]).normalized()\n\
         \t\tfor ei in 3:\n\
         \t\t\tvar a = verts[ti + ei]\n\
         \t\t\tvar b = verts[ti + (ei + 1) % 3]\n\
         \t\t\tvar ka = \"%0.4f,%0.4f,%0.4f\" % [a.x, a.y, a.z]\n\
         \t\t\tvar kb = \"%0.4f,%0.4f,%0.4f\" % [b.x, b.y, b.z]\n\
         \t\t\tvar key = ka + \"|\" + kb if ka < kb else kb + \"|\" + ka\n\
         \t\t\tif not edge_map.has(key):\n\
         \t\t\t\tedge_map[key] = []\n\
         \t\t\tedge_map[key].append({{\"normal\": tri_n, \"a\": a, \"b\": b}})\n\
         \tvar sharp_edges = []\n\
         \tfor key in edge_map:\n\
         \t\tvar faces = edge_map[key]\n\
         \t\tif faces.size() == 2:\n\
         \t\t\tvar dot = faces[0][\"normal\"].dot(faces[1][\"normal\"])\n\
         \t\t\tif dot < 0.95:\n\
         \t\t\t\tsharp_edges.append({{\"a\": faces[0][\"a\"], \"b\": faces[0][\"b\"], \"n1\": faces[0][\"normal\"], \"n2\": faces[1][\"normal\"]}})\n\
         \tvar new_verts = []\n\
         \tfor v in verts:\n\
         \t\tnew_verts.append(v)\n\
         \tfor edge in sharp_edges:\n\
         \t\tvar ea = edge[\"a\"]\n\
         \t\tvar eb = edge[\"b\"]\n\
         \t\tvar n1 = edge[\"n1\"]\n\
         \t\tvar n2 = edge[\"n2\"]\n\
         \t\tvar edge_dir = (eb - ea).normalized()\n\
         \t\tvar off1 = n1 * radius\n\
         \t\tvar off2 = n2 * radius\n\
         \t\tfor s in segments:\n\
         \t\t\tvar t0 = float(s) / segments\n\
         \t\t\tvar t1 = float(s + 1) / segments\n\
         \t\t\tvar p0a = ea + off1.lerp(off2, t0)\n\
         \t\t\tvar p0b = eb + off1.lerp(off2, t0)\n\
         \t\t\tvar p1a = ea + off1.lerp(off2, t1)\n\
         \t\t\tvar p1b = eb + off1.lerp(off2, t1)\n\
         \t\t\tnew_verts.append(p0a)\n\
         \t\t\tnew_verts.append(p0b)\n\
         \t\t\tnew_verts.append(p1a)\n\
         \t\t\tnew_verts.append(p0b)\n\
         \t\t\tnew_verts.append(p1b)\n\
         \t\t\tnew_verts.append(p1a)\n\
         \tvar st = SurfaceTool.new()\n\
         \tst.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \tfor v in new_verts:\n\
         \t\tst.add_vertex(v)\n\
         \tst.generate_normals()\n\
         \tvar mesh = st.commit()\n\
         \tvar final_verts = mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX]\n\
         \tvar centroid = Vector3.ZERO\n\
         \tfor v in final_verts:\n\
         \t\tcentroid += v\n\
         \tcentroid /= final_verts.size()\n\
         \tvar need_fix = false\n\
         \tfor ti in range(0, final_verts.size(), 3):\n\
         \t\tvar a = final_verts[ti]\n\
         \t\tvar b = final_verts[ti + 1]\n\
         \t\tvar c = final_verts[ti + 2]\n\
         \t\tvar face_n = (b - a).cross(c - a)\n\
         \t\tvar tri_center = (a + b + c) / 3.0\n\
         \t\tif face_n.dot(tri_center - centroid) < 0:\n\
         \t\t\tfinal_verts[ti + 1] = c\n\
         \t\t\tfinal_verts[ti + 2] = b\n\
         \t\t\tneed_fix = true\n\
         \tif need_fix:\n\
         \t\tvar st2 = SurfaceTool.new()\n\
         \t\tst2.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \t\tfor v in final_verts:\n\
         \t\t\tst2.add_vertex(v)\n\
         \t\tst2.generate_normals()\n\
         \t\tmesh_inst.mesh = st2.commit()\n\
         \telse:\n\
         \t\tmesh_inst.mesh = mesh\n\
         \tmesh_inst.material_override = null\n\
         \tvar vc = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar d = {{}}\n\
         \td[\"radius\"] = radius\n\
         \td[\"segments\"] = segments\n\
         \td[\"sharp_edges\"] = sharp_edges.size()\n\
         \td[\"vertex_count\"] = vc\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh info`.
pub fn generate_info() -> String {
    "extends Node\n\
     \n\
     func run():\n\
     \tvar root = get_tree().get_root()\n\
     \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
     \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
     \tvar mesh_name = helper.get_meta(\"active_mesh\")\n\
     \tvar mesh_inst = helper.get_node_or_null(mesh_name)\n\
     \tif mesh_inst == null: return \"ERROR: mesh node not found\"\n\
     \tvar d = {}\n\
     \td[\"name\"] = mesh_name\n\
     \td[\"has_mesh\"] = mesh_inst.mesh != null\n\
     \tif mesh_inst.mesh:\n\
     \t\tvar m = mesh_inst.mesh\n\
     \t\td[\"surface_count\"] = m.get_surface_count()\n\
     \t\tif m.get_surface_count() > 0:\n\
     \t\t\tvar arrays = m.surface_get_arrays(0)\n\
     \t\t\tvar verts = arrays[Mesh.ARRAY_VERTEX]\n\
     \t\t\td[\"vertex_count\"] = verts.size()\n\
     \t\t\td[\"face_count\"] = verts.size() / 3\n\
     \t\t\tvar aabb = m.get_aabb()\n\
     \t\t\td[\"aabb_position\"] = [aabb.position.x, aabb.position.y, aabb.position.z]\n\
     \t\t\td[\"aabb_size\"] = [aabb.size.x, aabb.size.y, aabb.size.z]\n\
     \t\t\td[\"aabb_end\"] = [aabb.end.x, aabb.end.y, aabb.end.z]\n\
     \telse:\n\
     \t\td[\"vertex_count\"] = 0\n\
     \td[\"profile_plane\"] = helper.get_meta(\"profile_plane\")\n\
     \td[\"profile_point_count\"] = helper.get_meta(\"profile_points\").size()\n\
     \treturn JSON.stringify(d)\n"
        .to_string()
}

/// Generate the GDScript for `mesh translate`.
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
         \tvar old_pos = target.position\n\
         {position_line}\
         \tvar d = {{}}\n\
         \td[\"name\"] = name\n\
         \td[\"old_position\"] = [old_pos.x, old_pos.y, old_pos.z]\n\
         \td[\"new_position\"] = [target.position.x, target.position.y, target.position.z]\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh rotate`.
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
         \tvar old_rot = target.rotation_degrees\n\
         \ttarget.rotation_degrees = Vector3({rx}, {ry}, {rz})\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = name\n\
         \td[\"old_rotation\"] = [old_rot.x, old_rot.y, old_rot.z]\n\
         \td[\"new_rotation\"] = [{rx}, {ry}, {rz}]\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh scale`.
pub fn generate_scale(part: Option<&str>, sx: f64, sy: f64, sz: f64) -> String {
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
         \tvar old_scale = target.scale\n\
         \ttarget.scale = Vector3({sx}, {sy}, {sz})\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = name\n\
         \td[\"old_scale\"] = [old_scale.x, old_scale.y, old_scale.z]\n\
         \td[\"new_scale\"] = [{sx}, {sy}, {sz}]\n\
         \treturn JSON.stringify(d)\n"
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
         \td[\"parts\"] = new_parts\n\
         \td[\"active\"] = helper.get_meta(\"active_mesh\", \"\")\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh info --all`.
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
     \t\tif mi and mi.mesh and mi.mesh.get_surface_count() > 0:\n\
     \t\t\tvar verts = mi.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX]\n\
     \t\t\tpd[\"vertex_count\"] = verts.size()\n\
     \t\t\tpd[\"face_count\"] = verts.size() / 3\n\
     \t\t\ttotal_vc += verts.size()\n\
     \t\t\ttotal_fc += verts.size() / 3\n\
     \t\t\tvar aabb = mi.mesh.get_aabb()\n\
     \t\t\tpd[\"aabb_position\"] = [aabb.position.x, aabb.position.y, aabb.position.z]\n\
     \t\t\tpd[\"aabb_size\"] = [aabb.size.x, aabb.size.y, aabb.size.z]\n\
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

/// Generate the GDScript for `mesh add-part`.
pub fn generate_add_part(name: &str, primitive: &str) -> String {
    let primitive_line = match primitive {
        "cube" => "\tmesh_inst.mesh = BoxMesh.new()\n",
        "sphere" => "\tmesh_inst.mesh = SphereMesh.new()\n",
        "cylinder" => "\tmesh_inst.mesh = CylinderMesh.new()\n",
        _ => "",
    };

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
         {primitive_line}\
         \thelper.add_child(mesh_inst)\n\
         \tparts.append(\"{name}\")\n\
         \thelper.set_meta(\"mesh_parts\", parts)\n\
         \thelper.set_meta(\"active_mesh\", \"{name}\")\n\
         \thelper.set_meta(\"profile_points\", [])\n\
         \thelper.set_meta(\"profile_plane\", \"\")\n\
         \tfor child in helper.get_children():\n\
         \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\tchild.visible = (child.name == \"{name}\")\n\
         \tvar aabb = mesh_inst.get_aabb() if mesh_inst.mesh else AABB(Vector3.ZERO, Vector3(2, 2, 2))\n\
         \tvar center = aabb.get_center()\n\
         \tvar dims = aabb.size\n\
         \tvar sz = max(max(dims.x, dims.y), dims.z) * 1.5\n\
         \tif sz < 2.0: sz = 2.0\n\
         \t_retarget(helper, center, sz)\n\
         \tvar vc = 0\n\
         \tif mesh_inst.mesh and mesh_inst.mesh.get_surface_count() > 0:\n\
         \t\tvc = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar part_names = []\n\
         \tfor p in parts:\n\
         \t\tpart_names.append(p)\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = \"{name}\"\n\
         \td[\"parts\"] = part_names\n\
         \td[\"active\"] = \"{name}\"\n\
         \td[\"vertex_count\"] = vc\n\
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
         \tvar parts = helper.get_meta(\"mesh_parts\", [])\n\
         \tvar part_names = []\n\
         \tfor p in parts:\n\
         \t\tpart_names.append(p)\n\
         \tvar d = {{}}\n\
         \td[\"active\"] = \"{name}\"\n\
         \td[\"parts\"] = part_names\n\
         \td[\"vertex_count\"] = vc\n\
         \tvar ab = target.get_aabb() if target.mesh else AABB()\n\
         \td[\"aabb_position\"] = [ab.position.x, ab.position.y, ab.position.z]\n\
         \td[\"aabb_size\"] = [ab.size.x, ab.size.y, ab.size.z]\n\
         \treturn JSON.stringify(d)\n"
    )
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
     \tvar part_names = []\n\
     \tfor p in parts:\n\
     \t\tpart_names.append(p)\n\
     \tvar d = {}\n\
     \td[\"active\"] = active\n\
     \td[\"parts\"] = part_names\n\
     \td[\"visible\"] = \"all\"\n\
     \treturn JSON.stringify(d)\n"
        .to_string()
}
