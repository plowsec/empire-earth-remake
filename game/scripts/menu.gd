extends Node3D
## Main menu: live AI-vs-AI battle in the background, title, skirmish setup.

const ACCENT := Color(0.86, 0.68, 0.36)

@onready var game_view = $GameView
@onready var rig = $CameraRig
@onready var ui: CanvasLayer = $UI

var font_title: Font
var font_body: Font
var font_bold: Font
var main_panel: Control
var skirmish_panel: Control
var settings_panel: Control
var credits_panel: Control
var orbit := 0.0
var _hot_t := 0.0
var _focus := Vector3.ZERO
var opts := {"players": 2, "difficulty": 3, "map_size": 2, "resources": 150, "start_res": 10000, "pop_limit": 3000, "seed": 0, "reveal": 1}

func _ready() -> void:
	preload("res://scripts/env_setup.gd").apply($WorldEnvironment, $Sun)
	font_title = load("res://assets/fonts/Cinzel-Variable.ttf")
	font_body = load("res://assets/fonts/Rajdhani-SemiBold.ttf")
	font_bold = load("res://assets/fonts/Rajdhani-Bold.ttf")
	if Settings.last_skirmish.size() > 0:
		opts.merge(Settings.last_skirmish, true)
	# backdrop battle
	game_view.set_camera(rig.cam)
	game_view.start_game({"seed": randi() % 100000, "players": 3, "difficulty": 3, "ai_self": 1, "reveal": 1, "map_size": 0})
	game_view.set_reveal(true)
	game_view.warp(20 * 60 * 9)
	game_view.set_speed(1.0)
	rig.game_view = game_view
	rig.map_size = game_view.map_size()
	rig.edge_pan = false
	rig.set_process_unhandled_input(false)
	$VFX.bind(game_view, rig)
	_focus = game_view.home_position()
	rig.focus(_focus, true)
	rig.dist = 95.0
	rig._dist_goal = 95.0
	_build_ui()
	var music := AudioStreamPlayer.new()
	music.bus = "Music"
	music.stream = load("res://assets/audio/music/menu_heroic_age.mp3")
	music.volume_db = -6.0
	add_child(music)
	music.play()

func _process(dt: float) -> void:
	orbit += dt * 0.05
	rig._yaw_goal = orbit
	_hot_t -= dt
	if _hot_t <= 0.0:
		_hot_t = 9.0
		var h: Vector3 = game_view.hotspot()
		if h.y > -0.5:
			_focus = h
	rig.target = rig.target.lerp(Vector3(_focus.x, rig.target.y, _focus.z), 1.0 - exp(-dt * 0.4))
	for e in game_view.take_events():
		if has_node("VFX"):
			$VFX.on_event(e)

# ------------------------------------------------------------------ UI helpers

func _lbl(text: String, size: int, color := Color(0.92, 0.9, 0.85), font: Font = null) -> Label:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", size)
	l.add_theme_color_override("font_color", color)
	l.add_theme_font_override("font", font if font else font_body)
	l.add_theme_color_override("font_shadow_color", Color(0, 0, 0, 0.8))
	l.add_theme_constant_override("shadow_offset_x", 2)
	l.add_theme_constant_override("shadow_offset_y", 2)
	return l

func _btn(text: String, cb: Callable, w := 320, h := 52) -> Button:
	var b := Button.new()
	b.text = text
	b.custom_minimum_size = Vector2(w, h)
	b.add_theme_font_override("font", font_bold)
	b.add_theme_font_size_override("font_size", 24)
	b.alignment = HORIZONTAL_ALIGNMENT_LEFT
	var n := StyleBoxFlat.new()
	n.bg_color = Color(0.04, 0.05, 0.06, 0.55)
	n.border_color = Color(ACCENT, 0.0)
	n.border_width_left = 3
	n.content_margin_left = 22
	var hv := n.duplicate()
	hv.bg_color = Color(0.12, 0.10, 0.06, 0.85)
	hv.border_color = ACCENT
	var pr := hv.duplicate()
	pr.bg_color = Color(0.25, 0.19, 0.08, 0.9)
	b.add_theme_stylebox_override("normal", n)
	b.add_theme_stylebox_override("hover", hv)
	b.add_theme_stylebox_override("pressed", pr)
	b.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	b.add_theme_color_override("font_color", Color(0.9, 0.88, 0.82))
	b.add_theme_color_override("font_hover_color", ACCENT.lightened(0.3))
	b.pressed.connect(cb)
	return b

func _panel(w: int, h: int) -> PanelContainer:
	var p := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.03, 0.035, 0.045, 0.86)
	sb.border_color = Color(ACCENT, 0.5)
	sb.set_border_width_all(1)
	sb.set_corner_radius_all(4)
	sb.content_margin_left = 30; sb.content_margin_right = 30
	sb.content_margin_top = 24; sb.content_margin_bottom = 24
	sb.shadow_color = Color(0, 0, 0, 0.5)
	sb.shadow_size = 18
	p.add_theme_stylebox_override("panel", sb)
	p.custom_minimum_size = Vector2(w, h)
	return p

func _option(parent: Control, label: String, items: Array, key: String, values: Array) -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 16)
	var l := _lbl(label, 22, Color(0.78, 0.76, 0.7))
	l.custom_minimum_size = Vector2(230, 0)
	row.add_child(l)
	var ob := OptionButton.new()
	ob.custom_minimum_size = Vector2(260, 40)
	ob.add_theme_font_override("font", font_body)
	ob.add_theme_font_size_override("font_size", 20)
	for i in items.size():
		ob.add_item(items[i], i)
	var cur := values.find(opts[key])
	ob.select(max(cur, 0))
	ob.item_selected.connect(func(i): opts[key] = values[i])
	row.add_child(ob)
	parent.add_child(row)

# ------------------------------------------------------------------ layout

func _build_ui() -> void:
	var root := Control.new()
	root.set_anchors_preset(Control.PRESET_FULL_RECT)
	ui.add_child(root)
	# vignette + left gradient for legibility
	var grad := TextureRect.new()
	var g := Gradient.new()
	g.set_color(0, Color(0, 0, 0, 0.82))
	g.set_color(1, Color(0, 0, 0, 0.0))
	var gt := GradientTexture2D.new()
	gt.gradient = g
	gt.fill_from = Vector2(0, 0.5)
	gt.fill_to = Vector2(0.62, 0.5)
	grad.texture = gt
	grad.set_anchors_preset(Control.PRESET_FULL_RECT)
	grad.stretch_mode = TextureRect.STRETCH_SCALE
	grad.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(grad)

	var title_box := VBoxContainer.new()
	title_box.position = Vector2(90, 90)
	title_box.add_theme_constant_override("separation", -6)
	root.add_child(title_box)
	var t1 := _lbl("EMPIRE EARTH", 96, Color(0.95, 0.83, 0.56), font_title)
	t1.add_theme_constant_override("shadow_offset_y", 4)
	title_box.add_child(t1)
	var t2 := _lbl("A T O M I C   C O N Q U E S T", 30, Color(0.85, 0.85, 0.82), font_bold)
	title_box.add_child(t2)
	var rule := ColorRect.new()
	rule.custom_minimum_size = Vector2(560, 2)
	rule.color = Color(ACCENT, 0.7)
	title_box.add_child(rule)

	main_panel = VBoxContainer.new()
	main_panel.position = Vector2(90, 380)
	main_panel.add_theme_constant_override("separation", 10)
	root.add_child(main_panel)
	main_panel.add_child(_btn("Skirmish", func(): _show(skirmish_panel)))
	main_panel.add_child(_btn("Load Game", func(): _refresh_saves(); _show(load_panel)))
	main_panel.add_child(_btn("LAN Game", func(): _show(lan_panel)))
	main_panel.add_child(_btn("Settings", func(): _show(settings_panel)))
	main_panel.add_child(_btn("Credits", func(): _show(credits_panel)))
	main_panel.add_child(_btn("Quit", func(): get_tree().quit()))

	var ver := _lbl("Atomic Modern · Big Islands · v0.1", 16, Color(0.6, 0.6, 0.58))
	ver.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	ver.position = Vector2(90, -50)
	root.add_child(ver)

	_build_skirmish(root)
	_build_load(root)
	lan_panel = load("res://scripts/lan_menu.gd").new()
	lan_panel.position = Vector2(470, 280)
	lan_panel.visible = false
	root.add_child(lan_panel)
	lan_panel.setup(self)
	if OS.get_cmdline_user_args().has("--lanhost") or Array(OS.get_cmdline_user_args()).any(func(a): return a.begins_with("--lanjoin=")):
		get_tree().create_timer(1.0).timeout.connect(_lan_autotest)
	_build_settings(root)
	_build_credits(root)

func _lan_autotest() -> void:
	_show(lan_panel)
	lan_panel.autotest(OS.get_cmdline_user_args())

func _show(p: Control) -> void:
	for x in [skirmish_panel, load_panel, lan_panel, settings_panel, credits_panel]:
		x.visible = x == p and not p.visible

func _build_skirmish(root: Control) -> void:
	var p := _panel(620, 560)
	p.position = Vector2(470, 300)
	p.visible = false
	root.add_child(p)
	skirmish_panel = p
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 12)
	p.add_child(v)
	v.add_child(_lbl("SKIRMISH", 34, ACCENT, font_title))
	v.add_child(_lbl("Big Islands · Atomic Modern", 18, Color(0.65, 0.64, 0.6)))
	_option(v, "Opponents", ["1 AI", "2 AI", "3 AI"], "players", [2, 3, 4])
	_option(v, "Difficulty", ["Easy", "Normal", "Hard", "Hardest"], "difficulty", [0, 1, 2, 3])
	_option(v, "Map size", ["Small", "Medium", "Large"], "map_size", [0, 1, 2])
	_option(v, "Resources on map", ["Standard", "High", "Very high"], "resources", [100, 150, 220])
	_option(v, "Starting resources", ["Low", "Standard", "High", "Deathmatch"], "start_res", [600, 1500, 3000, 10000])
	_option(v, "Population limit", ["200", "300", "500", "1000", "2000", "3000"], "pop_limit", [200, 300, 500, 1000, 2000, 3000])
	_option(v, "Fog of war", ["On", "Revealed map"], "reveal", [0, 1])
	var hb := HBoxContainer.new()
	hb.alignment = BoxContainer.ALIGNMENT_END
	hb.add_theme_constant_override("separation", 12)
	hb.add_child(_btn("Back", func(): _show(null), 150, 50))
	var start := _btn("Start Game", _start, 220, 50)
	hb.add_child(start)
	v.add_child(Control.new())
	v.add_child(hb)

var load_panel: Control
var lan_panel: Control
var load_list: VBoxContainer

func _build_load(root: Control) -> void:
	var p := _panel(620, 560)
	p.position = Vector2(470, 300)
	p.visible = false
	root.add_child(p)
	load_panel = p
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 12)
	p.add_child(v)
	v.add_child(_lbl("LOAD GAME", 34, ACCENT, font_title))
	var sc := ScrollContainer.new()
	sc.custom_minimum_size = Vector2(580, 400)
	sc.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	v.add_child(sc)
	load_list = VBoxContainer.new()
	load_list.add_theme_constant_override("separation", 6)
	load_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	sc.add_child(load_list)
	var hb := HBoxContainer.new()
	hb.alignment = BoxContainer.ALIGNMENT_END
	hb.add_child(_btn("Back", func(): _show(null), 150, 50))
	v.add_child(hb)

func _refresh_saves() -> void:
	for c in load_list.get_children():
		c.queue_free()
	var saves: Array = game_view.list_saves() if game_view else []
	if saves.is_empty():
		load_list.add_child(_lbl("No saved games yet. Save from the pause menu (Esc) or press F5 in game.", 18, Color(0.65, 0.64, 0.6)))
		return
	for sv in saves:
		var path: String = sv["path"]
		var b := _btn("%s   ·   %s" % [sv["name"], sv["time"]], func(): _load(path), 570, 44)
		b.add_theme_font_size_override("font_size", 18)
		load_list.add_child(b)

func _load(path: String) -> void:
	Engine.set_meta("load_save", path)
	get_tree().change_scene_to_file("res://scenes/main.tscn")

func _start() -> void:
	if Engine.has_meta("load_save"):
		Engine.remove_meta("load_save")
	Settings.last_skirmish = opts.duplicate()
	Settings.save()
	var cfg := opts.duplicate()
	cfg["seed"] = randi() % 1000000 if opts["seed"] == 0 else opts["seed"]
	Engine.set_meta("match_config", cfg)
	get_tree().change_scene_to_file("res://scenes/main.tscn")

func _slider(parent: Control, label: String, value: float, cb: Callable) -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 16)
	var l := _lbl(label, 22, Color(0.78, 0.76, 0.7))
	l.custom_minimum_size = Vector2(230, 0)
	row.add_child(l)
	var s := HSlider.new()
	s.min_value = 0.0
	s.max_value = 1.0
	s.step = 0.01
	s.value = value
	s.custom_minimum_size = Vector2(260, 30)
	s.value_changed.connect(cb)
	row.add_child(s)
	parent.add_child(row)

func _check(parent: Control, label: String, value: bool, cb: Callable) -> void:
	var c := CheckBox.new()
	c.text = label
	c.button_pressed = value
	c.add_theme_font_override("font", font_body)
	c.add_theme_font_size_override("font_size", 22)
	c.toggled.connect(cb)
	parent.add_child(c)

func _build_settings(root: Control) -> void:
	var p := _panel(620, 440)
	p.position = Vector2(470, 300)
	p.visible = false
	root.add_child(p)
	settings_panel = p
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 14)
	p.add_child(v)
	v.add_child(_lbl("SETTINGS", 34, ACCENT, font_title))
	_slider(v, "Master volume", Settings.master_volume, func(x): Settings.master_volume = x; Settings.apply())
	_slider(v, "Music volume", Settings.music_volume, func(x): Settings.music_volume = x; Settings.apply())
	_slider(v, "Effects volume", Settings.sfx_volume, func(x): Settings.sfx_volume = x; Settings.apply())
	_check(v, "Fullscreen", Settings.fullscreen, func(x): Settings.fullscreen = x; Settings.apply())
	_check(v, "Edge scrolling", Settings.edge_pan, func(x): Settings.edge_pan = x)
	var hb := HBoxContainer.new()
	hb.alignment = BoxContainer.ALIGNMENT_END
	hb.add_child(_btn("Done", func(): Settings.save(); _show(null), 150, 50))
	v.add_child(hb)

func _build_credits(root: Control) -> void:
	var p := _panel(620, 400)
	p.position = Vector2(470, 300)
	p.visible = false
	root.add_child(p)
	credits_panel = p
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 10)
	p.add_child(v)
	v.add_child(_lbl("CREDITS", 34, ACCENT, font_title))
	var txt := RichTextLabel.new()
	txt.bbcode_enabled = true
	txt.fit_content = true
	txt.add_theme_font_override("normal_font", font_body)
	txt.add_theme_font_size_override("normal_font_size", 19)
	txt.text = "A modern homage to [b]Empire Earth[/b] (Stainless Steel Studios, 2001).\n\nEngine: Godot 4 + Rust simulation core\nModels: procedurally generated in Blender\nTextures & sky: Poly Haven (CC0)\nFonts: Cinzel, Rajdhani (SIL Open Font License)\nMusic: Kevin MacLeod (incompetech.com), CC BY 4.0 —\n  Heroic Age, Prelude and Action, Five Armies,\n  Volatile Reaction, Rynos Theme, Clash Defiant\nSound effects: procedurally synthesized\n\nAll gameplay code is original."
	v.add_child(txt)
	var hb := HBoxContainer.new()
	hb.alignment = BoxContainer.ALIGNMENT_END
	hb.add_child(_btn("Back", func(): _show(null), 150, 50))
	v.add_child(hb)
