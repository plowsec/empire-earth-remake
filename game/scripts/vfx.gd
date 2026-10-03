extends Node3D
## Pooled particle effects driven by game events.

var gv: Node
var rig: Node
var pools := {}
var lights: Array[OmniLight3D] = []
var light_i := 0
var soft_tex: Texture2D
var smoke_tex: Texture2D
var markers: Array = []
var audio: Node

const EXPLOSIVE := [1, 2, 5, 6, 7, 9]   # Cannon, Explosive, Torpedo, NavalGun, Bomb, Missile

func bind(game_view: Node, camera_rig: Node) -> void:
	gv = game_view
	rig = camera_rig
	soft_tex = _radial_tex(Color(1, 1, 1, 1), Color(1, 1, 1, 0))
	smoke_tex = _smoke_tex()
	pools["fire"] = _make_pool(20, _fireball)
	pools["smoke"] = _make_pool(28, _smoke)
	pools["sparks"] = _make_pool(20, _sparks)
	pools["muzzle"] = _make_pool(40, _muzzle)
	pools["splash"] = _make_pool(14, _splash)
	pools["dust"] = _make_pool(10, _dust)
	pools["puff"] = _make_pool(120, _puff)
	pools["blood"] = _make_pool(16, _dirt_kick)
	for i in 10:
		var l := OmniLight3D.new()
		l.light_color = Color(1.0, 0.62, 0.3)
		l.omni_range = 12.0
		l.light_energy = 0.0
		l.shadow_enabled = false
		add_child(l)
		lights.append(l)
	if has_node("../Audio"):
		audio = get_node("../Audio")

func _radial_tex(inner: Color, outer: Color) -> Texture2D:
	var g := Gradient.new()
	g.set_color(0, inner)
	g.set_color(1, outer)
	g.add_point(0.45, Color(inner, inner.a * 0.55))
	var t := GradientTexture2D.new()
	t.gradient = g
	t.fill = GradientTexture2D.FILL_RADIAL
	t.fill_from = Vector2(0.5, 0.5)
	t.fill_to = Vector2(1.0, 0.5)
	t.width = 128
	t.height = 128
	return t

func _smoke_tex() -> Texture2D:
	# soft noisy puff: radial falloff baked with noise into an image
	var n := FastNoiseLite.new()
	n.frequency = 0.045
	n.fractal_octaves = 4
	var img := Image.create(128, 128, false, Image.FORMAT_RGBA8)
	for y in 128:
		for x in 128:
			var d := Vector2(x - 64, y - 64).length() / 64.0
			var fall: float = clamp(1.0 - d, 0.0, 1.0)
			var v := (n.get_noise_2d(x, y) * 0.5 + 0.5)
			var a: float = clamp(fall * fall * (0.55 + v * 0.9), 0.0, 1.0)
			img.set_pixel(x, y, Color(0.9 + v * 0.1, 0.9 + v * 0.1, 0.9 + v * 0.1, a))
	return ImageTexture.create_from_image(img)

func _mat(tex: Texture2D, unshaded: bool, additive := false) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_texture = tex
	m.vertex_color_use_as_albedo = true
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED if unshaded else BaseMaterial3D.SHADING_MODE_PER_PIXEL
	if additive:
		m.blend_mode = BaseMaterial3D.BLEND_MODE_ADD
	m.disable_receive_shadows = true
	return m

func _quad(size: float) -> QuadMesh:
	var q := QuadMesh.new()
	q.size = Vector2(size, size)
	return q

func _ramp(stops: Array) -> GradientTexture1D:
	var g := Gradient.new()
	g.offsets = PackedFloat32Array(stops.map(func(s): return s[0]))
	g.colors = PackedColorArray(stops.map(func(s): return s[1]))
	var t := GradientTexture1D.new()
	t.gradient = g
	return t

func _curve(points: Array) -> CurveTexture:
	var c := Curve.new()
	for p in points:
		c.add_point(Vector2(p[0], p[1]))
	var t := CurveTexture.new()
	t.curve = c
	return t

func _particles(amount: int, life: float, mesh: Mesh, mat: Material, pm: ParticleProcessMaterial) -> GPUParticles3D:
	var p := GPUParticles3D.new()
	p.amount = amount
	p.lifetime = life
	p.one_shot = true
	p.emitting = false
	p.explosiveness = 0.92
	p.draw_pass_1 = mesh
	p.material_override = mat
	p.process_material = pm
	p.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	p.visibility_aabb = AABB(Vector3(-30, -5, -30), Vector3(60, 60, 60))
	p.local_coords = false
	return p

func _fireball() -> Node3D:
	var pm := ParticleProcessMaterial.new()
	pm.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_SPHERE
	pm.emission_sphere_radius = 0.6
	pm.direction = Vector3.UP
	pm.spread = 180.0
	pm.initial_velocity_min = 2.0
	pm.initial_velocity_max = 6.5
	pm.gravity = Vector3(0, 2.5, 0)
	pm.damping_min = 4.0
	pm.damping_max = 7.0
	pm.scale_min = 0.8
	pm.scale_max = 1.6
	pm.scale_curve = _curve([[0.0, 0.4], [0.25, 1.0], [1.0, 1.4]])
	pm.color_ramp = _ramp([[0.0, Color(1, 0.95, 0.75, 1)], [0.2, Color(1, 0.6, 0.15, 1)], [0.5, Color(0.6, 0.18, 0.05, 0.8)], [1.0, Color(0.1, 0.08, 0.07, 0)]])
	pm.angle_min = -180; pm.angle_max = 180
	var m := _mat(soft_tex, true, true)
	m.albedo_color = Color(2.2, 1.6, 1.0)
	return _particles(26, 0.9, _quad(2.4), m, pm)

func _smoke() -> Node3D:
	var pm := ParticleProcessMaterial.new()
	pm.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_SPHERE
	pm.emission_sphere_radius = 1.0
	pm.direction = Vector3.UP
	pm.spread = 35.0
	pm.initial_velocity_min = 1.5
	pm.initial_velocity_max = 4.0
	pm.gravity = Vector3(0.6, 1.2, 0.2)
	pm.damping_min = 0.6
	pm.damping_max = 1.2
	pm.scale_min = 1.2
	pm.scale_max = 2.2
	pm.scale_curve = _curve([[0.0, 0.5], [1.0, 2.6]])
	pm.color_ramp = _ramp([[0.0, Color(0.2, 0.18, 0.16, 0.0)], [0.08, Color(0.24, 0.22, 0.2, 0.85)], [0.6, Color(0.42, 0.41, 0.4, 0.45)], [1.0, Color(0.6, 0.6, 0.6, 0)]])
	pm.angle_min = -180; pm.angle_max = 180
	pm.angular_velocity_min = -20; pm.angular_velocity_max = 20
	var p := _particles(18, 3.6, _quad(3.2), _mat(smoke_tex, false), pm)
	p.explosiveness = 0.7
	return p

func _sparks() -> Node3D:
	var pm := ParticleProcessMaterial.new()
	pm.direction = Vector3.UP
	pm.spread = 70.0
	pm.initial_velocity_min = 7.0
	pm.initial_velocity_max = 16.0
	pm.gravity = Vector3(0, -14, 0)
	pm.scale_min = 0.25
	pm.scale_max = 0.6
	pm.color_ramp = _ramp([[0.0, Color(1, 0.85, 0.5, 1)], [1.0, Color(1, 0.3, 0.05, 0)]])
	var m := _mat(soft_tex, true, true)
	m.albedo_color = Color(3, 2, 1)
	var p := _particles(28, 1.1, _quad(0.5), m, pm)
	return p

func _muzzle() -> Node3D:
	var pm := ParticleProcessMaterial.new()
	pm.spread = 25.0
	pm.direction = Vector3(0, 0.2, 1)
	pm.initial_velocity_min = 0.5
	pm.initial_velocity_max = 2.0
	pm.gravity = Vector3.ZERO
	pm.scale_min = 0.5
	pm.scale_max = 1.0
	pm.color_ramp = _ramp([[0.0, Color(1, 0.9, 0.6, 1)], [1.0, Color(1, 0.5, 0.1, 0)]])
	var m := _mat(soft_tex, true, true)
	m.albedo_color = Color(3, 2.4, 1.4)
	return _particles(5, 0.09, _quad(0.9), m, pm)

func _splash() -> Node3D:
	var pm := ParticleProcessMaterial.new()
	pm.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_RING
	pm.emission_ring_radius = 0.8
	pm.emission_ring_inner_radius = 0.2
	pm.emission_ring_height = 0.2
	pm.emission_ring_axis = Vector3.UP
	pm.direction = Vector3.UP
	pm.spread = 12.0
	pm.initial_velocity_min = 6.0
	pm.initial_velocity_max = 13.0
	pm.gravity = Vector3(0, -16, 0)
	pm.scale_min = 0.8
	pm.scale_max = 1.8
	pm.scale_curve = _curve([[0.0, 0.6], [1.0, 1.5]])
	pm.color_ramp = _ramp([[0.0, Color(0.95, 0.98, 1, 0.95)], [1.0, Color(0.85, 0.92, 0.95, 0)]])
	return _particles(30, 1.4, _quad(1.6), _mat(smoke_tex, false), pm)

func _dust() -> Node3D:
	var pm := ParticleProcessMaterial.new()
	pm.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_BOX
	pm.emission_box_extents = Vector3(5, 1, 5)
	pm.direction = Vector3.UP
	pm.spread = 60.0
	pm.initial_velocity_min = 1.0
	pm.initial_velocity_max = 5.0
	pm.gravity = Vector3(0, 0.4, 0)
	pm.damping_min = 0.5
	pm.damping_max = 1.0
	pm.scale_min = 2.0
	pm.scale_max = 4.0
	pm.scale_curve = _curve([[0.0, 0.5], [1.0, 2.2]])
	pm.color_ramp = _ramp([[0.0, Color(0.55, 0.48, 0.38, 0)], [0.1, Color(0.55, 0.48, 0.38, 0.85)], [1.0, Color(0.6, 0.55, 0.48, 0)]])
	var p := _particles(40, 5.0, _quad(4.0), _mat(smoke_tex, false), pm)
	p.explosiveness = 0.6
	return p

func _puff() -> Node3D:
	var pm := ParticleProcessMaterial.new()
	pm.direction = Vector3.UP
	pm.spread = 30.0
	pm.initial_velocity_min = 0.2
	pm.initial_velocity_max = 0.6
	pm.gravity = Vector3(0, 0.3, 0)
	pm.scale_min = 0.5
	pm.scale_max = 0.8
	pm.scale_curve = _curve([[0.0, 0.6], [1.0, 2.0]])
	pm.color_ramp = _ramp([[0.0, Color(0.9, 0.88, 0.85, 0.7)], [1.0, Color(0.8, 0.8, 0.8, 0)]])
	return _particles(2, 1.4, _quad(0.9), _mat(smoke_tex, false), pm)

func _dirt_kick() -> Node3D:
	var pm := ParticleProcessMaterial.new()
	pm.direction = Vector3.UP
	pm.spread = 40.0
	pm.initial_velocity_min = 2.0
	pm.initial_velocity_max = 5.0
	pm.gravity = Vector3(0, -12, 0)
	pm.scale_min = 0.3
	pm.scale_max = 0.6
	pm.color_ramp = _ramp([[0.0, Color(0.35, 0.28, 0.2, 1)], [1.0, Color(0.35, 0.28, 0.2, 0)]])
	return _particles(10, 0.8, _quad(0.4), _mat(smoke_tex, false), pm)

func _make_pool(n: int, ctor: Callable) -> Dictionary:
	var items := []
	for i in n:
		var node: Node3D = ctor.call()
		add_child(node)
		items.append(node)
	return {"items": items, "i": 0}

func _play(pool: String, pos: Vector3, scale := 1.0, dir := Vector3.ZERO) -> void:
	var p: Dictionary = pools[pool]
	var node: GPUParticles3D = p["items"][p["i"]]
	p["i"] = (p["i"] + 1) % p["items"].size()
	var b := Basis.IDENTITY
	if dir.length() > 0.01:
		b = Basis.looking_at(dir.normalized() * -1.0, Vector3.UP if abs(dir.normalized().y) < 0.95 else Vector3.RIGHT)
	node.global_transform = Transform3D(b.scaled(Vector3.ONE * scale), pos)
	node.restart()

func _flash(pos: Vector3, energy: float, rng: float) -> void:
	var l := lights[light_i]
	light_i = (light_i + 1) % lights.size()
	l.global_position = pos + Vector3.UP * 1.5
	l.omni_range = rng
	l.light_energy = energy
	var tw := create_tween()
	tw.tween_property(l, "light_energy", 0.0, 0.35).set_ease(Tween.EASE_OUT)

func _near_camera(p: Vector3, lim := 260.0) -> bool:
	if rig == null: return true
	return Vector2(p.x, p.z).distance_to(Vector2(rig.target.x, rig.target.z)) < lim + rig.dist

func on_event(e: Dictionary) -> void:
	var kind: String = e["kind"]
	var pos: Vector3 = e["pos"]
	match kind:
		"shot":
			if not _near_camera(pos): return
			var to: Vector3 = e["to"]
			_play("muzzle", pos, 0.8 + clamp(e["size"] / 80.0, 0.0, 1.5), to - pos)
			var dmg: int = e["dmg"]
			if dmg in [1, 6]:
				_play("puff", pos, 2.0)
				_flash(pos, 2.0, 8.0)
			if audio: audio.play_shot(dmg, pos, e["text"])
		"impact":
			if not _near_camera(pos): return
			var size: float = max(e["size"], 1.5)
			var dmg: int = e["dmg"]
			if dmg in EXPLOSIVE:
				var s: float = clamp(size / 3.0, 0.6, 3.5)
				_play("fire", pos + Vector3.UP * 0.5, s)
				_play("smoke", pos, s * 0.9)
				_play("sparks", pos, s)
				_flash(pos, 6.0 * s, 10.0 + size * 2.0)
				if audio: audio.play_explosion(s, pos)
			else:
				_play("blood", pos, 1.0)
				if audio: audio.play_hit(pos)
		"splash":
			if not _near_camera(pos): return
			var s: float = clamp(max(e["size"], 2.0) / 3.0, 0.6, 3.0)
			_play("splash", pos, s)
			if audio: audio.play_splash(pos)
		"death":
			if not _near_camera(pos): return
			var t: int = e["dmg"]
			if t == 2:
				var s: float = e["size"] / 10.0
				_play("dust", pos, s * 1.4)
				_play("fire", pos + Vector3.UP * 2.0, s * 2.5)
				_play("smoke", pos + Vector3.UP, s * 3.0)
				_flash(pos, 8.0, 25.0)
				if audio: audio.play_explosion(3.0, pos)
				if audio: audio._play3d(audio._load("collapse"), pos, 4.0)
			elif t == 1 or t == 3:
				var s: float = clamp(e["size"] / 3.0, 0.8, 3.0)
				_play("fire", pos + Vector3.UP, s * 1.4)
				_play("smoke", pos, s * 1.5)
				_play("sparks", pos, s * 1.2)
				_flash(pos, 10.0, 16.0)
				if audio: audio.play_explosion(s * 1.3, pos)
		"trail":
			if randf() < 0.35 and _near_camera(pos, 180.0):
				_play("puff", pos, 0.8)
		"move_marker":
			_marker(pos, Color(1, 0.3, 0.25) if e["mine"] else Color(0.4, 1.0, 0.5))
		_:
			if audio: audio.on_event(e)

func _marker(pos: Vector3, col: Color) -> void:
	var m := MeshInstance3D.new()
	var t := TorusMesh.new()
	t.inner_radius = 0.9
	t.outer_radius = 1.15
	m.mesh = t
	var mat := StandardMaterial3D.new()
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.albedo_color = col
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mat.emission_enabled = true
	mat.emission = col
	m.material_override = mat
	m.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(m)
	m.global_position = pos + Vector3.UP * 0.25
	var tw := create_tween()
	tw.set_parallel(true)
	tw.tween_property(m, "scale", Vector3(0.2, 1, 0.2), 0.5).from(Vector3(1.6, 1, 1.6))
	tw.tween_property(mat, "albedo_color:a", 0.0, 0.5)
	tw.chain().tween_callback(m.queue_free)
