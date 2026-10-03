extends "res://scripts/autotest.gd"
## Opt-in input + screenshot regression run: --featuretest --seed=5 --reveal

var output := ProjectSettings.globalize_path("res://../build/verification")

func _process(_dt: float) -> void:
	pass

func frames(n := 6) -> void:
	for i in n:
		await get_tree().process_frame

func snapshot(name: String) -> void:
	await RenderingServer.frame_post_draw
	get_viewport().get_texture().get_image().save_png(output.path_join(name + ".png"))

func modifier(code: Key, pressed: bool) -> void:
	var e := InputEventKey.new()
	e.keycode = code
	e.physical_keycode = code
	e.pressed = pressed
	Input.parse_input_event(e)

func card_button(label: String) -> Button:
	var card: Array = gv.command_card()
	for i in card.size():
		if card[i]["label"] == label:
			return hud.card_grid.get_child(i) as Button
	return null

func _ready() -> void:
	super._ready()
	get_tree().create_timer(110.0).timeout.connect(func():
		print("FAIL feature test timed out")
		get_tree().quit(1))
	DirAccess.make_dir_recursive_absolute(output)
	await frames(30)
	if "--effects-only" in OS.get_cmdline_user_args():
		await preview_nuclear()
		return
	gv.set_speed(1.0)
	# Idle button and the real group hotkey/click paths.
	var idle: int = gv.idle_citizen_count()
	click(hud.idle_btn.get_global_rect().get_center())
	await frames()
	check("idle button selects one citizen", gv.selection_count() == 1)
	var first: int = gv.selection_info()[0]["id"]
	click(hud.idle_btn.get_global_rect().get_center())
	await frames()
	check("idle button cycles", gv.selection_info()[0]["id"] != first)
	modifier(KEY_SHIFT, true)
	click(hud.idle_btn.get_global_rect().get_center())
	modifier(KEY_SHIFT, false)
	await frames()
	check("Shift idle selects all", gv.selection_count() == idle)
	var group := InputEventKey.new()
	group.keycode = KEY_1
	group.physical_keycode = KEY_1
	group.pressed = true
	group.ctrl_pressed = true
	Input.parse_input_event(group)
	group = group.duplicate()
	group.pressed = false
	Input.parse_input_event(group)
	await frames(20)
	check("control group bar populated", hud.group_bar.get_child_count() == 1)
	click(gv.screen_pos_of("capitol"))
	await frames()
	click(hud.group_bar.get_child(0).get_global_rect().get_center())
	await frames()
	check("group click recalls units", gv.selection_count() == idle)
	rig.target += Vector3(15, 0, 0)
	click(hud.group_bar.get_child(0).get_global_rect().get_center())
	await frames(2)
	click(hud.group_bar.get_child(0).get_global_rect().get_center())
	await frames()
	var selected_center: Vector3 = gv.selection_center()
	check("group double click centers", Vector2(rig.target.x, rig.target.z).distance_to(Vector2(selected_center.x, selected_center.z)) < 0.1)
	check("compact action strip", hud.card_grid.size.y <= 100)
	check("selection above actions", hud.sel_panel.get_global_rect().end.y < hud.card_grid.global_position.y)
	check("minimap above action strip", hud.minimap.get_global_rect().end.y < hud.card_grid.global_position.y)
	await snapshot("idle-and-groups")
	# Camera events run through the same unhandled-input path as user gestures.
	var old_target: Vector3 = rig.target
	var old_yaw: float = rig._yaw_goal
	mouse(Vector2(600, 300), MOUSE_BUTTON_MIDDLE, true)
	var motion := InputEventMouseMotion.new()
	motion.position = win(Vector2(640, 320))
	motion.relative = Vector2(40, 20)
	Input.parse_input_event(motion)
	mouse(Vector2(640, 320), MOUSE_BUTTON_MIDDLE, false)
	await frames()
	check("middle drag pans without rotation", rig.target.distance_to(old_target) > 1 and rig._yaw_goal == old_yaw)
	mouse(Vector2(600, 300), MOUSE_BUTTON_MIDDLE, true)
	motion.shift_pressed = true
	Input.parse_input_event(motion)
	mouse(Vector2(640, 320), MOUSE_BUTTON_MIDDLE, false)
	await frames()
	check("Shift middle drag rotates", rig._yaw_goal != old_yaw)
	old_target = rig.target
	var pan := InputEventPanGesture.new()
	pan.position = win(Vector2(600, 300))
	pan.delta = Vector2(3, 2)
	Input.parse_input_event(pan)
	await frames()
	check("trackpad gesture pans", rig.target.distance_to(old_target) > 1)
	# Airfield, flag, clear rally and mass training.
	rig._yaw_goal = 0.0
	rig.yaw = 0.0
	rig.focus(gv.debug_feature_scene("airfield"), true)
	rig.dist = 100.0
	rig._dist_goal = 100.0
	await frames(20)
	var rally_screen := Vector2(800, 390)
	click(rally_screen, MOUSE_BUTTON_RIGHT)
	await get_tree().create_timer(0.4).timeout
	check("airfield rally set by right click", not "rally None" in gv.entity_debug("airport"))
	key(KEY_Q, true)
	await get_tree().create_timer(0.4).timeout
	check("Shift hotkey queues ten", gv.production_queue().size() == 10)
	await snapshot("airfield-rally")
	var clear := card_button("Clear Rally")
	check("Clear Rally button visible", clear != null)
	if clear:
		click(clear.get_global_rect().get_center())
	await get_tree().create_timer(0.4).timeout
	check("Clear Rally removes flag destination", "rally None" in gv.entity_debug("airport"))
	# Save a building group using the same keyboard event.
	group.keycode = KEY_2
	group.physical_keycode = KEY_2
	group.pressed = true
	Input.parse_input_event(group)
	group = group.duplicate()
	group.pressed = false
	Input.parse_input_event(group)
	await frames(20)
	check("building group added", hud.group_bar.get_child_count() == 2)
	gv.select_all_of("citizen", 999)
	await frames()
	click(hud.group_bar.get_child(1).get_global_rect().get_center())
	await frames()
	check("building group click recalls airfield", gv.selection_info().size() == 1 and gv.selection_info()[0]["key"] == "airport")
	# Boarding the landing ship from the coastline.
	rig.focus(gv.debug_feature_scene("landing"), true)
	await frames(20)
	click(gv.screen_pos_of("transport"), MOUSE_BUTTON_RIGHT)
	await get_tree().create_timer(0.5).timeout
	check("boarding calls ship toward shore", "order Move" in gv.entity_debug("transport"))
	await snapshot("landing-approach")
	gv.set_speed(4.0)
	await get_tree().create_timer(6.0).timeout
	check("landing ship loaded soldier", "cargo 1" in gv.entity_debug("transport"))
	gv.set_speed(1.0)
	# Actual nuclear weapon firing drives particles, sound, tree removal and shake.
	rig.focus(gv.debug_feature_scene("nuclear"), true)
	rig.dist = 290.0
	rig._dist_goal = 290.0
	var saw_alert := false
	var saw_shake := false
	var flash_saved := false
	var shock_saved := false
	var cloud_saved := false
	for i in 110:
		await get_tree().create_timer(0.1).timeout
		saw_alert = saw_alert or gv.attack_markers().size() > 0
		saw_shake = saw_shake or rig._shake > 0.1
		var blast_start: int = get_parent().get_node("VFX").last_nuclear_msec
		if blast_start >= 0:
			var elapsed := (Time.get_ticks_msec() - blast_start) / 1000.0
			if not flash_saved:
				await snapshot("nuclear-flash")
				flash_saved = true
			if not shock_saved and elapsed >= 0.85:
				await snapshot("nuclear-shockwave")
				shock_saved = true
			if not cloud_saved and elapsed >= 6.0:
				await snapshot("nuclear-cloud")
				cloud_saved = true
	check("attack appears on minimap", saw_alert)
	check("nuclear impact shakes camera", saw_shake)
	check("flash, shockwave and cloud captured", flash_saved and shock_saved and cloud_saved)
	for i in 4:
		check("alert variant %d loads" % i, get_parent().get_node("Audio")._load("alert_%d" % i) != null)
	await get_tree().create_timer(16.0).timeout
	check("attack markers expire", gv.attack_markers().size() == 0)
	check("dedicated nuclear sound loads", get_parent().get_node("Audio")._load("nuclear_blast") != null)
	# Destruction keeps render-only wrecks after the simulation removes the units.
	rig.focus(gv.debug_feature_scene("wrecks"), true)
	rig.dist = 105.0
	rig._dist_goal = 105.0
	await get_tree().create_timer(0.3).timeout
	check("destroyed aircraft fall and ships sink", gv.sim_stats()["falling_aircraft"] >= 2 and gv.sim_stats()["sinking_ships"] >= 1)
	await snapshot("wrecks-falling")
	await get_tree().create_timer(0.5).timeout
	await snapshot("wrecks-descent")
	await get_tree().create_timer(3.0).timeout
	check("aircraft reach terrain or water", gv.sim_stats()["falling_aircraft"] == 0)
	await snapshot("wrecks-impact")
	await get_tree().create_timer(3.0).timeout
	check("ship sinking persists after initial hit", gv.sim_stats()["sinking_ships"] >= 1)
	await snapshot("wrecks-sinking")
	await get_tree().create_timer(13.0).timeout
	check("wreck animations finish", gv.sim_stats()["wrecks"] == 0)
	print("FEATURETEST DONE failures=%d" % failures)
	get_tree().quit(1 if failures > 0 else 0)

func preview_nuclear() -> void:
	rig.focus(gv.debug_feature_scene("nuclear"), true)
	rig.dist = 290.0
	rig._dist_goal = 290.0
	while get_parent().get_node("VFX").last_nuclear_msec < 0:
		await get_tree().process_frame
	await snapshot("nuclear-flash")
	await get_tree().create_timer(1.0).timeout
	await snapshot("nuclear-shockwave")
	await get_tree().create_timer(5.0).timeout
	await snapshot("nuclear-cloud")
	get_tree().quit()
