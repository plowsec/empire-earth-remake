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
	top.position = Vector2(-460, 6)
	top.custom_minimum_size = Vector2(920, 44)
	top.mouse_filter = Control.MOUSE_FILTER_STOP
	root.add_child(top)
	var hb := HBoxContainer.new()
	hb.add_theme_constant_override("separation", 22)
	hb.alignment = BoxContainer.ALIGNMENT_CENTER
	top.add_child(hb)
	for i in 5:
		var box := HBoxContainer.new()
		box.add_theme_constant_override("separation", 6)
		var icon := ColorRect.new()
		icon.custom_minimum_size = Vector2(14, 14)
		icon.color = RES_COLORS[i]
		var icon_wrap := CenterContainer.new()
		icon_wrap.add_child(icon)
		box.add_child(icon_wrap)
		var nm := _label(RES_NAMES[i].capitalize(), 14, Color(0.7, 0.68, 0.62))
		box.add_child(nm)
		var l := _label("0", 20, Color(1, 0.97, 0.9), true)
		l.custom_minimum_size = Vector2(54, 0)
		box.add_child(l)
		res_labels.append(l)
		hb.add_child(box)
	var sep := VSeparator.new()
	hb.add_child(sep)
	pop_label = _label("Pop 0/0", 19, Color(0.85, 0.92, 1.0), true)
	hb.add_child(pop_label)
	clock_label = _label("00:00", 19, ACCENT, true)
	hb.add_child(clock_label)
	var menu_btn := Button.new()
	menu_btn.text = "Menu"
	menu_btn.custom_minimum_size = Vector2(70, 30)
	menu_btn.pressed.connect(_toggle_menu)
	hb.add_child(menu_btn)

	# --- bottom: minimap (left)
	var mm_panel := PanelContainer.new()
	mm_panel.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	mm_panel.position = Vector2(10, -278)
	mm_panel.custom_minimum_size = Vector2(268, 268)
	mm_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	root.add_child(mm_panel)
	minimap = TextureRect.new()
	minimap.texture = gv.minimap_texture()
	minimap.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	minimap.stretch_mode = TextureRect.STRETCH_SCALE
	minimap.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	minimap.custom_minimum_size = Vector2(248, 248)
	minimap.mouse_filter = Control.MOUSE_FILTER_STOP
	minimap.gui_input.connect(_on_minimap_input)
	mm_panel.add_child(minimap)
	minimap_overlay = Control.new()
	minimap_overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	minimap_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	minimap_overlay.draw.connect(_draw_minimap_overlay)
	minimap.add_child(minimap_overlay)

	# --- bottom center: selection + queue
	sel_panel = PanelContainer.new()
	sel_panel.set_anchors_preset(Control.PRESET_CENTER_BOTTOM)
	sel_panel.position = Vector2(-420, -196)
	sel_panel.custom_minimum_size = Vector2(560, 186)
	sel_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	root.add_child(sel_panel)
	var sv := VBoxContainer.new()
	sel_panel.add_child(sv)
	queue_box = HBoxContainer.new()
	queue_box.add_theme_constant_override("separation", 4)
	sv.add_child(queue_box)
	sel_box = VBoxContainer.new()
	sel_box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	sv.add_child(sel_box)

	# --- bottom right: command card
	var card_panel := PanelContainer.new()
	card_panel.set_anchors_preset(Control.PRESET_BOTTOM_RIGHT)
	card_panel.position = Vector2(-470, -196)
	card_panel.custom_minimum_size = Vector2(460, 186)
	card_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	root.add_child(card_panel)
	card_grid = GridContainer.new()
	card_grid.columns = 5
	card_grid.add_theme_constant_override("h_separation", 5)
	card_grid.add_theme_constant_override("v_separation", 5)
	card_panel.add_child(card_grid)

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
	notice_box.position = Vector2(-300, 64)
	notice_box.custom_minimum_size = Vector2(600, 0)
	notice_box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	notice_box.alignment = BoxContainer.ALIGNMENT_BEGIN
	root.add_child(notice_box)

	hover_label = _label("", 15, Color(1, 1, 0.9))
	hover_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(hover_label)

	debug_label = _label("", 13, Color(0.7, 0.9, 0.7))
	debug_label.position = Vector2(12, 60)
	debug_label.visible = false
	root.add_child(debug_label)

	# --- drag rectangle
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
	pop_label.text = "Pop %d/%d" % [st.get("pop", 0), st.get("pop_cap", 0)]
	pop_label.add_theme_color_override("font_color", Color(1, 0.4, 0.35) if st.get("pop", 0) >= st.get("pop_cap", 0) else Color(0.85, 0.92, 1.0))
	var secs: int = st.get("seconds", 0)
	clock_label.text = "%02d:%02d" % [secs / 60, secs % 60]
	refresh_t -= dt
	if refresh_t <= 0.0:
		refresh_t = 0.15
		_refresh_selection()
		_refresh_card()
	minimap_overlay.queue_redraw()
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
		sw.custom_minimum_size = Vector2(6, 46)
		sw.color = x["owner_color"]
		head.add_child(sw)
		var nv := VBoxContainer.new()
		nv.add_child(_label(x["name"], 24, ACCENT, true))
		nv.add_child(_label(x.get("role", ""), 14, Color(0.75, 0.73, 0.68)))
		head.add_child(nv)
		sel_box.add_child(head)
		var hp := ProgressBar.new()
		hp.custom_minimum_size = Vector2(0, 14)
		hp.max_value = max(1, x["max_hp"])
		hp.value = x["hp"]
		hp.show_percentage = false
		sel_box.add_child(hp)
		var stats := "HP %d/%d   Armor %d" % [x["hp"], x["max_hp"], x.get("armor", 0)]
		if x.has("attack"): stats += "   Attack %d   Range %.1f" % [x["attack"], x["range"]]
		if x.has("amount"): stats = "Remaining: %d" % x["amount"]
		if x.has("carry"): stats += "   Carrying %d %s" % [x["carry"], x["carry_res"]]
		if x.has("cargo_max") and x["cargo_max"] > 0: stats += "   Cargo %d/%d" % [x["cargo"], x["cargo_max"]]
		if x.has("fuel"): stats += "   Fuel %d%%" % int(x["fuel"] * 100)
		if x.get("kills", 0) > 0: stats += "   Kills %d" % x["kills"]
		if x.has("complete") and not x["complete"]: stats += "   Under construction %d%%" % int(x["progress"] * 100)
		sel_box.add_child(_label(stats, 15))
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
		b.custom_minimum_size = Vector2(40, 40)
		b.text = (x["name"] as String).substr(0, 3)
		b.tooltip_text = "%s  %d/%d" % [x["name"], x["hp"], x["max_hp"]]
		var f: float = float(x["hp"]) / max(1.0, float(x["max_hp"]))
		b.add_theme_color_override("font_color", Color(1, 1, 1).lerp(Color(1, 0.3, 0.2), 1.0 - f))
		var id: int = x["id"]
		b.pressed.connect(func(): gv.select_ids(PackedInt64Array([id])))
		grid.add_child(b)

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
		btn.custom_minimum_size = Vector2(84, 52)
		btn.clip_text = true
		var label: String = b["label"]
		btn.text = label if label.length() <= 12 else label.left(11) + "."
		btn.disabled = not b["enabled"] and b["action"] in ["train", "research"]
		if not b["enabled"]:
			btn.modulate = Color(1, 0.75, 0.7)
		var hk: String = b["hotkey"]
		if hk != "":
			btn.text = "%s\n[%s]" % [btn.text, hk]
		var a: String = b["action"]
		var k: String = b["key"]
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
	tooltip_label.text = t
	tooltip.visible = true
	tooltip.reset_size()
	tooltip.position = btn.global_position + Vector2(-340 + btn.size.x, -tooltip.size.y - 10)

func _do_action(action: String, key: String) -> void:
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
			_do_action(b["action"], b["key"])
			return
	if code == KEY_A and gv.selection_count() > 0:
		mode = "attack_move"
