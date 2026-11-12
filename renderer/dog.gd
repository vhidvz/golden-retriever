class_name PlaceholderDog
extends Node3D
## A stand-in Golden Retriever built from fur-covered ellipsoids, animated
## entirely procedurally (no clips).
##
## Units: the adult stands 1.0 tall, +X is forward, the paws rest on Y = 0.
## A better model replaces this file by offering the same methods the renderer
## calls: apply_growth, set_activity, set_facing, set_excitement, advance, get_reach.

const SHELLS := 20
const FUR := preload("res://fur.gdshader")
const STRAND := 0.0045           # width of one strand cell, in units
const STRIDE := 0.75             # ground covered by one full walk cycle
const WALK_SPEED := 0.55         # heights per second; matches the daemon's walk speed
const YAW_RIGHT := -0.35         # three-quarter view towards the viewer
const YAW_LEFT := -2.79

## Joint targets per activity. Angles in degrees about Z: positive swings a
## hanging limb forward and lifts the nose. "hip" is hip height as a fraction of standing.
const POSES := {
	"stand": {"hip": 1.0, "pitch": 0.0, "front": 0.0, "hind": 0.0, "neck": 0.0, "head": 0.0, "tail": 35.0, "eyes": 1.0},
	"walk": {"hip": 1.0, "pitch": 0.0, "front": 0.0, "hind": 0.0, "neck": -6.0, "head": 4.0, "tail": 8.0, "eyes": 1.0},
	"sit": {"hip": 0.45, "pitch": 28.0, "front": -28.0, "hind": 57.0, "neck": -8.0, "head": -20.0, "tail": 10.0, "eyes": 1.0},
	"lie": {"hip": 0.24, "pitch": 0.0, "front": 80.0, "hind": 75.0, "neck": 6.0, "head": -4.0, "tail": 15.0, "eyes": 1.0},
	"sleep": {"hip": 0.24, "pitch": 0.0, "front": 80.0, "hind": 75.0, "neck": -32.0, "head": -18.0, "tail": 15.0, "eyes": 0.0},
}

var _growth := -1.0
var _activity := "stand"
var _facing := 1
var _excitement := 0.0
var _pose: Dictionary = POSES["stand"].duplicate()
var _yaw := YAW_RIGHT
var _hip_height := 0.44
var _time := 0.0
var _phase := 0.0
var _gait := 0.0
var _breath := 0.0
var _wag := 0.0
var _next_blink := 3.0
var _mesh_cache := {}

var _body: Node3D
var _torso: Node3D
var _neck: Node3D
var _head: Node3D
var _tail: Node3D
var _legs: Array[Node3D] = []   # front-left, front-right, hind-left, hind-right
var _ears: Array[Node3D] = []
var _eyes: Array[Node3D] = []


## Half-width and height, in units at full scale, that every pose stays inside.
func get_reach() -> Vector2:
	return Vector2(1.0, 1.15)


## growth: 0 (newborn proportions) to 1 (adult). Growing is not a uniform
## scale: a puppy has a bigger head, a shorter muzzle and legs, and fluffier, paler fur.
func apply_growth(growth: float) -> void:
	if absf(growth - _growth) < 0.01:
		return
	_growth = clampf(growth, 0.0, 1.0)
	_build()


func set_activity(activity: String) -> void:
	_activity = activity if POSES.has(activity) else "stand"


func set_facing(direction: int) -> void:
	_facing = 1 if direction >= 0 else -1


func set_excitement(amount: float) -> void:
	_excitement = clampf(amount, 0.0, 1.0)


## Advance the animation. `travelled` is how far he actually moved along the
## ground this frame (in units), so the legs keep pace with the window.
func advance(delta: float, travelled: float) -> void:
	_time += delta
	var ease := 1.0 - exp(-5.0 * delta)
	var target: Dictionary = POSES[_activity]
	for joint in target:
		_pose[joint] = lerpf(_pose[joint], target[joint], ease)

	var speed := absf(travelled) / maxf(delta, 0.001)
	_gait = lerpf(_gait, clampf(speed / WALK_SPEED, 0.0, 1.2), 1.0 - exp(-8.0 * delta))
	_phase += absf(travelled) * TAU / STRIDE
	var swing := 26.0 * _gait

	_yaw = lerpf(_yaw, YAW_RIGHT if _facing > 0 else YAW_LEFT, 1.0 - exp(-6.0 * delta))
	rotation.y = _yaw

	var awake: float = _pose["eyes"]
	_body.position.y = _hip_height * _pose["hip"] + 0.012 * _gait * absf(sin(_phase))
	_body.rotation.z = deg_to_rad(_pose["pitch"])

	# Breathing: slow and deep asleep, quicker after moving.
	_breath += delta * TAU * lerpf(0.3, 0.55 + 0.9 * _gait, awake)
	var breath := sin(_breath) * lerpf(1.6, 1.0, awake)
	_torso.scale = Vector3(1.0, 1.0 + 0.018 * breath, 1.0 + 0.03 * breath)

	# Diagonal pairs move together, as in a walk.
	var offsets := [0.0, PI, PI, 0.0]
	for i in 4:
		var base: float = _pose["front"] if i < 2 else _pose["hind"]
		_legs[i].rotation.z = deg_to_rad(base + swing * sin(_phase + offsets[i]))

	_neck.rotation.z = deg_to_rad(_pose["neck"] + 3.0 * _gait * sin(2.0 * _phase))
	_head.rotation.z = deg_to_rad(_pose["head"])
	_head.rotation.y = awake * (1.0 - minf(_gait, 1.0)) * (0.12 * sin(_time * 0.31) + 0.07 * sin(_time * 0.83))

	_wag += delta * TAU * (1.1 + 3.5 * _excitement)
	_tail.rotation.z = deg_to_rad(_pose["tail"] - 30.0 * _excitement)
	_tail.rotation.y = deg_to_rad((6.0 + 34.0 * _excitement + 8.0 * _gait) * awake * sin(_wag))

	for i in 2:
		var side := 1.0 if i == 0 else -1.0
		_ears[i].rotation.x = side * (0.12 + 0.08 * _gait * sin(2.0 * _phase))

	_next_blink -= delta
	if _next_blink < -0.12:
		_next_blink = randf_range(2.0, 6.0)
	var lids := awake * (0.1 if _next_blink < 0.0 else 1.0)
	for eye in _eyes:
		eye.scale.y = maxf(lids, 0.08)


func _build() -> void:
	for child in get_children():
		remove_child(child)
		child.free()
	_legs.clear()
	_ears.clear()
	_eyes.clear()

	var g := _growth
	var leg := lerpf(0.72, 1.0, g)
	var trunk := lerpf(0.86, 1.0, g)
	var fluff := lerpf(1.35, 1.0, g)
	var muzzle := lerpf(0.68, 1.0, g)
	# Puppies are paler; the coat deepens to gold with age.
	var root := Color(0.74, 0.58, 0.36).lerp(Color(0.62, 0.40, 0.16), g)
	var tip := Color(0.97, 0.87, 0.66).lerp(Color(0.93, 0.74, 0.42), g)
	_hip_height = 0.44 * leg

	_body = _pivot(self, Vector3(-0.30 * trunk, _hip_height, 0.0))
	_torso = _furred(_body, Vector3(0.30 * trunk, 0.11, 0.0), Vector3(0.42 * trunk, 0.20, 0.17), 0.03 * fluff, root, tip)
	_furred(_body, Vector3(0.55 * trunk, 0.13, 0.0), Vector3(0.22, 0.23, 0.19), 0.034 * fluff, root, tip)

	_neck = _pivot(_body, Vector3(0.66 * trunk, 0.22, 0.0))
	_furred(_neck, Vector3(0.09, 0.06, 0.0), Vector3(0.13, 0.18, 0.12), 0.04 * fluff, root, tip)
	_head = _pivot(_neck, Vector3(0.17, 0.20, 0.0))
	_head.scale = Vector3.ONE * lerpf(1.32, 1.0, g)
	_furred(_head, Vector3(0.02, 0.02, 0.0), Vector3(0.15, 0.13, 0.12), 0.022 * fluff, root, tip)
	_furred(_head, Vector3(0.06 + 0.11 * muzzle, -0.03, 0.0), Vector3(0.11 * muzzle, 0.06, 0.06), 0.008, root.lightened(0.12), tip.lightened(0.12))
	_smooth(_head, Vector3(0.06 + 0.215 * muzzle, -0.02, 0.0), 0.026)
	for side in [1.0, -1.0]:
		_eyes.append(_smooth(_head, Vector3(0.115, 0.045, 0.078 * side), 0.021))
		var ear := _pivot(_head, Vector3(-0.03, 0.05, 0.11 * side))
		_furred(ear, Vector3(0.0, -0.09, 0.0), Vector3(0.05, 0.11, 0.03), 0.03 * fluff, root.darkened(0.12), tip.darkened(0.12))
		_ears.append(ear)

	_tail = _pivot(_body, Vector3(-0.10 * trunk, 0.20, 0.0))
	_furred(_tail, Vector3(-0.19, 0.0, 0.0), Vector3(0.20, 0.045, 0.04), 0.055 * fluff, root, tip, Vector3(-1.0, -0.5, 0.0))

	for along in [0.60 * trunk, 0.0]:
		for side in [1.0, -1.0]:
			var hip := _pivot(_body, Vector3(along, 0.0, 0.10 * side))
			var half := (_hip_height - 0.02) * 0.5
			_furred(hip, Vector3(0.0, -half, 0.0), Vector3(0.05, half, 0.055), 0.014 * fluff, root, tip, Vector3.DOWN)
			_legs.append(hip)


func _pivot(parent: Node3D, at: Vector3) -> Node3D:
	var node := Node3D.new()
	node.position = at
	parent.add_child(node)
	return node


func _furred(parent: Node3D, centre: Vector3, radii: Vector3, fur_length: float, root: Color, tip: Color, comb := Vector3(-0.8, -0.6, 0.0)) -> Node3D:
	var shells := MultiMesh.new()
	shells.transform_format = MultiMesh.TRANSFORM_3D
	shells.use_custom_data = true
	shells.mesh = _ellipsoid(radii)
	shells.instance_count = SHELLS + 1
	for i in SHELLS + 1:
		shells.set_instance_transform(i, Transform3D.IDENTITY)
		shells.set_instance_custom_data(i, Color(float(i) / SHELLS, 0.0, 0.0, 0.0))

	var material := ShaderMaterial.new()
	material.shader = FUR
	material.set_shader_parameter("fur_length", fur_length)
	material.set_shader_parameter("density", Vector2(TAU * 0.5 * (radii.x + radii.z), PI * radii.y) / STRAND)
	material.set_shader_parameter("comb", comb)
	material.set_shader_parameter("root_color", root)
	material.set_shader_parameter("tip_color", tip)

	var node := MultiMeshInstance3D.new()
	node.multimesh = shells
	node.material_override = material
	node.position = centre
	# The shells bulge past the base mesh; widen the culling box to match.
	node.extra_cull_margin = fur_length * 2.0
	parent.add_child(node)
	return node


func _smooth(parent: Node3D, centre: Vector3, radius: float) -> Node3D:
	var mesh := SphereMesh.new()
	mesh.radius = radius
	mesh.height = radius * 2.0
	mesh.radial_segments = 16
	mesh.rings = 8
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.05, 0.04, 0.035)
	material.roughness = 0.25
	var node := MeshInstance3D.new()
	node.mesh = mesh
	node.material_override = material
	node.position = centre
	parent.add_child(node)
	return node


## A sphere stretched to the given radii, with normals corrected for the stretch
## so the fur stands perpendicular to the surface.
func _ellipsoid(radii: Vector3) -> ArrayMesh:
	if _mesh_cache.has(radii):
		return _mesh_cache[radii]
	const SEGMENTS := 32
	const RINGS := 20
	var vertices := PackedVector3Array()
	var normals := PackedVector3Array()
	var uvs := PackedVector2Array()
	var indices := PackedInt32Array()
	for ring in RINGS + 1:
		var v := float(ring) / RINGS
		for segment in SEGMENTS + 1:
			var u := float(segment) / SEGMENTS
			var unit := Vector3(sin(v * PI) * cos(u * TAU), cos(v * PI), sin(v * PI) * sin(u * TAU))
			vertices.append(unit * radii)
			normals.append((unit / radii).normalized())
			uvs.append(Vector2(u, v))
	for ring in RINGS:
		for segment in SEGMENTS:
			var a := ring * (SEGMENTS + 1) + segment
			var b := a + SEGMENTS + 1
			indices.append_array([a, b, a + 1, a + 1, b, b + 1])
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = vertices
	arrays[Mesh.ARRAY_NORMAL] = normals
	arrays[Mesh.ARRAY_TEX_UV] = uvs
	arrays[Mesh.ARRAY_INDEX] = indices
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	_mesh_cache[radii] = mesh
	return mesh
