extends Node3D
## Draws Buddy in a transparent overlay window and passes on what the user does
## to him. All decisions are made by the daemon; this process renders the state
## it is sent and can be killed and restarted without Buddy noticing.
##
## Self-test (no daemon needed): renders one pose to a PNG and reports whether it fits.
##   godot --path renderer --display-driver x11 -- --selftest out.png [--activity sit] [--variant 1]
##       [--growth 0.2] [--facing -1] [--at 1.5] [--gaze 1] [--arousal 0.8] [--size 420]

const GROUND_MARGIN := 0.11        # of his height, kept below the paws for the shadow and for limbs that reach towards the viewer
const CAMERA_PITCH := 0.14         # radians; a slight look down so the ground reads as a surface
const POINTER_REACH := 650.0        # pixels: how near the pointer must come before he watches it
const DRAG_THRESHOLD := 8.0        # pixels the pointer must travel before a press becomes a drag
const RECONNECT_SECONDS := 1.0
const ORPHAN_SECONDS := 10.0       # a supervised renderer exits if its daemon stays gone this long
const QUICK := ["walk", "zoomies", "tail_chase", "shake", "scratch", "greet", "invite", "play_bow", "roll", "eat", "drink"]
const SLOW := {"lie": 20, "sleep": 10}
const Dog := preload("res://retriever.gd")   # the skinned model; "res://dog.gd" is the older stand-in

var _dog := Dog.new()
var _camera := Camera3D.new()
var _shadow := MeshInstance3D.new()
var _bowl := MeshInstance3D.new()
var _menu := PopupMenu.new()
var _voice := preload("res://voice.gd").new()

var _peer := StreamPeerUDS.new()
var _inbox := PackedByteArray()
var _online := false
var _since_attempt := RECONNECT_SECONDS
var _offline_for := 0.0

var _state := {}
var _instance := -1                      # seed of the activity instance being shown
var _layout_key := Vector2(-1.0, -1.0)   # (scale, size_px) the window is currently laid out for
var _pixels_per_unit := 420.0
var _usable := Rect2i()
var _margin := 24                        # GROUND_MARGIN in pixels at his current size
var _walk_px := 1000.0
var _shown_x := -1.0                     # where the window's left edge is drawn, in walk pixels

var _pressed := false
var _dragging := false
var _press_at := Vector2.ZERO
var _grab_offset := 0.0

var _selftest_path := ""
var _selftest_at := 1.5
var _drawn_at_start := 0
var _selftest_speed := 0.0               # heights per second to pretend he is moving at
var _clock := 0.0


func _ready() -> void:
	get_viewport().transparent_bg = true
	var window := get_window()
	window.borderless = true
	window.always_on_top = true
	window.transparent = true
	# Unfocusable makes this an override-redirect X11 window: always on top, on every
	# workspace, out of the taskbar, and never taking keyboard focus when clicked.
	window.unfocusable = true
	window.unresizable = true
	window.title = "Buddy"

	var environment := Environment.new()
	environment.background_mode = Environment.BG_CLEAR_COLOR
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(1.0, 0.97, 0.92)
	environment.ambient_light_energy = 0.55
	var world := WorldEnvironment.new()
	world.environment = environment
	add_child(world)

	var sun := DirectionalLight3D.new()
	sun.rotation = Vector3(deg_to_rad(-50.0), deg_to_rad(-35.0), 0.0)   # from the upper left, like most desktop themes
	sun.light_energy = 1.1
	add_child(sun)

	_camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	_camera.near = 0.1
	_camera.far = 20.0
	add_child(_camera)

	var ground := PlaneMesh.new()
	ground.size = Vector2(1.7, 0.6)
	var shade := ShaderMaterial.new()
	shade.shader = preload("res://shadow.gdshader")
	_shadow.mesh = ground
	_shadow.material_override = shade
	_shadow.position.y = 0.002
	add_child(_shadow)

	var dish := CylinderMesh.new()
	dish.top_radius = 0.15
	dish.bottom_radius = 0.11
	dish.height = 0.07
	var steel := StandardMaterial3D.new()
	steel.albedo_color = Color(0.62, 0.66, 0.72)
	steel.metallic = 0.7
	steel.roughness = 0.35
	_bowl.mesh = dish
	_bowl.material_override = steel
	_bowl.visible = false
	add_child(_bowl)
	add_child(_dog)
	add_child(_voice)

	for entry in [["Good boy!", "praise"], ["Give a treat", "treat"], ["Give food", "feed"], ["Give water", "water"], [], ["Go to sleep", "sleep"], ["Wake up", "wake"],
			["Hide for 15 minutes", "hide"], [], ["Quiet mode", "quiet"], ["Privacy mode", "privacy"], [], ["Quit Buddy", "quit"]]:
		if entry.is_empty():
			_menu.add_separator()
			continue
		if entry[1] in ["quiet", "privacy"]:
			_menu.add_check_item(entry[0])
		else:
			_menu.add_item(entry[0])
		_menu.set_item_metadata(_menu.item_count - 1, entry[1])
	_menu.index_pressed.connect(_on_menu)
	add_child(_menu)

	var args := OS.get_cmdline_user_args()
	var option := func(flag: String, fallback: String) -> String:
		var i := args.find(flag)
		return args[i + 1] if i >= 0 and i + 1 < args.size() else fallback
	_selftest_path = option.call("--selftest", "")
	if _selftest_path != "":
		RenderingServer.viewport_set_measure_render_time(get_viewport().get_viewport_rid(), true)
		_selftest_at = float(option.call("--at", "1.5"))
		_drawn_at_start = Engine.get_frames_drawn()
		_selftest_speed = float(option.call("--speed", "0"))
		var growth := float(option.call("--growth", "1.0"))
		_apply_state({
			"activity": option.call("--activity", "stand"), "variant": int(option.call("--variant", "0")), "intensity": 0.7,
			"started": Time.get_unix_time_from_system(), "planned_secs": 2.0 * _selftest_at, "seed": 7,
			"growth": growth, "scale": lerpf(0.45, 1.0, growth), "size_px": float(option.call("--size", "420")), "x_frac": 0.7, "facing": int(option.call("--facing", "1")),
			"hidden": false, "privacy": false, "quiet": false, "ducked": false, "user_here": true, "load": 0.0, "valence": 0.6, "arousal": float(option.call("--arousal", "0.3")), "sleepiness": 0.2, "confusion": float(option.call("--confusion", "0.0")),
		})
		if option.call("--gaze", "0") == "1":
			_dog.look_at_viewer(60.0)
		elif option.call("--look", "") != "":
			_look.call_deferred(option.call("--look", ""), 60.0)


func _socket_path() -> String:
	var explicit := OS.get_environment("BUDDY_SOCKET")
	return explicit if explicit != "" else OS.get_environment("XDG_RUNTIME_DIR").path_join("buddy/buddy.sock")


func _send(message: Dictionary) -> void:
	if _online:
		_peer.put_data((JSON.stringify(message) + "\n").to_utf8_buffer())


func _command(command_name: String, args: Dictionary = {}) -> void:
	_send({"type": "command", "name": command_name, "args": args})


func _pump_socket(delta: float) -> void:
	_peer.poll()
	var connected := _peer.get_status() == StreamPeerSocket.STATUS_CONNECTED
	if connected and not _online:
		_online = true
		_offline_for = 0.0
		_send({"type": "hello", "role": "renderer"})
		_send({"type": "screen", "walk_px": _walk_px})
	elif not connected:
		_online = false
		_offline_for += delta
		_since_attempt += delta
		if OS.get_environment("BUDDY_SUPERVISED") != "" and _offline_for > ORPHAN_SECONDS:
			get_tree().quit()
		if _since_attempt >= RECONNECT_SECONDS and _peer.get_status() != StreamPeerSocket.STATUS_CONNECTING:
			_since_attempt = 0.0
			_peer = StreamPeerUDS.new()
			_peer.connect_to_host(_socket_path())
		return

	var waiting := _peer.get_available_bytes()
	if waiting > 0:
		_inbox.append_array(_peer.get_data(waiting)[1])
	while true:
		var newline := _inbox.find(10)
		if newline < 0:
			break
		var message = JSON.parse_string(_inbox.slice(0, newline).get_string_from_utf8())
		_inbox = _inbox.slice(newline + 1)
		if not message is Dictionary:
			continue
		if message.get("type") == "state":
			_apply_state(message)
		elif message.get("type") == "event" and not _state.is_empty():
			var args: Dictionary = message.get("args", {})
			match message.get("name"):
				"vocalize":
					_voice.say(args.get("kind", ""), args.get("intensity", 0.5), _state["growth"])
				"look":
					_look(args.get("at", "you"), args.get("secs", 2.0))
				"pointer":
					# He watches the pointer when it comes near, as a dog watches a hand.
					var offset := Vector2(args.get("x", 0.0), args.get("y", 0.0)) - _head_on_screen()
					if offset.length() < POINTER_REACH * _layout_key.x + 150.0:
						_dog.look_towards(_towards(offset), 0.9)


## Where his head is on the screen, in pixels.
func _head_on_screen() -> Vector2:
	var window := get_window()
	return Vector2(window.position) + Vector2(window.size.x * 0.5, window.size.y - _margin - 0.85 * _pixels_per_unit * _layout_key.x)


## A direction in the scene for something `offset` pixels from his head on the
## screen. It leans out of the glass a little so his face stays towards the viewer.
func _towards(offset: Vector2) -> Vector3:
	return Vector3(offset.x, -offset.y, 0.3 * offset.length() + 1.0)


func _look(at: String, secs: float) -> void:
	match at:
		"screen":          # the middle of the screen, above him
			_dog.look_towards(_towards(Vector2(_usable.get_center()) - _head_on_screen()), secs)
		"notification":    # GNOME shows them at the top centre
			_dog.look_towards(_towards(Vector2(_usable.get_center().x, _usable.position.y) - _head_on_screen()), secs)
		"sound":           # he cannot tell where it came from: up and to one side
			_dog.look_towards(Vector3(randf_range(-1.0, 1.0), 0.35, 0.5), secs)
		_:
			_dog.look_at_viewer(secs)


func _apply_state(state: Dictionary) -> void:
	if state["hidden"]:
		# The daemon stops this process while he is hidden; leave rather than draw an unwanted dog.
		get_tree().quit()
		return
	_state = state
	_dog.apply_growth(state["growth"])
	_dog.scale = Vector3.ONE * float(state["scale"])
	_dog.set_facing(int(state["facing"]))
	_dog.set_mood(state["valence"], state["arousal"], state["sleepiness"], state.get("confusion", 0.0))
	if int(state["seed"]) != _instance:
		_instance = int(state["seed"])
		# Joining part-way through (after a restart, say) picks the activity up where it is.
		var already := maxf(Time.get_unix_time_from_system() - float(state["started"]), 0.0)
		_dog.begin(state["activity"], int(state["variant"]), state["intensity"], state["planned_secs"], _instance, already)
	_voice.muted = state["quiet"] or state.get("ducked", false)
	for i in _menu.item_count:
		var what = _menu.get_item_metadata(i)
		if what in ["quiet", "privacy"]:
			_menu.set_item_checked(i, state[what])

	var key := Vector2(state["scale"], state["size_px"])
	if not key.is_equal_approx(_layout_key):
		_layout_key = key
		_layout()

	var eating: bool = state["activity"] in ["eat", "drink"]
	_bowl.visible = eating
	if eating:
		_bowl.scale = Vector3.ONE * key.x
		_bowl.position = Vector3(float(state["facing"]) * 0.60, 0.035, 0.2) * key.x


## Size the window to everything the dog can reach at his current size, so no
## pose is ever cut off, and aim the camera so his paws sit on the window's ground line.
func _layout() -> void:
	_pixels_per_unit = _layout_key.y
	var reach: Vector2 = _dog.get_reach() * _layout_key.x
	_margin = maxi(ceili(GROUND_MARGIN * _pixels_per_unit * _layout_key.x), 14)
	var size := Vector2i(ceili(2.0 * reach.x * _pixels_per_unit), ceili(reach.y * _pixels_per_unit) + _margin)
	var window := get_window()
	if window.size != size:
		window.size = size

	var height_units := float(size.y) / _pixels_per_unit
	_camera.size = height_units
	var aim_y := (height_units * 0.5 - float(_margin) / _pixels_per_unit) / cos(CAMERA_PITCH)
	var toward_viewer := Vector3(0.0, sin(CAMERA_PITCH), cos(CAMERA_PITCH))
	_camera.position = Vector3(0.0, aim_y, 0.0) + toward_viewer * 6.0
	_camera.rotation = Vector3(-CAMERA_PITCH, 0.0, 0.0)
	_shadow.scale = Vector3.ONE * _layout_key.x

	_usable = DisplayServer.screen_get_usable_rect(DisplayServer.window_get_current_screen())
	_walk_px = maxf(float(_usable.size.x - size.x), 100.0)
	_send({"type": "screen", "walk_px": _walk_px})

	# Only an ellipse around his body takes clicks; the rest of the window is click-through.
	var centre := Vector2(size.x * 0.5, size.y - _margin - reach.y * _pixels_per_unit * 0.42)
	var radii := Vector2(reach.x * 0.75, reach.y * 0.46) * _pixels_per_unit
	var outline := PackedVector2Array()
	for i in 24:
		outline.append(centre + Vector2(cos(i * TAU / 24.0), sin(i * TAU / 24.0)) * radii)
	DisplayServer.window_set_mouse_passthrough(outline)
	_shown_x = -1.0


func _process(delta: float) -> void:
	_clock += minf(delta, 0.05)   # a slow first frame (loading the model) must not count as time passed
	if _selftest_path == "":
		_pump_socket(delta)
	if _state.is_empty():
		return

	var target_x := float(_state["x_frac"]) * _walk_px
	var previous := _shown_x
	if _shown_x < 0.0 or _dragging:
		_shown_x = target_x
		previous = target_x
	else:
		_shown_x = lerpf(_shown_x, target_x, 1.0 - exp(-18.0 * delta))
	var window := get_window()
	if _state.get("ducked", false):
		# Out of the way (lock screen, overview, something fullscreen): off the screen and barely ticking.
		window.position = Vector2i(-30000, -30000)
		Engine.max_fps = 2
		return
	var position := Vector2i(_usable.position.x + roundi(_shown_x), _usable.end.y - window.size.y)
	if window.position != position:
		window.position = position

	# Without a camera, "you" are in front of the middle of the monitor.
	var centre_x := float(position.x) + window.size.x * 0.5
	_dog.set_viewer_offset(clampf((_usable.get_center().x - centre_x) / maxf(_usable.size.x, 1.0), -0.5, 0.5))
	var travelled := (_shown_x - previous) / (_pixels_per_unit * _layout_key.x)
	_dog.advance(delta, travelled if _selftest_speed == 0.0 else _selftest_speed * minf(delta, 0.05))

	# Draw only as often as what he is doing needs.
	var activity: String = _state["activity"]
	var fps: int = 50 if activity in QUICK or _state.get("being_dragged", false) else SLOW.get(activity, 30)
	fps = maxi(fps, 40) if _dog.is_lively() else fps
	# When the machine is working hard he takes less of it.
	Engine.max_fps = fps if float(_state.get("load", 0.0)) < 0.8 else maxi(fps * 3 / 5, 8)

	# Only frames that were really drawn count: a window that is slow to appear draws nothing at first.
	if _selftest_path != "" and _clock >= _selftest_at and Engine.get_frames_drawn() >= _drawn_at_start + 20:
		_finish_selftest()


func _input(event: InputEvent) -> void:
	if _state.is_empty():
		return
	var pointer := Vector2(DisplayServer.mouse_get_position())
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		if event.pressed:
			_pressed = true
			_dragging = false
			_press_at = pointer
			_grab_offset = pointer.x - float(get_window().position.x)
		elif _pressed:
			_pressed = false
			if _dragging:
				_dragging = false
				_command("drag_end")
			else:
				_command("pet")
	elif event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_RIGHT and event.pressed:
		_menu.popup(Rect2i(Vector2i(pointer), Vector2i.ZERO))
	elif event is InputEventMouseMotion and _pressed:
		if not _dragging and pointer.distance_to(_press_at) > DRAG_THRESHOLD:
			_dragging = true
		if _dragging:
			var left := pointer.x - _grab_offset - float(_usable.position.x)
			var x_frac := clampf(left / _walk_px, 0.0, 1.0)
			_state["x_frac"] = x_frac
			_command("drag", {"x_frac": x_frac})


func _on_menu(index: int) -> void:
	var what: String = _menu.get_item_metadata(index)
	match what:
		"hide":
			_command("hide", {"minutes": 15})
		"quiet", "privacy":
			_command(what, {"on": not _state.get(what, false)})
		_:
			_command(what)


## Saves what was drawn and reports whether the dog fits inside the window.
func _finish_selftest() -> void:
	var image := get_viewport().get_texture().get_image()
	image.save_png(_selftest_path)
	# Bounding box of everything clearly visible (the shadow's faint outer edge does not count).
	var used := Rect2i()
	for y in image.get_height():
		for x in image.get_width():
			if image.get_pixel(x, y).a > 0.2:
				used = used.expand(Vector2i(x, y)) if used.has_area() else Rect2i(x, y, 1, 1)
	var clipped := used.position.x <= 0 or used.position.y <= 0 or used.end.x >= image.get_width() - 1 or used.end.y >= image.get_height() - 1
	var rid := get_viewport().get_viewport_rid()
	print("SELFTEST activity=%s/%d growth=%.2f window=%dx%d drawn=%dx%d clipped=%s gpu_ms=%.2f cpu_ms=%.2f sounds=%d" % [
		_state["activity"], _state["variant"], _state["growth"], image.get_width(), image.get_height(), used.size.x, used.size.y,
		clipped, RenderingServer.viewport_get_measured_render_time_gpu(rid), RenderingServer.viewport_get_measured_render_time_cpu(rid),
		_voice.sound_count()])
	get_tree().quit()
