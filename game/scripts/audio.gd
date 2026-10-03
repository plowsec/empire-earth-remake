extends Node
## Game audio: pooled positional SFX with rate limiting, ambience, UI cues, music.

const SFX := "res://assets/audio/sfx/"
const MUSIC := "res://assets/audio/music/"
const PLAYLIST := ["game_prelude_and_action", "game_five_armies", "game_rynos_theme", "game_volatile_reaction", "battle_clash_defiant"]

var voices: Array[AudioStreamPlayer3D] = []
var voice_i := 0
var ui_player: AudioStreamPlayer
var music_a: AudioStreamPlayer
var music_b: AudioStreamPlayer
var ambient: AudioStreamPlayer
var cache := {}
var budget := {}
var track_i := 0
var rig: Node = null
var _last_alert := -1

func _ready() -> void:
	for i in 32:
		var p := AudioStreamPlayer3D.new()
		p.bus = "SFX"
		p.unit_size = 18.0
		p.max_distance = 420.0
		p.attenuation_model = AudioStreamPlayer3D.ATTENUATION_INVERSE_DISTANCE
		p.panning_strength = 0.8
		add_child(p)
		voices.append(p)
	ui_player = AudioStreamPlayer.new()
	ui_player.bus = "SFX"
	add_child(ui_player)
	music_a = AudioStreamPlayer.new()
	music_b = AudioStreamPlayer.new()
	for m in [music_a, music_b]:
		m.bus = "Music"
		add_child(m)
		m.finished.connect(_next_track)
	ambient = AudioStreamPlayer.new()
	ambient.bus = "SFX"
	ambient.volume_db = -14.0
	add_child(ambient)
	var ocean := _load("ambient_ocean")
	if ocean is AudioStreamWAV:
		ocean.loop_mode = AudioStreamWAV.LOOP_FORWARD
		ocean.loop_end = ocean.data.size() / 2
	ambient.stream = ocean
	ambient.play()
	track_i = randi() % PLAYLIST.size()
	_play_music(PLAYLIST[track_i])
	var p := get_parent()
	if p and p.has_node("CameraRig"):
		rig = p.get_node("CameraRig")

func _load(name: String) -> AudioStream:
	if cache.has(name):
		return cache[name]
	var path := SFX + name + ".wav"
	var s: AudioStream = load(path) if ResourceLoader.exists(path) else null
	cache[name] = s
	return s

func _pick(base: String, n: int) -> AudioStream:
	return _load("%s_%d" % [base, randi() % n])

func _allow(cat: String, limit: int) -> bool:
	var now := Time.get_ticks_msec()
	var b: Array = budget.get(cat, [])
	b = b.filter(func(x): return now - x < 120)
	if b.size() >= limit:
		budget[cat] = b
		return false
	b.append(now)
	budget[cat] = b
	return true

func _play3d(stream: AudioStream, pos: Vector3, vol := 0.0, pitch_var := 0.08) -> void:
	if stream == null:
		return
	if rig:
		var d := Vector2(pos.x, pos.z).distance_to(Vector2(rig.target.x, rig.target.z))
		if d > rig.dist * 2.5 + 150.0:
			return
	var v := voices[voice_i]
	voice_i = (voice_i + 1) % voices.size()
	v.stream = stream
	v.global_position = pos
	v.volume_db = vol
	v.pitch_scale = 1.0 + randf_range(-pitch_var, pitch_var)
	v.play()

func nuclear_blast(pos: Vector3) -> void:
	# A dedicated voice preserves the long tail when combat/UI sounds arrive.
	if get_tree().get_nodes_in_group("nuclear_audio").size() >= 3:
		return
	var voice := AudioStreamPlayer3D.new()
	voice.add_to_group("nuclear_audio")
	voice.bus = "SFX"
	voice.stream = _load("nuclear_blast")
	voice.unit_size = 180.0
	voice.max_distance = 2200.0
	voice.volume_db = -3.0
	voice.panning_strength = 0.45
	add_child(voice)
	voice.global_position = pos
	voice.finished.connect(voice.queue_free)
	voice.volume_db = 6.0
	# the body of the blast is felt everywhere, not just near the camera
	var body := AudioStreamPlayer.new()
	body.bus = "SFX"
	body.stream = voice.stream
	body.volume_db = -1.0
	add_child(body)
	body.finished.connect(body.queue_free)
	var distance: float = pos.distance_to(rig.cam.global_position) if rig else 0.0
	await get_tree().create_timer(clampf(distance / 343.0, 0.08, 1.8)).timeout
	if is_instance_valid(voice):
		voice.play()
	body.play()
	# duck the music under the blast, then bring it back
	var mb := AudioServer.get_bus_index("Music")
	if mb >= 0:
		var base := AudioServer.get_bus_volume_db(mb)
		var tw := create_tween()
		tw.tween_method(func(v): AudioServer.set_bus_volume_db(mb, v), base, base - 24.0, 0.3)
		tw.tween_interval(14.0)
		tw.tween_method(func(v): AudioServer.set_bus_volume_db(mb, v), base - 24.0, base, 6.0)


func ui(name: String, vol := -6.0) -> void:
	var s := _load(name)
	if s:
		ui_player.stream = s
		ui_player.volume_db = vol
		ui_player.play()

# --- called by VFX/HUD ------------------------------------------------------

func play_shot(dmg: int, pos: Vector3, unit := "") -> void:
	var cat := "shot"
	var s: AudioStream
	var vol := -4.0
	match unit:
		"machine_gunner", "recon", "guard_tower", "capitol":
			s = _pick("mg", 3); cat = "mg"
		"sniper":
			s = _pick("sniper", 2)
		"tank", "at_gun":
			s = _pick("cannon", 3); cat = "cannon"; vol = 0.0
		"battleship":
			s = _pick("naval_gun", 2); cat = "naval"; vol = 6.0
		"howitzer":
			s = _pick("artillery", 2); cat = "cannon"; vol = 2.0
		"mortar":
			s = _pick("mortar", 2); cat = "cannon"
		"aa_vehicle", "frigate":
			s = _pick("flak", 2); cat = "flak"
		"submarine":
			s = _load("torpedo")
			if _allow("sonar", 1):
				_play3d(_load("sonar_ping"), pos, -6.0, 0.02)
		"fighter":
			s = _pick("mg", 3); cat = "mg"; vol = 0.0
		"bazooka", "stinger", "helicopter", "strike_fighter", "aa_site", "abm_site":
			s = _pick("missile", 3); cat = "missile"; vol = -2.0
		"bomber", "nuke_bomber":
			s = _pick("bomb_whistle", 2); cat = "whistle"; vol = 2.0
		_:
			s = _pick("rifle", 4)
	if _allow(cat, 4):
		_play3d(s, pos, vol)

func play_explosion(size: float, pos: Vector3) -> void:
	if not _allow("boom", 5):
		return
	var s: AudioStream
	if size > 2.2:
		s = _pick("explosion_big", 2)
	elif size > 1.1:
		s = _pick("explosion_med", 3)
	else:
		s = _pick("explosion_small", 3)
	_play3d(s, pos, 2.0 + size)

func play_death(pos: Vector3) -> void:
	# Empire Earth style: every fallen soldier cries out (rate limited in big battles)
	if _allow("death", 3):
		_play3d(_pick("death", 8), pos, -3.0, 0.1)

func play_engine(kind: String, pos: Vector3) -> void:
	match kind:
		"jet":
			if _allow("jet", 2): _play3d(_pick("jet", 2), pos, 2.0, 0.12)
		"heli":
			if _allow("heli", 1): _play3d(_load("heli_loop"), pos, -2.0, 0.05)
		"ship":
			if _allow("ship", 1): _play3d(_pick("ship_engine", 2), pos, -4.0, 0.1)

func play_big(name: String, pos: Vector3, vol := 4.0, unit := 120.0) -> void:
	# long, loud one-shots (launches, interceptions) on their own voice
	var voice := AudioStreamPlayer3D.new()
	voice.bus = "SFX"
	voice.stream = _load(name)
	voice.unit_size = unit
	voice.max_distance = 2000.0
	voice.volume_db = vol
	add_child(voice)
	voice.global_position = pos
	voice.finished.connect(voice.queue_free)
	voice.play()

func play_splash(pos: Vector3) -> void:
	if _allow("splash", 3):
		_play3d(_pick("splash", 3), pos, -2.0)

func play_hit(pos: Vector3) -> void:
	if _allow("hit", 3):
		_play3d(_pick("bullet_hit", 4), pos, -10.0, 0.2)

func on_event(e: Dictionary) -> void:
	match e["kind"]:
		"complete": ui("complete", -4.0)
		"research": ui("research", -4.0)
		"under_attack":
			var k := randi() % 4
			if k == _last_alert:
				k = (k + 1) % 4
			_last_alert = k
			ui("alert_%d" % k, -8.0)
		"notice": ui("notify", -10.0)
		"nuke_alarm": ui("air_raid_siren", -2.0)
		"trained": ui("ui_click", -14.0)
		"placed": _play3d(_pick("hammer", 3), e["pos"], -4.0)
		"game_over":
			music_a.stop(); music_b.stop()
			ui("victory" if e["mine"] else "defeat", 0.0)
		"ack": ui("ui_click", -12.0)

# --- music --------------------------------------------------------------------

func _play_music(name: String) -> void:
	var path := MUSIC + name + ".mp3"
	if not ResourceLoader.exists(path):
		return
	var s: AudioStream = load(path)
	var cur := music_a if music_a.playing else music_b
	var nxt := music_b if cur == music_a else music_a
	nxt.stream = s
	nxt.volume_db = -40.0
	nxt.play()
	var tw := create_tween()
	tw.set_parallel(true)
	tw.tween_property(nxt, "volume_db", -6.0, 3.0)
	if cur.playing:
		tw.tween_property(cur, "volume_db", -40.0, 3.0)
		tw.chain().tween_callback(cur.stop)

func _next_track() -> void:
	track_i = (track_i + 1) % PLAYLIST.size()
	_play_music(PLAYLIST[track_i])
