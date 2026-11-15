extends SceneTree
## Writes down how Buddy moves, frame by frame, so that Blender can draw the same
## movements with a real coat (tools/bake_sprites.py). The poses come from the very
## code that moves him on the desktop, so the pictures and the live dog agree.
##
##   godot --headless --path renderer -s res://export_poses.gd -- --growth 0.02 --size 420 --out poses.json [--only stand,walk]
##
## For every frame and every bone it records the matrix that carries the body at
## rest to where that bone has moved it, already including his size and which way
## he faces; and the eyes, eyelids and tongue, which are not part of the skeleton.
## It also records the window and camera his window would have, so the pictures can
## be laid over it pixel for pixel.

const Dog := preload("res://retriever.gd")
const CAMERA_PITCH := 0.14         # these three must match main.gd
const GROUND_MARGIN := 0.11
const STEP := 1.0 / 60.0

## What is drawn. "loop": a stretch of an activity that is played round and round;
## "cycle": one stride, shown according to how far he has walked; "once": a change of
## posture, played through once. "gaze" turns his attention towards a direction
## (degrees around him: 0 is the viewer, positive to the right).
static func clips() -> Array:
	var all := [
		{"name": "stand", "activity": "stand", "secs": 4.0, "fps": 12, "kind": "loop"},
		{"name": "sit", "activity": "sit", "secs": 4.0, "fps": 12, "kind": "loop"},
		{"name": "lie", "activity": "lie", "secs": 4.0, "fps": 12, "kind": "loop"},
		{"name": "lie_chin", "activity": "lie", "variant": 2, "secs": 4.0, "fps": 12, "kind": "loop"},
		{"name": "sleep_side", "activity": "sleep", "variant": 0, "secs": 5.0, "fps": 10, "kind": "loop", "sleepy": true},
		{"name": "sleep_curled", "activity": "sleep", "variant": 1, "secs": 5.0, "fps": 10, "kind": "loop", "sleepy": true},
		{"name": "sleep_chin", "activity": "sleep", "variant": 2, "secs": 5.0, "fps": 10, "kind": "loop", "sleepy": true},
		{"name": "walk", "activity": "walk", "frames": 16, "kind": "cycle"},
	]
	for posture in ["stand", "sit", "lie"]:
		for angle in [-60, -25, 0, 25, 60]:
			all.append({"name": "%s_look%+d" % [posture, angle], "activity": posture, "gaze": angle, "secs": 2.0, "fps": 12, "kind": "loop"})
	for pair in [["stand", "sit"], ["sit", "stand"], ["stand", "lie"], ["lie", "stand"], ["sit", "lie"], ["lie", "sit"]]:
		all.append({"name": "%s_to_%s" % pair, "from": pair[0], "activity": pair[1], "secs": 1.5, "fps": 12, "kind": "once"})
	all.append({"name": "lie_to_sleep_side", "from": "lie", "activity": "sleep", "variant": 0, "secs": 1.5, "fps": 12, "kind": "once"})
	all.append({"name": "sleep_side_to_lie", "from": "sleep", "from_variant": 0, "activity": "lie", "secs": 1.5, "fps": 12, "kind": "once"})
	return all


var _dog
var _skeleton: Skeleton3D
var _started := false


## The scene is only live from the first frame on: transforms read before then are empty.
func _process(_delta: float) -> bool:
	if not _started:
		_started = true
		_export()
	return false


func _export() -> void:
	var args := OS.get_cmdline_user_args()
	var option := func(flag: String, fallback: String) -> String:
		var i := args.find(flag)
		return args[i + 1] if i >= 0 and i + 1 < args.size() else fallback
	var growth := float(option.call("--growth", "1.0"))
	var size_px := float(option.call("--size", "420"))
	var out: String = option.call("--out", "poses.json")
	var only: PackedStringArray = option.call("--only", "").split(",", false)
	var scale := lerpf(0.45, 1.0, growth)     # as the daemon sizes him

	_dog = Dog.new()
	root.add_child(_dog)
	_dog.apply_growth(growth)
	_dog.scale = Vector3.ONE * scale
	_dog.set_facing(1)
	_skeleton = _dog._skeleton

	# The window and camera main.gd would set up for him at this size.
	var reach: Vector2 = _dog.get_reach() * scale
	var margin := maxi(ceili(GROUND_MARGIN * size_px * scale), 14)
	var window := Vector2i(ceili(2.0 * reach.x * size_px), ceili(reach.y * size_px) + margin)
	var height_units := float(window.y) / size_px
	var aim_y := (height_units * 0.5 - float(margin) / size_px) / cos(CAMERA_PITCH)
	var toward_viewer := Vector3(0.0, sin(CAMERA_PITCH), cos(CAMERA_PITCH))
	var camera := Transform3D(Basis.from_euler(Vector3(-CAMERA_PITCH, 0.0, 0.0)), Vector3(0.0, aim_y, 0.0) + toward_viewer * 6.0)

	var names := []
	for b in _skeleton.get_bone_count():
		names.append(_skeleton.get_bone_name(b))
	var recorded := []
	for clip in clips():
		if only.is_empty() or clip["name"] in only:
			recorded.append(_record(clip))
			print("recorded %s: %d frames" % [clip["name"], recorded[-1]["frames"].size()])
	var file := FileAccess.open(out, FileAccess.WRITE)
	file.store_string(JSON.stringify({
		"growth": growth, "scale": scale, "size_px": size_px, "window": [window.x, window.y], "margin": margin,
		"camera": {"transform": _numbers(camera), "size": height_units, "near": 0.1, "far": 20.0},
		"bones": names, "clips": recorded,
	}))
	file.close()
	quit()


func _record(clip: Dictionary) -> Dictionary:
	var sleepy: bool = clip.get("sleepy", false)
	_dog.set_mood(0.6, 0.3, 0.9 if sleepy else 0.2, 0.0)
	_dog._chance.seed = hash(clip["name"])
	var frames := []
	if clip["kind"] == "cycle":
		# One full stride at walking pace, in equal steps of the stride.
		_dog.begin(clip["activity"], 0, 0.6, 600.0, 11)
		_run(2.0, _dog.WALK_SPEED)
		var count: int = clip["frames"]
		var stride_secs: float = _dog.STRIDE / _dog.WALK_SPEED
		# Start where the stride starts, so frame k is at k/count of it.
		while _dog.stride().x > 0.02:
			_run(STEP, _dog.WALK_SPEED)
		for k in count:
			frames.append(_snap())
			_run(stride_secs / count, _dog.WALK_SPEED)
		return {"name": clip["name"], "kind": "cycle", "fps": count / stride_secs, "frames": frames}

	if clip.has("from"):
		_dog.begin(clip["from"], int(clip.get("from_variant", 0)), 0.6, 600.0, 13)
		_run(4.0, 0.0)
	_dog.begin(clip["activity"], int(clip.get("variant", 0)), 0.6, 600.0, 17)
	if not clip.has("from"):
		_run(4.0, 0.0, clip.get("gaze", null))     # settled into the posture first
	var fps: int = clip["fps"]
	var count := int(round(float(clip["secs"]) * fps))
	for k in count:
		frames.append(_snap())
		_run(1.0 / fps, 0.0, clip.get("gaze", null))
	return {"name": clip["name"], "kind": clip["kind"], "fps": fps, "gaze": clip.get("gaze", null), "frames": frames}


## Moves him on for `secs`, walking at `speed` (units a second), his attention held by `gaze` if given.
func _run(secs: float, speed: float, gaze = null) -> void:
	var left := secs
	while left > 1e-6:
		var dt := minf(STEP, left)
		if gaze != null:
			var a := deg_to_rad(float(gaze))
			_dog.look_towards(Vector3(sin(a), 0.35, cos(a)), 1.0)
		_dog.advance(dt, speed * dt)
		left -= dt


func _snap() -> Dictionary:
	var base: Transform3D = _skeleton.global_transform
	var bones := []
	for b in _skeleton.get_bone_count():
		bones.append_array(_numbers(base * _skeleton.get_bone_global_pose(b) * _skeleton.get_bone_global_rest(b).affine_inverse()))
	var eyes := []
	for i in 2:
		eyes.append({
			"transform": _numbers(_dog._eyes[i].global_transform), "radius": _dog._eyes[i].get_meta("radius"),
			"upper": _dog._lids[i * 3].rotation.z, "lower": _dog._lids[i * 3 + 1].rotation.z,
		})
	return {"bones": bones, "eyes": eyes, "tongue": {"shown": _dog._tongue.visible, "transform": _numbers(_dog._tongue.global_transform)}}


static func _numbers(t: Transform3D) -> Array:
	var b := t.basis
	return [b.x.x, b.x.y, b.x.z, b.y.x, b.y.y, b.y.z, b.z.x, b.z.y, b.z.z, t.origin.x, t.origin.y, t.origin.z]
