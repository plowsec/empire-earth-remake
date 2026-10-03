extends Node3D
## Root of the game scene: environment, lighting, match startup, screenshot harness.

@onready var game_view = $GameView
@onready var rig = $CameraRig
@onready var hud = $HUD
@onready var vfx = $VFX

var shot_path := ""
var shot_frames := 0
var _frame := 0
var start_cfg := {}
var follow_action := false
var warp_ticks := 0
var stress := 0
var select_key := ""
var feature_scene := ""
var _perf_t := 0.0
var _perf_frames := 0
var _perf_sim := 0.0
var _follow_t := 0.0

func _ready() -> void:
	_setup_environment()
	for a in OS.get_cmdline_user_args():
		if a.begins_with("--shot="): shot_path = a.substr(7)
		elif a.begins_with("--shot-frames="): shot_frames = int(a.substr(14))
		elif a.begins_with("--seed="): start_cfg["seed"] = int(a.substr(7))
		elif a.begins_with("--reveal"): start_cfg["reveal"] = 1
		elif a.begins_with("--players="): start_cfg["players"] = int(a.substr(10))
		elif a.begins_with("--speed="): start_cfg["speed"] = float(a.substr(8))
		elif a.begins_with("--ai0"): start_cfg["ai_self"] = 1
		elif a.begins_with("--stress="): stress = int(a.substr(9))
		elif a.begins_with("--select="): select_key = a.substr(9)
		elif a.begins_with("--feature="): feature_scene = a.substr(10)
		elif a == "--featuretest":
			var t = load("res://scripts/featuretest.gd").new()
			t.name = "FeatureTest"
			call_deferred("add_child", t)
		elif a == "--autotest":
			var t = load("res://scripts/autotest.gd").new()
			t.name = "AutoTest"
			call_deferred("add_child", t)
		elif a.begins_with("--dist="): start_cfg["dist"] = float(a.substr(7))
		elif a.begins_with("--yaw="): start_cfg["yaw"] = float(a.substr(6))
		elif a.begins_with("--offset="):
			var q = a.substr(9).split(",")
			start_cfg["offset"] = Vector3(float(q[0]), 0, float(q[1]))
		elif a.begins_with("--follow"): follow_action = true
		elif a.begins_with("--warp="): warp_ticks = int(a.substr(7))
		elif a.begins_with("--cam="):
			var p = a.substr(6).split(",")
			start_cfg["cam"] = Vector3(float(p[0]), 0, float(p[1]))
			if p.size() > 2: start_cfg["dist"] = float(p[2])
			if p.size() > 3: start_cfg["yaw"] = float(p[3])
	if Engine.has_meta("match_config"):
		start_cfg.merge(Engine.get_meta("match_config"), true)
	start_match(start_cfg)

func start_match(cfg: Dictionary) -> void:
	game_view.set_camera(rig.cam)
	var c := {"seed": 7, "players": 2, "difficulty": 1, "map_size": 1, "resources": 100, "pop_limit": 300}
	c.merge(cfg, true)
	game_view.start_game(c)
	rig.game_view = game_view
	rig.edge_pan = Settings.edge_pan
	rig.map_size = game_view.map_size()
	if cfg.has("cam"):
		rig.focus(cfg["cam"], true)
	else:
		var h: Vector3 = game_view.home_position()
		rig.focus(h + Vector3(0, 0, 6) + cfg.get("offset", Vector3.ZERO), true)
	if cfg.has("dist"):
		rig.dist = cfg["dist"]; rig._dist_goal = cfg["dist"]
	if cfg.has("yaw"):
		rig.yaw = cfg["yaw"]; rig._yaw_goal = cfg["yaw"]
	if cfg.has("speed"):
		game_view.set_speed(cfg["speed"])
	if cfg.get("reveal", 0) == 1:
		game_view.set_reveal(true)
	hud.bind(game_view, rig, self)
	vfx.bind(game_view, rig)

func _process(dt: float) -> void:
	if feature_scene != "" and game_view.is_running():
		rig.focus(game_view.debug_feature_scene(feature_scene), true)
		feature_scene = ""
	if select_key != "" and game_view.is_running():
		var parts := select_key.split(":")
		game_view.select_all_of(parts[0], int(parts[1]) if parts.size() > 1 else 999)
		select_key = ""
	if stress > 0 and game_view.is_running():
		var c: Vector3 = game_view.debug_spawn_battle(stress)
		rig.focus(c, true)
		var dd: float = start_cfg.get("dist", 110.0)
		rig.dist = dd
		rig._dist_goal = dd
		stress = -1
	if stress == -1:
		_perf_t += dt
		_perf_frames += 1
		_perf_sim += game_view.sim_stats()["sim_ms"]
		if _perf_t >= 3.0:
			var st: Dictionary = game_view.sim_stats()
			print("PERF fps %.1f sim %.2f ms/frame units %d projectiles %d" % [_perf_frames / _perf_t, _perf_sim / _perf_frames, st["units"], st["projectiles"]])
			_perf_t = 0.0
			_perf_frames = 0
			_perf_sim = 0.0
	if warp_ticks > 0 and game_view.is_running():
		game_view.warp(warp_ticks)
		warp_ticks = 0
	if follow_action:
		_follow_t -= dt
		if _follow_t <= 0.0:
			_follow_t = 2.0
			var h: Vector3 = game_view.hotspot()
			if h.y > -0.5:
				rig.focus(h)

func _setup_environment() -> void:
	EnvSetup.apply($WorldEnvironment, $Sun)
