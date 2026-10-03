extends Node3D
## RTS camera: WASD/arrows/edge pan, wheel zoom (with smooth pitch), Q/E or
## middle-drag rotate. Clamped to the map.

@export var pan_speed := 70.0
@export var edge_margin := 14
@export var min_dist := 22.0
@export var max_dist := 190.0

var target := Vector3(400, 0, 400)
var yaw := 0.0
var dist := 70.0
var _dist_goal := 70.0
var _yaw_goal := 0.0
var map_size := Vector2(800, 800)
var game_view: Node = null
var edge_pan := true
var _mid_drag := false

@onready var cam: Camera3D = $Camera3D

func _ready() -> void:
	_dist_goal = dist
	_apply()

func focus(p: Vector3, instant := false) -> void:
	target = Vector3(p.x, 0, p.z)
	if instant:
		_apply()

func _unhandled_input(e: InputEvent) -> void:
	if e is InputEventMouseButton:
		if e.button_index == MOUSE_BUTTON_WHEEL_UP and e.pressed:
			_dist_goal = max(min_dist, _dist_goal * 0.88)
		elif e.button_index == MOUSE_BUTTON_WHEEL_DOWN and e.pressed:
			_dist_goal = min(max_dist, _dist_goal * 1.12)
		elif e.button_index == MOUSE_BUTTON_MIDDLE:
			_mid_drag = e.pressed
	elif e is InputEventMouseMotion and _mid_drag:
		if Input.is_key_pressed(KEY_SHIFT) or Input.is_key_pressed(KEY_ALT):
			_yaw_goal -= e.relative.x * 0.006
		else:
			# grab-and-drag the map
			var k := dist * 0.0032
			target -= (_right() * e.relative.x - _fwd() * e.relative.y) * k
	elif e is InputEventMagnifyGesture:
		_dist_goal = clamp(_dist_goal / e.factor, min_dist, max_dist)
	elif e is InputEventPanGesture:
		# two-finger trackpad scroll pans the map (scaled with zoom)
		var k := dist * 0.045
		target += (_right() * e.delta.x - _fwd() * e.delta.y) * k

func _fwd() -> Vector3:
	return Vector3(sin(yaw), 0, cos(yaw)) * -1.0

func _right() -> Vector3:
	return Vector3(cos(yaw), 0, -sin(yaw))

func _process(dt: float) -> void:
	var v := Vector2.ZERO
	if not Input.is_key_pressed(KEY_CTRL) and not Input.is_key_pressed(KEY_META):
		if Input.is_key_pressed(KEY_LEFT) or Input.is_key_pressed(KEY_KP_4): v.x -= 1
		if Input.is_key_pressed(KEY_RIGHT): v.x += 1
		if Input.is_key_pressed(KEY_UP): v.y += 1
		if Input.is_key_pressed(KEY_DOWN): v.y -= 1
	if edge_pan and DisplayServer.window_is_focused():
		var mp := get_viewport().get_mouse_position()
		var vs := get_viewport().get_visible_rect().size
		if mp.x >= 0 and mp.y >= 0 and mp.x <= vs.x and mp.y <= vs.y:
			if mp.x < edge_margin: v.x -= 1
			if mp.x > vs.x - edge_margin: v.x += 1
			if mp.y < edge_margin: v.y += 1
			if mp.y > vs.y - edge_margin: v.y -= 1
	if Input.is_key_pressed(KEY_PAGEUP) or Input.is_key_pressed(KEY_COMMA): _yaw_goal += dt * 1.6
	if Input.is_key_pressed(KEY_PAGEDOWN) or Input.is_key_pressed(KEY_PERIOD): _yaw_goal -= dt * 1.6
	var speed := pan_speed * (dist / 70.0)
	target += (_right() * v.x + _fwd() * v.y) * speed * dt
	target.x = clamp(target.x, -40.0, map_size.x + 40.0)
	target.z = clamp(target.z, -40.0, map_size.y + 40.0)
	dist = lerp(dist, _dist_goal, 1.0 - exp(-dt * 10.0))
	yaw = lerp_angle(yaw, _yaw_goal, 1.0 - exp(-dt * 10.0))
	_apply()

func _apply() -> void:
	# pitch flattens slightly as we zoom in for a more cinematic close-up
	var t: float = clamp((dist - min_dist) / (max_dist - min_dist), 0.0, 1.0)
	var pitch: float = lerp(0.62, 0.98, t)
	var ground := 0.0
	if game_view and game_view.is_running():
		ground = game_view.height_at(target.x, target.z)
	target.y = lerp(target.y, ground, 0.2)
	var back := Vector3(sin(yaw), 0, cos(yaw))
	var offset := back * cos(pitch) * dist + Vector3.UP * sin(pitch) * dist
	cam.global_position = target + offset
	cam.look_at(target, Vector3.UP)
	cam.fov = lerp(42.0, 38.0, t)
	cam.far = 2400.0
	cam.near = 0.5
