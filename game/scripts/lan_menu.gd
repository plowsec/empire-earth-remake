extends PanelContainer
## LAN multiplayer: find / host / join games, then the lobby (seats, AIs, settings,
## chat). Starting the match hands the connection to the game scene.

const ACCENT := Color(0.95, 0.78, 0.38)
const DIFFS := ["Easy", "Normal", "Hard", "Hardest"]

var lan = null
var menu: Node          # the main menu (fonts, helpers, panel switching)
var body: VBoxContainer
var mode := "browse"    # browse | lobby
var name_edit: LineEdit
var addr_edit: LineEdit
var games_box: VBoxContainer
var error_label: Label
var slots_box: VBoxContainer
var chat_label: Label
var chat_edit: LineEdit
var settings_box: VBoxContainer
var start_btn: Button
var status_label: Label
var _refresh := 0.0
var _slots_key := ""
var _role := ""

func setup(m: Node) -> void:
	menu = m
	lan = ClassDB.instantiate("Lan")
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.03, 0.035, 0.045, 0.9)
	sb.border_color = Color(ACCENT, 0.5)
	sb.set_border_width_all(1)
	sb.set_corner_radius_all(4)
	sb.content_margin_left = 30; sb.content_margin_right = 30
	sb.content_margin_top = 24; sb.content_margin_bottom = 24
	add_theme_stylebox_override("panel", sb)
	custom_minimum_size = Vector2(720, 600)
	body = VBoxContainer.new()
	body.add_theme_constant_override("separation", 12)
	add_child(body)
	_show_browse()

func _lbl(t: String, size := 20, color := Color(0.88, 0.86, 0.8)) -> Label:
	return menu._lbl(t, size, color)

func _button(t: String, cb: Callable, w := 160) -> Button:
	var b := Button.new()
	b.text = t
	b.custom_minimum_size = Vector2(w, 40)
	b.add_theme_font_override("font", menu.font_bold)
	b.add_theme_font_size_override("font_size", 20)
	b.pressed.connect(cb)
	return b

func _clear() -> void:
	for c in body.get_children():
		c.queue_free()

# ---------------------------------------------------------------- browse

func _show_browse() -> void:
	mode = "browse"
	_clear()
	body.add_child(menu._lbl("LAN GAME", 34, ACCENT, menu.font_title))
	var nrow := HBoxContainer.new()
	nrow.add_theme_constant_override("separation", 12)
	nrow.add_child(_lbl("Your name"))
	name_edit = LineEdit.new()
	name_edit.text = Settings.player_name if "player_name" in Settings else "Commander"
	name_edit.custom_minimum_size = Vector2(260, 38)
	nrow.add_child(name_edit)
	nrow.add_child(_button("Host a game", _host, 200))
	body.add_child(nrow)
	body.add_child(_lbl("Games on your network", 22, ACCENT))
	var sc := ScrollContainer.new()
	sc.custom_minimum_size = Vector2(0, 230)
	sc.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	games_box = VBoxContainer.new()
	games_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	sc.add_child(games_box)
	body.add_child(sc)
	var arow := HBoxContainer.new()
	arow.add_theme_constant_override("separation", 12)
	arow.add_child(_lbl("Join by address"))
	addr_edit = LineEdit.new()
	addr_edit.placeholder_text = "192.168.1.20"
	addr_edit.custom_minimum_size = Vector2(260, 38)
	arow.add_child(addr_edit)
	arow.add_child(_button("Join", func(): _join(addr_edit.text), 120))
	body.add_child(arow)
	error_label = _lbl("", 18, Color(1, 0.5, 0.4))
	body.add_child(error_label)
	var back := HBoxContainer.new()
	back.alignment = BoxContainer.ALIGNMENT_END
	back.add_child(_button("Back", func(): lan.leave(); menu._show(null), 150))
	body.add_child(back)

func _host() -> void:
	_save_name()
	var err: String = lan.host(name_edit.text.strip_edges())
	if err != "":
		error_label.text = err
		return
	_show_lobby()

func _join(addr: String) -> void:
	_save_name()
	if addr.strip_edges() == "":
		error_label.text = "Enter the host's address"
		return
	var err: String = lan.join(addr.strip_edges(), name_edit.text.strip_edges())
	if err != "":
		error_label.text = err
		return
	_show_lobby()

func _save_name() -> void:
	if "player_name" in Settings:
		Settings.player_name = name_edit.text.strip_edges()
		Settings.save()

# ---------------------------------------------------------------- lobby

func _show_lobby() -> void:
	mode = "lobby"
	_slots_key = ""
	_clear()
	body.add_child(menu._lbl("LOBBY", 34, ACCENT, menu.font_title))
	status_label = _lbl("", 18, Color(0.7, 0.7, 0.66))
	body.add_child(status_label)
	slots_box = VBoxContainer.new()
	slots_box.add_theme_constant_override("separation", 6)
	body.add_child(slots_box)
	settings_box = VBoxContainer.new()
	body.add_child(settings_box)
	chat_label = _lbl("", 16, Color(0.8, 0.8, 0.75))
	chat_label.custom_minimum_size = Vector2(0, 110)
	chat_label.vertical_alignment = VERTICAL_ALIGNMENT_BOTTOM
	body.add_child(chat_label)
	var crow := HBoxContainer.new()
	chat_edit = LineEdit.new()
	chat_edit.placeholder_text = "Say something…"
	chat_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	chat_edit.text_submitted.connect(func(t): if t.strip_edges() != "": lan.say(t.strip_edges()); chat_edit.text = "")
	crow.add_child(chat_edit)
	body.add_child(crow)
	error_label = _lbl("", 18, Color(1, 0.5, 0.4))
	body.add_child(error_label)
	var brow := HBoxContainer.new()
	brow.alignment = BoxContainer.ALIGNMENT_END
	brow.add_theme_constant_override("separation", 12)
	brow.add_child(_button("Leave", func(): lan.leave(); _show_browse(), 150))
	start_btn = _button("Start Game", _start, 200)
	brow.add_child(start_btn)
	body.add_child(brow)

func _start() -> void:
	if lan.launch(randi() % 1000000):
		Engine.set_meta("lan_start", true)
		get_tree().change_scene_to_file("res://scenes/main.tscn")

func _rebuild_slots(st: Dictionary) -> void:
	for c in slots_box.get_children():
		c.queue_free()
	var host: bool = st.get("role", "") == "host"
	var slots: Array = st.get("slots", [])
	for i in slots.size():
		var sl: Dictionary = slots[i]
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 12)
		var dot := _lbl("●", 22, menu.PLAYER_COLORS[i % menu.PLAYER_COLORS.size()] if "PLAYER_COLORS" in menu else ACCENT)
		row.add_child(dot)
		var nm := _lbl(sl["name"] + ("  (you)" if sl["me"] else ""), 22)
		nm.custom_minimum_size = Vector2(260, 0)
		row.add_child(nm)
		# team (same team = allies at the start)
		if host:
			var tb := OptionButton.new()
			for t in 4:
				tb.add_item("Team %d" % (t + 1), t)
			tb.select(clampi(int(sl.get("team", i)), 0, 3))
			var tslot := i
			tb.item_selected.connect(func(t): lan.set_team(tslot, t))
			row.add_child(tb)
		else:
			row.add_child(_lbl("Team %d" % (int(sl.get("team", i)) + 1), 18, Color(0.8, 0.8, 0.7)))
		if sl["human"]:
			row.add_child(_lbl("Human", 18, Color(0.7, 0.85, 1.0)))
		elif host:
			var ob := OptionButton.new()
			for d in DIFFS.size():
				ob.add_item(DIFFS[d], d)
			ob.select(int(sl["difficulty"]))
			var slot := i
			ob.item_selected.connect(func(d): lan.set_ai_difficulty(slot, d))
			row.add_child(ob)
			row.add_child(_button("Remove", func(): lan.remove_ai(slot), 110))
		else:
			row.add_child(_lbl("AI · " + DIFFS[clampi(int(sl["difficulty"]), 0, 3)], 18, Color(0.9, 0.75, 0.5)))
		slots_box.add_child(row)
	if host and slots.size() < 4:
		slots_box.add_child(_button("+ Add AI", func(): lan.add_ai(2), 160))
	# settings
	for c in settings_box.get_children():
		c.queue_free()
	var s: Dictionary = st.get("settings", {})
	var opts := [
		["Map size", "map_size", ["Small", "Medium", "Large"], [0, 1, 2]],
		["Resources", "resources", ["Standard", "High", "Very high"], [100, 150, 220]],
		["Starting resources", "start_res", ["Low", "Standard", "High", "Deathmatch"], [600, 1500, 3000, 10000]],
		["Population limit", "pop_limit", ["200", "300", "500", "1000", "2000", "3000"], [200, 300, 500, 1000, 2000, 3000]],
	]
	for o in opts:
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 12)
		var l := _lbl(o[0], 18, Color(0.75, 0.73, 0.68))
		l.custom_minimum_size = Vector2(220, 0)
		row.add_child(l)
		var cur := int(s.get(o[1], 0))
		var vals: Array = o[3]
		if host:
			var ob := OptionButton.new()
			for k in o[2].size():
				ob.add_item(o[2][k], k)
			ob.select(max(vals.find(cur), 0))
			var key: String = o[1]
			ob.item_selected.connect(func(k): lan.set_setting(key, vals[k]))
			row.add_child(ob)
		else:
			var k: int = vals.find(cur)
			row.add_child(_lbl(o[2][k] if k >= 0 else str(cur), 18))
		settings_box.add_child(row)

func _process(dt: float) -> void:
	if lan == null:
		return
	_refresh -= dt
	if mode == "browse":
		if _refresh <= 0.0:
			_refresh = 1.0
			for c in games_box.get_children():
				c.queue_free()
			var games: Array = lan.games()
			if games.is_empty():
				games_box.add_child(_lbl("No games found yet. Ask the host to open one, or join by address.", 17, Color(0.6, 0.6, 0.56)))
			for g in games:
				var addr: String = g["addr"]
				games_box.add_child(_button("%s   ·   %d players   ·   %s" % [g["name"], g["players"], addr], func(): _join(addr), 620))
		return
	var st: Dictionary = lan.pump()
	if st.get("started", false):
		Engine.set_meta("lan_start", true)
		get_tree().change_scene_to_file("res://scenes/main.tscn")
		return
	_auto_start(st)
	var role: String = st.get("role", "none")
	var key := str(st.get("slots", [])) + str(st.get("settings", {}))
	if key != _slots_key or role != _role:
		_slots_key = key
		_role = role
		_rebuild_slots(st)
	start_btn.visible = role == "host"
	status_label.text = ("You are hosting. Others can find this game on the network or join your IP." if role == "host" else "Waiting for %s to start the game…" % st.get("host_name", "the host"))
	var chat: PackedStringArray = st.get("chat", PackedStringArray())
	chat_label.text = "\n".join(chat)
	var err: String = st.get("error", "")
	error_label.text = err
	if role == "none" and err != "":
		_show_browse()
		error_label.text = err

# ---------------------------------------------------------------- test hooks
# --lanhost: open a game and start it once another human has joined
# --lanjoin=ADDR: join that game
var _auto_host := false

var _autotested := false
func autotest(args: PackedStringArray) -> void:
	if _autotested:
		return
	_autotested = true
	for a in args:
		if a == "--lanhost":
			_auto_host = true
			name_edit.text = "HostTest"
			_host()
		elif a.begins_with("--lanjoin="):
			name_edit.text = "JoinTest"
			_join(a.substr(10))

func _auto_start(st: Dictionary) -> void:
	if not _auto_host:
		return
	var humans := 0
	for sl in st.get("slots", []):
		if sl["human"]:
			humans += 1
	if humans >= 2:
		_auto_host = false
		print("LANTEST host starting with ", humans, " humans")
		_start()
