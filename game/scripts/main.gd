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
