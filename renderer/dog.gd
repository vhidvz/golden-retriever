class_name PlaceholderDog
extends Node3D
## A stand-in Golden Retriever built from fur-covered ellipsoids and animated
## entirely procedurally: every pose is a set of joint targets, blended over
## time, with motion, mood and gaze layered on top. No clips are played, so no
## two instances of an activity look the same.
##
## Units: the adult stands 1.0 tall, +X is forward, the paws rest on Y = 0.
## A better model replaces this file by offering the same methods the renderer
## calls: apply_growth, begin, set_facing, set_mood, set_viewer_offset,
## look_at_viewer, look_towards, advance, get_reach.

const SHELLS := 20
const FUR := preload("res://fur.gdshader")
const STRAND := 0.0045          # width of one strand cell, in units
const STRIDE := 0.75            # ground covered by one full walk cycle
const WALK_SPEED := 0.55        # heights per second; matches the daemon's walk speed
const YAW_RIGHT := -0.35        # three-quarter view towards the viewer
const YAW_LEFT := -2.79
const HEAD_TURN_SPEED := 3.2    # radians per second: the fastest his head turns to follow something

## Joint channels. Angles are degrees. For limbs, neck and head, positive swings
## forward / lifts the nose. "*_yaw" and "face_viewer" are positive towards the viewer.
## "hip" is hip height as a fraction of standing; "ears" runs -1 (pricked) to 1 (laid back).
const NEUTRAL := {
	"hip": 1.0, "lift": 0.0, "pitch": 0.0, "roll": 0.0, "spine_yaw": 0.0, "spine_pitch": 0.0,
	"front_l": 0.0, "front_r": 0.0, "hind_l": 0.0, "hind_r": 0.0,
	"neck": 0.0, "head": 0.0, "head_yaw": 0.0, "head_tilt": 0.0,
	"tail": 35.0, "tail_yaw": 0.0, "jaw": 0.0, "eyes": 1.0, "ears": 0.0, "face_viewer": 0.0,
}
const SIT := {"hip": 0.45, "pitch": 28.0, "front_l": -28.0, "front_r": -28.0, "hind_l": 57.0, "hind_r": 57.0, "neck": -8.0, "head": -20.0, "tail": 10.0}
const SPHINX := {"hip": 0.24, "front_l": 80.0, "front_r": 80.0, "hind_l": 75.0, "hind_r": 75.0, "neck": 6.0, "head": -4.0, "tail": 15.0}
const ON_SIDE := {"hip": 0.40, "roll": -80.0, "front_l": 55.0, "front_r": 38.0, "hind_l": 50.0, "hind_r": 32.0, "neck": 4.0, "head": 12.0, "tail": 5.0}

var _growth := -1.0
var _activity := "stand"
var _variant := 0
var _intensity := 0.5
var _planned := 0.0
var _since := 0.0                 # seconds into the current activity
var _luck := [0.5, 0.5, 0.5, 0.5]  # fixed for one instance of an activity, different for the next
var _posture := "stand"           # the resting posture brief actions (a yawn, say) happen in
var _facing := 1
var _valence := 0.5
var _arousal := 0.3
var _sleepiness := 0.2
var _confusion := 0.0
var _viewer := Vector3(0.0, 0.45, 1.0).normalized()
var _gaze_target := Vector3.BACK  # where his attention is aimed this frame, in scene space

var _pose: Dictionary = NEUTRAL.duplicate()
var _yaw := YAW_RIGHT
var _spin := 0.0
var _hip_height := 0.44
var _time := 0.0
var _phase := 0.0
var _gait := 0.0
var _breath := 0.0
var _wag := 0.0
var _panting := 0.0
var _next_blink := 3.0
var _eye_contact := 0.0           # seconds of looking at the viewer still owed
var _attention := 0.0             # 0..1, how much of his gaze is held by the viewer or by something on screen
var _attention_speed := 0.0
var _aim := Vector3.RIGHT         # the direction of something on the screen he is looking at
var _aim_left := 0.0              # seconds of looking that way still owed
var _gaze_yaw := 0.0              # where his head has actually turned to; it follows what he looks at and never jumps there
var _gaze_yaw_speed := 0.0
var _gaze_lift := 0.0             # the same for how far his head is raised towards it
var _gaze_lift_speed := 0.0
var _wander_yaw := 0.0            # where an idle glance is pointing
var _wander_pitch := 0.0
var _wander_left := 0.0
var _next_wander := 4.0
var _ear_flick := [0.0, 0.0]
var _next_flick := 5.0
var _dream := 0.0
var _next_dream := 30.0
var _chance := RandomNumberGenerator.new()
var _mesh_cache := {}

var _body: Node3D
var _torso: Node3D
var _chest: Node3D
var _neck: Node3D
var _head: Node3D
var _jaw: Node3D
var _tongue: Node3D
var _tail: Node3D
var _legs: Array[Node3D] = []   # front-left, front-right, hind-left, hind-right
var _ears: Array[Node3D] = []
var _eyes: Array[Node3D] = []
var _pupils: Array[Node3D] = []


func _init() -> void:
	_chance.randomize()


## Half-width and height, in units at full scale, that every pose stays inside.
func get_reach() -> Vector2:
	return Vector2(1.05, 1.25)


## How far in front of him, in units at full scale, his bowl should stand so that his nose reaches it.
func get_bowl_distance() -> float:
	return 0.60


## growth: 0 (newborn proportions) to 1 (adult). Growing is not a uniform
## scale: a puppy has a bigger head, a shorter muzzle and legs, and fluffier, paler fur.
func apply_growth(growth: float) -> void:
	if absf(growth - _growth) < 0.01:
		return
	_growth = clampf(growth, 0.0, 1.0)
	_build()


## Start (or switch to) an activity. `seed` differs for every instance, so
## the same activity never plays out identically twice.
func begin(activity: String, variant: int, intensity: float, planned_secs: float, seed: int, already: float = 0.0) -> void:
	_activity = activity
	_variant = variant
	_intensity = intensity
	_planned = planned_secs
	_since = already
	var instance := RandomNumberGenerator.new()
	instance.seed = seed
	_luck = [instance.randf(), instance.randf(), instance.randf(), instance.randf()]


func set_facing(direction: int) -> void:
	_facing = 1 if direction >= 0 else -1


func set_mood(valence: float, arousal: float, sleepiness: float, confusion: float = 0.0) -> void:
	_valence = valence
	_arousal = arousal
	_sleepiness = sleepiness
	_confusion = confusion


## Where the person is, as seen from the dog: `sideways` is -0.5 (far to his
## left on screen) to 0.5. Without a camera this is the middle of the monitor,
## a little above him, well out in front of the glass.
func set_viewer_offset(sideways: float) -> void:
	_viewer = Vector3(sideways * 1.2, 0.45, 1.0).normalized()


## Look at the person in front of the screen for about this long.
func look_at_viewer(secs: float) -> void:
	_eye_contact = maxf(_eye_contact, secs)


## Look in a direction (in scene space: +X right, +Y up, +Z out of the screen)
## for about this long: at the pointer, at a notification, towards a sound.
func look_towards(direction: Vector3, secs: float) -> void:
	_aim = direction.normalized()
	_aim_left = secs


## Advance the animation. `travelled` is how far he actually moved along the
## ground this frame (in units), so the legs keep pace with the window.
func advance(delta: float, travelled: float) -> void:
	_time += delta
	_since += delta
	var progress := clampf(_since / _planned, 0.0, 1.0) if _planned > 0.0 else 0.0
	var layers := _compose(_since, progress)
	var target: Dictionary = layers[0]
	var motion: Dictionary = layers[1]

	# Postures blend into each other; quick motions ride on top unsmoothed.
	var ease := 1.0 - exp(-5.0 * delta)
	for channel in target:
		_pose[channel] = lerpf(_pose[channel], target[channel], ease)
	var p := _pose.duplicate()
	for channel in motion:
		if p.has(channel):   # the rest ("wag", "spin", …) are cues read further down
			p[channel] += motion[channel]

	var awake: float = p["eyes"]
	var pleased := maxf(_valence, 0.0)

	# --- gait: walk and trot move diagonal pairs together; a gallop pairs front with front.
	var speed := absf(travelled) / maxf(delta, 0.001) / WALK_SPEED
	_gait = lerpf(_gait, clampf(speed, 0.0, 3.6), 1.0 - exp(-8.0 * delta))
	var galloping := smoothstep(2.4, 3.0, _gait)
	_phase += absf(travelled) * TAU / (STRIDE * lerpf(1.0, 2.2, galloping)) + delta * motion.get("stepping", 0.0)
	var stepping: float = clampf(_gait + (1.0 if motion.has("stepping") else 0.0), 0.0, 1.0)
	var swing := lerpf(26.0, 44.0, clampf((_gait - 1.0) / 2.0, 0.0, 1.0)) * stepping
	var beat := [0.0, lerpf(PI, 0.4, galloping), PI, lerpf(0.0, PI + 0.4, galloping)]
	var bounce := (0.012 + 0.03 * clampf(_gait - 1.0, 0.0, 1.0)) * stepping * absf(sin(_phase))
	p["pitch"] += 6.0 * galloping * sin(_phase)

	# --- body
	var facing_yaw := YAW_RIGHT if _facing > 0 else YAW_LEFT
	_yaw = lerpf(_yaw, lerpf(facing_yaw, -PI / 2.0, 0.8 * clampf(p["face_viewer"], 0.0, 1.0)), 1.0 - exp(-6.0 * delta))
	_spin += delta * motion.get("spin", 0.0)
	if not motion.has("spin"):
		_spin = lerpf(wrapf(_spin, -PI, PI), 0.0, 1.0 - exp(-7.0 * delta))   # unwind by the short way round
	rotation.y = _yaw + _spin
	_body.position.y = _hip_height * p["hip"] + p["lift"] + bounce
	_body.rotation = Vector3(deg_to_rad(p["roll"]), 0.0, deg_to_rad(p["pitch"]))
	_chest.rotation = Vector3(0.0, -deg_to_rad(p["spine_yaw"]), deg_to_rad(p["spine_pitch"]))

	# --- breathing and panting
	_panting = lerpf(_panting, 1.0 if (_arousal > 0.62 or _gait > 1.5) and awake > 0.5 else 0.0, 1.0 - exp(-1.5 * delta))
	_breath += delta * TAU * lerpf(lerpf(0.3, 0.6, awake), 2.6, _panting)
	var breath := sin(_breath) * lerpf(1.7, 1.0, awake)
	_torso.scale = Vector3(1.0, 1.0 + 0.018 * breath, 1.0 + 0.03 * breath)

	# --- legs
	var names := ["front_l", "front_r", "hind_l", "hind_r"]
	for i in 4:
		var twitch := _dream * 7.0 * sin(_time * 44.0 + i * 1.7)
		_legs[i].rotation.z = deg_to_rad(p[names[i]] + swing * sin(_phase + beat[i]) + twitch)

	# --- attention: the eyes go first, the head follows
	_eye_contact = maxf(_eye_contact - delta, 0.0)
	_aim_left = maxf(_aim_left - delta, 0.0)
	# Not understanding shows as it does in a real dog: eyes on you, head tipped to one side, then the other.
	var puzzled := _confusion * awake
	p["head_tilt"] += 20.0 * puzzled * (1.0 if fmod(_time, 2.6) < 1.3 else -1.0)
	p["ears"] -= 0.6 * puzzled
	var at_you: bool = _eye_contact > 0.0 or motion.get("hold_gaze", 0.0) > 0.5 or puzzled > 0.3
	var gaze := _viewer if at_you or _aim_left <= 0.0 else _aim
	_gaze_target = gaze
	var wants_viewer: bool = (at_you or _aim_left > 0.0) and awake > 0.5
	var held := _follow(_attention, 1.0 if wants_viewer else 0.0, _attention_speed, 0.28 if wants_viewer else 0.5, 8.0, delta)
	_attention = held.x
	_attention_speed = held.y
	_idle_glances(delta, awake, wants_viewer)

	var forward_angle: float = rotation.y - deg_to_rad(p["spine_yaw"])
	var to_viewer: float = clampf(wrapf(atan2(-gaze.z, gaze.x) - forward_angle, -PI, PI), -1.25, 1.25)
	var own_yaw: float = -deg_to_rad(p["head_yaw"] + _wander_yaw)
	# A head has weight: it gathers speed, slows before it arrives, and cannot keep up with
	# something that darts about. A sleepy or calm dog turns more slowly than an excited one.
	var settle: float = lerpf(0.5, 0.3, clampf(_arousal, 0.0, 1.0)) * lerpf(1.0, 1.5, _sleepiness)
	var turned := _follow(_gaze_yaw, to_viewer, _gaze_yaw_speed, settle, HEAD_TURN_SPEED, delta)
	_gaze_yaw = turned.x
	_gaze_yaw_speed = turned.y
	var lifted := _follow(_gaze_lift, gaze.y, _gaze_lift_speed, settle, HEAD_TURN_SPEED, delta)
	_gaze_lift = lifted.x
	_gaze_lift_speed = lifted.y
	_neck.rotation.z = deg_to_rad(p["neck"] + 3.0 * minf(_gait, 1.0) * sin(2.0 * _phase) - 6.0 * _sleepiness * awake + (9.0 + 22.0 * _gaze_lift) * _attention)
	_head.rotation = Vector3(
		lerp_angle(_head.rotation.x, deg_to_rad(p["head_tilt"]), 1.0 - exp(-9.0 * delta)),
		lerpf(own_yaw, _gaze_yaw, _attention),
		deg_to_rad(p["head"] + _wander_pitch * (1.0 - _attention)))

	# --- mouth: yawns and licking come from the pose; panting from exertion and excitement
	var jaw: float = clampf(p["jaw"] + _panting * (0.32 + 0.06 * sin(_breath)), 0.0, 1.0)
	_jaw.rotation.z = -deg_to_rad(30.0 * jaw)
	_tongue.visible = jaw > 0.12
	_tongue.position.x = _tongue.get_meta("rest") + 0.03 * maxf(_panting, motion.get("lick", 0.0)) * (1.0 + 0.3 * sin(_breath))

	# --- tail: height and wag say how he feels
	_wag += delta * TAU * (1.0 + 4.2 * _arousal * pleased + 2.0 * motion.get("wag", 0.0))
	var wag: float = (4.0 + 42.0 * _arousal * pleased + 9.0 * minf(_gait, 1.0) + 30.0 * motion.get("wag", 0.0) + 14.0 * _attention * pleased) * awake
	_tail.rotation.z = deg_to_rad(p["tail"] - 30.0 * _arousal * pleased * awake)
	_tail.rotation.y = deg_to_rad(p["tail_yaw"] + wag * sin(_wag) + _dream * 12.0 * sin(_time * 30.0))

	# --- ears: back when pleased or sleepy, pricked when something has his attention
	_next_flick -= delta
	if _next_flick < 0.0:
		_next_flick = _chance.randf_range(4.0, 15.0)
		_ear_flick[_chance.randi() % 2] = 1.0
	var set_back: float = clampf(p["ears"] + 0.4 * _sleepiness - 0.5 * _attention * (1.0 - pleased) - 0.3 * clampf(_arousal - 0.5, 0.0, 0.5), -1.0, 1.0)
	for i in 2:
		var side := 1.0 if i == 0 else -1.0
		_ear_flick[i] = maxf(_ear_flick[i] - delta * 5.0, 0.0)
		var flap: float = 0.08 * stepping * sin(2.0 * _phase) + motion.get("ear_flap", 0.0) * sin(_time * 50.0 + i)
		_ears[i].rotation = Vector3(side * (0.12 + flap + 0.5 * _ear_flick[i]), 0.0, -0.45 * set_back)

	# --- eyes: lids, blinks, and where the pupils point
	_next_blink -= delta
	if _next_blink < -0.12:
		_next_blink = _chance.randf_range(2.0, 6.0) * lerpf(1.0, 0.5, _sleepiness)
	var lids: float = awake * lerpf(1.0, 0.55, _sleepiness * (1.0 - _attention)) * (0.1 if _next_blink < 0.0 else 1.0)
	var head_basis := _head.global_transform.basis.orthonormalized()
	var look: Vector3 = (head_basis.inverse() * gaze).lerp(Vector3.RIGHT, 1.0 - _attention).normalized()
	for i in 2:
		_eyes[i].scale.y = maxf(lids, 0.08)
		var radius: float = _eyes[i].get_meta("radius")
		_pupils[i].position = _pupils[i].position.lerp(look * radius * 0.62, 1.0 - exp(-14.0 * delta))

	# --- dreaming: now and then a sleeping dog twitches
	if _activity == "sleep":
		_next_dream -= delta
		if _next_dream < 0.0:
			_next_dream = _chance.randf_range(40.0, 120.0)
			_dream = 1.0
	_dream = maxf(_dream - delta * (0.4 if _activity == "sleep" else 4.0), 0.0)


## How much his gaze is held by the viewer or by something on the screen, 0..1.
func attention() -> float:
	return _attention


## Which way that something is, as an angle in degrees around him seen from above:
## 0 is straight out of the screen at the viewer, positive to the screen's right.
func gaze_angle() -> float:
	return rad_to_deg(atan2(_gaze_target.x, _gaze_target.z))


## Where his legs are in a stride, 0..1, and how fast he is going (1 = a walk).
func stride() -> Vector2:
	return Vector2(fposmod(_phase, TAU) / TAU, _gait)


## The posture he is in or settling into: "stand", "sit" or "lie".
func posture() -> String:
	return _posture


## True while something quick is happening and a low frame rate would show.
func is_lively() -> bool:
	return _dream > 0.05 or _attention > 0.05 and _attention < 0.95 or _panting > 0.1 or _wander_left > 0.0


## Moves `from` towards `to` without ever jumping: it speeds up, slows down as it
## arrives in about `settle` seconds, and never goes faster than `top_speed`.
## Returns the new value in x and the new speed in y.
static func _follow(from: float, to: float, speed: float, settle: float, top_speed: float, delta: float) -> Vector2:
	var omega := 2.0 / maxf(settle, 0.001)
	var x := omega * delta
	var decay := 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x)
	var gap := clampf(from - to, -top_speed * settle, top_speed * settle)
	var push := (speed + omega * gap) * delta
	var value := from - gap + (gap + push) * decay
	var new_speed := (speed - omega * push) * decay
	if (to - from > 0.0) == (value > to):   # arrived: do not swing past it
		return Vector2(to, 0.0)
	return Vector2(value, new_speed)


## Unprompted looks around: a dog at rest does not hold his head still.
func _idle_glances(delta: float, awake: float, busy: bool) -> void:
	var calm := _activity in ["stand", "sit", "lie"] and awake > 0.5 and not busy
	_wander_left -= delta
	_next_wander -= delta
	if calm and _next_wander < 0.0:
		_next_wander = _chance.randf_range(5.0, 16.0) * lerpf(1.3, 0.6, _arousal)
		_wander_left = _chance.randf_range(0.8, 2.6)
		_wander_yaw = _chance.randf_range(-40.0, 45.0)
		_wander_pitch = _chance.randf_range(-8.0, 12.0)
	if _wander_left <= 0.0 or not calm:
		var back := 1.0 - exp(-4.0 * delta)
		_wander_yaw = lerpf(_wander_yaw, 0.0, back)
		_wander_pitch = lerpf(_wander_pitch, 0.0, back)


## The pose for the current activity: [joint targets, quick motions to add on top].
## `t` is seconds into the activity and `done` its completed fraction (0 if open-ended).
func _compose(t: float, done: float) -> Array:
	var pose := NEUTRAL.duplicate()
	var motion := {}
	var vigour := _intensity
	var side := 1.0 if _luck[0] < 0.5 else -1.0
	var swell := sin(PI * done)            # rises and falls over the activity
	match _activity:
		"stand":
			_posture = "stand"
			if _variant == 1:
				pose["head_yaw"] = side * lerpf(15.0, 35.0, _luck[1])
			elif _variant == 2:
				pose.merge({"neck": 10.0, "head": 4.0, "ears": -0.7}, true)
			motion["roll"] = 1.5 * sin(t * 0.7 + _luck[2] * 6.0)
		"walk":
			_posture = "stand"
			if _variant == 2:        # nose down, following a scent
				pose.merge({"neck": -50.0, "head": -25.0, "tail": 20.0}, true)
				motion["head_yaw"] = 8.0 * sin(t * 1.9 + _luck[1] * 6.0)
			elif _variant == 1:      # trot
				pose.merge({"neck": 4.0, "head": 2.0, "tail": -8.0, "ears": 0.3}, true)
			else:
				pose.merge({"neck": -6.0, "head": 4.0, "tail": 8.0}, true)
		"zoomies":
			_posture = "stand"
			pose.merge({"neck": -14.0, "head": 10.0, "tail": -4.0, "ears": 1.0, "jaw": 0.3}, true)
		"sit":
			_posture = "sit"
			pose.merge(SIT, true)
			if _variant == 1:        # relaxed, leaning on one hip
				pose.merge({"roll": side * 7.0, "hip": 0.43, "head_yaw": 20.0 * _luck[1]}, true)
		"lie":
			_posture = "lie"
			if _variant == 1:
				pose.merge(ON_SIDE, true)
			else:
				pose.merge(SPHINX, true)
				if _variant == 2:    # chin on paws
					pose.merge({"neck": -30.0, "head": -12.0}, true)
		"sleep":
			_posture = "lie"
			if _variant == 0:
				pose.merge(ON_SIDE, true)
			elif _variant == 1:      # curled nose to tail
				pose.merge(SPHINX, true)
				pose.merge({"spine_yaw": 50.0, "head_yaw": 55.0, "neck": -34.0, "head": -10.0, "tail_yaw": 70.0}, true)
			else:
				pose.merge(SPHINX, true)
				pose.merge({"neck": -30.0, "head": -12.0}, true)
			pose["eyes"] = 0.0
		"stretch":
			_posture = "stand"
			var reach := smoothstep(0.0, 0.25, done) * (1.0 - smoothstep(0.8, 1.0, done))
			if _variant == 0:        # front down, rump in the air
				pose.merge({"pitch": -24.0 * reach, "front_l": 100.0 * reach, "front_r": 100.0 * reach, "hind_l": 24.0 * reach, "hind_r": 24.0 * reach, "neck": 10.0 * reach, "head": 12.0 * reach, "tail": 35.0 - 60.0 * reach}, true)
			else:                    # hind legs trailing out behind
				pose.merge({"pitch": 8.0 * reach, "hind_l": -38.0 * reach, "hind_r": -30.0 * reach, "hip": 1.0 - 0.1 * reach, "neck": 18.0 * reach, "head": 10.0 * reach}, true)
			motion["roll"] = 0.8 * reach * sin(t * 23.0)   # the tremble of a good stretch
		"yawn":
			pose.merge(_resting_pose(), true)
			var gape := pow(sin(PI * clampf(done * 1.1, 0.0, 1.0)), 0.7)
			pose.merge({"jaw": gape, "head": pose["head"] + 16.0 * gape, "ears": 0.8 * gape, "eyes": 1.0 - 0.9 * gape}, true)
			motion["lick"] = gape
		"scratch":
			_posture = "sit"
			pose.merge(SIT, true)
			var leg := "hind_l" if _variant == 0 else "hind_r"
			pose.merge({leg: 150.0, "roll": (9.0 if _variant == 0 else -9.0), "neck": -15.0, "head_tilt": (24.0 if _variant == 0 else -24.0), "eyes": 0.5, "jaw": 0.12}, true)
			motion[leg] = 15.0 * sin(t * lerpf(30.0, 40.0, vigour))
			motion["head"] = 2.0 * sin(t * 36.0)
		"groom":
			if _variant == 0:        # licking a front paw
				_posture = "lie"
				pose.merge(SPHINX, true)
				pose.merge({"neck": -38.0, "head": -30.0, "head_yaw": side * 12.0, "jaw": 0.22}, true)
			else:                    # turned back to lick his flank
				_posture = "sit"
				pose.merge(SIT, true)
				pose.merge({"spine_yaw": 32.0, "head_yaw": 70.0, "neck": -22.0, "jaw": 0.22}, true)
			motion["head"] = 6.0 * sin(t * 6.5)
			motion["lick"] = 0.5 + 0.5 * sin(t * 6.5)
		"shake":
			_posture = "stand"
			var burst := sin(PI * done) * lerpf(0.7, 1.0, vigour)
			motion.merge({"roll": 16.0 * burst * sin(t * 52.0), "head_tilt": 24.0 * burst * sin(t * 52.0 + 1.0), "ear_flap": 0.5 * burst, "wag": burst}, true)
			pose["eyes"] = 0.3
		"sniff":
			_posture = "stand"
			if _variant == 0:        # nose to the ground
				pose.merge({"neck": -55.0, "head": -28.0, "tail": 15.0}, true)
				motion["head_yaw"] = 10.0 * sin(t * 1.3 + _luck[1] * 6.0) + 4.0 * sin(t * 3.1)
			else:                    # testing the air
				pose.merge({"neck": 15.0, "head": 24.0, "ears": -0.5}, true)
			motion["head"] = 1.5 * sin(t * 19.0)
		"tail_chase":
			_posture = "stand"
			var turn := 1.0 if _variant == 0 else -1.0
			pose.merge({"spine_yaw": 34.0 * turn, "head_yaw": 45.0 * turn, "neck": -8.0, "tail_yaw": -50.0 * turn, "jaw": 0.25}, true)
			motion["spin"] = -turn * lerpf(4.5, 7.5, vigour) * (0.4 + 0.6 * swell)
			motion["stepping"] = 16.0
		"play_bow":
			_posture = "stand"
			pose.merge({"pitch": -26.0, "front_l": 104.0, "front_r": 104.0, "hind_l": 26.0, "hind_r": 26.0, "neck": 14.0, "head": 16.0, "tail": -10.0, "jaw": 0.3, "face_viewer": 0.6}, true)
			motion.merge({"pitch": 4.0 * sin(t * 9.0), "wag": 1.0, "hold_gaze": 1.0, "head_tilt": 10.0 * sin(t * 1.7)}, true)
		"roll":
			_posture = "lie"
			pose.merge(ON_SIDE, true)
			var over := smoothstep(0.0, 0.2, done) * (1.0 - smoothstep(0.85, 1.0, done))
			pose.merge({"roll": -80.0 - 95.0 * over, "hip": 0.42 + 0.3 * over, "neck": 4.0 - 70.0 * over, "head": 12.0 - 30.0 * over, "jaw": 0.3 * over, "front_l": 45.0, "front_r": 45.0, "hind_l": 50.0, "hind_r": 50.0}, true)
			var wriggle := over * (0.6 + 0.4 * vigour)
			motion.merge({"roll": 20.0 * wriggle * sin(t * (5.0 if _variant == 0 else 7.0)), "front_l": 22.0 * wriggle * sin(t * 6.0), "front_r": 22.0 * wriggle * sin(t * 6.0 + 2.0),
				"hind_l": 20.0 * wriggle * sin(t * 5.0 + 1.0), "hind_r": 20.0 * wriggle * sin(t * 5.0 + 3.0), "spine_yaw": 10.0 * wriggle * sin(t * 4.0)}, true)
		"greet":
			_posture = "stand"
			pose.merge({"face_viewer": 0.8, "ears": 0.8, "jaw": 0.3, "neck": 6.0}, true)
			var hop := sin(t * lerpf(7.0, 9.5, vigour))
			motion.merge({"spine_yaw": 12.0 * hop, "roll": 5.0 * sin(t * 9.0 + 1.0), "front_l": 16.0 * maxf(hop, 0.0), "front_r": 16.0 * maxf(-hop, 0.0),
				"lift": 0.03 * absf(hop), "wag": 1.0, "hold_gaze": (1.0 if fmod(t, 3.2) < 2.5 else 0.0)}, true)
			if _variant == 1:        # too pleased to stand still: one full turn, then a bow
				if done < 0.35:
					motion["spin"] = side * TAU / maxf(0.35 * _planned, 0.5)
					motion["stepping"] = 14.0
				elif done > 0.6:
					pose.merge({"pitch": -20.0, "front_l": 96.0, "front_r": 96.0, "hind_l": 20.0, "hind_r": 20.0}, true)
		"invite":
			# "I noticed what you're doing — let me help!": on his feet, facing you, bouncing, head tipping.
			_posture = "stand"
			pose.merge({"face_viewer": 0.9, "ears": -0.6, "jaw": 0.25, "neck": 8.0}, true)
			var bounce := absf(sin(t * lerpf(5.0, 7.0, vigour)))
			motion.merge({"lift": 0.05 * bounce, "front_l": 12.0 * bounce, "front_r": 12.0 * bounce, "wag": 1.0, "hold_gaze": 1.0, "head_tilt": 14.0 * sin(t * 2.1)}, true)
			if _variant == 1:        # dipping into a play bow between bounces
				var dip := maxf(sin(t * 2.4), 0.0)
				pose.merge({"pitch": -18.0 * dip, "front_l": 70.0 * dip, "front_r": 70.0 * dip, "hind_l": 18.0 * dip, "hind_r": 18.0 * dip}, true)
			elif _variant == 2 and done < 0.3:   # one quick turn on the spot first
				motion["spin"] = side * TAU / maxf(0.3 * _planned, 0.5)
				motion["stepping"] = 14.0
		"seek_attention":
			_posture = "sit"
			pose.merge(SIT, true)
			pose.merge({"face_viewer": 1.0, "ears": -0.5, "head_tilt": (16.0 if fmod(t + _luck[1] * 3.0, 3.4) < 1.7 else -14.0)}, true)
			motion.merge({"hold_gaze": (1.0 if fmod(t, 4.0) < 3.2 else 0.0), "wag": 0.35}, true)
			if _variant == 1:        # a paw offered
				pose["front_l"] = 30.0
				motion["front_l"] = 8.0 * sin(t * 3.0)
		"eat", "drink":
			_posture = "stand"
			pose.merge({"neck": -52.0, "head": -26.0, "tail": 20.0}, true)
			var rate := 14.0 if _activity == "eat" else 20.0
			motion.merge({"head": (5.0 if _activity == "eat" else 2.5) * sin(t * rate), "jaw": 0.18 + 0.16 * sin(t * rate), "lick": (0.0 if _activity == "eat" else 0.5 + 0.5 * sin(t * rate))}, true)
	return [pose, motion]


func _resting_pose() -> Dictionary:
	match _posture:
		"sit":
			return SIT
		"lie":
			return SPHINX
	return {}


func _build() -> void:
	for child in get_children():
		remove_child(child)
		child.free()
	_legs.clear()
	_ears.clear()
	_eyes.clear()
	_pupils.clear()

	var g := _growth
	var leg := lerpf(0.72, 1.0, g)
	var trunk := lerpf(0.86, 1.0, g)
	var fluff := lerpf(1.35, 1.0, g)
	var muzzle := lerpf(0.68, 1.0, g)
	# Puppies are paler; the coat deepens to gold with age.
	var root := Color(0.74, 0.58, 0.36).lerp(Color(0.62, 0.40, 0.16), g)
	var tip := Color(0.97, 0.87, 0.66).lerp(Color(0.93, 0.74, 0.42), g)
	_hip_height = 0.44 * leg

	# Hips carry the rear half; the chest pivots on the spine so he can bend, curl and look back.
	_body = _pivot(self, Vector3(-0.30 * trunk, _hip_height, 0.0))
	_torso = _furred(_body, Vector3(0.26 * trunk, 0.11, 0.0), Vector3(0.38 * trunk, 0.20, 0.17), 0.03 * fluff, root, tip)
	_tail = _pivot(_body, Vector3(-0.10 * trunk, 0.20, 0.0))
	_furred(_tail, Vector3(-0.19, 0.0, 0.0), Vector3(0.20, 0.045, 0.04), 0.055 * fluff, root, tip, Vector3(-1.0, -0.5, 0.0))

	_chest = _pivot(_body, Vector3(0.42 * trunk, 0.12, 0.0))
	_furred(_chest, Vector3(0.13 * trunk, 0.01, 0.0), Vector3(0.22, 0.23, 0.19), 0.034 * fluff, root, tip)
	_neck = _pivot(_chest, Vector3(0.24 * trunk, 0.10, 0.0))
	_furred(_neck, Vector3(0.09, 0.06, 0.0), Vector3(0.13, 0.18, 0.12), 0.04 * fluff, root, tip)

	_head = _pivot(_neck, Vector3(0.17, 0.20, 0.0))
	_head.scale = Vector3.ONE * lerpf(1.32, 1.0, g)
	_furred(_head, Vector3(0.02, 0.02, 0.0), Vector3(0.15, 0.13, 0.12), 0.022 * fluff, root, tip)
	var pale_root := root.lightened(0.12)
	var pale_tip := tip.lightened(0.12)
	_furred(_head, Vector3(0.06 + 0.11 * muzzle, -0.02, 0.0), Vector3(0.11 * muzzle, 0.045, 0.06), 0.008, pale_root, pale_tip)
	_smooth(_head, Vector3(0.06 + 0.215 * muzzle, -0.01, 0.0), 0.026, Color(0.05, 0.04, 0.035))
	_jaw = _pivot(_head, Vector3(0.07, -0.055, 0.0))
	_furred(_jaw, Vector3(0.085 * muzzle, -0.012, 0.0), Vector3(0.085 * muzzle, 0.022, 0.045), 0.006, pale_root, pale_tip)
	_tongue = _smooth(_jaw, Vector3(0.10 * muzzle, 0.008, 0.0), 0.03, Color(0.86, 0.42, 0.46))
	_tongue.scale = Vector3(2.0 * muzzle, 0.3, 0.9)
	_tongue.set_meta("rest", 0.10 * muzzle)

	for side in [1.0, -1.0]:
		var eye := _pivot(_head, Vector3(0.115, 0.045, 0.078 * side))
		var radius := 0.021
		_smooth(eye, Vector3.ZERO, radius, Color(0.24, 0.13, 0.05))            # iris
		_pupils.append(_smooth(eye, Vector3.RIGHT * radius * 0.62, radius * 0.62, Color(0.01, 0.01, 0.01)))
		var glint := _smooth(eye, Vector3(0.3, 0.6, 0.45 * side).normalized() * radius * 0.9, radius * 0.2, Color.WHITE)
		(glint.material_override as StandardMaterial3D).shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		eye.set_meta("radius", radius)
		_eyes.append(eye)
		var ear := _pivot(_head, Vector3(-0.03, 0.05, 0.11 * side))
		_furred(ear, Vector3(0.0, -0.09, 0.0), Vector3(0.05, 0.11, 0.03), 0.03 * fluff, root.darkened(0.12), tip.darkened(0.12))
		_ears.append(ear)

	for front in [true, false]:
		for side in [1.0, -1.0]:
			var joint := _pivot(_chest, Vector3(0.18 * trunk, -0.12, 0.10 * side)) if front else _pivot(_body, Vector3(0.0, 0.0, 0.10 * side))
			var half := (_hip_height - 0.02) * 0.5
			_furred(joint, Vector3(0.0, -half, 0.0), Vector3(0.05, half, 0.055), 0.014 * fluff, root, tip, Vector3.DOWN)
			_legs.append(joint)


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


func _smooth(parent: Node3D, centre: Vector3, radius: float, colour: Color) -> Node3D:
	var mesh := SphereMesh.new()
	mesh.radius = radius
	mesh.height = radius * 2.0
	mesh.radial_segments = 16
	mesh.rings = 8
	var material := StandardMaterial3D.new()
	material.albedo_color = colour
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
