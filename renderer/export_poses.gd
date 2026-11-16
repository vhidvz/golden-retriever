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

## What is drawn. "loop": a stretch of an activity that is played forwards and back;
## "cycle": one stride, shown according to how far he has walked (at "speed" times his
## walking pace); "once": a change of posture, played through once; "progress": an
## activity that unfolds over its whole length (a stretch), shown according to how far
## through it he is; "repeat": exactly one round of a quick, regular movement (a hind leg
## scratching, licking), played forwards and round again at the speed it was recorded;
## "spin": one whole turn on the spot, shown according to how far round he has turned.
## "from" is the posture he is in when it starts. "gaze" turns his attention towards a
## direction (degrees around him: 0 is the viewer, positive to the right). "bowl": his
## bowl is in the picture.
static func clips() -> Array:
	var all := [
		{"name": "stand", "activity": "stand", "secs": 4.0, "fps": 12, "kind": "loop"},
		{"name": "sit", "activity": "sit", "secs": 4.0, "fps": 12, "kind": "loop"},
		{"name": "lie", "activity": "lie", "secs": 4.0, "fps": 12, "kind": "loop"},
		{"name": "lie_chin", "activity": "lie", "variant": 2, "secs": 4.0, "fps": 12, "kind": "loop"},
		{"name": "sleep_side", "activity": "sleep", "variant": 0, "secs": 5.0, "fps": 10, "kind": "loop", "sleepy": true},
		{"name": "sleep_curled", "activity": "sleep", "variant": 1, "secs": 5.0, "fps": 10, "kind": "loop", "sleepy": true},
		{"name": "sleep_chin", "activity": "sleep", "variant": 2, "secs": 5.0, "fps": 10, "kind": "loop", "sleepy": true},
		{"name": "walk", "activity": "walk", "frames": 16, "kind": "cycle", "speed": 1.0},
		{"name": "trot", "activity": "walk", "frames": 16, "kind": "cycle", "speed": 2.0},
		{"name": "gallop", "activity": "zoomies", "frames": 16, "kind": "cycle", "speed": 3.4},
		{"name": "eat", "activity": "eat", "secs": 4.0, "fps": 12, "kind": "loop", "bowl": true},
		{"name": "drink", "activity": "drink", "secs": 4.0, "fps": 12, "kind": "loop", "bowl": true},
		{"name": "sniff", "activity": "sniff", "variant": 0, "secs": 4.0, "fps": 12, "kind": "loop"},
		{"name": "sniff_air", "activity": "sniff", "variant": 1, "secs": 3.0, "fps": 12, "kind": "loop"},
		{"name": "play_bow", "activity": "play_bow", "secs": 3.7, "fps": 12, "kind": "loop"},
		{"name": "stretch", "activity": "stretch", "variant": 0, "frames": 30, "kind": "progress", "secs": 3.0},
		{"name": "stretch_back", "activity": "stretch", "variant": 1, "frames": 30, "kind": "progress", "secs": 3.0},
		{"name": "yawn_stand", "from": "stand", "activity": "yawn", "frames": 24, "kind": "progress", "secs": 2.5},
		{"name": "yawn_sit", "from": "sit", "activity": "yawn", "frames": 24, "kind": "progress", "secs": 2.5},
		{"name": "yawn_lie", "from": "lie", "activity": "yawn", "frames": 24, "kind": "progress", "secs": 2.5},
		{"name": "shake", "activity": "shake", "frames": 60, "kind": "progress", "secs": 2.0},
		{"name": "roll", "activity": "roll", "frames": 48, "kind": "progress", "secs": 4.0},
		{"name": "scratch_l", "activity": "scratch", "variant": 0, "frames": 8, "kind": "repeat", "secs": TAU / 36.0},
		{"name": "scratch_r", "activity": "scratch", "variant": 1, "frames": 8, "kind": "repeat", "secs": TAU / 36.0},
		{"name": "groom_paw", "activity": "groom", "variant": 0, "frames": 12, "kind": "repeat", "secs": TAU / 6.5},
		{"name": "groom_flank", "activity": "groom", "variant": 1, "frames": 12, "kind": "repeat", "secs": TAU / 6.5},
		{"name": "tail_chase", "activity": "tail_chase", "variant": 0, "frames": 24, "kind": "spin"},
		{"name": "tail_chase_back", "activity": "tail_chase", "variant": 1, "frames": 24, "kind": "spin"},
		{"name": "greet", "activity": "greet", "variant": 0, "secs": 3.0, "fps": 12, "kind": "loop"},
		# Greeting with a whole turn and then a bow: the turn and the bow follow how far through it he is.
		{"name": "greet_turn", "activity": "greet", "variant": 1, "frames": 48, "kind": "progress", "secs": 4.0},
		{"name": "invite", "activity": "invite", "variant": 0, "secs": 3.0, "fps": 12, "kind": "loop"},
		{"name": "invite_bow", "activity": "invite", "variant": 1, "secs": 2.6, "fps": 12, "kind": "loop"},
		# The quick turn on the spot that starts the third way of inviting: by how far round he is.
		{"name": "invite_turn", "activity": "invite", "variant": 2, "frames": 24, "kind": "spin", "planned": 8.0, "settle": 0.0},
		{"name": "seek_attention", "activity": "seek_attention", "variant": 0, "secs": 3.4, "fps": 12, "kind": "loop"},
		{"name": "seek_paw", "activity": "seek_attention", "variant": 1, "secs": 3.4, "fps": 12, "kind": "loop"},
		{"name": "lie_side", "activity": "lie", "variant": 1, "secs": 4.0, "fps": 12, "kind": "loop"},
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
		# One full stride at the given pace, in equal steps of the stride.
		var speed: float = _dog.WALK_SPEED * float(clip["speed"])
		_dog.begin(clip["activity"], 0, 0.6, 600.0, 11)
		_run(2.0, speed)
		var count: int = clip["frames"]
		# How long one stride takes at this pace (strides lengthen into a gallop).
		var before: float = _dog._phase
		_run(0.5, speed)
		var stride_secs := 0.5 * TAU / maxf(_dog._phase - before, 1e-6)
		# Start where the stride starts, so frame k is at k/count of it.
		while _dog.stride().x > 0.01:
			_run(STEP / 8.0, speed)
		for k in count:
			frames.append(_snap())
			_run(stride_secs / count, speed)
		return {"name": clip["name"], "kind": "cycle", "speed": clip["speed"], "frames": frames}

	if clip["kind"] == "spin":
		# One whole turn on the spot, in equal steps of the turn. A turn that is only part of
		# an activity (the start of an invitation) is given a short planned length, so that it
		# is quick, and recorded from the very start.
		_dog.begin(clip["activity"], int(clip.get("variant", 0)), 0.6, float(clip.get("planned", 600.0)), 23)
		_run(float(clip.get("settle", 2.0)), 0.0)
		var count: int = clip["frames"]
		var start: float = _dog._spin
		var probe: float = _dog._spin
		_run(STEP / 4.0, 0.0)
		var direction: float = signf(_dog._spin - probe)
		for k in count:
			frames.append(_snap())
			if k == count - 1:
				break
			var target := start + direction * TAU * (k + 1) / count
			var steps := 0
			while (_dog._spin - target) * direction < 0.0 and steps < 100000:     # (a turn that stops short ends the clip there)
				_run(STEP / 4.0, 0.0)
				steps += 1
		# Where in the turn the first picture is, so a picture can be matched to an angle.
		return {"name": clip["name"], "kind": "spin", "direction": direction, "start_turn": fposmod(start, TAU) / TAU, "frames": frames}

	if clip["kind"] == "repeat":
		# One round of a quick, regular movement, in equal steps of time.
		_dog.begin(clip["activity"], int(clip.get("variant", 0)), 0.6, 600.0, 29)
		_run(4.0, 0.0)
		var count: int = clip["frames"]
		var secs: float = clip["secs"]
		for k in count:
			frames.append(_snap())
			_run(secs / count, 0.0)
		return {"name": clip["name"], "kind": "repeat", "fps": count / secs, "frames": frames}

	if clip["kind"] == "progress":
		# The whole activity, from start to end, in equal steps.
		var secs: float = clip["secs"]
		if clip.has("from"):
			_dog.begin(clip["from"], 0, 0.6, 600.0, 13)
			_run(4.0, 0.0)
		_dog.begin(clip["activity"], int(clip.get("variant", 0)), 0.6, secs, 19)
		var count: int = clip["frames"]
		for k in count:
			frames.append(_snap())
			_run(secs / (count - 1), 0.0)
		return {"name": clip["name"], "kind": "progress", "frames": frames}

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
	var recorded := {"name": clip["name"], "kind": clip["kind"], "fps": fps, "gaze": clip.get("gaze", null), "frames": frames}
	if clip.get("bowl", false):
		# Where main.gd puts his bowl, at his size, facing right.
		recorded["bowl"] = {"at": _vector(Vector3(_dog.get_bowl_distance(), 0.035, 0.2) * _dog.scale.x), "scale": _dog.scale.x, "water": clip["activity"] == "drink"}
	return recorded


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
			"look": _vector(_dog._pupils[i].position.normalized()),     # where the pupil points, in the eye's own space
		})
	return {"bones": bones, "eyes": eyes, "tongue": {"shown": _dog._tongue.visible, "transform": _numbers(_dog._tongue.global_transform)}}


static func _vector(v: Vector3) -> Array:
	return [v.x, v.y, v.z]


static func _numbers(t: Transform3D) -> Array:
	var b := t.basis
	return [b.x.x, b.x.y, b.x.z, b.y.x, b.y.y, b.y.z, b.z.x, b.z.y, b.z.z, t.origin.x, t.origin.y, t.origin.z]
