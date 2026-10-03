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
	idle_btn.custom_minimum_size = Vector2(76, 36)
	idle_btn.icon = _icon("citizen")
	idle_btn.expand_icon = true
	idle_btn.tooltip_text = "Idle citizens (;). Shift-click: select all idle citizens."
	idle_btn.pressed.connect(_on_idle_pressed)
	strip.add_child(idle_btn)
	group_bar = HBoxContainer.new()
	group_bar.add_theme_constant_override("separation", 4)
	strip.add_child(group_bar)
	utility.add_child(_label("SELECT  Left click / drag     COMMAND  Right click     ATTACK  A", 13, Color(0.60, 0.61, 0.57)))

	var card_frame := PanelContainer.new()
	card_frame.add_theme_stylebox_override("panel", _inset_style())
	row.add_child(card_frame)
	card_grid = GridContainer.new()
	card_grid.columns = 10
	card_grid.add_theme_constant_override("h_separation", 4)
	card_grid.add_theme_constant_override("v_separation", 4)
	card_grid.custom_minimum_size = Vector2(10 * 54 + 9 * 4, 2 * 42 + 4)
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

func _build_menu() -> void:
	menu_panel = PanelContainer.new()
	menu_panel.set_anchors_preset(Control.PRESET_CENTER)
	menu_panel.position = Vector2(-160, -150)
	menu_panel.custom_minimum_size = Vector2(320, 300)
	menu_panel.visible = false
	menu_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	root.add_child(menu_panel)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 10)
	menu_panel.add_child(v)
	v.add_child(_label("PAUSED", 30, ACCENT, true))
	for item in [["Resume", _toggle_menu], ["Game Speed: Normal", _cycle_speed], ["Reveal Map (debug)", _toggle_reveal], ["Restart", _restart], ["Main Menu", _to_menu], ["Quit", func(): get_tree().quit()]]:
		var b := Button.new()
		b.text = item[0]
		b.custom_minimum_size = Vector2(0, 38)
		b.pressed.connect(item[1])
		v.add_child(b)

func _build_end_panel() -> void:
	end_panel = PanelContainer.new()
	end_panel.set_anchors_preset(Control.PRESET_CENTER)
	end_panel.position = Vector2(-300, -220)
	end_panel.custom_minimum_size = Vector2(600, 440)
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
	var shape := Input.CURSOR_ARROW
	if mode == "attack_move" or (h.get("enemy", false) and gv.selection_count() > 0):
		shape = Input.CURSOR_CROSS
	elif mode == "place":
		shape = Input.CURSOR_DRAG
	Input.set_default_cursor_shape(shape)

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
	for item in q:
		var b := Button.new()
		b.custom_minimum_size = Vector2(64, 30)
		b.text = (item["name"] as String).left(9)
		b.tooltip_text = "%s (click to cancel)" % item["name"]
		var idx: int = item["index"]
		b.pressed.connect(func(): gv.do_action("cancel", str(idx)))
		if item["index"] == 0:
			var pb := ProgressBar.new()
			pb.show_percentage = false
			pb.custom_minimum_size = Vector2(64, 4)
			pb.value = item["progress"] * 100.0
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

func _on_idle_pressed() -> void:
	if Input.is_key_pressed(KEY_SHIFT):
		gv.select_all_idle_citizens()
	else:
		var c: Vector3 = gv.select_idle_citizen()
		if c.y > -0.5:
			rig.focus(c)
	sel_cache = ""; card_cache = ""

func _refresh_strip() -> void:
	var n: int = gv.idle_citizen_count()
	idle_btn.text = str(n)
	idle_btn.modulate = Color(1, 0.85, 0.5) if n > 0 else Color(0.7, 0.7, 0.7, 0.7)
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
		"complete": notify("%s complete" % e["text"], Color(0.7, 1.0, 0.7))
		"research": notify("Research complete: %s" % e["text"], Color(0.7, 0.85, 1.0))
		"under_attack": notify("We are under attack!", Color(1, 0.4, 0.35))
		"defeated":
			notify("%s has been defeated" % e["text"], Color(1, 0.6, 0.4))
	if main and main.has_node("VFX"):
		main.get_node("VFX").on_event(e)

func _show_end(won: bool) -> void:
	end_panel.visible = true
	for c in end_panel.get_children(): c.queue_free()
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 12)
	end_panel.add_child(v)
	var title := _label("VICTORY" if won else "DEFEAT", 54, ACCENT if won else Color(0.9, 0.35, 0.3), true)
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	v.add_child(title)
	var grid := GridContainer.new()
	grid.columns = 6
	grid.add_theme_constant_override("h_separation", 18)
	for h in ["Player", "Units trained", "Kills", "Losses", "Buildings", "Gathered"]:
		grid.add_child(_label(h, 15, Color(0.7, 0.68, 0.6)))
	for p in gv.players():
		grid.add_child(_label(p["name"], 17, p["color"], true))
		grid.add_child(_label(str(p["trained"]), 17))
		grid.add_child(_label(str(p["kills"]), 17))
		grid.add_child(_label(str(p["lost"]), 17))
		grid.add_child(_label(str(p["built"]), 17))
		grid.add_child(_label(str(p["gathered"]), 17))
	v.add_child(grid)
	var hb := HBoxContainer.new()
	hb.alignment = BoxContainer.ALIGNMENT_CENTER
	hb.add_theme_constant_override("separation", 16)
	for item in [["Play Again", _restart], ["Main Menu", _to_menu], ["Keep Watching", func(): end_panel.visible = false]]:
		var b := Button.new()
		b.text = item[0]
		b.custom_minimum_size = Vector2(150, 42)
		b.pressed.connect(item[1])
		hb.add_child(b)
	v.add_child(hb)

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

func _restart() -> void:
	get_tree().reload_current_scene()

func _to_menu() -> void:
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
	if code == KEY_F10 or code == KEY_P:
		_toggle_menu()
		return
	if code >= KEY_0 and code <= KEY_9:
		var g := code - KEY_0
		if k.ctrl_pressed or k.meta_pressed:
			gv.set_group(g)
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
