extends Node
## Buddy's voice: recorded dog sounds, never speech.
##
## Each kind of sound ("bark", "whine", "yawn", …) is a set of recordings named
## `<kind>_<anything>.ogg` or `.wav`. They are looked for in the project's
## `sounds/` folder and in `~/.local/share/buddy/sounds/`, so more can be added
## without touching the program. A kind with no recordings is simply silent.
## Every playback differs a little in pitch and loudness, and the same
## recording is not used twice running.

const LOUDEST_DB := -9.0
const SOFTEST_DB := -24.0

var muted := false
var _recordings := {}      # kind -> Array[AudioStream]
var _last := {}            # kind -> index played last
var _players: Array[AudioStreamPlayer] = []
var _chance := RandomNumberGenerator.new()


func _ready() -> void:
	_chance.randomize()
	for i in 3:
		var player := AudioStreamPlayer.new()
		add_child(player)
		_players.append(player)
	var data_home := OS.get_environment("XDG_DATA_HOME")
	if data_home == "":
		data_home = OS.get_environment("HOME").path_join(".local/share")
	for folder in [ProjectSettings.globalize_path("res://sounds"), data_home.path_join("buddy/sounds")]:
		_load_folder(folder)


func sound_count() -> int:
	var total := 0
	for kind in _recordings:
		total += _recordings[kind].size()
	return total


func _load_folder(folder: String) -> void:
	var dir := DirAccess.open(folder)
	if dir == null:
		return
	for file in dir.get_files():
		var stream: AudioStream = null
		match file.get_extension().to_lower():
			"ogg":
				stream = AudioStreamOggVorbis.load_from_file(folder.path_join(file))
			"wav":
				stream = AudioStreamWAV.load_from_file(folder.path_join(file))
		if stream != null:
			var kind := file.get_basename().rsplit("_", true, 1)[0]
			if not _recordings.has(kind):
				_recordings[kind] = []
			_recordings[kind].append(stream)


## Make a sound. `intensity` is 0..1; `growth` is 0 (puppy) to 1 (adult) and
## raises the pitch for a young dog.
func say(kind: String, intensity: float, growth: float) -> void:
	if muted or not _recordings.has(kind):
		return
	var takes: Array = _recordings[kind]
	var pick := _chance.randi() % takes.size()
	if takes.size() > 1 and pick == _last.get(kind, -1):
		pick = (pick + 1) % takes.size()
	_last[kind] = pick
	for player in _players:
		if not player.playing:
			player.stream = takes[pick]
			player.pitch_scale = _chance.randf_range(0.93, 1.08) * lerpf(1.35, 1.0, growth)
			player.volume_db = lerpf(SOFTEST_DB, LOUDEST_DB, clampf(intensity, 0.0, 1.0)) + _chance.randf_range(-1.5, 1.5)
			player.play()
			return
