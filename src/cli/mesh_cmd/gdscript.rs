/// GDScript array literal — 8 distinct medium-saturation colors for auto-assignment.
const PALETTE: &str = "[Color(0.69,0.69,0.69),Color(0.88,0.44,0.31),\
     Color(0.31,0.63,0.88),Color(0.38,0.75,0.38),\
     Color(0.88,0.75,0.31),Color(0.63,0.38,0.75),\
     Color(0.31,0.75,0.69),Color(0.88,0.50,0.56)]";

/// GDScript snippet to restore part color from metadata after a mesh rebuild.
/// Assumes `mesh_inst` is in scope.
const RESTORE_COLOR: &str = "\tif mesh_inst.has_meta(\"part_color\"):\n\
     \t\tvar _mat = StandardMaterial3D.new()\n\
     \t\t_mat.albedo_color = mesh_inst.get_meta(\"part_color\")\n\
     \t\tmesh_inst.material_override = _mat\n\
     \telse:\n\
     \t\tmesh_inst.material_override = null\n";

/// Generate the GDScript for `mesh create`.
#[allow(clippy::too_many_lines)]
pub fn generate_create(name: &str, primitive: &str) -> String {
    let (primitive_line, primitive_size) = match primitive {
        "cube" => ("\tmesh_inst.mesh = BoxMesh.new()\n", "[1, 1, 1]"),
        "sphere" => ("\tmesh_inst.mesh = SphereMesh.new()\n", "[2, 2, 2]"),
        "cylinder" => ("\tmesh_inst.mesh = CylinderMesh.new()\n", "[2, 2, 2]"),
        _ => ("", "[0, 0, 0]"),
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
         \tenv_res.background_color = Color(0.08, 0.12, 0.18)\n\
         \tenv_res.ambient_light_source = 1\n\
         \tenv_res.ambient_light_color = Color(0.3, 0.3, 0.3)\n\
         \tenv_res.tonemap_mode = 3\n\
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
         \tvar mesh_inst = MeshInstance3D.new()\n\
         \tmesh_inst.name = \"{name}\"\n\
         {primitive_line}\
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
         \tvar vc = 0\n\
         \tif mesh_inst.mesh and mesh_inst.mesh.get_surface_count() > 0:\n\
         \t\tvc = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = \"{name}\"\n\
         \td[\"primitive\"] = \"{primitive}\"\n\
         \td[\"default_size\"] = {primitive_size}\n\
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
         \tif mesh_inst.has_meta(\"part_color\"):\n\
         \t\tmat.albedo_color = mesh_inst.get_meta(\"part_color\")\n\
         \tmesh_inst.material_override = mat\n\
         \tvar d = {{}}\n\
         \td[\"plane\"] = \"{plane}\"\n\
         \td[\"point_count\"] = points.size()\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh extrude`.
///
/// Uses analytic winding based on the 2D signed area of the profile and the
/// plane's coordinate handedness. The "front" plane maps (X,Y) right-handed,
/// while "side" and "top" produce a parity flip, so we XOR the two conditions:
/// `flip = (ccw_profile) != (plane == "front")`.
///
/// Caps and side walls use the SAME `flip` variable but with OPPOSITE sense:
/// `Geometry2D.triangulate_polygon` returns indices in Godot's Y-down 2D
/// convention, which inverts the winding relative to the profile's Y-up math.
/// So caps swap their if/else branches compared to side walls.
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
         \tvar area2 = 0.0\n\
         \tfor i in points.size():\n\
         \t\tvar jw = (i + 1) % points.size()\n\
         \t\tarea2 += points[i][0] * points[jw][1] - points[jw][0] * points[i][1]\n\
         \tvar flip = (area2 > 0) != (plane == \"front\")\n\
         \tvar st = SurfaceTool.new()\n\
         \tst.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \tfor ti in range(0, indices.size(), 3):\n\
         \t\tif flip:\n\
         \t\t\tst.add_vertex(front[indices[ti]])\n\
         \t\t\tst.add_vertex(front[indices[ti + 1]])\n\
         \t\t\tst.add_vertex(front[indices[ti + 2]])\n\
         \t\telse:\n\
         \t\t\tst.add_vertex(front[indices[ti + 2]])\n\
         \t\t\tst.add_vertex(front[indices[ti + 1]])\n\
         \t\t\tst.add_vertex(front[indices[ti]])\n\
         \tfor ti in range(0, indices.size(), 3):\n\
         \t\tif flip:\n\
         \t\t\tst.add_vertex(back[indices[ti + 2]])\n\
         \t\t\tst.add_vertex(back[indices[ti + 1]])\n\
         \t\t\tst.add_vertex(back[indices[ti]])\n\
         \t\telse:\n\
         \t\t\tst.add_vertex(back[indices[ti]])\n\
         \t\t\tst.add_vertex(back[indices[ti + 1]])\n\
         \t\t\tst.add_vertex(back[indices[ti + 2]])\n\
         \tvar n_pts = front.size()\n\
         \tfor i in n_pts:\n\
         \t\tvar j = (i + 1) % n_pts\n\
         \t\tif flip:\n\
         \t\t\tst.add_vertex(front[i])\n\
         \t\t\tst.add_vertex(front[j])\n\
         \t\t\tst.add_vertex(back[i])\n\
         \t\t\tst.add_vertex(front[j])\n\
         \t\t\tst.add_vertex(back[j])\n\
         \t\t\tst.add_vertex(back[i])\n\
         \t\telse:\n\
         \t\t\tst.add_vertex(front[i])\n\
         \t\t\tst.add_vertex(back[i])\n\
         \t\t\tst.add_vertex(front[j])\n\
         \t\t\tst.add_vertex(front[j])\n\
         \t\t\tst.add_vertex(back[i])\n\
         \t\t\tst.add_vertex(back[j])\n\
         \tst.generate_normals()\n\
         \tmesh_inst.mesh = st.commit()\n\
         {RESTORE_COLOR}\
         \tvar vc = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar fc = vc / 3\n\
         \tvar d = {{}}\n\
         \td[\"depth\"] = {depth}\n\
         \td[\"plane\"] = plane\n\
         \td[\"depth_range\"] = [-{half}, {half}]\n\
         \td[\"vertex_count\"] = vc\n\
         \td[\"face_count\"] = fc\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh revolve`.
///
/// After building the surface of revolution, checks the first triangle's normal
/// against the "away from revolution axis" direction. If inverted, flips all
/// triangle windings (all quads share the same winding pattern).
#[allow(clippy::too_many_lines)]
pub fn generate_revolve(axis: &str, angle: f64, segments: u32, cap: bool) -> String {
    let cap_code = if cap {
        "\tif angle_deg < 360.0:\n\
         \t\tvar wall_arr = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX]\n\
         \t\tvar st3 = SurfaceTool.new()\n\
         \t\tst3.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \t\tfor v in wall_arr:\n\
         \t\t\tst3.add_vertex(v)\n\
         \t\tvar c0 = Vector3.ZERO\n\
         \t\tfor v in rings[0]: c0 += v\n\
         \t\tc0 /= pn\n\
         \t\tvar tang0 = rings[1][0] - rings[0][0]\n\
         \t\tvar e1 = rings[0][0] - c0\n\
         \t\tvar e2 = rings[0][1] - c0\n\
         \t\tvar tn = e1.cross(e2)\n\
         \t\tvar flip0 = tn.dot(-tang0) < 0\n\
         \t\tfor i in pn:\n\
         \t\t\tvar j = (i + 1) % pn\n\
         \t\t\tif flip0:\n\
         \t\t\t\tst3.add_vertex(c0)\n\
         \t\t\t\tst3.add_vertex(rings[0][j])\n\
         \t\t\t\tst3.add_vertex(rings[0][i])\n\
         \t\t\telse:\n\
         \t\t\t\tst3.add_vertex(c0)\n\
         \t\t\t\tst3.add_vertex(rings[0][i])\n\
         \t\t\t\tst3.add_vertex(rings[0][j])\n\
         \t\tvar cN = Vector3.ZERO\n\
         \t\tfor v in rings[segments]: cN += v\n\
         \t\tcN /= pn\n\
         \t\tvar tangN = rings[segments][0] - rings[segments - 1][0]\n\
         \t\te1 = rings[segments][0] - cN\n\
         \t\te2 = rings[segments][1] - cN\n\
         \t\ttn = e1.cross(e2)\n\
         \t\tvar flipN = tn.dot(tangN) < 0\n\
         \t\tfor i in pn:\n\
         \t\t\tvar j = (i + 1) % pn\n\
         \t\t\tif flipN:\n\
         \t\t\t\tst3.add_vertex(cN)\n\
         \t\t\t\tst3.add_vertex(rings[segments][j])\n\
         \t\t\t\tst3.add_vertex(rings[segments][i])\n\
         \t\t\telse:\n\
         \t\t\t\tst3.add_vertex(cN)\n\
         \t\t\t\tst3.add_vertex(rings[segments][i])\n\
         \t\t\t\tst3.add_vertex(rings[segments][j])\n\
         \t\tst3.generate_normals()\n\
         \t\tmesh_inst.mesh = st3.commit()\n"
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
         \tvar arr = mesh.surface_get_arrays(0)\n\
         \tvar verts = arr[Mesh.ARRAY_VERTEX]\n\
         \tvar norms = arr[Mesh.ARRAY_NORMAL]\n\
         \tvar tc = (verts[0] + verts[1] + verts[2]) / 3.0\n\
         \tvar outward\n\
         \tif axis == \"x\":\n\
         \t\toutward = Vector3(0, tc.y, tc.z)\n\
         \telif axis == \"y\":\n\
         \t\toutward = Vector3(tc.x, 0, tc.z)\n\
         \telse:\n\
         \t\toutward = Vector3(tc.x, tc.y, 0)\n\
         \tif outward.length() > 0.001 and norms[0].dot(outward) < 0:\n\
         \t\tvar st2 = SurfaceTool.new()\n\
         \t\tst2.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \t\tfor ti in range(0, verts.size(), 3):\n\
         \t\t\tst2.add_vertex(verts[ti + 2])\n\
         \t\t\tst2.add_vertex(verts[ti + 1])\n\
         \t\t\tst2.add_vertex(verts[ti])\n\
         \t\tst2.generate_normals()\n\
         \t\tmesh_inst.mesh = st2.commit()\n\
         \telse:\n\
         \t\tmesh_inst.mesh = mesh\n\
         {cap_code}\
         {RESTORE_COLOR}\
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
         \tverts[idx] = verts[idx] + Vector3({dx}, {dy}, {dz})\n\
         \tvar am = ArrayMesh.new()\n\
         \tarrays[Mesh.ARRAY_VERTEX] = verts\n\
         \tam.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)\n\
         \tmesh_inst.mesh = am\n\
         \tvar d = {{}}\n\
         \td[\"index\"] = idx\n\
         \td[\"position\"] = [verts[idx].x, verts[idx].y, verts[idx].z]\n\
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
         \tvar materials = {{}}\n\
         \tfor mi in mesh_children:\n\
         \t\tvar res_path = \"{base_path}_\" + mi.name + \".tres\"\n\
         \t\tvar err = ResourceSaver.save(mi.mesh, res_path)\n\
         \t\tif err != OK: return \"ERROR: failed to save mesh '\" + mi.name + \"': \" + str(err)\n\
         \t\tresources.append({{\"name\": mi.name, \"resource\": res_path}})\n\
         \t\ttransforms[mi.name] = mi.transform\n\
         \t\tif mi.material_override:\n\
         \t\t\tvar mat_path = \"{base_path}_\" + mi.name + \"_mat.tres\"\n\
         \t\t\tvar merr = ResourceSaver.save(mi.material_override, mat_path)\n\
         \t\t\tif merr == OK:\n\
         \t\t\t\tmaterials[mi.name] = mat_path\n\
         \tvar scene_root\n\
         \tif mesh_children.size() == 1:\n\
         \t\tvar node = MeshInstance3D.new()\n\
         \t\tnode.name = mesh_children[0].name\n\
         \t\tnode.mesh = load(resources[0][\"resource\"])\n\
         \t\tnode.transform = transforms[node.name]\n\
         \t\tif materials.has(node.name):\n\
         \t\t\tnode.material_override = load(materials[node.name])\n\
         \t\tscene_root = node\n\
         \telse:\n\
         \t\tscene_root = Node3D.new()\n\
         \t\tscene_root.name = \"MeshRoot\"\n\
         \t\tfor r in resources:\n\
         \t\t\tvar node = MeshInstance3D.new()\n\
         \t\t\tnode.name = r[\"name\"]\n\
         \t\t\tnode.mesh = load(r[\"resource\"])\n\
         \t\t\tnode.transform = transforms[r[\"name\"]]\n\
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
/// get `end_scale`, with linear interpolation between. Preserves the input
/// mesh's winding order (no centroid fix needed — relies on correct extrude).
pub fn generate_taper(
    part: Option<&str>,
    axis: &str,
    start_scale: f64,
    end_scale: f64,
    midpoint: Option<f64>,
    range: Option<(f64, f64)>,
) -> String {
    let (axis_component, other1, other2) = match axis {
        "x" => ("x", "y", "z"),
        "z" => ("z", "x", "y"),
        _ => ("y", "x", "z"), // y default
    };
    let target = part.map_or(
        String::from("\tvar mesh_name = helper.get_meta(\"active_mesh\")\n"),
        |p| format!("\tvar mesh_name = \"{p}\"\n"),
    );
    let midpoint_val = midpoint.unwrap_or(-1.0);
    let (range_from, range_to) = range.unwrap_or((-1.0, -1.0));
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         {target}\
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
         \tvar mid = {midpoint_val}\n\
         \tvar r_from = {range_from}\n\
         \tvar r_to = {range_to}\n\
         \tfor i in verts.size():\n\
         \t\tvar v = verts[i]\n\
         \t\tvar t = (v.{axis_component} - axis_min) / axis_range\n\
         \t\tvar s\n\
         \t\tif r_from >= 0 and (t < r_from or t > r_to):\n\
         \t\t\ts = 1.0\n\
         \t\telse:\n\
         \t\t\tvar lt = t\n\
         \t\t\tif r_from >= 0:\n\
         \t\t\t\tlt = (t - r_from) / (r_to - r_from) if (r_to - r_from) > 0.0001 else 0.0\n\
         \t\t\tif mid < 0:\n\
         \t\t\t\ts = start_s + (end_s - start_s) * lt\n\
         \t\t\telif lt <= mid:\n\
         \t\t\t\ts = end_s + (start_s - end_s) * (lt / mid) if mid > 0.0001 else start_s\n\
         \t\t\telse:\n\
         \t\t\t\ts = start_s + (end_s - start_s) * ((lt - mid) / (1.0 - mid)) if (1.0 - mid) > 0.0001 else end_s\n\
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
         \tmesh_inst.mesh = st.commit()\n\
         {RESTORE_COLOR}\
         \tvar vc = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar d = {{}}\n\
         \td[\"axis\"] = \"{axis}\"\n\
         \td[\"start_scale\"] = start_s\n\
         \td[\"end_scale\"] = end_s\n\
         \tif mid >= 0:\n\
         \t\td[\"midpoint\"] = mid\n\
         \tif r_from >= 0:\n\
         \t\td[\"range\"] = [r_from, r_to]\n\
         \td[\"vertex_count\"] = vc\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh bevel`.
///
/// Chamfers all sharp edges by inserting new faces. Works by detecting edges
/// shared by exactly two triangles with a dihedral angle above a threshold,
/// then cutting corners by offsetting vertices along edge normals. Preserves
/// the input mesh's winding order (no centroid fix needed).
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
         \tmesh_inst.mesh = st.commit()\n\
         {RESTORE_COLOR}\
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

/// Generate the GDScript for `mesh checkpoint`.
///
/// Saves the current surface arrays + material color for every part as metadata.
pub fn generate_checkpoint() -> String {
    "extends Node\n\
     \n\
     func run():\n\
     \tvar root = get_tree().get_root()\n\
     \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
     \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
     \tvar count = 0\n\
     \tfor child in helper.get_children():\n\
     \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
     \t\t\tif child.mesh and child.mesh.get_surface_count() > 0:\n\
     \t\t\t\tvar arrays = child.mesh.surface_get_arrays(0)\n\
     \t\t\t\tchild.set_meta(\"checkpoint_verts\", arrays[Mesh.ARRAY_VERTEX])\n\
     \t\t\t\tchild.set_meta(\"checkpoint_normals\", arrays[Mesh.ARRAY_NORMAL])\n\
     \t\t\tcount += 1\n\
     \tvar d = {}\n\
     \td[\"parts_saved\"] = count\n\
     \treturn JSON.stringify(d)\n"
        .to_string()
}

/// Generate the GDScript for `mesh restore`.
///
/// Rebuilds meshes from saved checkpoint metadata.
pub fn generate_restore() -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         \tvar count = 0\n\
         \tfor child in helper.get_children():\n\
         \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
         \t\t\tif child.has_meta(\"checkpoint_verts\"):\n\
         \t\t\t\tvar verts = child.get_meta(\"checkpoint_verts\")\n\
         \t\t\t\tvar st = SurfaceTool.new()\n\
         \t\t\t\tst.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \t\t\t\tfor v in verts:\n\
         \t\t\t\t\tst.add_vertex(v)\n\
         \t\t\t\tst.generate_normals()\n\
         \t\t\t\tchild.mesh = st.commit()\n\
         {RESTORE_COLOR}\
         \t\t\t\tcount += 1\n\
         \tvar d = {{}}\n\
         \td[\"parts_restored\"] = count\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh flip-normals`.
///
/// Inverts all triangle winding on the target part, then regenerates normals.
pub fn generate_flip_normals(part: Option<&str>, caps: Option<&str>) -> String {
    let target = part.map_or(
        String::from("\tvar mesh_name = helper.get_meta(\"active_mesh\")\n"),
        |p| format!("\tvar mesh_name = \"{p}\"\n"),
    );
    let caps_axis = match caps {
        Some("x") => "Vector3(1, 0, 0)",
        Some("y") => "Vector3(0, 1, 0)",
        Some("z") => "Vector3(0, 0, 1)",
        _ => "",
    };
    let filter_code = if caps.is_some() {
        format!(
            "\tvar cap_axis = {caps_axis}\n\
             \tvar flipped_count = 0\n\
             \tvar flipped = PackedVector3Array()\n\
             \tflipped.resize(verts.size())\n\
             \tfor i in range(0, verts.size(), 3):\n\
             \t\tvar e1 = verts[i + 1] - verts[i]\n\
             \t\tvar e2 = verts[i + 2] - verts[i]\n\
             \t\tvar fn = e1.cross(e2).normalized()\n\
             \t\tif abs(fn.dot(cap_axis)) > 0.7:\n\
             \t\t\tflipped[i] = verts[i + 2]\n\
             \t\t\tflipped[i + 1] = verts[i + 1]\n\
             \t\t\tflipped[i + 2] = verts[i]\n\
             \t\t\tflipped_count += 1\n\
             \t\telse:\n\
             \t\t\tflipped[i] = verts[i]\n\
             \t\t\tflipped[i + 1] = verts[i + 1]\n\
             \t\t\tflipped[i + 2] = verts[i + 2]\n"
        )
    } else {
        "\tvar flipped_count = verts.size() / 3\n\
         \tvar flipped = PackedVector3Array()\n\
         \tflipped.resize(verts.size())\n\
         \tfor i in range(0, verts.size(), 3):\n\
         \t\tflipped[i] = verts[i + 2]\n\
         \t\tflipped[i + 1] = verts[i + 1]\n\
         \t\tflipped[i + 2] = verts[i]\n"
            .to_string()
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
         \tif mesh_inst.mesh == null or mesh_inst.mesh.get_surface_count() == 0: return \"ERROR: no mesh data\"\n\
         \tvar verts = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX]\n\
         {filter_code}\
         \tvar st = SurfaceTool.new()\n\
         \tst.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \tfor v in flipped:\n\
         \t\tst.add_vertex(v)\n\
         \tst.generate_normals()\n\
         \tmesh_inst.mesh = st.commit()\n\
         {RESTORE_COLOR}\
         \tvar vc = flipped.size()\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = mesh_name\n\
         \td[\"vertex_count\"] = vc\n\
         \td[\"face_count\"] = vc / 3\n\
         \td[\"flipped_faces\"] = flipped_count\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh duplicate-part --mirror`.
///
/// Duplicates the source part, then mirrors all mesh vertices across the given axis
/// and reverses triangle winding to fix normals (flipping one axis inverts handedness).
/// Also mirrors the transform position on the same axis.
#[allow(clippy::too_many_lines)]
pub fn generate_mirror_part(src: &str, dst: &str, axis: &str) -> String {
    // Which component to negate: x=0, y=1, z=2
    let axis_idx = match axis {
        "x" => "0",
        "y" => "1",
        _ => "2",
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
         \tmi.position = pos\n\
         \tif src.mesh and src.mesh.get_surface_count() > 0:\n\
         \t\tvar arrays = src.mesh.surface_get_arrays(0)\n\
         \t\tvar verts = arrays[Mesh.ARRAY_VERTEX]\n\
         \t\tvar st = SurfaceTool.new()\n\
         \t\tst.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \t\tfor i in range(0, verts.size(), 3):\n\
         \t\t\tvar v0 = verts[i]\n\
         \t\t\tvar v1 = verts[i + 1]\n\
         \t\t\tvar v2 = verts[i + 2]\n\
         \t\t\tv0[axis_idx] = -v0[axis_idx]\n\
         \t\t\tv1[axis_idx] = -v1[axis_idx]\n\
         \t\t\tv2[axis_idx] = -v2[axis_idx]\n\
         \t\t\tst.add_vertex(v0)\n\
         \t\t\tst.add_vertex(v2)\n\
         \t\t\tst.add_vertex(v1)\n\
         \t\tst.generate_normals()\n\
         \t\tmi.mesh = st.commit()\n\
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
         \tmesh_inst.set_meta(\"part_color\", [color.r, color.g, color.b])\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = mesh_name\n\
         \td[\"color\"] = hex\n\
         \td[\"rgb\"] = [snapped(color.r, 0.01), snapped(color.g, 0.01), snapped(color.b, 0.01)]\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate the GDScript for `mesh loop-cut`.
///
/// Splits all triangles that straddle an axis-aligned plane at the given position.
/// Each straddling triangle is split into 2 or 3 sub-triangles by computing edge
/// intersection points. Triangles fully on one side are preserved unchanged.
#[allow(clippy::too_many_lines)]
pub fn generate_loop_cut(part: Option<&str>, axis: &str, at: f64) -> String {
    let axis_component = match axis {
        "x" => "x",
        "z" => "z",
        _ => "y",
    };
    let target = part.map_or(
        String::from("\tvar mesh_name = helper.get_meta(\"active_mesh\")\n"),
        |p| format!("\tvar mesh_name = \"{p}\"\n"),
    );
    format!(
        "extends Node\n\
         \n\
         func _lerp_edge(a, b, plane_val, axis):\n\
         \tvar da = a[axis] - plane_val\n\
         \tvar db = b[axis] - plane_val\n\
         \tvar t = da / (da - db)\n\
         \treturn a.lerp(b, t)\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
         \tif helper == null: return \"ERROR: no mesh session — run 'gd mesh create' first\"\n\
         {target}\
         \tvar mesh_inst = helper.get_node_or_null(mesh_name)\n\
         \tif mesh_inst == null: return \"ERROR: mesh node not found\"\n\
         \tif mesh_inst.mesh == null or mesh_inst.mesh.get_surface_count() == 0: return \"ERROR: no mesh data\"\n\
         \tvar verts = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX]\n\
         \tvar plane_val = {at}\n\
         \tvar axis = \"{axis_component}\"\n\
         \tvar ai\n\
         \tif axis == \"x\": ai = 0\n\
         \telif axis == \"y\": ai = 1\n\
         \telse: ai = 2\n\
         \tvar new_verts = PackedVector3Array()\n\
         \tvar splits = 0\n\
         \tfor ti in range(0, verts.size(), 3):\n\
         \t\tvar v0 = verts[ti]\n\
         \t\tvar v1 = verts[ti + 1]\n\
         \t\tvar v2 = verts[ti + 2]\n\
         \t\tvar s0 = v0[ai] - plane_val\n\
         \t\tvar s1 = v1[ai] - plane_val\n\
         \t\tvar s2 = v2[ai] - plane_val\n\
         \t\tvar pos = (1 if s0 > 0 else 0) + (1 if s1 > 0 else 0) + (1 if s2 > 0 else 0)\n\
         \t\tvar neg = (1 if s0 < 0 else 0) + (1 if s1 < 0 else 0) + (1 if s2 < 0 else 0)\n\
         \t\tif pos == 0 or neg == 0 or (pos + neg) < 2:\n\
         \t\t\tnew_verts.append(v0)\n\
         \t\t\tnew_verts.append(v1)\n\
         \t\t\tnew_verts.append(v2)\n\
         \t\t\tcontinue\n\
         \t\tsplits += 1\n\
         \t\tvar alone\n\
         \t\tvar pair0\n\
         \t\tvar pair1\n\
         \t\tif (s0 >= 0) != (s1 >= 0) and (s0 >= 0) != (s2 >= 0):\n\
         \t\t\talone = v0; pair0 = v1; pair1 = v2\n\
         \t\telif (s1 >= 0) != (s0 >= 0) and (s1 >= 0) != (s2 >= 0):\n\
         \t\t\talone = v1; pair0 = v2; pair1 = v0\n\
         \t\telse:\n\
         \t\t\talone = v2; pair0 = v0; pair1 = v1\n\
         \t\tvar m0 = _lerp_edge(alone, pair0, plane_val, ai)\n\
         \t\tvar m1 = _lerp_edge(alone, pair1, plane_val, ai)\n\
         \t\tnew_verts.append(alone)\n\
         \t\tnew_verts.append(m0)\n\
         \t\tnew_verts.append(m1)\n\
         \t\tnew_verts.append(m0)\n\
         \t\tnew_verts.append(pair0)\n\
         \t\tnew_verts.append(pair1)\n\
         \t\tnew_verts.append(m0)\n\
         \t\tnew_verts.append(pair1)\n\
         \t\tnew_verts.append(m1)\n\
         \tvar st = SurfaceTool.new()\n\
         \tst.begin(Mesh.PRIMITIVE_TRIANGLES)\n\
         \tfor v in new_verts:\n\
         \t\tst.add_vertex(v)\n\
         \tst.generate_normals()\n\
         \tmesh_inst.mesh = st.commit()\n\
         {RESTORE_COLOR}\
         \tvar d = {{}}\n\
         \td[\"name\"] = mesh_name\n\
         \td[\"axis\"] = axis\n\
         \td[\"at\"] = plane_val\n\
         \td[\"triangles_split\"] = splits\n\
         \td[\"vertex_count\"] = new_verts.size()\n\
         \td[\"face_count\"] = new_verts.size() / 3\n\
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
        "glass" => "\tmat.metallic = 0.0\n\
                     \tmat.roughness = 0.05\n\
                     \tmat.transparency = 1\n\
                     \tmat.albedo_color.a = 0.3\n\
                     \tmat.specular = 0.5\n\
                     \tmat.refraction_enabled = true\n\
                     \tmat.refraction_scale = 0.02\n",
        "metal" => "\tmat.metallic = 0.9\n\
                     \tmat.roughness = 0.3\n\
                     \tmat.specular = 0.8\n",
        "chrome" => "\tmat.metallic = 1.0\n\
                      \tmat.roughness = 0.05\n\
                      \tmat.specular = 1.0\n",
        "rubber" => "\tmat.metallic = 0.0\n\
                      \tmat.roughness = 0.95\n\
                      \tmat.specular = 0.1\n",
        "paint" => "\tmat.metallic = 0.1\n\
                     \tmat.roughness = 0.4\n\
                     \tmat.specular = 0.5\n",
        "wood" => "\tmat.metallic = 0.0\n\
                    \tmat.roughness = 0.7\n\
                    \tmat.specular = 0.2\n",
        "matte" => "\tmat.metallic = 0.0\n\
                     \tmat.roughness = 1.0\n\
                     \tmat.specular = 0.0\n",
        // plastic
        _ => "\tmat.metallic = 0.0\n\
              \tmat.roughness = 0.4\n\
              \tmat.specular = 0.5\n",
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
         \tmesh_inst.set_meta(\"part_color\", [c.r, c.g, c.b])\n\
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
     \t\t\tchild.material_override = mat\n\
     \t\t\tcount += 1\n\
     \tvar d = {}\n\
     \td[\"mode\"] = \"normal_debug\"\n\
     \td[\"parts_affected\"] = count\n\
     \treturn JSON.stringify(d)\n"
        .to_string()
}

/// Generate the GDScript to remove the face orientation debug overlay.
pub fn generate_normal_debug_clear() -> String {
    "extends Node\n\
     \n\
     func run():\n\
     \tvar root = get_tree().get_root()\n\
     \tvar helper = root.get_node_or_null(\"_GdMeshHelper\")\n\
     \tif helper == null: return \"ok\"\n\
     \tfor child in helper.get_children():\n\
     \t\tif child is MeshInstance3D and not child.name.begins_with(\"_\"):\n\
     \t\t\tif child.has_meta(\"part_color\"):\n\
     \t\t\t\tvar mat = StandardMaterial3D.new()\n\
     \t\t\t\tmat.albedo_color = child.get_meta(\"part_color\")\n\
     \t\t\t\tchild.material_override = mat\n\
     \t\t\telse:\n\
     \t\t\t\tchild.material_override = null\n\
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
         \t\t\tcam.size = sz\n\
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
         \t\t\tcam.size = sz\n\
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
         \tvar _palette = {PALETTE}\n\
         \tvar _color = _palette[(parts.size() - 1) % _palette.size()]\n\
         \tmesh_inst.set_meta(\"part_color\", _color)\n\
         \tvar _mat = StandardMaterial3D.new()\n\
         \t_mat.albedo_color = _color\n\
         \tmesh_inst.material_override = _mat\n\
         \tvar vc = 0\n\
         \tif mesh_inst.mesh and mesh_inst.mesh.get_surface_count() > 0:\n\
         \t\tvc = mesh_inst.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX].size()\n\
         \tvar d = {{}}\n\
         \td[\"name\"] = \"{name}\"\n\
         \td[\"part_count\"] = parts.size()\n\
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
         \tvar d = {{}}\n\
         \td[\"active\"] = \"{name}\"\n\
         \td[\"vertex_count\"] = vc\n\
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
     \tvar d = {}\n\
     \td[\"active\"] = active\n\
     \td[\"part_count\"] = parts.size()\n\
     \td[\"visible\"] = \"all\"\n\
     \treturn JSON.stringify(d)\n"
        .to_string()
}
