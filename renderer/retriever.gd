extends "res://dog.gd"
## Buddy's real body: one continuous skinned mesh (made by tools/make_dog.py)
## with a skeleton, a coat whose length and shade vary over the body, and legs
## that bend and keep their paws on the ground.
##
## It reuses everything the stand-in knows about behaviour: the poses, the
## motion layers, mood and gaze all still drive a small tree of invisible
## pivots. Each frame those pivots are copied onto the skeleton, and the legs
## are solved from where each paw should be.

const MODEL := "res://dog/retriever.json"
const COAT := preload("res://coat.gdshader")
const COAT_SHELLS := 16
const STEP_HEIGHT := 0.06          # how high a paw lifts in a stride, in units

static var _model := {}            # the parsed model file, shared by every instance
static var _layers: ArrayMesh      # the body repeated once per coat layer

var _skeleton: Skeleton3D
var _coat: ShaderMaterial
var _bone := {}                    # bone name -> index
var _joint := {}                   # bone name -> where it is at rest, in the dog's space
var _leg_scale := 1.0
var _wag_trail: Array[float] = []  # recent tail swings, so the tip follows the root late


func get_reach() -> Vector2:
	return Vector2(1.08, 1.25)


func _build() -> void:
	for child in get_children():
		remove_child(child)
		child.free()
	_legs.clear()
	_ears.clear()
	_eyes.clear()
	_pupils.clear()
	if _model.is_empty():
		_model = JSON.parse_string(FileAccess.get_file_as_string(MODEL))
		_layers = _make_layers()
	for i in _model["bones"].size():
		var bone: Dictionary = _model["bones"][i]
		_bone[bone["name"]] = i
		_joint[bone["name"]] = Vector3(bone["at"][0], bone["at"][1], bone["at"][2])

	# How a puppy differs from the adult he will be: shorter legs and muzzle, a bigger head, a fluffier, paler coat.
	var g := _growth
	_leg_scale = lerpf(0.72, 1.0, g)
	var muzzle := lerpf(0.70, 1.0, g)
	var head := lerpf(1.30, 1.0, g)
	var legs_span: float = _joint["hind_l_upper"].y
	_hip_height = _joint["hips"].y - (1.0 - _leg_scale) * legs_span

	_skeleton = Skeleton3D.new()
	add_child(_skeleton)
	for bone in _model["bones"]:
		var index := _skeleton.add_bone(bone["name"])
		var origin: Vector3 = _joint[bone["name"]]
		if bone["parent"] != null:
			_skeleton.set_bone_parent(index, _bone[bone["parent"]])
			origin -= _joint[bone["parent"]]
		_skeleton.set_bone_rest(index, Transform3D(Basis.IDENTITY, origin))
	_skeleton.reset_bone_poses()   # every bone starts where it rests; only what is animated is set each frame
	var body := MeshInstance3D.new()
	body.mesh = _layers
	_skeleton.add_child(body)
	body.skin = _skeleton.create_skin_from_rest_transforms()
	body.skeleton = body.get_path_to(_skeleton)
	_coat = ShaderMaterial.new()
	_coat.shader = COAT
	_coat.set_shader_parameter("coat_scale", lerpf(1.35, 1.0, g))
	_coat.set_shader_parameter("root_color", Color(0.74, 0.58, 0.36).lerp(Color(0.62, 0.40, 0.16), g))
	_coat.set_shader_parameter("tip_color", Color(0.97, 0.87, 0.66).lerp(Color(0.93, 0.74, 0.42), g))
	body.material_override = _coat
	body.extra_cull_margin = 0.3

	# The pivots the behaviour code moves. They sit exactly where the bones do.
	_body = _pivot(self, Vector3(_joint["hips"].x, _hip_height, 0.0))
	_torso = _pivot(_body, Vector3.ZERO)
	_tail = _pivot(_body, _between("hips", "tail1"))
	_chest = _pivot(_body, _between("hips", "chest"))
	_neck = _pivot(_chest, _between("chest", "neck"))
	_head = _pivot(_neck, _between("neck", "head"))
	_head.scale = Vector3.ONE * head
	_jaw = _pivot(_head, _between("head", "jaw"))
	_jaw.scale = Vector3(muzzle, 1.0, 1.0)
	_tongue = _smooth(_jaw, Vector3(0.075, 0.012, 0.0), 0.022, Color(0.86, 0.42, 0.46))
	_tongue.scale = Vector3(2.4, 0.3, 1.0)
	_tongue.set_meta("rest", 0.075)
	var snout := _pivot(_head, _between("head", "muzzle"))
	snout.scale = Vector3(muzzle, 1.0, 1.0)
	var nose := _smooth(snout, Vector3(0.176, -0.002, 0.0), 0.027, Color(0.05, 0.04, 0.035))
	nose.scale = Vector3(0.85, 0.85, 1.2)
	for side in [1.0, -1.0]:
		var eye := _pivot(_head, Vector3(0.108, 0.050, 0.070 * side))
		var radius := 0.017
		_smooth(eye, Vector3.ZERO, radius, Color(0.22, 0.12, 0.05))
		_pupils.append(_smooth(eye, Vector3.RIGHT * radius * 0.62, radius * 0.62, Color(0.01, 0.01, 0.01)))
		var glint := _smooth(eye, Vector3(0.3, 0.6, 0.45 * side).normalized() * radius * 0.9, radius * 0.2, Color.WHITE)
		(glint.material_override as StandardMaterial3D).shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		eye.set_meta("radius", radius)
		_eyes.append(eye)
		_ears.append(_pivot(_head, _between("head", "ear_l" if side > 0.0 else "ear_r")))
	for name in ["front_l_upper", "front_r_upper"]:
		_legs.append(_pivot(_chest, _between("chest", name)))
	for name in ["hind_l_upper", "hind_r_upper"]:
		_legs.append(_pivot(_body, _between("hips", name)))


func _between(parent: String, child: String) -> Vector3:
	return _joint[child] - _joint[parent]


func advance(delta: float, travelled: float) -> void:
	super.advance(delta, travelled)
	_copy("hips", _body)
	_copy("chest", _chest, _torso.scale)   # breathing swells the ribcage
	_copy("neck", _neck)
	_copy("head", _head)
	_copy("jaw", _jaw)
	_skeleton.set_bone_pose_scale(_bone["muzzle"], _jaw.scale)
	_copy("ear_l", _ears[0])
	_copy("ear_r", _ears[1])

	# The tail swings as a chain: each segment follows the one before it a moment later.
	_wag_trail.push_front(_tail.rotation.y)
	_wag_trail.resize(mini(_wag_trail.size(), 12))
	var names := ["tail1", "tail2", "tail3"]
	for i in 3:
		var swing: float = _wag_trail[mini(i * 4, _wag_trail.size() - 1)]
		_skeleton.set_bone_pose_rotation(_bone[names[i]], Quaternion.from_euler(Vector3(0.0, swing * [0.5, 0.7, 0.8][i], _tail.rotation.z * [0.55, 0.3, 0.15][i])))

	var gallop := smoothstep(2.4, 3.0, _gait)
	var beat := [0.0, lerpf(PI, 0.4, gallop), PI, lerpf(0.0, PI + 0.4, gallop)]
	var upper := ["front_l_upper", "front_r_upper", "hind_l_upper", "hind_r_upper"]
	for i in 4:
		_place_leg(upper[i], upper[i].replace("upper", "lower"), _legs[i], i < 2, _phase + beat[i])


func _copy(bone: String, pivot: Node3D, swell := Vector3.ONE) -> void:
	var index: int = _bone[bone]
	_skeleton.set_bone_pose_position(index, pivot.position)
	_skeleton.set_bone_pose_rotation(index, pivot.quaternion)
	_skeleton.set_bone_pose_scale(index, pivot.scale * swell)


## Bends one leg so that its paw is where it should be: swung by the pivot's
## angle, lifted during the forward half of a stride, and never below the ground.
func _place_leg(upper: String, lower: String, pivot: Node3D, front: bool, stride: float) -> void:
	var shoulder: Vector3 = _joint[upper]
	var elbow: Vector3 = _joint[lower]
	var sole := Vector3(_model["paws"]["front" if front else "hind"][0], 0.0, shoulder.z)
	var thigh := (elbow - shoulder) * _leg_scale
	var shin := (sole - elbow) * _leg_scale
	var parent: Node3D = pivot.get_parent()
	var in_dog := global_transform.affine_inverse() * parent.global_transform
	var parent_turn := in_dog.basis.orthonormalized()
	var lying_on_side := absf(_body.rotation.x) > 0.35

	var upper_turn: float
	var lower_turn: float
	if lying_on_side:
		# Off the ground there is nothing to stand on: swing from the shoulder with a relaxed bend.
		upper_turn = pivot.rotation.z
		lower_turn = pivot.rotation.z + (0.5 if front else -0.5)
	else:
		var joint := in_dog * pivot.position
		var lean := atan2(parent_turn.x.y, parent_turn.x.x)
		var reach := (thigh + shin).rotated(Vector3.BACK, pivot.rotation.z + lean)
		var paw := Vector2(joint.x + reach.x, maxf(joint.y + reach.y, 0.012))
		var stepping := clampf(_gait, 0.0, 1.0)
		if stepping > 0.02:
			paw.y = lerpf(paw.y, STEP_HEIGHT * _leg_scale * minf(_gait, 1.6) * maxf(cos(stride), 0.0), stepping)
		# Two bones, one target: the law of cosines gives the bend. Elbows point back, knees forward.
		var a := thigh.length()
		var b := shin.length()
		var to_paw := paw - Vector2(joint.x, joint.y)
		var distance := clampf(to_paw.length(), absf(a - b) + 0.001, a + b - 0.001)
		var bend := acos(clampf((a * a + distance * distance - b * b) / (2.0 * a * distance), -1.0, 1.0))
		var thigh_angle := to_paw.angle() + (-bend if front else bend)
		var knee := Vector2(joint.x, joint.y) + Vector2.from_angle(thigh_angle) * a
		if knee.y < 0.03:
			# Lying down, the usual bend would push the joint into the ground: fold the other way, onto it.
			thigh_angle = to_paw.angle() + (bend if front else -bend)
			knee = Vector2(joint.x, joint.y) + Vector2.from_angle(thigh_angle) * a
		upper_turn = thigh_angle - Vector2(thigh.x, thigh.y).angle() - lean
		lower_turn = (paw - knee).angle() - Vector2(shin.x, shin.y).angle() - lean
	_skeleton.set_bone_pose_rotation(_bone[upper], Quaternion(Vector3.BACK, upper_turn))
	_skeleton.set_bone_pose_rotation(_bone[lower], Quaternion(Vector3.BACK, lower_turn - upper_turn))
	_skeleton.set_bone_pose_scale(_bone[upper], Vector3.ONE * _leg_scale)


## The stand-in's poses, adjusted for a body with real legs and a spine at the top of the back.
func _compose(t: float, done: float) -> Array:
	var layers := super._compose(t, done)
	var pose: Dictionary = layers[0]
	if pose["hip"] < 0.9:
		var on_side: bool = absf(pose["roll"]) > 30.0
		var sitting: bool = pose["pitch"] > 10.0 and not on_side
		pose["hip"] = 0.30 if on_side else (0.36 if sitting else 0.38)
		if absf(pose["roll"]) > 120.0:
			pose["hip"] = 0.34   # on his back
		if sitting:
			# Sitting tall enough that the forelegs are straight under the shoulders.
			pose["pitch"] += 10.0
			pose["front_l"] -= 10.0
			pose["front_r"] -= 10.0
			pose["neck"] -= 4.0
			pose["head"] -= 6.0
			pose["tail"] = -20.0 - pose["pitch"]   # along the ground behind him, not through it
		elif not on_side:
			pose["tail"] = -20.0
	return layers


## The body once per coat layer, in a single mesh, so that every layer bends with the skeleton.
static func _make_layers() -> ArrayMesh:
	var count: int = _model["vertices"].size() / 3
	var vertices := PackedVector3Array()
	var normals := PackedVector3Array()
	var uvs := PackedVector2Array()
	var colors := PackedColorArray()
	var bones := PackedInt32Array(_model["bone_ids"])
	var weights := PackedFloat32Array(_model["bone_weights"])
	var triangles := PackedInt32Array(_model["triangles"])
	vertices.resize(count)
	normals.resize(count)
	uvs.resize(count)
	colors.resize(count)
	for i in count:
		vertices[i] = Vector3(_model["vertices"][i * 3], _model["vertices"][i * 3 + 1], _model["vertices"][i * 3 + 2])
		normals[i] = Vector3(_model["normals"][i * 3], _model["normals"][i * 3 + 1], _model["normals"][i * 3 + 2])
		uvs[i] = Vector2(_model["uvs"][i * 2], _model["uvs"][i * 2 + 1])
		colors[i] = Color(_model["colors"][i * 4], _model["colors"][i * 4 + 1], _model["colors"][i * 4 + 2], _model["colors"][i * 4 + 3])

	var all_vertices := PackedVector3Array()
	var all_normals := PackedVector3Array()
	var all_uvs := PackedVector2Array()
	var all_layer := PackedVector2Array()
	var all_colors := PackedColorArray()
	var all_bones := PackedInt32Array()
	var all_weights := PackedFloat32Array()
	var all_triangles := PackedInt32Array()
	for layer in COAT_SHELLS + 1:
		all_vertices.append_array(vertices)
		all_normals.append_array(normals)
		all_uvs.append_array(uvs)
		all_colors.append_array(colors)
		all_bones.append_array(bones)
		all_weights.append_array(weights)
		var mark := PackedVector2Array()
		mark.resize(count)
		mark.fill(Vector2(float(layer) / COAT_SHELLS, 0.0))
		all_layer.append_array(mark)
		var shifted := triangles.duplicate()
		for i in shifted.size():
			shifted[i] += layer * count
		all_triangles.append_array(shifted)

	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = all_vertices
	arrays[Mesh.ARRAY_NORMAL] = all_normals
	arrays[Mesh.ARRAY_TEX_UV] = all_uvs
	arrays[Mesh.ARRAY_TEX_UV2] = all_layer
	arrays[Mesh.ARRAY_COLOR] = all_colors
	arrays[Mesh.ARRAY_BONES] = all_bones
	arrays[Mesh.ARRAY_WEIGHTS] = all_weights
	arrays[Mesh.ARRAY_INDEX] = all_triangles
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	return mesh
