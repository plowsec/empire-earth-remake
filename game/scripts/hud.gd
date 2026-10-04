extends CanvasLayer
## In-game HUD + world input. Built in code so layout and theme live in one place.

const RES_NAMES := ["food", "wood", "stone", "gold", "iron"]
const RES_COLORS := [Color(0.95, 0.55, 0.35), Color(0.62, 0.45, 0.28), Color(0.72, 0.72, 0.7), Color(1.0, 0.82, 0.25), Color(0.6, 0.68, 0.78)]
const ACCENT := Color(0.86, 0.68, 0.36)
const PANEL_BG := Color(0.055, 0.065, 0.075, 0.88)

var gv: Node
var rig: Node
var main: Node
var theme_res: Theme

var root: Control
var res_labels: Array = []
var pop_label: Label
var clock_label: Label
var minimap: TextureRect
var minimap_overlay: Control
var sel_panel: PanelContainer
var sel_box: VBoxContainer
var card_grid: GridContainer
var queue_box: HBoxContainer
var tooltip: PanelContainer
var tooltip_label: RichTextLabel
var notice_box: VBoxContainer
var drag_rect: Panel
var hover_label: Label
var end_panel: PanelContainer
var menu_panel: PanelContainer
var debug_label: Label

var idle_btn: Button
var group_bar: HBoxContainer
var group_cache := ""
var _group_click := {}
var mode := ""            # "", "place", "attack_move", "unload"
var dragging := false
var drag_start := Vector2.ZERO
var last_click_time := 0.0
var card_cache := ""
var sel_cache := ""
var refresh_t := 0.0
var font_title: Font
var font_body: Font

func _ready() -> void:
	layer = 10

func bind(game_view: Node, camera_rig: Node, main_node: Node) -> void:
	gv = game_view
	rig = camera_rig
	main = main_node
	_build()

# ---------------------------------------------------------------- theme

func _make_theme() -> Theme:
	var t := Theme.new()
	font_title = _load_font("res://assets/fonts/Rajdhani-Bold.ttf")
	font_body = _load_font("res://assets/fonts/Rajdhani-SemiBold.ttf")
	if font_body:
		t.default_font = font_body
	t.default_font_size = 17
	var sb := StyleBoxFlat.new()
	sb.bg_color = PANEL_BG
	sb.border_color = Color(ACCENT, 0.55)
	sb.set_border_width_all(1)
	sb.set_corner_radius_all(6)
	sb.shadow_color = Color(0, 0, 0, 0.45)
	sb.shadow_size = 8
	sb.content_margin_left = 10; sb.content_margin_right = 10
	sb.content_margin_top = 8; sb.content_margin_bottom = 8
	t.set_stylebox("panel", "PanelContainer", sb)
	var b := StyleBoxFlat.new()
	b.bg_color = Color(0.13, 0.15, 0.17, 0.95)
	b.border_color = Color(0.32, 0.30, 0.26)
	b.set_border_width_all(1)
	b.set_corner_radius_all(5)
	b.content_margin_left = 4; b.content_margin_right = 4
	var bh := b.duplicate()
	bh.bg_color = Color(0.2, 0.22, 0.24, 1.0)
	bh.border_color = ACCENT
	var bp := b.duplicate()
	bp.bg_color = Color(0.32, 0.26, 0.15, 1.0)
	bp.border_color = ACCENT
	var bd := b.duplicate()
	bd.bg_color = Color(0.09, 0.09, 0.1, 0.9)
	bd.border_color = Color(0.2, 0.2, 0.2)
	t.set_stylebox("normal", "Button", b)
	t.set_stylebox("hover", "Button", bh)
	t.set_stylebox("pressed", "Button", bp)
	t.set_stylebox("disabled", "Button", bd)
	t.set_stylebox("focus", "Button", StyleBoxEmpty.new())
	t.set_color("font_color", "Button", Color(0.92, 0.9, 0.84))
	t.set_color("font_hover_color", "Button", Color(1, 0.95, 0.8))
	t.set_color("font_disabled_color", "Button", Color(0.5, 0.48, 0.45))
	t.set_color("font_color", "Label", Color(0.92, 0.9, 0.85))
	t.set_font_size("font_size", "Button", 15)
	var pb_bg := StyleBoxFlat.new(); pb_bg.bg_color = Color(0.05, 0.05, 0.05, 0.9); pb_bg.set_corner_radius_all(3)
	var pb_fg := StyleBoxFlat.new(); pb_fg.bg_color = Color(0.3, 0.85, 0.35); pb_fg.set_corner_radius_all(3)
	t.set_stylebox("background", "ProgressBar", pb_bg)
	t.set_stylebox("fill", "ProgressBar", pb_fg)
	return t

func _load_font(path: String) -> Font:
	if ResourceLoader.exists(path):
		return load(path)
	return null

func _label(text: String, size := 17, color := Color(0.92, 0.9, 0.85), title := false) -> Label:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", size)
	l.add_theme_color_override("font_color", color)
	if title and font_title:
		l.add_theme_font_override("font", font_title)
	l.add_theme_color_override("font_shadow_color", Color(0, 0, 0, 0.7))
	l.add_theme_constant_override("shadow_offset_y", 1)
	return l

# ---------------------------------------------------------------- layout

func _icon(key: String) -> Texture2D:
	var path := "res://assets/icons/%s.png" % key
	if ResourceLoader.exists(path):
		return load(path)
	return null

func _console_style() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.045, 0.05, 0.058, 0.94)
	sb.border_color = Color(ACCENT, 0.75)
	sb.border_width_top = 2
	sb.shadow_color = Color(0, 0, 0, 0.55)
	sb.shadow_size = 14
	sb.content_margin_left = 10; sb.content_margin_right = 10
	sb.content_margin_top = 10; sb.content_margin_bottom = 8
	return sb

func _inset_style() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.02, 0.025, 0.03, 0.9)
	sb.border_color = Color(0.3, 0.27, 0.2, 0.9)
	sb.set_border_width_all(1)
	sb.set_corner_radius_all(4)
	sb.content_margin_left = 8; sb.content_margin_right = 8
	sb.content_margin_top = 6; sb.content_margin_bottom = 6
	return sb

func _build() -> void:
	if root:
		root.queue_free()
	theme_res = _make_theme()
	root = Control.new()
	root.theme = theme_res
	root.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(root)

	# --- top bar
	var top := PanelContainer.new()
	top.set_anchors_preset(Control.PRESET_CENTER_TOP)
	top.position = Vector2(-470, 4)
	top.custom_minimum_size = Vector2(940, 46)
	top.mouse_filter = Control.MOUSE_FILTER_STOP
	root.add_child(top)
	var hb := HBoxContainer.new()
	hb.add_theme_constant_override("separation", 18)
	hb.alignment = BoxContainer.ALIGNMENT_CENTER
	top.add_child(hb)
	for i in 5:
		var box := HBoxContainer.new()
		box.add_theme_constant_override("separation", 4)
		box.tooltip_text = RES_NAMES[i].capitalize()
		box.mouse_filter = Control.MOUSE_FILTER_PASS
		var icon := TextureRect.new()
		icon.texture = _icon("res_" + RES_NAMES[i])
		icon.custom_minimum_size = Vector2(34, 34)
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		box.add_child(icon)
		var l := _label("0", 22, Color(1, 0.97, 0.9), true)
		l.custom_minimum_size = Vector2(62, 0)
		box.add_child(l)
		res_labels.append(l)
		hb.add_child(box)
	hb.add_child(VSeparator.new())
	var pbox := HBoxContainer.new()
	var picon := TextureRect.new()
	picon.texture = _icon("res_pop")
	picon.custom_minimum_size = Vector2(34, 34)
	picon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	picon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	pbox.add_child(picon)
	pop_label = _label("0/0", 21, Color(0.85, 0.92, 1.0), true)
	pbox.add_child(pop_label)
	hb.add_child(pbox)
	clock_label = _label("00:00", 21, ACCENT, true)
	hb.add_child(clock_label)
	var dip_btn := Button.new()
	dip_btn.text = "Diplomacy"
	dip_btn.tooltip_text = "Alliances and tribute"
	dip_btn.custom_minimum_size = Vector2(96, 32)
	dip_btn.pressed.connect(_toggle_diplomacy)
	hb.add_child(dip_btn)
	var save_btn := Button.new()
	save_btn.text = "Save"
	save_btn.tooltip_text = "Save the game (F5 quicksaves)"
	save_btn.custom_minimum_size = Vector2(64, 32)
	save_btn.pressed.connect(_open_save_dialog)
	hb.add_child(save_btn)
	var menu_btn := Button.new()
	menu_btn.text = "Menu"
	menu_btn.custom_minimum_size = Vector2(72, 32)
	menu_btn.pressed.connect(_toggle_menu)
	hb.add_child(menu_btn)

	# Empire Earth-style shallow command strip, with raised side panels.
	var console := PanelContainer.new()
	console.add_theme_stylebox_override("panel", _console_style())
	console.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	console.offset_top = -112
	console.offset_bottom = 0
	console.mouse_filter = Control.MOUSE_FILTER_STOP
	root.add_child(console)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 12)
	console.add_child(row)

	var utility := VBoxContainer.new()
	utility.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	utility.add_theme_constant_override("separation", 8)
	row.add_child(utility)
	var strip := HBoxContainer.new()
	strip.add_theme_constant_override("separation", 6)
	utility.add_child(strip)
	idle_btn = Button.new()
	idle_btn.custom_minimum_size = Vector2(110, 46)
	idle_btn.add_theme_font_size_override("font_size", 20)
	idle_btn.icon = _icon("citizen")
	idle_btn.expand_icon = true
	idle_btn.tooltip_text = "Idle citizens (;). Ctrl-click: 8 nearest idle citizens. Shift-click: all idle citizens."
	idle_btn.pressed.connect(_on_idle_pressed)
	strip.add_child(idle_btn)
	var idle8 := Button.new()
	idle8.text = "×8"
	idle8.custom_minimum_size = Vector2(46, 46)
	idle8.add_theme_font_size_override("font_size", 18)
	idle8.tooltip_text = "Select 8 idle citizens (the next one and the 7 nearest to it)"
	idle8.pressed.connect(func(): _select_idle(8))
	strip.add_child(idle8)
	var save_grp := Button.new()
	save_grp.text = "+ Group"
	save_grp.custom_minimum_size = Vector2(84, 46)
	save_grp.tooltip_text = "Save the selection as a group (units or buildings).\nCtrl/Alt+0-9: save to that number · Shift+0-9: add to it · 0-9: select · twice: center on it"
	save_grp.pressed.connect(_save_group)
	strip.add_child(save_grp)
	group_bar = HBoxContainer.new()
	group_bar.add_theme_constant_override("separation", 4)
	strip.add_child(group_bar)
	_build_formation_bar(utility)
	hint_label = _label("SELECT  Left click / drag     COMMAND  Right click     ATTACK  A     RALLY  Shift+Right click adds points", 13, Color(0.60, 0.61, 0.57))
	utility.add_child(hint_label)

	var card_frame := PanelContainer.new()
	card_frame.add_theme_stylebox_override("panel", _inset_style())
	row.add_child(card_frame)
	card_grid = GridContainer.new()
	card_grid.columns = 11
	card_grid.add_theme_constant_override("h_separation", 4)
	card_grid.add_theme_constant_override("v_separation", 4)
	card_grid.custom_minimum_size = Vector2(11 * 54 + 10 * 4, 2 * 42 + 4)
	card_frame.add_child(card_grid)

	# Raised left side: minimap. The center of the battlefield stays unobstructed.
	var mm_frame := PanelContainer.new()
	mm_frame.add_theme_stylebox_override("panel", _inset_style())
	mm_frame.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	mm_frame.position = Vector2(8, -322)
	mm_frame.size = Vector2(210, 204)
	root.add_child(mm_frame)
	minimap = TextureRect.new()
	minimap.texture = gv.minimap_texture()
	minimap.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	minimap.stretch_mode = TextureRect.STRETCH_SCALE
	minimap.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	minimap.custom_minimum_size = Vector2(194, 192)
	minimap.mouse_filter = Control.MOUSE_FILTER_STOP
	minimap.gui_input.connect(_on_minimap_input)
	mm_frame.add_child(minimap)
	minimap_overlay = Control.new()
	minimap_overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	minimap_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	minimap_overlay.draw.connect(_draw_minimap_overlay)
	minimap.add_child(minimap_overlay)

	# Raised right side: selection overview, aligned above the command icons.
	sel_panel = PanelContainer.new()
	sel_panel.add_theme_stylebox_override("panel", _inset_style())
	sel_panel.set_anchors_preset(Control.PRESET_BOTTOM_RIGHT)
	sel_panel.position = Vector2(-610, -322)
	sel_panel.size = Vector2(602, 204)
	root.add_child(sel_panel)
	var sv := VBoxContainer.new()
	sv.add_theme_constant_override("separation", 4)
	sel_panel.add_child(sv)
	var overview := ScrollContainer.new()
	overview.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	overview.size_flags_vertical = Control.SIZE_EXPAND_FILL
	sv.add_child(overview)
	sel_box = VBoxContainer.new()
	sel_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	overview.add_child(sel_box)
	var queue_scroll := ScrollContainer.new()
	queue_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	queue_scroll.custom_minimum_size.y = 40
	sv.add_child(queue_scroll)
	queue_box = HBoxContainer.new()
	queue_box.add_theme_constant_override("separation", 4)
	queue_scroll.add_child(queue_box)

	# --- tooltip
	tooltip = PanelContainer.new()
	tooltip.visible = false
	tooltip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	tooltip.custom_minimum_size = Vector2(320, 0)
	root.add_child(tooltip)
	tooltip_label = RichTextLabel.new()
	tooltip_label.bbcode_enabled = true
	tooltip_label.fit_content = true
	tooltip_label.custom_minimum_size = Vector2(300, 0)
	tooltip_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	tooltip.add_child(tooltip_label)

	# --- notifications
	notice_box = VBoxContainer.new()
	notice_box.set_anchors_preset(Control.PRESET_CENTER_TOP)
	notice_box.position = Vector2(-300, 62)
	notice_box.custom_minimum_size = Vector2(600, 0)
	notice_box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(notice_box)

	hover_label = _label("", 15, Color(1, 1, 0.9))
	hover_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(hover_label)

	debug_label = _label("", 13, Color(0.7, 0.9, 0.7))
	debug_label.position = Vector2(12, 60)
	debug_label.visible = false
	root.add_child(debug_label)

	drag_rect = Panel.new()
	var ds := StyleBoxFlat.new()
	ds.bg_color = Color(0.4, 1.0, 0.5, 0.08)
	ds.border_color = Color(0.5, 1.0, 0.6, 0.9)
	ds.set_border_width_all(1)
	drag_rect.add_theme_stylebox_override("panel", ds)
	drag_rect.visible = false
	drag_rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(drag_rect)

	_build_menu()
	_build_end_panel()
	_build_net_label()

func _build_menu() -> void:
	menu_panel = PanelContainer.new()
	menu_panel.set_anchors_preset(Control.PRESET_CENTER)
	menu_panel.position = Vector2(-160, -150)
	menu_panel.custom_minimum_size = Vector2(320, 340)
	menu_panel.visible = false
	menu_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	root.add_child(menu_panel)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 10)
	menu_panel.add_child(v)
	v.add_child(_label("PAUSED", 30, ACCENT, true))
	for item in [["Resume", _toggle_menu], ["Game Speed: Normal", _cycle_speed], ["Save Game", _save_game], ["Reveal Map (debug)", _toggle_reveal], ["Restart", _restart], ["Main Menu", _to_menu], ["Quit", _quit]]:
		var b := Button.new()
		b.text = item[0]
		b.custom_minimum_size = Vector2(0, 38)
		b.pressed.connect(item[1])
		v.add_child(b)

var net_label: Label
var net_t := 0.0

func _build_net_label() -> void:
	net_label = _label("", 20, Color(1, 0.85, 0.5), true)
	net_label.set_anchors_preset(Control.PRESET_CENTER_TOP)
	net_label.position = Vector2(-300, 96)
	net_label.custom_minimum_size = Vector2(600, 0)
	net_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	net_label.add_theme_constant_override("outline_size", 6)
	net_label.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.8))
	net_label.visible = false
	root.add_child(net_label)

func _update_net(dt: float) -> void:
	net_t -= dt
	if net_t > 0.0:
		return
	net_t = 0.4
	if not gv.is_lan():
		net_label.visible = false
		return
	var st: Dictionary = gv.net_status()
	var lines: Array[String] = []
	if st.get("host_lost", false):
		lines.append("Connection to the host lost")
	var waiting: PackedStringArray = st.get("waiting", PackedStringArray())
	if waiting.size() > 0:
		lines.append("Waiting for " + ", ".join(waiting) + "…")
	if int(st.get("desync_tick", -1)) >= 0:
		lines.append("DESYNC detected at tick %d: the game states differ" % st["desync_tick"])
	var dropped: PackedStringArray = st.get("dropped", PackedStringArray())
	if dropped.size() > 0:
		lines.append(", ".join(dropped) + " left the game")
	net_label.text = "\n".join(lines)
	net_label.visible = lines.size() > 0

func _build_end_panel() -> void:
	end_panel = PanelContainer.new()
	end_panel.set_anchors_preset(Control.PRESET_FULL_RECT)
	end_panel.add_theme_stylebox_override("panel", StyleBoxEmpty.new())
	end_panel.visible = false
	end_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	root.add_child(end_panel)

# ---------------------------------------------------------------- per frame

func _process(dt: float) -> void:
	if gv == null or not gv.is_running():
		return
	var st: Dictionary = gv.player_state()
	var res: Array = st.get("res", [])
	for i in min(5, res.size()):
		res_labels[i].text = str(res[i])
	pop_label.text = "%d/%d" % [st.get("pop", 0), st.get("pop_cap", 0)]
	pop_label.add_theme_color_override("font_color", Color(1, 0.4, 0.35) if st.get("pop", 0) >= st.get("pop_cap", 0) else Color(0.85, 0.92, 1.0))
	var secs: int = st.get("seconds", 0)
	clock_label.text = "%02d:%02d" % [secs / 60, secs % 60]
	refresh_t -= dt
	if refresh_t <= 0.0:
		refresh_t = 0.15
		_refresh_selection()
		_refresh_card()
		_refresh_strip()
	minimap_overlay.queue_redraw()
	gv.set_show_all_bars(Input.is_key_pressed(KEY_ALT))
	_update_hover()
	_update_net(dt)
	if dip_panel != null and dip_panel.visible:
		dip_t -= dt
		if dip_t <= 0.0:
			dip_t = 0.5
			_refresh_diplomacy()
	if debug_label.visible:
		debug_label.text = "%d fps | %s" % [Engine.get_frames_per_second(), gv.debug_line()]
	for e in gv.take_events():
		_on_event(e)
	if mode == "place":
		gv.placement_update(get_viewport().get_mouse_position())
	if st.get("game_over", false) and not end_panel.visible:
		_show_end(st.get("won", false))

func _update_hover() -> void:
	var mp := get_viewport().get_mouse_position()
	if _over_gui(mp):
		hover_label.visible = false
		_set_cursor("")
		return
	var h: Dictionary = gv.hover(mp)
	if h.has("name"):
		var t: String = h["name"]
		if h.get("resource", false):
			t += "  (%d)" % h.get("amount", 0)
		hover_label.text = t
		hover_label.position = mp + Vector2(18, 14)
		hover_label.add_theme_color_override("font_color", Color(1, 0.55, 0.5) if h.get("enemy", false) else Color(1, 1, 0.9))
		hover_label.visible = true
	else:
		hover_label.visible = false
	var ctx: String = ""
	if mode == "attack_move":
		ctx = "attack"
	elif mode == "unload":
		ctx = "board"
	elif mode == "nuke_target":
		ctx = "attack"
	elif mode == "place":
		ctx = "build"
	elif gv.selection_count() > 0:
		ctx = gv.cursor_context(mp)
	_set_cursor(ctx)

# ---------------------------------------------------------------- cursors
# Drawn in code: each shows what a right-click would do here.
var _cursors := {}
var _cursor_now := "-"

func _cur_img() -> Image:
	return Image.create(40, 40, false, Image.FORMAT_RGBA8)

func _plot(img: Image, x: float, y: float, c: Color, r := 1.4) -> void:
	for dy in range(-2, 3):
		for dx in range(-2, 3):
			var px := int(x) + dx
			var py := int(y) + dy
			if px < 0 or py < 0 or px >= img.get_width() or py >= img.get_height():
				continue
			var d := Vector2(px + 0.5 - x, py + 0.5 - y).length()
			var a: float = clamp(r - d + 0.5, 0.0, 1.0) * c.a
			if a <= 0.0:
				continue
			var old := img.get_pixel(px, py)
			img.set_pixel(px, py, Color(c.r, c.g, c.b, max(old.a, a)) if old.a < a else old)

func _line(img: Image, a: Vector2, b: Vector2, c: Color, w := 1.4) -> void:
	var n := int(a.distance_to(b) * 2.0) + 1
	for i in n + 1:
		var p := a.lerp(b, float(i) / n)
		_plot(img, p.x, p.y, Color(0, 0, 0, 0.8), w + 1.2)
	for i in n + 1:
		var p := a.lerp(b, float(i) / n)
		_plot(img, p.x, p.y, c, w)

func _circle(img: Image, ctr: Vector2, r: float, c: Color, w := 1.4) -> void:
	var pts := []
	for i in 49:
		pts.append(ctr + Vector2(cos(i * TAU / 48.0), sin(i * TAU / 48.0)) * r)
	for i in 48:
		_line(img, pts[i], pts[i + 1], c, w)

func _build_cursors() -> void:
	var red := Color(1.0, 0.25, 0.2)
	var green := Color(0.4, 1.0, 0.45)
	var gold := Color(1.0, 0.8, 0.25)
	var blue := Color(0.45, 0.75, 1.0)
	var c := Vector2(20, 20)
	# attack: red reticle
	var img := _cur_img()
	_circle(img, c, 11, red, 1.6)
	for d in [Vector2(1, 0), Vector2(-1, 0), Vector2(0, 1), Vector2(0, -1)]:
		_line(img, c + d * 6, c + d * 17, red, 1.6)
	_plot(img, 20, 20, red, 2.0)
	_cursors["attack"] = [ImageTexture.create_from_image(img), c]
	# move: green chevrons pointing down
	img = _cur_img()
	_line(img, Vector2(9, 12), Vector2(20, 22), green, 1.8)
	_line(img, Vector2(31, 12), Vector2(20, 22), green, 1.8)
	_line(img, Vector2(12, 21), Vector2(20, 29), green, 1.4)
	_line(img, Vector2(28, 21), Vector2(20, 29), green, 1.4)
	_cursors["move"] = [ImageTexture.create_from_image(img), Vector2(20, 26)]
	# gather: pickaxe
	img = _cur_img()
	_line(img, Vector2(8, 32), Vector2(28, 12), Color(0.75, 0.55, 0.3), 1.8)
	_line(img, Vector2(18, 6), Vector2(34, 22), gold, 2.0)
	_cursors["gather"] = [ImageTexture.create_from_image(img), Vector2(8, 32)]
	# build / repair: hammer (blue) / wrench (green)
	img = _cur_img()
	_line(img, Vector2(8, 33), Vector2(24, 17), Color(0.75, 0.55, 0.3), 1.8)
	_line(img, Vector2(18, 9), Vector2(32, 23), blue, 3.0)
	_cursors["build"] = [ImageTexture.create_from_image(img), Vector2(8, 33)]
	img = _cur_img()
	_line(img, Vector2(8, 32), Vector2(26, 14), green, 2.0)
	_circle(img, Vector2(28, 12), 6, green, 1.8)
	_cursors["repair"] = [ImageTexture.create_from_image(img), Vector2(8, 32)]
	# board: arrow into a hull
	img = _cur_img()
	_line(img, Vector2(20, 4), Vector2(20, 22), blue, 1.8)
	_line(img, Vector2(13, 15), Vector2(20, 22), blue, 1.8)
	_line(img, Vector2(27, 15), Vector2(20, 22), blue, 1.8)
	_line(img, Vector2(6, 27), Vector2(34, 27), blue, 1.8)
	_line(img, Vector2(6, 27), Vector2(12, 35), blue, 1.8)
	_line(img, Vector2(34, 27), Vector2(28, 35), blue, 1.8)
	_line(img, Vector2(12, 35), Vector2(28, 35), blue, 1.8)
	_cursors["board"] = [ImageTexture.create_from_image(img), Vector2(20, 22)]
	# land: descending plane arrow
	img = _cur_img()
	_line(img, Vector2(20, 6), Vector2(20, 30), blue, 2.0)
	_line(img, Vector2(8, 16), Vector2(32, 16), blue, 1.8)
	_line(img, Vector2(14, 28), Vector2(26, 28), blue, 1.6)
	_line(img, Vector2(6, 36), Vector2(34, 36), green, 1.4)
	_cursors["land"] = [ImageTexture.create_from_image(img), Vector2(20, 30)]
	# rally: flag
	img = _cur_img()
	_line(img, Vector2(12, 34), Vector2(12, 6), Color(0.85, 0.85, 0.85), 1.6)
	_line(img, Vector2(12, 7), Vector2(30, 12), gold, 1.8)
	_line(img, Vector2(30, 12), Vector2(12, 18), gold, 1.8)
	_cursors["rally"] = [ImageTexture.create_from_image(img), Vector2(12, 34)]

func _set_cursor(ctx: String) -> void:
	if ctx == _cursor_now:
		return
	_cursor_now = ctx
	if _cursors.is_empty():
		_build_cursors()
	if _cursors.has(ctx):
		Input.set_custom_mouse_cursor(_cursors[ctx][0], Input.CURSOR_ARROW, _cursors[ctx][1])
	else:
		Input.set_custom_mouse_cursor(null)

func _over_gui(p: Vector2) -> bool:
	var c := get_viewport().gui_get_hovered_control()
	return c != null and c != root

# ---------------------------------------------------------------- selection panel

func _refresh_selection() -> void:
	var info: Array = gv.selection_info()
	var key := str(info.size())
	for x in info:
		key += "|%s:%d:%s:%s" % [x["id"], x["hp"], x.get("order", ""), x.get("progress", 0.0)]
	sel_panel.visible = not info.is_empty()
	var q: Array = gv.production_queue()
	for item in q:
		key += "|q%s%.2f" % [item["key"], item["progress"]]
	if key == sel_cache:
		return
	sel_cache = key
	for c in queue_box.get_children(): c.queue_free()
	for c in sel_box.get_children(): c.queue_free()
	# group consecutive identical items: one icon with a count badge
	var groups := []
	for item in q:
		if groups.size() > 0 and groups[-1]["key"] == item["key"]:
			groups[-1]["count"] += 1
			groups[-1]["last"] = item["index"]
		else:
			groups.append({"key": item["key"], "name": item["name"], "count": 1, "first": item["index"], "last": item["index"], "progress": item["progress"]})
	for g in groups:
		var b := Button.new()
		b.custom_minimum_size = Vector2(50, 44)
		var ic: Texture2D = _icon(g["key"]) if not (g["key"] as String).begins_with("tech:") else null
		if ic:
			b.icon = ic
			b.expand_icon = true
		else:
			b.text = (g["name"] as String).left(6)
			b.add_theme_font_size_override("font_size", 12)
		b.tooltip_text = "%s x%d (click: cancel one, Shift-click: cancel all)" % [g["name"], g["count"]]
		var first: int = g["first"]
		var last: int = g["last"]
		b.pressed.connect(func():
			if Input.is_key_pressed(KEY_SHIFT):
				for i in range(last, first - 1, -1):
					gv.do_action("cancel", str(i))
			else:
				gv.do_action("cancel", str(last)))
		if g["count"] > 1:
			var badge := _label("x%d" % g["count"], 15, Color(1, 0.95, 0.75), true)
			badge.position = Vector2(26, 24)
			badge.mouse_filter = Control.MOUSE_FILTER_IGNORE
			b.add_child(badge)
		if g["first"] == 0:
			var pb := ProgressBar.new()
			pb.show_percentage = false
			pb.custom_minimum_size = Vector2(50, 4)
			pb.value = g["progress"] * 100.0
			var vb := VBoxContainer.new()
			vb.add_theme_constant_override("separation", 1)
			vb.add_child(b)
			vb.add_child(pb)
			queue_box.add_child(vb)
		else:
			queue_box.add_child(b)
	if info.is_empty():
		sel_box.add_child(_label("Nothing selected", 16, Color(0.6, 0.6, 0.55)))
		sel_box.add_child(_label("Left-drag to select · Right-click to command · A: attack-move", 13, Color(0.5, 0.5, 0.46)))
		return
	if info.size() == 1:
		var x: Dictionary = info[0]
		var head := HBoxContainer.new()
		head.add_theme_constant_override("separation", 14)
		var sw := ColorRect.new()
		sw.custom_minimum_size = Vector2(4, 48)
		sw.color = x["owner_color"]
		head.add_child(sw)
		var portrait := TextureRect.new()
		portrait.texture = _icon(x["key"])
		portrait.custom_minimum_size = Vector2(52, 52)
		portrait.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		portrait.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		head.add_child(portrait)
		var nv := VBoxContainer.new()
		nv.add_child(_label(x["name"], 20, ACCENT, true))
		var role := _label(x.get("role", ""), 13, Color(0.75, 0.73, 0.68))
		role.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		nv.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		nv.add_child(role)
		head.add_child(nv)
		sel_box.add_child(head)
		var hp := ProgressBar.new()
		hp.custom_minimum_size = Vector2(360, 12)
		hp.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
		hp.max_value = max(1, x["max_hp"])
		hp.value = x["hp"]
		hp.show_percentage = false
		sel_box.add_child(hp)
		var stats := "HP %d/%d   Armor %d" % [x["hp"], x["max_hp"], x.get("armor", 0)]
		if x.has("attack"): stats += "   Attack %d   Range %.1f" % [x["attack"], x["range"]]
		if x.has("amount"): stats = "Remaining: %d" % x["amount"]
		if x.has("growth"): stats = "Growing: %d%%" % int(x["growth"] * 100)
		if x.has("carry"): stats += "   Carrying %d %s" % [x["carry"], x["carry_res"]]
		if x.has("cargo_max") and x["cargo_max"] > 0: stats += "   Cargo %d/%d" % [x["cargo"], x["cargo_max"]]
		if x.has("fuel"): stats += "   Fuel %d%%" % int(x["fuel"] * 100)
		if x.get("kills", 0) > 0: stats += "   Kills %d" % x["kills"]
		if x.has("complete") and not x["complete"]: stats += "   Under construction %d%%" % int(x["progress"] * 100)
		var stats_label := _label(stats, 14)
		stats_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		sel_box.add_child(stats_label)
		if x.get("own", false) and x.has("order"):
			sel_box.add_child(_label("Order: " + str(x["order"]), 13, Color(0.6, 0.62, 0.58)))
		return
	# multi-select grid
	var grid := GridContainer.new()
	grid.columns = 12
	grid.add_theme_constant_override("h_separation", 3)
	grid.add_theme_constant_override("v_separation", 3)
	sel_box.add_child(_label("%d selected" % info.size(), 15, ACCENT, true))
	sel_box.add_child(grid)
	for x in info:
		var b := Button.new()
		b.custom_minimum_size = Vector2(42, 40)
		var ic: Texture2D = _icon(x["key"])
		if ic:
			b.icon = ic
			b.expand_icon = true
		else:
			b.text = (x["name"] as String).substr(0, 3)
		b.tooltip_text = "%s  %d/%d" % [x["name"], x["hp"], x["max_hp"]]
		var f: float = float(x["hp"]) / max(1.0, float(x["max_hp"]))
		b.add_theme_color_override("font_color", Color(1, 1, 1).lerp(Color(1, 0.3, 0.2), 1.0 - f))
		var id: int = x["id"]
		b.pressed.connect(func(): gv.select_ids(PackedInt64Array([id])))
		grid.add_child(b)

func _select_idle(n: int) -> void:
	var c: Vector3 = gv.select_idle_citizens(n)
	if c.y > -0.5:
		rig.focus(c)
	sel_cache = ""; card_cache = ""

func _on_idle_pressed() -> void:
	if Input.is_key_pressed(KEY_CTRL) or Input.is_key_pressed(KEY_META):
		_select_idle(8)
		return
	if Input.is_key_pressed(KEY_SHIFT):
		gv.select_all_idle_citizens()
	else:
		var c: Vector3 = gv.select_idle_citizen()
		if c.y > -0.5:
			rig.focus(c)
	sel_cache = ""; card_cache = ""

func _refresh_strip() -> void:
	if form_bar != null:
		var show: bool = gv.units_selected()
		if show != form_bar.visible:
			form_bar.visible = show
			if hint_label: hint_label.visible = not show
			if show: _sync_formation_bar()
	var n: int = gv.idle_citizen_count()
	idle_btn.text = "Idle %d" % n
	if n > 0:
		var pulse := 0.75 + 0.25 * sin(Time.get_ticks_msec() / 180.0)
		idle_btn.modulate = Color(1.0, 0.75 + 0.15 * pulse, 0.35, 1.0)
	else:
		idle_btn.modulate = Color(0.7, 0.7, 0.7, 0.75)
	var groups: Array = gv.group_info()
	var key := ""
	for g in groups:
		key += "%d:%d:%s|" % [g["group"], g["count"], g["key"]]
	if key == group_cache:
		return
	group_cache = key
	for c in group_bar.get_children(): c.queue_free()
	for g in groups:
		var b := Button.new()
		b.custom_minimum_size = Vector2(74, 40)
		b.icon = _icon(g["key"])
		b.expand_icon = true
		b.text = "%d" % g["count"]
		b.tooltip_text = "Group %d (Ctrl+%d to set, %d to select, double to center)" % [g["group"], g["group"], g["group"]]
		var gi: int = g["group"]
		var lbl := _label(str(gi), 12, ACCENT, true)
		lbl.position = Vector2(3, 0)
		lbl.mouse_filter = Control.MOUSE_FILTER_IGNORE
		b.add_child(lbl)
		b.pressed.connect(func(): _select_group(gi))
		group_bar.add_child(b)

func _save_group() -> void:
	var g: int = gv.save_selection_group()
	if g < 0:
		notify("Select units or buildings first (or all 10 groups are in use)", Color(1, 0.6, 0.5))
	else:
		notify("Saved as group %d — press %d to select it" % [g, g], Color(0.6, 1.0, 0.7))
	group_cache = ""

# ---------------------------------------------------------------- formations

const FormationIcon = preload("res://scripts/formation_icon.gd")
const SHAPES := [["Block", "Compact block (default)"], ["Line", "Line abreast: a broad front"], ["Wedge", "Wedge: punch through at the point"], ["Column", "Column: narrow, for straits and corridors"], ["Wide", "Wide spread: a thin front several times wider, to hit a whole coast at once and swamp its defenses"]]
const TIMINGS := [["Free", "Each unit at full speed (they string out by speed and distance)"], ["Together", "Synchronized: everyone slows to the slowest so the whole group arrives at the same moment"], ["Next wave", "Follow-up: arrive together, 10 seconds after the previous synchronized wave"]]
var form_bar: HBoxContainer
var hint_label: Label
var form_btns := []
var timing_btns := []

func _build_formation_bar(parent: Control) -> void:
	form_bar = HBoxContainer.new()
	form_bar.add_theme_constant_override("separation", 3)
	form_bar.add_child(_label("FORMATION", 12, Color(0.65, 0.64, 0.6)))
	for i in SHAPES.size():
		form_btns.append(_form_button(form_bar, "shape", i, SHAPES[i]))
	var sep := VSeparator.new()
	form_bar.add_child(sep)
	form_bar.add_child(_label("ARRIVAL", 12, Color(0.65, 0.64, 0.6)))
	for i in TIMINGS.size():
		timing_btns.append(_form_button(form_bar, "timing", i, TIMINGS[i]))
	form_bar.visible = false
	parent.add_child(form_bar)

func _form_button(bar: HBoxContainer, kind: String, i: int, info: Array) -> Button:
	var b := Button.new()
	b.toggle_mode = true
	b.custom_minimum_size = Vector2(36, 32)
	b.tooltip_text = "%s — %s" % [info[0], info[1]]
	var ic := FormationIcon.new()
	ic.kind = kind
	ic.index = i
	ic.set_anchors_preset(Control.PRESET_FULL_RECT)
	b.add_child(ic)
	b.pressed.connect(func(): _pick_formation(kind, i))
	bar.add_child(b)
	return b

func _pick_formation(kind: String, i: int) -> void:
	var f: Vector2i = gv.formation()
	if kind == "shape": f.x = i
	else: f.y = i
	gv.set_formation(f.x, f.y)
	_sync_formation_bar()

func _sync_formation_bar() -> void:
	var f: Vector2i = gv.formation()
	for i in form_btns.size():
		form_btns[i].set_pressed_no_signal(i == f.x)
	for i in timing_btns.size():
		timing_btns[i].set_pressed_no_signal(i == f.y)

func _select_group(g: int) -> void:
	if gv.recall_group(g):
		var now := Time.get_ticks_msec() / 1000.0
		if now - _group_click.get(g, 0.0) < 0.35:
			var c: Vector3 = gv.selection_center()
			if c.y > -0.5: rig.focus(c)
		_group_click[g] = now
	sel_cache = ""; card_cache = ""

# ---------------------------------------------------------------- command card

func _refresh_card() -> void:
	var card: Array = gv.command_card()
	var key := ""
	for b in card:
		key += "%s%s%s|" % [b["action"], b["key"], b["enabled"]]
	if key == card_cache:
		return
	card_cache = key
	for c in card_grid.get_children(): c.queue_free()
	for b in card:
		var btn := Button.new()
		btn.custom_minimum_size = Vector2(54, 42)
		btn.clip_text = true
		var a: String = b["action"]
		var k: String = b["key"]
		var ic: Texture2D = _icon(k) if k != "" and a != "research" else null
		if ic:
			btn.icon = ic
			btn.expand_icon = true
			btn.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
			btn.add_theme_constant_override("icon_max_width", 38)
		else:
			var label: String = b["label"]
			btn.text = label if label.length() <= 10 else label.left(9) + "."
			btn.autowrap_mode = TextServer.AUTOWRAP_WORD
			btn.add_theme_font_size_override("font_size", 13)
		btn.disabled = not b["enabled"] and a in ["train", "research"]
		if not b["enabled"]:
			btn.modulate = Color(1, 0.7, 0.65, 0.85)
		var hk: String = b["hotkey"]
		if hk != "":
			var hl := _label(hk if hk.length() <= 2 else hk.left(3), 12, ACCENT, true)
			hl.position = Vector2(4, 1)
			hl.mouse_filter = Control.MOUSE_FILTER_IGNORE
			btn.add_child(hl)
		if a == "research":
			var tl := _label("R", 11, Color(0.6, 0.85, 1.0), true)
			tl.position = Vector2(43, 1)
			tl.mouse_filter = Control.MOUSE_FILTER_IGNORE
			btn.add_child(tl)
		btn.pressed.connect(func(): _do_action(a, k))
		btn.mouse_entered.connect(func(): _show_tooltip(b, btn))
		btn.mouse_exited.connect(func(): tooltip.visible = false)
		card_grid.add_child(btn)

func _show_tooltip(b: Dictionary, btn: Control) -> void:
	var t := "[b][color=#dcb060]%s[/color][/b]" % b["label"]
	if b.has("cost"):
		var parts := []
		for r in b["cost"].keys():
			parts.append("[color=#%s]%s %d[/color]" % [RES_COLORS[RES_NAMES.find(r)].to_html(false), r.capitalize(), b["cost"][r]])
		t += "\n" + "  ".join(parts)
	if b["hotkey"] != "":
		t += "   [color=#888]Hotkey %s[/color]" % b["hotkey"]
	if b["tooltip"] != "":
		t += "\n[color=#cfcabd]%s[/color]" % b["tooltip"]
	if b["action"] == "train":
		t += "\n[color=#888]Shift-click: queue 10 in every selected building[/color]"
	tooltip_label.text = t
	tooltip.visible = true
	tooltip.reset_size()
	var vs := get_viewport().get_visible_rect().size
	tooltip.position = Vector2(min(btn.global_position.x, vs.x - tooltip.size.x - 8), btn.global_position.y - tooltip.size.y - 10)

func _do_action(action: String, key: String, shift := false) -> void:
	if action == "train" and (shift or Input.is_key_pressed(KEY_SHIFT)):
		action = "train_mass"
	if main and main.has_node("Audio"):
		main.get_node("Audio").ui("ui_click", -10.0)
	var m: String = gv.do_action(action, key)
	if m != "":
		mode = m
	card_cache = ""

# ---------------------------------------------------------------- minimap

func _minimap_to_world(p: Vector2) -> Vector3:
	var ms: Vector2 = gv.map_size()
	var s := minimap.size
	return Vector3(p.x / s.x * ms.x, 0, p.y / s.y * ms.y)

func _on_minimap_input(e: InputEvent) -> void:
	if e is InputEventMouseButton and e.pressed:
		var w := _minimap_to_world(e.position)
		if e.button_index == MOUSE_BUTTON_LEFT:
			rig.focus(w)
		elif e.button_index == MOUSE_BUTTON_RIGHT:
			gv.command_world(w, Input.is_key_pressed(KEY_SHIFT))
	elif e is InputEventMouseMotion and Input.is_mouse_button_pressed(MOUSE_BUTTON_LEFT):
		rig.focus(_minimap_to_world(e.position))

func _draw_minimap_overlay() -> void:
	var vs := get_viewport().get_visible_rect().size
	var ms: Vector2 = gv.map_size()
	if ms.x <= 0: return
	var s := minimap.size
	var pts := PackedVector2Array()
	for c in [Vector2(0, 0), Vector2(vs.x, 0), Vector2(vs.x, vs.y), Vector2(0, vs.y)]:
		var g: Vector3 = gv.screen_to_ground(c)
		if g.y < -0.5:
			var cam: Camera3D = rig.cam
			var o := cam.project_ray_origin(c)
			var d := cam.project_ray_normal(c)
			var t: float = (-o.y) / minf(d.y, -0.05)
			g = o + d * t
		pts.append(Vector2(g.x / ms.x * s.x, g.z / ms.y * s.y))
	pts.append(pts[0])
	minimap_overlay.draw_polyline(pts, Color(1, 1, 1, 0.85), 1.2, true)
	# red crosses where we were attacked (fade over 15 s)
	for m in gv.attack_markers():
		var c := Vector2(m.x / ms.x * s.x, m.z / ms.y * s.y)
		var a: float = clamp(1.0 - m.y / 15.0, 0.0, 1.0)
		var pulse := 1.0 + 0.3 * sin(m.y * 8.0)
		var r := 5.0 * pulse
		var col := Color(1, 0.15, 0.1, a)
		minimap_overlay.draw_line(c + Vector2(-r, -r), c + Vector2(r, r), col, 2.5)
		minimap_overlay.draw_line(c + Vector2(-r, r), c + Vector2(r, -r), col, 2.5)

# ---------------------------------------------------------------- events

func notify(text: String, color := Color(1, 0.95, 0.85)) -> void:
	var l := _label(text, 19, color, true)
	l.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	notice_box.add_child(l)
	if notice_box.get_child_count() > 5:
		notice_box.get_child(0).queue_free()
	var tw := create_tween()
	tw.tween_interval(3.0)
	tw.tween_property(l, "modulate:a", 0.0, 1.0)
	tw.tween_callback(l.queue_free)

func _on_event(e: Dictionary) -> void:
	match e["kind"]:
		"notice": notify(e["text"], Color(1, 0.8, 0.5))
		"autosaved": notify("Autosaved (%s)" % e["text"], Color(0.6, 0.85, 1.0))
		"diplomacy":
			var col := Color(1, 0.45, 0.4) if int(e["dmg"]) == 2 else Color(0.6, 1.0, 0.7)
			notify(e["text"], col)
			if dip_panel != null and dip_panel.visible: _refresh_diplomacy()
		"complete": notify("%s complete" % e["text"], Color(0.7, 1.0, 0.7))
		"research": notify("Research complete: %s" % e["text"], Color(0.7, 0.85, 1.0))
		"under_attack": notify("We are under attack!", Color(1, 0.4, 0.35))
		"air_raid": notify(e["text"], Color(1, 0.35, 0.3))
		"defeated":
			notify("%s has been defeated" % e["text"], Color(1, 0.6, 0.4))
	if main and main.has_node("VFX"):
		main.get_node("VFX").on_event(e)

func _show_end(won: bool) -> void:
	end_panel.visible = true
	for c in end_panel.get_children(): c.queue_free()
	var screen = load("res://scripts/end_screen.gd").new()
	screen.on_play_again = _restart
	screen.on_menu = _to_menu
	screen.on_watch = func(): end_panel.visible = false
	var cinzel := _load_font("res://assets/fonts/Cinzel-Variable.ttf")
	screen.setup(gv.end_stats(), won, [cinzel if cinzel else font_title, font_title, font_body])
	end_panel.add_child(screen)
	if main and main.has_node("Audio"):
		main.get_node("Audio").ui("victory" if won else "defeat", 0.0)

# ---------------------------------------------------------------- menu actions

func _toggle_menu() -> void:
	menu_panel.visible = not menu_panel.visible
	gv.set_paused(menu_panel.visible)

var _speeds := [1.0, 1.5, 2.0, 0.5]
var _speed_i := 0
func _cycle_speed() -> void:
	_speed_i = (_speed_i + 1) % _speeds.size()
	gv.set_speed(_speeds[_speed_i])
	var names := {1.0: "Normal", 1.5: "Fast", 2.0: "Very Fast", 0.5: "Slow"}
	(menu_panel.get_child(0).get_child(2) as Button).text = "Game Speed: %s" % names[_speeds[_speed_i]]

var _reveal := false
func _toggle_reveal() -> void:
	_reveal = not _reveal
	gv.set_reveal(_reveal)

func _save_game() -> void:
	_open_save_dialog()

# ---------------------------------------------------------------- diplomacy

var dip_panel: PanelContainer
var dip_list: VBoxContainer
var dip_t := 0.0
const RES_SHORT := ["Food", "Wood", "Stone", "Gold", "Iron"]

func _toggle_diplomacy() -> void:
	if dip_panel == null:
		dip_panel = PanelContainer.new()
		dip_panel.set_anchors_preset(Control.PRESET_CENTER)
		dip_panel.position = Vector2(-330, -220)
		dip_panel.custom_minimum_size = Vector2(660, 300)
		dip_panel.mouse_filter = Control.MOUSE_FILTER_STOP
		root.add_child(dip_panel)
		var v := VBoxContainer.new()
		v.add_theme_constant_override("separation", 10)
		dip_panel.add_child(v)
		var top := HBoxContainer.new()
		var t := _label("DIPLOMACY", 26, ACCENT, true)
		t.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		top.add_child(t)
		var close := Button.new()
		close.text = "Close"
		close.pressed.connect(func(): dip_panel.visible = false)
		top.add_child(close)
		v.add_child(top)
		v.add_child(_label("Allies share vision and never fight each other. When everyone left standing is allied, they win together.", 14, Color(0.7, 0.69, 0.64)))
		dip_list = VBoxContainer.new()
		dip_list.add_theme_constant_override("separation", 12)
		v.add_child(dip_list)
		dip_panel.visible = false
	dip_panel.visible = not dip_panel.visible
	if dip_panel.visible:
		_refresh_diplomacy()

var _dip_key := ""
func _refresh_diplomacy() -> void:
	var ps: Array = gv.players()
	var key := ""
	for p in ps:
		key += "%s%s%s%s%s|" % [p["id"], p["allied"], p["offered_to_me"], p["i_offered"], p["defeated"]]
	if key == _dip_key and dip_list.get_child_count() > 0:
		return
	_dip_key = key
	for c in dip_list.get_children():
		c.queue_free()
	for p in ps:
		if p["me"]:
			continue
		var pid: int = p["id"]
		var box := VBoxContainer.new()
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 12)
		var n := _label(p["name"], 20, p["color"], true)
		n.custom_minimum_size = Vector2(150, 0)
		row.add_child(n)
		var status := "Defeated" if p["defeated"] else ("Ally" if p["allied"] else ("Enemy — offers alliance" if p["offered_to_me"] else ("Enemy — offer sent" if p["i_offered"] else "Enemy")))
		var sc := Color(0.55, 0.55, 0.52) if p["defeated"] else (Color(0.5, 1.0, 0.6) if p["allied"] else Color(1, 0.55, 0.45))
		var st := _label(status, 17, sc)
		st.custom_minimum_size = Vector2(230, 0)
		row.add_child(st)
		if not p["defeated"]:
			var b := Button.new()
			if p["allied"]:
				b.text = "Break alliance"
				b.pressed.connect(func(): gv.diplomacy(pid, false))
			elif p["offered_to_me"]:
				b.text = "Accept alliance"
				b.pressed.connect(func(): gv.diplomacy(pid, true))
			elif p["i_offered"]:
				b.text = "Withdraw offer"
				b.pressed.connect(func(): gv.diplomacy(pid, false))
			else:
				b.text = "Offer alliance"
				b.pressed.connect(func(): gv.diplomacy(pid, true))
			row.add_child(b)
		box.add_child(row)
		if not p["defeated"]:
			var trow := HBoxContainer.new()
			trow.add_theme_constant_override("separation", 6)
			trow.add_child(_label("Send 500:", 15, Color(0.7, 0.69, 0.64)))
			for r in 5:
				var tb := Button.new()
				tb.text = RES_SHORT[r]
				tb.add_theme_font_size_override("font_size", 14)
				var res := r
				tb.pressed.connect(func(): gv.tribute(pid, res, 500))
				trow.add_child(tb)
			box.add_child(trow)
		dip_list.add_child(box)

var save_panel: PanelContainer
var save_name: LineEdit
var save_list: VBoxContainer

func _open_save_dialog() -> void:
	if save_panel == null:
		_build_save_dialog()
	menu_panel.visible = false
	save_panel.visible = true
	gv.set_paused(true)
	save_name.text = "Save " + Time.get_datetime_string_from_system().replace("T", " ").replace(":", "-")
	_refresh_save_list()
	save_name.grab_focus()
	save_name.select_all()

func _close_save_dialog() -> void:
	save_panel.visible = false
	gv.set_paused(menu_panel.visible)

func _build_save_dialog() -> void:
	save_panel = PanelContainer.new()
	save_panel.set_anchors_preset(Control.PRESET_CENTER)
	save_panel.position = Vector2(-260, -230)
	save_panel.custom_minimum_size = Vector2(520, 460)
	save_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	save_panel.visible = false
	root.add_child(save_panel)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 10)
	save_panel.add_child(v)
	v.add_child(_label("SAVE GAME", 26, ACCENT, true))
	save_name = LineEdit.new()
	save_name.custom_minimum_size = Vector2(0, 36)
	save_name.placeholder_text = "Name of the save"
	save_name.text_submitted.connect(func(_t): _do_save(save_name.text))
	v.add_child(save_name)
	v.add_child(_label("Or click an existing save to overwrite it:", 15, Color(0.75, 0.74, 0.7)))
	var sc := ScrollContainer.new()
	sc.custom_minimum_size = Vector2(0, 260)
	sc.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	v.add_child(sc)
	save_list = VBoxContainer.new()
	save_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	sc.add_child(save_list)
	var hb := HBoxContainer.new()
	hb.alignment = BoxContainer.ALIGNMENT_END
	hb.add_theme_constant_override("separation", 10)
	var cancel := Button.new()
	cancel.text = "Cancel"
	cancel.custom_minimum_size = Vector2(110, 36)
	cancel.pressed.connect(_close_save_dialog)
	hb.add_child(cancel)
	var ok := Button.new()
	ok.text = "Save"
	ok.custom_minimum_size = Vector2(140, 36)
	ok.pressed.connect(func(): _do_save(save_name.text))
	hb.add_child(ok)
	v.add_child(hb)

func _refresh_save_list() -> void:
	for c in save_list.get_children():
		c.queue_free()
	for sv in gv.list_saves():
		var n: String = sv["name"]
		var b := Button.new()
		b.text = "%s   ·   %s" % [n, sv["time"]]
		b.alignment = HORIZONTAL_ALIGNMENT_LEFT
		b.custom_minimum_size = Vector2(0, 30)
		b.pressed.connect(func(): save_name.text = n)
		save_list.add_child(b)

func _do_save(name: String) -> void:
	var err: String = gv.save_game(name.strip_edges())
	if err == "":
		_close_save_dialog()
		notify("Game saved: " + name.strip_edges(), Color(0.6, 1.0, 0.6))
	else:
		notify("Save failed: " + err, Color(1.0, 0.5, 0.4))

func _quicksave() -> void:
	var err: String = gv.save_game("Quicksave")
	notify("Quicksaved" if err == "" else "Save failed: " + err, Color(0.6, 1.0, 0.6) if err == "" else Color(1.0, 0.5, 0.4))

func _quit() -> void:
	gv.write_replay()
	get_tree().quit()

func _restart() -> void:
	if gv.is_lan():
		notify("A LAN game can't be restarted", Color(1, 0.6, 0.4))
		return
	get_tree().reload_current_scene()

func _to_menu() -> void:
	gv.write_replay()
	if ResourceLoader.exists("res://scenes/menu.tscn"):
		get_tree().change_scene_to_file("res://scenes/menu.tscn")

# ---------------------------------------------------------------- world input

func _unhandled_input(e: InputEvent) -> void:
	if gv == null or not gv.is_running():
		return
	if e is InputEventMouseButton:
		var mb := e as InputEventMouseButton
		var shift := mb.shift_pressed
		if mb.button_index == MOUSE_BUTTON_LEFT:
			if mb.pressed:
				if mode == "place":
					gv.placement_update(mb.position)
					if not gv.placement_confirm(shift):
						mode = ""
					return
				if mode == "attack_move":
					gv.attack_move_click(mb.position, shift)
					if not shift: mode = ""
					return
				if mode == "unload":
					gv.unload_click(mb.position)
					mode = ""
					return
				if mode == "nuke_target":
					gv.launch_click(mb.position)
					mode = ""
					return
				dragging = true
				drag_start = mb.position
			else:
				if dragging:
					dragging = false
					drag_rect.visible = false
					if drag_start.distance_to(mb.position) > 6:
						gv.select_rect(drag_start, mb.position, shift)
					else:
						var now := Time.get_ticks_msec() / 1000.0
						var dbl := now - last_click_time < 0.32
						last_click_time = now
						gv.select_click(mb.position, shift, dbl or mb.ctrl_pressed)
					sel_cache = ""; card_cache = ""
		elif mb.button_index == MOUSE_BUTTON_RIGHT and mb.pressed:
			if mode != "":
				if mode == "place": gv.cancel_placement()
				mode = ""
				return
			gv.right_click(mb.position, shift)
	elif e is InputEventMouseMotion and dragging:
		var a := drag_start
		var b := (e as InputEventMouseMotion).position
		drag_rect.position = a.min(b)
		drag_rect.size = (a - b).abs()
		drag_rect.visible = drag_rect.size.length() > 6
	elif e is InputEventKey and e.pressed and not e.echo:
		_on_key(e as InputEventKey)

func _on_key(k: InputEventKey) -> void:
	var code := k.keycode
	if code == KEY_ESCAPE and save_panel != null and save_panel.visible:
		_close_save_dialog(); return
	if code == KEY_ESCAPE:
		if mode != "":
			if mode == "place": gv.cancel_placement()
			mode = ""
		elif gv.selection_count() > 0:
			gv.select_ids(PackedInt64Array())
		else:
			_toggle_menu()
		return
	if code == KEY_F1:
		debug_label.visible = not debug_label.visible
		return
	if code == KEY_F5:
		_quicksave(); return
	if code == KEY_F10 or code == KEY_P:
		_toggle_menu()
		return
	if code >= KEY_0 and code <= KEY_9:
		var g := code - KEY_0
		if k.ctrl_pressed or k.meta_pressed or k.alt_pressed:
			gv.set_group(g)
			notify("Saved as group %d" % g, Color(0.6, 1.0, 0.7))
			group_cache = ""
		elif k.shift_pressed:
			gv.add_selection_to_group(g)
			notify("Added to group %d" % g, Color(0.6, 1.0, 0.7))
			group_cache = ""
		else:
			if gv.recall_group(g):
				var now := Time.get_ticks_msec() / 1000.0
				if now - last_click_time < 0.35:
					var c: Vector3 = gv.selection_center()
					if c.y > -0.5: rig.focus(c)
				last_click_time = now
		return
	if code == KEY_PERIOD and k.shift_pressed or code == KEY_SEMICOLON:
		var c: Vector3 = gv.select_idle_citizen()
		if c.y > -0.5: rig.focus(c)
		return
	if code == KEY_SPACE:
		var c: Vector3 = gv.selection_center()
		if c.y > -0.5: rig.focus(c)
		else: rig.focus(gv.home_position())
		return
	if code == KEY_H:
		_do_action("stop", ""); return
	if code == KEY_DELETE:
		_do_action("delete", ""); return
	if code == KEY_U:
		_do_action("unload", ""); return
	if code == KEY_Y:
		_do_action("rtb", ""); return
	# command-card hotkeys
	var hk := OS.get_keycode_string(code)
	var card: Array = gv.command_card()
	for b in card:
		if b["hotkey"] == hk and (b["enabled"] or b["action"] == "build"):
			_do_action(b["action"], b["key"], k.shift_pressed)
			return
	if code == KEY_A and gv.selection_count() > 0:
		mode = "attack_move"
