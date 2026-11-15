extends "res://dog.gd"
## Buddy's real body: one continuous skinned mesh (made in Blender by
## tools/build_dog.py) with a skeleton, a coat whose length, colour and lie vary
## over the body, a mouth that opens, eyes set in sockets, and legs that bend and
## keep their paws on the ground.
##
## The model holds the body twice over: as an adult and as an eight-week-old
## puppy, with the same points, bones and coat in both. Growing up moves every
## one of them from the one to the other, so a puppy is a puppy (short round
## body, short thick legs, big head, short muzzle, stub of a tail, even fluff)
## and not a small adult.
##
## It reuses everything the stand-in knows about behaviour: the poses, the
## motion layers, mood and gaze all still drive a small tree of invisible
## pivots. Each frame those pivots are copied onto the skeleton, and the legs
## are solved from where each paw should be.

const MODEL := "res://dog/retriever.json"
const COAT := preload("res://coat.gdshader")
const SKIN := preload("res://skin.gdshader")
const STEP_HEIGHT := 0.06          # how high a paw lifts in a stride, in units

static var _model := {}            # the parsed model file, shared by every instance

var _skeleton: Skeleton3D
var _coat: ShaderMaterial
var _skin: ShaderMaterial
var _bone := {}                    # bone name -> index
var _joint := {}                   # bone name -> where it is at rest, in the dog's space
var _leg_scale := 1.0              # kept at 1: the puppy's short legs come from his own bones now
var _sole := {}                    # "front"/"hind" -> how far along the body each sole meets the ground
var _lids: Array[Node3D] = []      # per eye: the upper lid, the lower lid, the glint
var _wag_trail: Array[float] = []  # recent tail swings, so the tip follows the root late


func get_reach() -> Vector2:
	return Vector2(1.08, 1.25)


func get_bowl_distance() -> float:
	return lerpf(0.54, 0.40, 1.0 - _growth)      # a puppy's nose does not reach as far


func _build() -> void:
	for child in get_children():
		remove_child(child)
		child.free()
	_legs.clear()
	_ears.clear()
	_eyes.clear()
	_lids.clear()
	_pupils.clear()
	if _model.is_empty():
		_model = JSON.parse_string(FileAccess.get_file_as_string(MODEL))
	# How much of the puppy is left in him: every bone and every point of the body is
	# this far from where the adult has it towards where the eight-week-old has it.
	var g := _growth
	var young := 1.0 - g
	var puppy: Dictionary = _model["puppy"]
	for i in _model["bones"].size():
		var bone: Dictionary = _model["bones"][i]
		_bone[bone["name"]] = i
		_joint[bone["name"]] = _point(bone["at"]).lerp(_point(puppy["bones"][i]), young)
	for which in ["front", "hind"]:
		_sole[which] = lerpf(_model["paws"][which][0], puppy["paws"][which][0], young)
	_hip_height = _joint["hips"].y

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
	body.mesh = _make_mesh(young)
	_skeleton.add_child(body)
	body.skin = _skeleton.create_skin_from_rest_transforms()
	body.skeleton = body.get_path_to(_skeleton)
	_skin = ShaderMaterial.new()
	_skin.shader = SKIN
	_coat = ShaderMaterial.new()
	_coat.shader = COAT
	_coat.render_priority = 1      # after the shadow on the ground, so the fringe of the coat is not shaded twice
	for material in [_skin, _coat]:
		material.set_shader_parameter("coat_scale", 1.0)
		material.set_shader_parameter("pale", 1.0 - g)
	body.set_surface_override_material(0, _skin)
	body.set_surface_override_material(1, _coat)
	body.extra_cull_margin = 0.3

	# The pivots the behaviour code moves. They sit exactly where the bones do.
	_body = _pivot(self, Vector3(_joint["hips"].x, _hip_height, 0.0))
	_torso = _pivot(_body, Vector3.ZERO)
	_tail = _pivot(_body, _between("hips", "tail1"))
	_chest = _pivot(_body, _between("hips", "chest"))
	_neck = _pivot(_chest, _between("chest", "neck"))
	_head = _pivot(_neck, _between("neck", "head"))
	_jaw = _pivot(_head, _between("head", "jaw"))
	var tongue_at := _point(_model["tongue"]["at"]).lerp(_point(puppy["tongue"]["at"]), young)
	var tongue_half := _point(_model["tongue"]["half"]).lerp(_point(puppy["tongue"]["half"]), young)
	_tongue = _smooth(_jaw, Vector3(tongue_at.x, tongue_at.y, 0.0), tongue_half.z, Color(0.80, 0.36, 0.40))
	_tongue.scale = tongue_half / tongue_half.z
	_tongue.set_meta("rest", tongue_at.x)
	for i in 2:
		# Each eye sits in its socket and is turned the way the socket faces, so a
		# pupil that looks "straight ahead" is in the middle of its own opening.
		var seat: Dictionary = _model["eyes"][i]
		var eye := _pivot(_head, _point(seat["at"]).lerp(_point(puppy["eyes"][i]["at"]), young))
		var facing := Vector3(seat["gaze"][0], seat["gaze"][1], seat["gaze"][2]).normalized()
		eye.quaternion = Quaternion(Vector3.RIGHT, facing)
		var radius: float = lerpf(seat["radius"], puppy["eyes"][i]["radius"], young)
		_smooth(eye, Vector3.ZERO, radius, Color(0.27, 0.14, 0.055))
		_pupils.append(_smooth(eye, Vector3.RIGHT * radius * 0.6, radius * 0.46, Color(0.012, 0.01, 0.01)))
		var glint := _smooth(eye, Vector3(0.62, 0.62, -0.3 if i == 0 else 0.5).normalized() * radius * 0.92, radius * 0.17, Color.WHITE)
		(glint.material_override as StandardMaterial3D).shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		# Lids: two pieces of a slightly larger ball around the eye. Open, they are out of
		# sight under the skin above and below the socket; to blink or sleep they turn
		# over the eye and meet a little below its middle, as a dog's do.
		var tone := Color(0.80, 0.60, 0.34).lerp(Color(0.92, 0.80, 0.58), 0.55 * (1.0 - g))
		_lids.append(_lid(eye, radius * 1.2, 40.0, 136.0, true, tone))
		_lids.append(_lid(eye, radius * 1.2, -132.0, -38.0, false, tone))
		_lids.append(glint)
		eye.set_meta("radius", radius)
		_eyes.append(eye)
		_ears.append(_pivot(_head, _between("head", "ear_l" if i == 0 else "ear_r")))
	for name in ["front_l_upper", "front_r_upper"]:
		_legs.append(_pivot(_chest, _between("chest", name)))
	for name in ["hind_l_upper", "hind_r_upper"]:
		_legs.append(_pivot(_body, _between("hips", name)))


static func _point(numbers: Array) -> Vector3:
	return Vector3(numbers[0], numbers[1], numbers[2])


func _between(parent: String, child: String) -> Vector3:
	return _joint[child] - _joint[parent]


func advance(delta: float, travelled: float) -> void:
	super.advance(delta, travelled)
	_copy("hips", _body)
	_copy("chest", _chest, _torso.scale)   # breathing swells the ribcage
	_copy("neck", _neck)
	_copy("head", _head)
	_copy("jaw", _jaw)
	_copy("ear_l", _ears[0])
	_copy("ear_r", _ears[1])

	# The stand-in blinks by flattening its eyes. Here that amount closes the lids instead.
	for i in 2:
		var open := clampf((_eyes[i].scale.y - 0.08) / 0.92, 0.0, 1.0)
		_eyes[i].scale = Vector3.ONE
		_lids[i * 3].rotation.z = deg_to_rad(lerpf(-70.0, 0.0, open))
		_lids[i * 3 + 1].rotation.z = deg_to_rad(lerpf(14.0, 0.0, open))
		_lids[i * 3 + 2].visible = open > 0.35

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


## One eyelid: the part of a ball around the eye between two heights (degrees above
## the line of sight), darker along the edge that meets the other lid.
func _lid(eye: Node3D, radius: float, from: float, to: float, upper: bool, tone: Color) -> MeshInstance3D:
	var vertices := PackedVector3Array()
	var normals := PackedVector3Array()
	var colors := PackedColorArray()
	var triangles := PackedInt32Array()
	var rim := Color(0.09, 0.06, 0.05)
	var rows := 10
	var columns := 12
	for row in rows + 1:
		var height := deg_to_rad(lerpf(from, to, float(row) / rows))
		var edge := float(row) / rows if upper else 1.0 - float(row) / rows      # 0 at the edge that closes
		for column in columns + 1:
			var across := deg_to_rad(lerpf(-74.0, 74.0, float(column) / columns))
			var out := Vector3(cos(across) * cos(height), cos(across) * sin(height), sin(across))
			vertices.append(out * radius)
			normals.append(out)
			colors.append(rim.lerp(tone, smoothstep(0.0, 0.22, edge)))
	for row in rows:
		for column in columns:
			var a := row * (columns + 1) + column
			var b := a + columns + 1
			triangles.append_array([a, b, a + 1, a + 1, b, b + 1])
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = vertices
	arrays[Mesh.ARRAY_NORMAL] = normals
	arrays[Mesh.ARRAY_COLOR] = colors
	arrays[Mesh.ARRAY_INDEX] = triangles
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	var material := StandardMaterial3D.new()
	material.vertex_color_use_as_albedo = true
	material.vertex_color_is_srgb = true
	material.roughness = 1.0
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	var lid := MeshInstance3D.new()
	lid.mesh = mesh
	lid.material_override = material
	eye.add_child(lid)
	return lid


func _copy(bone: String, pivot: Node3D, swell := Vector3.ONE) -> void:
	var index: int = _bone[bone]
	_skeleton.set_bone_pose_position(index, pivot.position)
	_skeleton.set_bone_pose_rotation(index, pivot.quaternion)
	_skeleton.set_bone_pose_scale(index, pivot.scale * swell)


## Bends one leg so that its paw is where it should be: swung by the pivot's
## angle, lifted during the forward half of a stride, and never below the ground.
## The toes stay flat on the ground whatever the leg above them does, and a hind
## leg's long foot (hock to toes) lies down along the ground as the hips sink,
## which is what lets him sit and lie as a dog does.
func _place_leg(upper: String, lower: String, pivot: Node3D, front: bool, stride: float) -> void:
	var toes_bone := upper.replace("upper", "paw")
	var foot_bone := upper.replace("upper", "foot")
	var top: Vector3 = _joint[upper]
	var middle: Vector3 = _joint[lower]
	var toes: Vector3 = _joint[toes_bone]
	var ankle: Vector3 = toes if front else _joint[foot_bone]
	var sole := Vector3(_sole["front" if front else "hind"], 0.0, top.z)
	var thigh := (middle - top) * _leg_scale
	var shin := (ankle - middle) * _leg_scale
	var foot := (toes - ankle) * _leg_scale       # nothing for a foreleg: its pastern is part of the forearm
	var tread := (sole - toes) * _leg_scale       # from where the toes begin to the middle of the sole
	var parent: Node3D = pivot.get_parent()
	var in_dog := global_transform.affine_inverse() * parent.global_transform
	var parent_turn := in_dog.basis.orthonormalized()
	var lying_on_side := absf(_body.rotation.x) > 0.35

	var upper_turn: float
	var lower_turn: float
	var foot_turn := 0.0      # how far the hind foot is turned from the way it stands
	var toes_turn := 0.0      # the same for the toes
	var lean := 0.0
	var slack := Quaternion.IDENTITY      # lying on his side: the turn that lays the upper leg down towards the floor
	if lying_on_side:
		var on_back := absf(_body.rotation.x) > 2.1
		var uppermost := (top.z > 0.0) == (_body.rotation.x < 0.0)
		if on_back:
			# Paws in the air, joints loosely bent.
			upper_turn = pivot.rotation.z
			lower_turn = upper_turn + (0.85 if front else -0.75)
			foot_turn = lower_turn + 0.60
			toes_turn = (lower_turn if front else foot_turn) - (0.55 if front else 0.30)
		else:
			# Lying on his side the legs lie slack on the floor: the forelegs reach forward and the
			# hind legs back, out from his belly, rather than pointing straight out of it (which,
			# seen from the side, makes them look like four parallel stumps). The legs of the upper
			# flank lie further out than the lower ones, so the two pairs do not cover each other.
			var belly := -signf(_body.rotation.x)
			var along := (0.80 if front else -0.74) if not uppermost else (0.52 if front else -0.46)
			var flat := Vector3(along, 0.0, belly * sqrt(1.0 - along * along))
			var height := (in_dog * pivot.position).y
			var reach := 0.85 * (thigh.length() + shin.length() + foot.length() + tread.length())
			var below := asin(clampf((height - 0.03) / reach, 0.0, 0.94))
			var wanted := flat * cos(below) + Vector3.DOWN * sin(below)
			upper_turn = 0.0
			lower_turn = (0.30 if front else -0.40) * (1.6 if uppermost else 1.0) + 0.06 * sin(pivot.rotation.z * 3.0)
			foot_turn = lower_turn + 0.45
			toes_turn = (lower_turn if front else foot_turn) - 0.35
			# Aim the whole leg, hip to paw, not just its top bone: a hind leg's shin sits at
			# nearly a right angle to the thigh, so aiming the thigh alone lifts the paw into the air.
			var whole := thigh + shin.rotated(Vector3.BACK, lower_turn) + foot.rotated(Vector3.BACK, foot_turn) + tread.rotated(Vector3.BACK, toes_turn)
			slack = Quaternion(whole.normalized(), (parent_turn.inverse() * wanted).normalized())
	else:
		var joint := in_dog * pivot.position
		lean = atan2(parent_turn.x.y, parent_turn.x.x)
		var reach := (thigh + shin + foot + tread).rotated(Vector3.BACK, pivot.rotation.z + lean)
		var paw := Vector2(joint.x + reach.x, maxf(joint.y + reach.y, 0.012))
		var stepping := clampf(_gait, 0.0, 1.0)
		if stepping > 0.02:
			paw.y = lerpf(paw.y, STEP_HEIGHT * (_hip_height / 0.6) * minf(_gait, 1.6) * maxf(cos(stride), 0.0), stepping)
		var target := paw - Vector2(tread.x, tread.y)      # where the toes begin
		if not front:
			# Standing, the foot is upright under the hock. As the hips come down there is no
			# room for that, and it lies along the ground with the hock behind the toes.
			var standing := -(thigh + shin + foot + tread).y
			var room := clampf((joint.y - 0.30 * standing) / (0.55 * standing), 0.0, 1.0)
			var upright := Vector2(-foot.x, -foot.y).angle()
			room = maxf(room, smoothstep(0.03, 0.14, target.y))      # a paw held up in the air keeps its foot in line
			var raised := lerp_angle(deg_to_rad(170.0), upright, smoothstep(0.0, 1.0, room))
			foot_turn = raised - upright
			target += Vector2.from_angle(raised) * foot.length()      # now the hock
		# Two bones, one target: the law of cosines gives the bend. Elbows point back, knees forward.
		var a := thigh.length()
		var b := shin.length()
		var to_target := target - Vector2(joint.x, joint.y)
		var distance := clampf(to_target.length(), absf(a - b) + 0.001, a + b - 0.001)
		var bend := acos(clampf((a * a + distance * distance - b * b) / (2.0 * a * distance), -1.0, 1.0))
		var thigh_angle := to_target.angle() + (-bend if front else bend)
		var knee := Vector2(joint.x, joint.y) + Vector2.from_angle(thigh_angle) * a
		if knee.y < 0.03:
			# Lying down, the usual bend would push the joint into the ground: fold the other way, onto it.
			thigh_angle = to_target.angle() + (bend if front else -bend)
			knee = Vector2(joint.x, joint.y) + Vector2.from_angle(thigh_angle) * a
		if not front and knee.y > joint.y + 0.07 and joint.y < 0.2 and target.y < 0.1:
			# Lying on his front: the knee rests beside the belly instead of standing up over
			# his back, and the shin folds back from it along the ground.
			thigh_angle = asin(clampf(-0.01 / a, -1.0, 1.0))     # level with the hip: higher, a long femur would stand up over his back
			knee = Vector2(joint.x, joint.y) + Vector2.from_angle(thigh_angle) * a
			target = knee + Vector2.from_angle(PI + asin(clampf((knee.y - 0.035) / b, 0.0, 1.0))) * b
		upper_turn = thigh_angle - Vector2(thigh.x, thigh.y).angle() - lean
		lower_turn = (target - knee).angle() - Vector2(shin.x, shin.y).angle() - lean
		foot_turn -= lean
		toes_turn = -lean
	_skeleton.set_bone_pose_rotation(_bone[upper], slack * Quaternion(Vector3.BACK, upper_turn))
	_skeleton.set_bone_pose_rotation(_bone[lower], Quaternion(Vector3.BACK, wrapf(lower_turn - upper_turn, -PI, PI)))
	if front:
		_skeleton.set_bone_pose_rotation(_bone[toes_bone], Quaternion(Vector3.BACK, wrapf(toes_turn - lower_turn, -PI, PI)))
	else:
		_skeleton.set_bone_pose_rotation(_bone[foot_bone], Quaternion(Vector3.BACK, wrapf(foot_turn - lower_turn, -PI, PI)))
		_skeleton.set_bone_pose_rotation(_bone[toes_bone], Quaternion(Vector3.BACK, wrapf(toes_turn - foot_turn, -PI, PI)))
	_skeleton.set_bone_pose_scale(_bone[upper], Vector3.ONE * _leg_scale)


## The stand-in's poses, adjusted for a body with real legs and a spine at the top of the back.
func _compose(t: float, done: float) -> Array:
	var layers := super._compose(t, done)
	var pose: Dictionary = layers[0]
	var young := 1.0 - _growth
	if pose["hip"] >= 0.9 and pose["neck"] < -30.0:
		# Nose to the ground while standing. The poses were made for the adult's long neck;
		# a puppy's is short, so he gets there by going down at the front instead of
		# tucking his chin in.
		var dip := lerpf(7.0, 17.0, young)
		pose["pitch"] -= dip
		for leg in ["front_l", "front_r", "hind_l", "hind_r"]:
			pose[leg] += dip             # the paws stay where they were while the body tips
		pose["neck"] += lerpf(4.0, 12.0, young)
		pose["head"] = lerpf(6.0, 2.0, young)
	if pose["hip"] < 0.9:
		var on_side: bool = absf(pose["roll"]) > 30.0
		var sitting: bool = pose["pitch"] > 10.0 and not on_side
		# How high his spine is off the floor. Sitting depends on the length of his legs;
		# lying depends on how deep or wide his body is, which is the same at any age.
		pose["hip"] = 0.175 / _hip_height if on_side else (0.36 if sitting else 0.225 / _hip_height)
		if absf(pose["roll"]) > 120.0:
			pose["hip"] = 0.12 / _hip_height   # on his back
		if sitting:
			# Sitting tall enough that the forelegs are straight under the shoulders.
			pose["pitch"] += 10.0
			pose["front_l"] -= 10.0
			pose["front_r"] -= 10.0
			pose["neck"] -= 4.0
			pose["head"] -= 6.0
			pose["tail"] = -20.0 - pose["pitch"]   # along the ground behind him, not through it
			# Hind paws on the ground under the belly, a little ahead of the hips
			# (unless that leg is busy, scratching for instance).
			for leg in ["hind_l", "hind_r"]:
				if absf(pose[leg] - SIT[leg]) < 0.5:
					pose[leg] = 38.0 - pose["pitch"]
				elif pose[leg] > 100.0:
					pose[leg] = 118.0      # scratching: the paw comes up to the neck
		elif not on_side:
			pose["tail"] = -20.0
			# Lying on his front, the hind legs are folded beside him.
			for leg in ["hind_l", "hind_r"]:
				if absf(pose[leg] - SPHINX[leg]) < 0.5:
					pose[leg] = 46.0
	return layers


## The body, and the coat as layers of that same body, each to be lifted a little
## further off the skin by the shader. The model lists its points and triangles
## with the longest coat first, so every layer is just the first so-many of both:
## a short coat gets two or three layers, the feathering up to thirty.
static func _make_mesh(young: float) -> ArrayMesh:
	var count: int = _model["vertices"].size() / 3
	var at: Array = _model["vertices"]
	var out: Array = _model["normals"]
	var young_at: Array = _model["puppy"]["vertices"]
	var young_out: Array = _model["puppy"]["normals"]
	var young_lengths: Array = _model["puppy"]["coat"]
	var coat := PackedFloat32Array()
	coat.resize(count)
	var lies: Array = _model["flow"]
	var along: Array = _model["grain"]
	var tones: Array = _model["colors"]
	var skins: Array = _model["skin"]
	var lengths: Array = _model["coat"]
	var deep: Array = _model["shells"]
	var vertices := PackedVector3Array()
	var normals := PackedVector3Array()
	var tangents := PackedFloat32Array()
	var grain := PackedFloat32Array()
	var colors := PackedColorArray()
	var skin := PackedVector2Array()
	var marks := PackedVector2Array()
	var bones := PackedInt32Array(_model["bone_ids"])
	var weights := PackedFloat32Array(_model["bone_weights"])
	var triangles := PackedInt32Array(_model["triangles"])
	vertices.resize(count)
	normals.resize(count)
	tangents.resize(count * 4)
	grain.resize(count * 4)
	colors.resize(count)
	skin.resize(count)
	marks.resize(count)
	for i in count:
		vertices[i] = Vector3(at[i * 3], at[i * 3 + 1], at[i * 3 + 2]).lerp(Vector3(young_at[i * 3], young_at[i * 3 + 1], young_at[i * 3 + 2]), young)
		normals[i] = Vector3(out[i * 3], out[i * 3 + 1], out[i * 3 + 2]).lerp(Vector3(young_out[i * 3], young_out[i * 3 + 1], young_out[i * 3 + 2]), young).normalized()
		coat[i] = lerpf(lengths[i], young_lengths[i], young)
		tangents[i * 4] = lies[i * 3]
		tangents[i * 4 + 1] = lies[i * 3 + 1]
		tangents[i * 4 + 2] = lies[i * 3 + 2]
		tangents[i * 4 + 3] = 1.0
		grain[i * 4] = along[i * 4]
		grain[i * 4 + 1] = along[i * 4 + 1]
		grain[i * 4 + 2] = along[i * 4 + 2]
		grain[i * 4 + 3] = along[i * 4 + 3]
		colors[i] = Color(tones[i * 4], tones[i * 4 + 1], tones[i * 4 + 2], tones[i * 4 + 3])
		skin[i] = Vector2(skins[i * 2], skins[i * 2 + 1])
		marks[i] = Vector2(0.0, coat[i])

	var mesh := ArrayMesh.new()
	var format := Mesh.ARRAY_CUSTOM_RGBA_FLOAT << Mesh.ARRAY_FORMAT_CUSTOM0_SHIFT
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, _arrays(vertices, normals, tangents, grain, colors, skin, marks, bones, weights, triangles), [], {}, format)

	var all_vertices := PackedVector3Array()
	var all_normals := PackedVector3Array()
	var all_tangents := PackedFloat32Array()
	var all_grain := PackedFloat32Array()
	var all_colors := PackedColorArray()
	var all_skin := PackedVector2Array()
	var all_marks := PackedVector2Array()
	var all_bones := PackedInt32Array()
	var all_weights := PackedFloat32Array()
	var all_triangles := PackedInt32Array()
	var layers: Array = _model["layers"]
	for layer in layers.size():
		var points: int = layers[layer][0]
		var corners: int = int(layers[layer][1]) * 3
		if corners == 0:
			break
		var first := all_vertices.size()
		all_vertices.append_array(vertices.slice(0, points))
		all_normals.append_array(normals.slice(0, points))
		all_tangents.append_array(tangents.slice(0, points * 4))
		all_grain.append_array(grain.slice(0, points * 4))
		all_colors.append_array(colors.slice(0, points))
		all_skin.append_array(skin.slice(0, points))
		all_bones.append_array(bones.slice(0, points * 4))
		all_weights.append_array(weights.slice(0, points * 4))
		var mark := PackedVector2Array()
		mark.resize(points)
		for i in points:
			# A point whose coat is already complete stays at its tips.
			mark[i] = Vector2(minf((layer + 1.0) / maxf(deep[i], 1.0), 1.0), coat[i])
		all_marks.append_array(mark)
		var part := triangles.slice(0, corners)
		for i in corners:
			part[i] += first
		all_triangles.append_array(part)
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, _arrays(all_vertices, all_normals, all_tangents, all_grain, all_colors, all_skin, all_marks, all_bones, all_weights, all_triangles), [], {}, format)
	return mesh


static func _arrays(vertices: PackedVector3Array, normals: PackedVector3Array, tangents: PackedFloat32Array, grain: PackedFloat32Array, colors: PackedColorArray,
		skin: PackedVector2Array, marks: PackedVector2Array, bones: PackedInt32Array, weights: PackedFloat32Array, triangles: PackedInt32Array) -> Array:
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = vertices
	arrays[Mesh.ARRAY_NORMAL] = normals
	arrays[Mesh.ARRAY_TANGENT] = tangents     # the way the coat lies
	arrays[Mesh.ARRAY_CUSTOM0] = grain        # where the point is along and around its part
	arrays[Mesh.ARRAY_COLOR] = colors         # the coat's colour, and how much light reaches it
	arrays[Mesh.ARRAY_TEX_UV] = skin          # bare skin, wet shine
	arrays[Mesh.ARRAY_TEX_UV2] = marks        # which layer of the coat, and its length
	arrays[Mesh.ARRAY_BONES] = bones
	arrays[Mesh.ARRAY_WEIGHTS] = weights
	arrays[Mesh.ARRAY_INDEX] = triangles
	return arrays
