extends SceneTree
## Run with godot --path game --script res://scripts/supporttest.gd
## Exercises custom skirmish teams, worker actions and live charts through the real UI.
var failures := 0
var output := ProjectSettings.globalize_path("res://../build/verification")

func _initialize() -> void:
	call_deferred("run")

func check(label: String, ok: bool) -> void:
	print(("PASS " if ok else "FAIL ") + label)
	if not ok: failures += 1

func frames(n := 6) -> void:
	for i in n: await process_frame

func snapshot(name: String) -> void:
	if DisplayServer.get_name() == "headless": return
	await RenderingServer.frame_post_draw
	root.get_texture().get_image().save_png(output.path_join(name + ".png"))

func run() -> void:
	create_timer(120).timeout.connect(func(): print("FAIL support UI test timed out"); quit(1))
	DirAccess.make_dir_recursive_absolute(output)
	Engine.set_meta("support_test", true)
	var menu = load("res://scenes/menu.tscn").instantiate()
	root.add_child(menu)
	current_scene = menu
	Engine.remove_meta("support_test")
	await frames()
	menu._show(menu.skirmish_panel)
	for option in menu.skirmish_panel.find_children("*", "OptionButton", true, false):
		if option.get_item_text(0) == "1 AI":
			option.select(2)
			option.item_selected.emit(2)
		elif option.get_item_text(0) == "Free for all":
			option.select(3)
			option.item_selected.emit(3)
	menu.opts["team_0"] = 0
	menu.opts["team_1"] = 1
	menu.opts["team_2"] = 0
	menu.opts["team_3"] = 1
	menu._update_alliances()
	check("custom alliance selectors shown", menu.alliance_box.visible and menu.alliance_rows.size() == 4)
	check("custom teams can start", not menu.skirmish_start.disabled)
	# Use each actual seat selector and its signal.
	for i in 4:
		var pick: OptionButton = menu.alliance_rows[i].get_child(1)
		pick.select(0)
		pick.item_selected.emit(0)
	check("all allied match is blocked", menu.skirmish_start.disabled)
	for i in [1, 3]:
		var pick: OptionButton = menu.alliance_rows[i].get_child(1)
		pick.select(1)
		pick.item_selected.emit(1)
	await frames()
	await snapshot("custom-alliances")
	var settings = root.get_node("Settings")
	var previous: Dictionary = settings.last_skirmish.duplicate(true)
	menu._start()
	settings.last_skirmish = previous
	settings.save()
	await frames(30)
	var main = current_scene
	var gv = main.get_node("GameView")
	var hud = main.get_node("HUD")
	var ps: Array = gv.players()
	check("selected teams reach simulation", ps.size() == 4 and ps[2]["allied"] and not ps[1]["allied"] and not ps[3]["allied"])
	gv.set_paused(true)
	gv.select_idle_citizens(8)
	check("idle batch selects citizens", gv.selection_count() == 8)
	gv.dispatch_idle_miners()
	gv.set_paused(false)
	await frames(30)
	var info: Array = gv.selection_info()
	check("mine button dispatches idle citizens", info.any(func(e): return e["order"] == "Gather"))
	hud._open_charts()
	await frames(20)
	check("live charts open", is_instance_valid(hud.charts_panel) and hud.charts_panel.live)
	var screen = hud.charts_panel
	check("charts fill the viewport", screen.size.x >= root.get_visible_rect().size.x - 1.0 and screen.size.y >= root.get_visible_rect().size.y - 1.0)
	check("resource and unit charts available", screen.stats["series"].has("gold") and screen.stats["series"].has("units"))
	var seconds: int = screen.stats["seconds"]
	for button in screen.find_children("*", "Button", true, false):
		if button.text == "Gold": button.pressed.emit()
	await create_timer(1.2).timeout
	check("charts refresh while playing", screen.stats["seconds"] > seconds and screen.current_metric == "gold")
	await snapshot("live-resource-charts")
	var event := InputEventKey.new()
	event.keycode = KEY_ESCAPE
	hud._on_key(event)
	await frames()
	check("Escape closes charts", not is_instance_valid(hud.charts_panel))
	hud._toggle_menu()
	hud._open_charts()
	await frames()
	hud._close_charts()
	check("charts preserve pause menu", hud.menu_panel.visible)
	hud._toggle_menu()
	var vfx = main.get_node("VFX")
	var rig = main.get_node("CameraRig")
	var pos: Vector3 = gv.home_position() + Vector3(22, 0, 20)
	pos.y = gv.height_at(pos.x, pos.z)
	rig.focus(pos, true)
	rig.dist = 55.0
	rig._dist_goal = 55.0
	vfx.on_event({"kind": "carpet_bomb", "pos": pos, "to": pos, "size": 22.0, "text": "", "dmg": 7, "mine": true})
	vfx.on_event({"kind": "loot", "pos": pos + Vector3.UP * 4.0, "to": pos, "size": 72.0, "text": "+25 gold  +47 iron", "dmg": 0, "mine": true})
	await create_timer(0.25).timeout
	check("loot popup displays resource reward", vfx.loot_labels.any(func(l): return is_instance_valid(l) and l.text.contains("+25 gold")))
	check("loot sound loaded", vfx.audio.cache.get("loot") != null)
	check("bombed area leaves a ground scar", vfx.decals.any(func(d): return d.modulate.a > 0.9 and d.size.x >= 44.0))
	await snapshot("carpet-bomb-and-loot")
	await create_timer(2.0).timeout
	check("loot animation cleans up", vfx.loot_labels.all(func(l): return not is_instance_valid(l)))
	await snapshot("bombed-ground")
	main.queue_free()
	await frames()
	print("SUPPORT UI: %d failures" % failures)
	quit(1 if failures else 0)
