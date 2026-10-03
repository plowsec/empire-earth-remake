extends "res://scripts/autotest.gd"
## Save/load round trip in the real game: --savetest
## Quicksaves with F5, reloads the save, checks the state matches and the replay exists.

func frames(n := 6) -> void:
	for i in n:
		await get_tree().process_frame

func _process(_dt: float) -> void:
	pass

func _ready() -> void:
	super._ready()
	get_tree().create_timer(90.0).timeout.connect(func():
		print("FAIL save test timed out")
		get_tree().quit(1))
	await frames(30)
	gv.set_speed(4.0)
	await get_tree().create_timer(6.0).timeout
	gv.set_paused(true)
	var before: Dictionary = gv.sim_stats()
	key(KEY_F5)
	await frames(4)
	var saves: Array = gv.list_saves()
	check("quicksave listed", saves.size() > 0 and saves[0]["name"] == "Quicksave")
	var replay: String = before.get("replay", "")
	check("replay file written on save", replay != "" and FileAccess.file_exists(replay))
	var err: String = gv.load_game(saves[0]["path"])
	check("save loads (%s)" % err, err == "")
	var after: Dictionary = gv.sim_stats()
	check("tick restored (%d vs %d)" % [after["tick"], before["tick"]], after["tick"] == before["tick"])
	check("units restored (%d vs %d)" % [after["units"], before["units"]], after["units"] == before["units"])
	check("state checksum identical", after["checksum"] == before["checksum"])
	await get_tree().create_timer(3.0).timeout
	var later: Dictionary = gv.sim_stats()
	check("loaded game keeps running", later["tick"] > after["tick"])
	print("SAVETEST DONE failures=%d" % failures)
	get_tree().quit(1 if failures > 0 else 0)
