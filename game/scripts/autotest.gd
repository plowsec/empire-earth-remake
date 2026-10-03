extends Node
## End-to-end UI test: drives the HUD with synthetic input events.
## Run: godot --path game -- --scene=main --autotest

var gv: Node
var hud: Node
var rig: Node
var step := 0
var wait := 0
var failures := 0
var results: Array = []
var spot := Vector2.ZERO

func _ready() -> void:
	var main := get_parent()
	gv = main.get_node("GameView")
	hud = main.get_node("HUD")
	rig = main.get_node("CameraRig")
	rig.edge_pan = false

func check(name: String, ok: bool) -> void:
	results.append(("PASS " if ok else "FAIL ") + name)
	print(("PASS " if ok else "FAIL ") + name)
	if not ok:
		failures += 1

func key(code: Key, shift := false) -> void:
	var e := InputEventKey.new()
	e.keycode = code
	e.physical_keycode = code
	e.pressed = true
	e.shift_pressed = shift
	Input.parse_input_event(e)
	var u := e.duplicate()
	u.pressed = false
	Input.parse_input_event(u)

func win(p: Vector2) -> Vector2:
	# viewport coords -> window coords (input events are fed in window space)
	return get_viewport().get_final_transform() * p

func mouse(pos: Vector2, button := MOUSE_BUTTON_LEFT, pressed := true) -> void:
	pos = win(pos)
	var m := InputEventMouseMotion.new()
	m.position = pos
	m.global_position = pos
	Input.parse_input_event(m)
	var e := InputEventMouseButton.new()
	e.position = pos
	e.global_position = pos
	e.button_index = button
	e.pressed = pressed
	Input.parse_input_event(e)

func click(pos: Vector2, button := MOUSE_BUTTON_LEFT) -> void:
	mouse(pos, button, true)
	mouse(pos, button, false)

func drag(a: Vector2, b: Vector2) -> void:
	mouse(a, MOUSE_BUTTON_LEFT, true)
	var m := InputEventMouseMotion.new()
	m.position = win(b)
	m.global_position = win(b)
	Input.parse_input_event(m)
	mouse(b, MOUSE_BUTTON_LEFT, false)

func _process(_dt: float) -> void:
	if wait > 0:
		wait -= 1
		return
	var vs := get_viewport().get_visible_rect().size
	match step:
		0:
			wait = 30
		1:
			# box-select everything near the capitol: should pick the 8 citizens
			drag(Vector2(vs.x * 0.2, vs.y * 0.12), Vector2(vs.x * 0.8, vs.y * 0.7))
			wait = 5
		2:
			check("box select citizens", gv.selection_count() >= 6)
			# build a house via the hotkey (Q = first build button), place at an open spot
			key(KEY_Q)
			wait = 3
		3:
			check("placement mode entered", gv.is_placing())
			# scan the screen for a valid spot, then click it like a player would
			var found := Vector2(-1, -1)
			for gy in range(3, 9):
				for gx in range(2, 9):
					var p := Vector2(vs.x * gx / 10.0, vs.y * gy / 11.0)
					gv.placement_update(p)
					if gv.placement_error() == "":
						found = p
						break
				if found.x >= 0:
					break
			if found.x < 0:
				print("no valid spot: ", gv.placement_error())
			spot = found
			var m := InputEventMouseMotion.new()
			m.position = win(spot)
			m.global_position = win(spot)
			Input.parse_input_event(m)
			wait = 3
		4:
			click(spot)
			wait = 30
		5:
			check("house foundation placed", gv.count_owned("house") >= 1)
			if gv.is_placing():
				key(KEY_ESCAPE)
			wait = 60 * 40
		6:
			print(gv.entity_debug("house"))
			print(gv.entity_debug("citizen"))
			check("house constructed (pop cap rose)", gv.player_state()["pop_cap"] >= 30)
			# select the capitol by clicking it and train 2 citizens with its hotkey
			var cp: Vector2 = gv.screen_pos_of("capitol")
			print("capitol at ", cp, " placing=", gv.is_placing(), " mode=", hud.mode, " ", gv.debug_pick(cp), " sel=", gv.selection_count())
			click(cp)
			wait = 5
		7:
			var info: Array = gv.selection_info()
			print("selection after click: ", info.map(func(x): return x["key"]))
			check("capitol selected by click", info.size() == 1 and info[0]["key"] == "capitol")
			key(KEY_Q)
			key(KEY_Q)
			wait = 30
		8:
			check("production queued", gv.production_queue().size() >= 1)
			wait = 60 * 30
		9:
			check("citizens trained", gv.count_owned("citizen") >= 9)
			# select citizens again and right-click a tree: they should gather wood
			drag(Vector2(vs.x * 0.1, vs.y * 0.05), Vector2(vs.x * 0.9, vs.y * 0.75))
			wait = 3
		10:
			var tp: Vector2 = gv.screen_pos_of_resource("tree")
			if tp.x < 0 or tp.y < 0 or tp.x > vs.x or tp.y > vs.y:
				rig.focus(rig.target)
			print("tree at ", tp, " sel=", gv.selection_count(), " ", gv.debug_pick(tp))
			click(tp, MOUSE_BUTTON_RIGHT)
			wait = 60 * 45
		11:
			print("wood now ", gv.player_state()["res"][1], " ", gv.entity_debug("citizen"))
			check("wood gathered", gv.player_state()["res"][1] > 1430)
			print("AUTOTEST DONE failures=%d" % failures)
			get_tree().quit(1 if failures > 0 else 0)
	step += 1
