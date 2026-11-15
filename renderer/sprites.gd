extends CanvasLayer
## Shows Buddy as pictures drawn beforehand with a real coat (tools/bake_sprites.py),
## in place of the live 3D body, whenever there are pictures for what he is doing.
##
## The 3D dog keeps moving underneath, unseen: it is still what decides his posture,
## his stride and where he is looking, and this only picks the pictures that match.
## For anything not drawn yet (a roll, a stretch, a trot) the 3D body is shown again.
##
## Pictures live in ~/.local/share/buddy/sprites/<set>/: one folder of numbered
## WebP pictures per clip and a manifest.json saying what each clip is. A set is
## drawn for one stage of his growth (by the daemon, while the user is away) and is
## used while he is near enough to it: a complete set before one still being drawn,
## then the one with more of his movements drawn, then the nearest, then the newest. Sets are looked for again every half minute, so
## a newly finished one takes over by itself.

const CROSSFADE := 0.25          # seconds to fade from one clip to the next
const POSTURE_FADE := 0.55       # when there is no picture of the change of posture itself
const GROWTH_TOLERANCE := 0.08   # a set drawn this far from his present growth still passes for him
const GAZE_ON := 0.55            # attention needed before he is shown looking at something …
const GAZE_OFF := 0.35           # … and how far it must drop before he is shown looking away again
const GAZE_STICK := 10.0         # degrees: a drawn look is kept until another fits this much better

var _sets := []                  # [{"path", "manifest"}] found on disk
var _set := {}                   # the set in use: {"path", "manifest", "clips": {name: {"info", "frames": [bytes]}}}
var _rect := ColorRect.new()
var _material := ShaderMaterial.new()
var _textures := {}              # "cur", "nxt", "old" -> ImageTexture
var _clip := ""                  # clip being shown
var _clip_time := 0.0
var _queued := ""                # loop to go on to when a change of posture has played
var _shown_frame := -1
var _fade := 0.0                 # weight of the old picture, fading to 0
var _fade_speed := 1.0 / CROSSFADE
var _visible := 0.0              # 0 = 3D body, 1 = pictures
var _base := ""                  # the posture family last shown ("stand", "sit", "lie", "sleep_side", …)
var _recheck := 30.0             # seconds until clips still being drawn are looked for again
var _facing := 1
var _decoded := {}               # frame key -> Image: the few pictures decoded last, so none is decoded twice
var _images := {}                # slot -> the Image last put there (reading a texture back loses its colour)
var _extra := ""                 # a set given for a test
var _growth := -1.0


func _ready() -> void:
	layer = 1
	_rect.set_anchors_preset(Control.PRESET_FULL_RECT)
	_rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_material.shader = preload("res://sprites.gdshader")
	_rect.material = _material
	_rect.visible = false
	add_child(_rect)
	find_sets()


## Looks for drawn sets, in the user's data folder or in `extra` (for tests).
func find_sets(extra := "") -> void:
	if extra != "":
		_extra = extra
	extra = _extra
	_sets.clear()
	var data_home := OS.get_environment("XDG_DATA_HOME")
	if data_home == "":
		data_home = OS.get_environment("HOME").path_join(".local/share")
	var folders := []
	if extra != "":
		folders.append(extra)
	var root := data_home.path_join("buddy/sprites")
	var dir := DirAccess.open(root)
	if dir != null:
		for name in dir.get_directories():
			folders.append(root.path_join(name))
	for folder in folders:
		var text := FileAccess.get_file_as_string(String(folder).path_join("manifest.json"))
		var manifest = JSON.parse_string(text) if text != "" else null
		if manifest is Dictionary:
			var stamp := FileAccess.get_modified_time(String(folder).path_join("manifest.json"))
			var drawn := 0
			for info in manifest.get("clips", []):
				if FileAccess.file_exists(String(folder).path_join(info["name"]).path_join("%03d.webp" % (int(info["frames"]) - 1))):
					drawn += 1
			_sets.append({"path": folder, "manifest": manifest, "complete": manifest.get("complete", false) == true, "drawn": drawn, "stamp": stamp})


func has_sets() -> bool:
	return not _sets.is_empty()


## His growth, as the daemon says it is now.
func use_growth(growth: float) -> void:
	if absf(growth - _growth) > 0.001 or _set.is_empty():
		_growth = growth
		_pick()


## Uses the best set near enough to his growth: complete first, then nearest, then newest.
func _pick() -> void:
	var best := {}
	var best_score := []
	for candidate in _sets:
		var off := absf(float(candidate["manifest"]["growth"]) - _growth)
		if off > GROWTH_TOLERANCE:
			continue
		var score := [1 if candidate["complete"] else 0, candidate["drawn"], -snappedf(off, 0.001), candidate["stamp"]]
		if best.is_empty() or score > best_score:
			best = candidate
			best_score = score
	if best.is_empty():
		_set = {}
		return
	if not _set.is_empty() and _set["path"] == best["path"]:
		return
	_set = {"path": best["path"], "manifest": best["manifest"], "clips": {}}
	_load_clips()
	_clip = ""


## Loads every clip of the set in use whose pictures are all there (a bake may still be drawing others).
func _load_clips() -> void:
	var clips: Dictionary = _set["clips"]
	for info in _set["manifest"]["clips"]:
		if clips.has(info["name"]):
			continue
		var frames := []
		for k in int(info["frames"]):
			var bytes := FileAccess.get_file_as_bytes(String(_set["path"]).path_join(info["name"]).path_join("%03d.webp" % k))
			if bytes.is_empty():
				frames = []
				break      # a clip not fully drawn yet is not used
			frames.append(bytes)
		if not frames.is_empty():
			clips[info["name"]] = {"info": info, "frames": frames}


## The clip that shows what the dog is doing now, or "" if none has been drawn.
func _choose(dog, activity: String, variant: int, facing: int) -> String:
	if _set.is_empty():
		return ""
	var clips: Dictionary = _set["clips"]
	var stride: Vector2 = dog.stride()
	if stride.y > 0.25:
		# The drawn stride nearest his pace: a walk, a trot, a gallop.
		var pace := "walk" if stride.y < 1.5 else ("trot" if stride.y < 2.7 else "gallop")
		return pace if clips.has(pace) else ""
	var base := ""
	match activity:
		"stand", "sit", "eat", "drink", "play_bow":
			base = activity
		"lie":
			base = {0: "lie", 2: "lie_chin"}.get(variant, "")
		"sleep":
			base = {0: "sleep_side", 1: "sleep_curled", 2: "sleep_chin"}.get(variant, "")
		"sniff":
			base = {0: "sniff", 1: "sniff_air"}.get(variant, "")
		"stretch":
			base = {0: "stretch", 1: "stretch_back"}.get(variant, "")
	if base == "" or not clips.has(base):
		return ""
	# Looking at you, or at something on the screen: the drawn look nearest to where it is.
	var posture := base if base in ["stand", "sit", "lie"] else ""
	var looking := _clip.begins_with(posture + "_look") or _queued.begins_with(posture + "_look")
	if posture != "" and dog.attention() > (GAZE_OFF if looking else GAZE_ON):
		var angle: float = dog.gaze_angle() * float(facing)     # the pictures face right; facing left they are mirrored
		var best := ""
		var nearest := 1e9
		for name in clips:
			var gaze = clips[name]["info"].get("gaze")
			if gaze != null and String(name).begins_with(posture + "_look") and absf(float(gaze) - angle) < nearest:
				nearest = absf(float(gaze) - angle)
				best = name
		# Keep the look already shown unless another fits clearly better: no flicking between two.
		if looking and clips.has(_clip) and _clip.begins_with(posture + "_look"):
			if absf(float(clips[_clip]["info"]["gaze"]) - angle) <= nearest + GAZE_STICK:
				return _clip
		if best != "" and nearest <= 35.0:     # (a look drawn far off where he is looking is worse than none)
			return best
	return base


## Called every frame. Returns true while pictures are being shown (so the 3D body can be hidden).
func update(delta: float, dog, activity: String, variant: int, facing: int) -> bool:
	_recheck -= delta
	if _recheck <= 0.0:
		_recheck = 30.0
		find_sets()
		var before: String = _set.get("path", "")
		_pick()
		if not _set.is_empty() and _set["path"] == before and _set["clips"].size() < _set["manifest"]["clips"].size():
			_load_clips()
	var wanted := _choose(dog, activity, variant, facing)
	if wanted != "":
		var family := _family(wanted)
		if wanted != _clip and wanted != _queued:
			var change := "%s_to_%s" % [_base, family]
			if _clip != "" and _base != "" and family != _base and _set["clips"].has(change):
				_start(change, CROSSFADE)
				_queued = wanted
			else:
				_start(wanted, POSTURE_FADE if family != _base and _clip != "" else CROSSFADE)
				_queued = ""
			_base = family
		elif wanted == _queued and _clip_done():
			_start(_queued, CROSSFADE)
			_queued = ""
	if facing != _facing and _rect.visible and _clip != "":
		_start(_clip, CROSSFADE, _clip_time)      # turning round: fade into the mirror image
	_facing = facing
	_visible = move_toward(_visible, 1.0 if wanted != "" else 0.0, delta / CROSSFADE)
	_rect.visible = _visible > 0.0 and _clip != ""
	if not _rect.visible:
		return false
	_clip_time += delta
	_fade = maxf(_fade - delta * _fade_speed, 0.0)
	_show(dog, facing)
	return _visible >= 1.0


## True while the picture shown has his bowl drawn in it (so the live one is not shown as well).
func shows_bowl() -> bool:
	return _rect.visible and _clip != "" and _set["clips"].has(_clip) and _set["clips"][_clip]["info"].get("bowl") != null


func _family(clip: String) -> String:
	if clip in ["walk", "trot", "gallop"]:
		return "stand"
	for posture in ["stand", "sit", "lie"]:
		if clip.begins_with(posture + "_look"):
			return posture
	return {"lie_chin": "lie"}.get(clip, clip)


func _clip_done() -> bool:
	if _clip == "":
		return true
	var info: Dictionary = _set["clips"][_clip]["info"]
	return info["kind"] == "once" and _clip_time * float(info["fps"]) >= float(info["frames"]) - 1.0


func _start(clip: String, fade: float, at := 0.0) -> void:
	# Fade out of what is on the screen now; after the 3D body there is nothing of ours to fade from.
	if _images.has("cur") and _rect.visible:
		_put("old", _images["cur"])
		_material.set_shader_parameter("flip_old", _material.get_shader_parameter("flip"))
		_fade = 1.0
		_fade_speed = 1.0 / fade
	else:
		_fade = 0.0
	_clip = clip
	_clip_time = at
	_shown_frame = -1


func _show(dog, facing: int) -> void:
	var entry: Dictionary = _set["clips"][_clip]
	var info: Dictionary = entry["info"]
	var count: int = entry["frames"].size()
	var position: float
	match info["kind"]:
		"cycle":
			# A stride is shown a picture at a time: blending two of them doubles the legs.
			position = floorf(dog.stride().x * count + 0.5)
		"once":
			position = minf(_clip_time * float(info["fps"]), count - 1.0)
		"progress":
			position = dog.progress() * (count - 1.0)
		_:
			# Forwards and back again: the end of a stretch of life never meets its start,
			# so going round would jump; turning back does not.
			position = pingpong(_clip_time * float(info["fps"]), count - 1.0)
	var frame := int(floor(position)) % count
	var next := frame + 1
	if next >= count:
		next = 0 if info["kind"] == "cycle" else frame
	if frame != _shown_frame:
		_put("cur", _picture(entry, frame))
		_put("nxt", _picture(entry, next))
		_shown_frame = frame
	_material.set_shader_parameter("blend", position - floor(position))
	_material.set_shader_parameter("fade", _fade)
	_material.set_shader_parameter("shown", _visible)
	_material.set_shader_parameter("flip", facing < 0)


func _picture(entry: Dictionary, index: int) -> Image:
	var key := "%s/%d" % [entry["info"]["name"], index]
	if not _decoded.has(key):
		if _decoded.size() >= 4:
			_decoded.erase(_decoded.keys()[0])
		_decoded[key] = _decode(entry["frames"][index])
	return _decoded[key]


func _decode(bytes: PackedByteArray) -> Image:
	var image := Image.new()
	image.load_webp_from_buffer(bytes)
	# Premultiplied before it reaches the graphics card: filtering a picture with straight alpha
	# pulls the black of the transparent pixels into his outline as a dark rim.
	image.premultiply_alpha()
	return image


func _put(slot: String, image: Image) -> void:
	if image == null or image.is_empty():
		return
	_images[slot] = image
	if _textures.has(slot) and _textures[slot].get_size() == Vector2(image.get_size()):
		_textures[slot].update(image)
	else:
		_textures[slot] = ImageTexture.create_from_image(image)
		_material.set_shader_parameter(slot, _textures[slot])
