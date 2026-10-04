extends Node
## Persistent user settings (autoload "Settings").

const PATH := "user://settings.cfg"
var master_volume := 0.8
var music_volume := 0.6
var sfx_volume := 0.9
var fullscreen := false
var edge_pan := true
var shadows_high := true
var last_skirmish := {}
var player_name := "Commander"

var _shot_path := ""
var _shot_frames := 0
var _frame := 0

func _process(_dt: float) -> void:
	if _shot_path == "":
		return
	_frame += 1
	if _frame == _shot_frames:
		await RenderingServer.frame_post_draw
		get_viewport().get_texture().get_image().save_png(_shot_path)
		print("saved ", _shot_path)
		get_tree().quit()

func _ready() -> void:
	for a in OS.get_cmdline_user_args():
		if a.begins_with("--shot="): _shot_path = a.substr(7)
		elif a.begins_with("--shot-frames="): _shot_frames = int(a.substr(14))
		elif a.begins_with("--scene="):
			call_deferred("_goto", a.substr(8))
	var c := ConfigFile.new()
	if c.load(PATH) == OK:
		master_volume = c.get_value("audio", "master", master_volume)
		music_volume = c.get_value("audio", "music", music_volume)
		sfx_volume = c.get_value("audio", "sfx", sfx_volume)
		fullscreen = c.get_value("video", "fullscreen", fullscreen)
		shadows_high = c.get_value("video", "shadows_high", shadows_high)
		edge_pan = c.get_value("input", "edge_pan", edge_pan)
		last_skirmish = c.get_value("game", "last_skirmish", {})
		player_name = c.get_value("game", "player_name", player_name)
	apply()

func apply() -> void:
	for bus in [["Master", master_volume], ["Music", music_volume], ["SFX", sfx_volume]]:
		var i := AudioServer.get_bus_index(bus[0])
		if i >= 0:
			AudioServer.set_bus_volume_db(i, linear_to_db(max(bus[1], 0.0001)))
	var mode := DisplayServer.WINDOW_MODE_FULLSCREEN if fullscreen else DisplayServer.WINDOW_MODE_WINDOWED
	if DisplayServer.window_get_mode() != mode and _shot_path == "":
		DisplayServer.window_set_mode(mode)

func save() -> void:
	var c := ConfigFile.new()
	c.set_value("audio", "master", master_volume)
	c.set_value("audio", "music", music_volume)
	c.set_value("audio", "sfx", sfx_volume)
	c.set_value("video", "fullscreen", fullscreen)
	c.set_value("video", "shadows_high", shadows_high)
	c.set_value("input", "edge_pan", edge_pan)
	c.set_value("game", "last_skirmish", last_skirmish)
	c.set_value("game", "player_name", player_name)
	c.save(PATH)
	apply()

func _goto(scene: String) -> void:
	get_tree().change_scene_to_file("res://scenes/%s.tscn" % scene)
