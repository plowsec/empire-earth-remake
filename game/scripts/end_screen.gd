extends Control
## Empire Earth-style post-game screen: victory/defeat banner, score breakdown,
## military / economy tables and history charts for every player.

const ACCENT := Color(0.95, 0.78, 0.38)
const DIM := Color(0.68, 0.66, 0.6)
const RES_NAMES := ["Food", "Wood", "Stone", "Gold", "Iron"]
const RES_COLORS := [Color(0.85, 0.35, 0.3), Color(0.55, 0.38, 0.22), Color(0.7, 0.7, 0.68), Color(0.95, 0.8, 0.3), Color(0.55, 0.62, 0.72)]
const CHARTS := [["score", "Score"], ["population", "Population"], ["military", "Army"], ["citizens", "Citizens"], ["gathered", "Resources gathered"], ["buildings", "Buildings"], ["kills", "Kills"], ["technologies", "Technologies"]]

var stats: Dictionary
var won := false
var font_title: Font
var font_bold: Font
var font_body: Font
var body: Control
var tab_buttons: Array[Button] = []
var chart: Chart
var on_play_again: Callable
var on_menu: Callable
var on_watch: Callable

class Chart extends Control:
	var times: Array = []
	var series: Array = []      # per player: PackedFloat32Array
	var colors: Array = []
	var names: Array = []
	var font: Font
	var reveal := 0.0          # draw-in animation 0..1

	func _process(dt: float) -> void:
		if reveal < 1.0:
			reveal = min(1.0, reveal + dt * 0.9)
			queue_redraw()

	func _draw() -> void:
		var r := Rect2(Vector2(60, 16), size - Vector2(80, 56))
		draw_rect(Rect2(Vector2.ZERO, size), Color(0, 0, 0, 0.35))
		var top := 1.0
		for s in series:
			for v in s:
				top = max(top, v)
		top = _nice(top)
		# grid + labels
		for k in 5:
			var y := r.position.y + r.size.y * (1.0 - k / 4.0)
			draw_line(Vector2(r.position.x, y), Vector2(r.end.x, y), Color(1, 1, 1, 0.07), 1.0)
			draw_string(font, Vector2(4, y + 5), _fmt(top * k / 4.0), HORIZONTAL_ALIGNMENT_LEFT, 54, 13, Color(1, 1, 1, 0.45))
		var n: int = times.size()
		if n < 2:
			draw_string(font, r.get_center() - Vector2(110, 0), "Not enough history yet", HORIZONTAL_ALIGNMENT_LEFT, -1, 18, Color(1, 1, 1, 0.5))
			return
		var t_end: float = max(1.0, float(times[n - 1]))
		for k in 7:
			var t := t_end * k / 6.0
			var x := r.position.x + r.size.x * k / 6.0
			draw_line(Vector2(x, r.end.y), Vector2(x, r.end.y + 4), Color(1, 1, 1, 0.3), 1.0)
			draw_string(font, Vector2(x - 18, r.end.y + 20), "%d:%02d" % [int(t) / 60, int(t) % 60], HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color(1, 1, 1, 0.45))
		var upto := int(ceil((n - 1) * reveal)) + 1
		for p in series.size():
			var s: PackedFloat32Array = series[p]
			var pts := PackedVector2Array()
			for i in min(upto, s.size()):
				var x := r.position.x + r.size.x * float(times[i]) / t_end
				var y := r.position.y + r.size.y * (1.0 - s[i] / top)
				pts.append(Vector2(x, y))
			if pts.size() >= 2:
				# soft glow under a crisp line
				draw_polyline(pts, Color(colors[p], 0.25), 7.0, true)
				draw_polyline(pts, colors[p], 2.5, true)
				var last: Vector2 = pts[pts.size() - 1]
				draw_circle(last, 4.5, colors[p])
				if reveal >= 1.0:
					draw_string(font, last + Vector2(8, 5), _fmt(s[pts.size() - 1]), HORIZONTAL_ALIGNMENT_LEFT, -1, 14, colors[p])

	func _nice(v: float) -> float:
		var e := pow(10.0, floor(log(v) / log(10.0)))
		for m in [1.0, 2.0, 2.5, 5.0, 10.0]:
			if v <= m * e:
				return m * e
		return v

	func _fmt(v: float) -> String:
		if v >= 1000000:
			return "%.1fM" % (v / 1000000.0)
		if v >= 10000:
			return "%dk" % int(v / 1000.0)
		return str(int(round(v)))

func setup(data: Dictionary, victory: bool, fonts: Array) -> void:
	stats = data
	won = victory
	font_title = fonts[0]
	font_bold = fonts[1]
	font_body = fonts[2]
	_build()

func _label(text: String, size: int, color := Color(0.92, 0.9, 0.84), font: Font = null) -> Label:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", size)
	l.add_theme_color_override("font_color", color)
	if font:
		l.add_theme_font_override("font", font)
	elif font_body:
		l.add_theme_font_override("font", font_body)
	return l

func _build() -> void:
	set_anchors_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_STOP
	var shade := ColorRect.new()
	shade.color = Color(0.02, 0.02, 0.03, 0.93)
	shade.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(shade)
	var margin := MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "right"]:
		margin.add_theme_constant_override("margin_" + side, 70)
	margin.add_theme_constant_override("margin_top", 64)
	margin.add_theme_constant_override("margin_bottom", 130)
	add_child(margin)
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 14)
	margin.add_child(v)

	# banner
	var title := _label("VICTORY" if won else "DEFEAT", 76, ACCENT if won else Color(0.9, 0.32, 0.28), font_title)
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title.add_theme_constant_override("shadow_offset_y", 4)
	title.add_theme_color_override("font_shadow_color", Color(0, 0, 0, 0.8))
	v.add_child(title)
	var secs: int = stats.get("seconds", 0)
	var sub := "Your empire stands supreme" if won else "Your empire has fallen"
	var subl := _label("%s  ·  %d:%02d:%02d" % [sub, secs / 3600, (secs / 60) % 60, secs % 60], 22, DIM, font_bold)
	subl.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	v.add_child(subl)
	var rule := ColorRect.new()
	rule.custom_minimum_size = Vector2(0, 2)
	rule.color = Color(ACCENT, 0.6)
	v.add_child(rule)

	# tabs
	var tabs := HBoxContainer.new()
	tabs.alignment = BoxContainer.ALIGNMENT_CENTER
	tabs.add_theme_constant_override("separation", 8)
	v.add_child(tabs)
	for t in ["Summary", "Military", "Economy", "Graphs"]:
		var b := Button.new()
		b.text = t
		b.toggle_mode = true
		b.custom_minimum_size = Vector2(150, 38)
		b.add_theme_font_override("font", font_bold)
		b.add_theme_font_size_override("font_size", 20)
		b.pressed.connect(_show_tab.bind(t))
		tabs.add_child(b)
		tab_buttons.append(b)

	body = MarginContainer.new()
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	v.add_child(body)

	var hb := HBoxContainer.new()
	hb.alignment = BoxContainer.ALIGNMENT_CENTER
	hb.add_theme_constant_override("separation", 16)
	for item in [["Play Again", func(): if on_play_again: on_play_again.call()], ["Main Menu", func(): if on_menu: on_menu.call()], ["Keep Watching", func(): if on_watch: on_watch.call()]]:
		var b := Button.new()
		b.text = item[0]
		b.custom_minimum_size = Vector2(170, 44)
		b.add_theme_font_override("font", font_bold)
		b.add_theme_font_size_override("font_size", 20)
		b.pressed.connect(item[1])
		hb.add_child(b)
	v.add_child(hb)
	_show_tab("Summary")

func _show_tab(t: String) -> void:
	for b in tab_buttons:
		b.button_pressed = b.text == t
	for c in body.get_children():
		c.queue_free()
	match t:
		"Summary": body.add_child(_summary())
		"Military": body.add_child(_table([["Units trained", "trained"], ["Kills", "kills"], ["Units lost", "lost"], ["Buildings razed", "razed"], ["Peak army", "peak_army"], ["Military score", "military"]]))
		"Economy": body.add_child(_economy())
		"Graphs": body.add_child(_graphs())

func _players() -> Array:
	var ps: Array = stats.get("players", [])
	ps.sort_custom(func(a, b): return a["score"] > b["score"])
	return ps

func _summary() -> Control:
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 18)
	var ps := _players()
	var top: float = max(1.0, float(ps[0]["score"])) if ps.size() > 0 else 1.0
	for i in ps.size():
		var p: Dictionary = ps[i]
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 16)
		var rank := _label("#%d" % (i + 1), 34, ACCENT if i == 0 else DIM, font_title)
		rank.custom_minimum_size = Vector2(70, 0)
		row.add_child(rank)
		var col := VBoxContainer.new()
		col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var name_l := _label("%s%s" % [p["name"], "  —  victorious" if p["won"] else ("  —  defeated" if p["defeated"] else "")], 26, p["color"], font_bold)
		col.add_child(name_l)
		# stacked score bar: military / economy / technology
		var bar := HBoxContainer.new()
		bar.add_theme_constant_override("separation", 0)
		bar.custom_minimum_size = Vector2(0, 22)
		var full := 900.0
		for part in [["military", Color(0.85, 0.32, 0.28)], ["economy", Color(0.9, 0.75, 0.3)], ["technology", Color(0.4, 0.65, 0.95)]]:
			var seg := ColorRect.new()
			seg.color = part[1]
			seg.custom_minimum_size = Vector2(full * float(p[part[0]]) / top, 22)
			bar.add_child(seg)
		col.add_child(bar)
		col.add_child(_label("Military %d   ·   Economy %d   ·   Technology %d" % [p["military"], p["economy"], p["technology"]], 16, DIM))
		row.add_child(col)
		var score := _label(str(p["score"]), 40, ACCENT, font_title)
		score.custom_minimum_size = Vector2(160, 0)
		score.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		row.add_child(score)
		v.add_child(row)
	v.add_child(_awards())
	return v

func _awards() -> Control:
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 10)
	var rule := ColorRect.new()
	rule.custom_minimum_size = Vector2(0, 1)
	rule.color = Color(ACCENT, 0.35)
	box.add_child(rule)
	box.add_child(_label("HONOURS", 22, ACCENT, font_bold))
	var grid := GridContainer.new()
	grid.columns = 3
	grid.add_theme_constant_override("h_separation", 40)
	grid.add_theme_constant_override("v_separation", 10)
	var ps: Array = stats.get("players", [])
	var awards := [
		["Warlord", "most enemies destroyed", "kills"],
		["Conqueror", "most buildings razed", "razed"],
		["Tycoon", "most resources gathered", "_gathered"],
		["Builder", "most buildings raised", "built"],
		["Marshal", "largest army fielded", "peak_army"],
		["Visionary", "most technologies", "techs"],
	]
	for a in awards:
		var best = null
		var best_v := -1
		for p in ps:
			var val := 0
			if a[2] == "_gathered":
				for g in p["gathered"]: val += int(g)
			else:
				val = int(p[a[2]])
			if val > best_v:
				best_v = val
				best = p
		if best == null or best_v <= 0:
			continue
		var cell := VBoxContainer.new()
		cell.add_child(_label("%s  —  %s" % [a[0], best["name"]], 22, best["color"], font_bold))
		cell.add_child(_label("%s (%d)" % [a[1], best_v], 15, DIM))
		grid.add_child(cell)
	box.add_child(grid)
	return box

func _table(cols: Array) -> Control:
	var grid := GridContainer.new()
	grid.columns = cols.size() + 1
	grid.add_theme_constant_override("h_separation", 34)
	grid.add_theme_constant_override("v_separation", 14)
	grid.add_child(_label("", 18))
	for c in cols:
		grid.add_child(_label(c[0], 18, DIM, font_bold))
	var ps := _players()
	# highlight the best value in each column
	var best := {}
	for c in cols:
		var b := -1
		for p in ps:
			b = max(b, int(p[c[1]]))
		best[c[1]] = b
	for p in ps:
		grid.add_child(_label(p["name"], 24, p["color"], font_bold))
		for c in cols:
			var val := int(p[c[1]])
			grid.add_child(_label(str(val), 24, ACCENT if val == best[c[1]] and val > 0 else Color(0.92, 0.9, 0.84), font_bold))
	var cc := CenterContainer.new()
	cc.add_child(grid)
	return cc

func _economy() -> Control:
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 22)
	var ps := _players()
	var top := 1.0
	for p in ps:
		var tot := 0
		for g in p["gathered"]:
			tot += int(g)
		top = max(top, tot)
	for p in ps:
		var row := VBoxContainer.new()
		var total := 0
		for g in p["gathered"]:
			total += int(g)
		row.add_child(_label("%s   ·   %d resources gathered   ·   %d buildings   ·   %d technologies" % [p["name"], total, p["built"], p["techs"]], 22, p["color"], font_bold))
		var bar := HBoxContainer.new()
		bar.add_theme_constant_override("separation", 0)
		for r in 5:
			var seg := ColorRect.new()
			seg.color = RES_COLORS[r]
			seg.custom_minimum_size = Vector2(1000.0 * float(p["gathered"][r]) / top, 26)
			seg.tooltip_text = "%s: %d" % [RES_NAMES[r], p["gathered"][r]]
			bar.add_child(seg)
		row.add_child(bar)
		var legend := HBoxContainer.new()
		legend.add_theme_constant_override("separation", 22)
		for r in 5:
			legend.add_child(_label("%s %d" % [RES_NAMES[r], p["gathered"][r]], 16, RES_COLORS[r]))
		row.add_child(legend)
		v.add_child(row)
	return v

func _graphs() -> Control:
	var v := VBoxContainer.new()
	v.add_theme_constant_override("separation", 10)
	var pick := HBoxContainer.new()
	pick.alignment = BoxContainer.ALIGNMENT_CENTER
	pick.add_theme_constant_override("separation", 6)
	v.add_child(pick)
	chart = Chart.new()
	chart.font = font_body
	chart.size_flags_vertical = Control.SIZE_EXPAND_FILL
	chart.custom_minimum_size = Vector2(0, 360)
	v.add_child(chart)
	var legend := HBoxContainer.new()
	legend.alignment = BoxContainer.ALIGNMENT_CENTER
	legend.add_theme_constant_override("separation", 30)
	for p in stats.get("players", []):
		legend.add_child(_label("●  " + p["name"], 18, p["color"], font_bold))
	v.add_child(legend)
	var buttons: Array[Button] = []
	for c in CHARTS:
		var b := Button.new()
		b.text = c[1]
		b.toggle_mode = true
		b.add_theme_font_override("font", font_bold)
		b.add_theme_font_size_override("font_size", 16)
		pick.add_child(b)
		buttons.append(b)
		b.pressed.connect(func():
			for o in buttons: o.button_pressed = o == b
			_set_chart(c[0]))
	buttons[0].button_pressed = true
	_set_chart("score")
	return v

func _set_chart(metric: String) -> void:
	var ps: Array = stats.get("players", [])
	chart.times = stats.get("times", [])
	chart.series = stats.get("series", {}).get(metric, [])
	chart.colors = ps.map(func(p): return p["color"])
	chart.reveal = 0.0
	chart.queue_redraw()
