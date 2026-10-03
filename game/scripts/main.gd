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
	rig.map_size = game_view.map_size()
	if cfg.has("cam"):
		rig.focus(cfg["cam"], true)
	else:
		var h: Vector3 = game_view.home_position()
		rig.focus(h + Vector3(0, 0, 6), true)
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
	if shot_path != "":
		_frame += 1
		if _frame == shot_frames:
			await RenderingServer.frame_post_draw
			var img := get_viewport().get_texture().get_image()
			img.save_png(shot_path)
			print("saved ", shot_path)
			get_tree().quit()

func _setup_environment() -> void:
	var env := Environment.new()
	var sky := Sky.new()
	var mat := PanoramaSkyMaterial.new()
	mat.panorama = load("res://assets/textures/src/sky.hdr")
	mat.energy_multiplier = 1.0
	sky.sky_material = mat
	sky.radiance_size = Sky.RADIANCE_SIZE_256
	env.background_mode = Environment.BG_SKY
	env.sky = sky
	env.ambient_light_source = Environment.AMBIENT_SOURCE_SKY
	env.ambient_light_sky_contribution = 1.0
	env.ambient_light_energy = 0.55
	env.reflected_light_source = Environment.REFLECTION_SOURCE_SKY
	env.tonemap_mode = Environment.TONE_MAPPER_AGX
	env.tonemap_exposure = 0.92
	env.tonemap_white = 6.0
	env.ssao_enabled = true
	env.ssao_radius = 1.6
	env.ssao_intensity = 2.2
	env.ssao_power = 1.4
	env.ssao_detail = 0.6
	env.ssil_enabled = true
	env.ssil_radius = 6.0
	env.ssil_intensity = 0.8
	env.glow_enabled = true
	env.glow_intensity = 0.45
	env.glow_strength = 0.9
	env.glow_bloom = 0.05
	env.glow_hdr_threshold = 1.1
	env.glow_blend_mode = Environment.GLOW_BLEND_MODE_SOFTLIGHT
	env.fog_enabled = true
	env.fog_mode = Environment.FOG_MODE_DEPTH
	env.fog_light_color = Color(0.68, 0.78, 0.9)
	env.fog_light_energy = 1.0
	env.fog_sun_scatter = 0.25
	env.fog_depth_begin = 260.0
	env.fog_depth_end = 1400.0
	env.fog_depth_curve = 1.6
	env.fog_aerial_perspective = 0.55
	env.fog_sky_affect = 0.0
	env.adjustment_enabled = true
	env.adjustment_saturation = 1.18
	env.adjustment_contrast = 1.08
	$WorldEnvironment.environment = env
	var sun: DirectionalLight3D = $Sun
	sun.light_color = Color(1.0, 0.95, 0.86)
	sun.light_energy = 1.7
	sun.shadow_enabled = true
	sun.shadow_blur = 1.2
	sun.directional_shadow_mode = DirectionalLight3D.SHADOW_PARALLEL_4_SPLITS
	sun.directional_shadow_max_distance = 420.0
	sun.directional_shadow_split_1 = 0.06
	sun.directional_shadow_split_2 = 0.18
	sun.directional_shadow_split_3 = 0.45
	sun.rotation_degrees = Vector3(-48, -35, 0)
