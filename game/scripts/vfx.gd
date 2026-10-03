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
var decals: Array[Decal] = []
var decal_i := 0
var audio: Node
var last_nuclear_msec := -1
var blast_smoke_textures: Array[Texture2D] = []

const EXPLOSIVE := [1, 2, 5, 6, 7, 9]   # Cannon, Explosive, Torpedo, NavalGun, Bomb, Missile

func bind(game_view: Node, camera_rig: Node) -> void:
	gv = game_view
	rig = camera_rig
	soft_tex = _radial_tex(Color(1, 1, 1, 1), Color(1, 1, 1, 0))
	smoke_tex = _smoke_tex()
	for i in 4:
		blast_smoke_textures.append(_blast_smoke_texture(31 + i * 7))
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
	var scorch := _scorch_tex()
	for i in 48:
		var d := Decal.new()
		d.texture_albedo = scorch
		d.size = Vector3(4, 6, 4)
		d.modulate = Color(1, 1, 1, 0)
		d.cull_mask = 1
		d.upper_fade = 0.3
		d.lower_fade = 0.3
		add_child(d)
		decals.append(d)

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

func _scorch_tex() -> Texture2D:
	var n := FastNoiseLite.new()
	n.frequency = 0.06
	var img := Image.create(128, 128, false, Image.FORMAT_RGBA8)
	for y in 128:
		for x in 128:
			var d := Vector2(x - 64, y - 64).length() / 64.0
			var v := n.get_noise_2d(x, y) * 0.5 + 0.5
			var a: float = clamp((1.0 - d) * 1.6 - v * 0.5, 0.0, 1.0)
			img.set_pixel(x, y, Color(0.05, 0.04, 0.035, a * 0.85))
	return ImageTexture.create_from_image(img)

func _scorch(pos: Vector3, size: float) -> void:
	var d := decals[decal_i]
	decal_i = (decal_i + 1) % decals.size()
	d.global_position = pos
	d.rotation.y = randf() * TAU
	d.size = Vector3(size, 8.0, size)
	d.modulate = Color(1, 1, 1, 1)
	var tw := create_tween()
	tw.tween_interval(12.0)
	tw.tween_property(d, "modulate:a", 0.0, 8.0)

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
	pm.color_ramp = _ramp([[0.0, Color(1, 0.9, 0.6, 1)], [0.12, Color(1, 0.55, 0.12, 1)], [0.35, Color(0.75, 0.22, 0.04, 0.85)], [0.7, Color(0.2, 0.08, 0.04, 0.4)], [1.0, Color(0.05, 0.04, 0.04, 0)]])
	pm.angle_min = -180; pm.angle_max = 180
	pm.angular_velocity_min = -90; pm.angular_velocity_max = 90
	var m := _mat(smoke_tex, true, true)
	m.albedo_color = Color(1.5, 1.15, 0.85)
	return _particles(30, 0.85, _quad(2.6), m, pm)

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
	pm.color_ramp = _ramp([[0.0, Color(0.12, 0.1, 0.09, 0.0)], [0.06, Color(0.14, 0.12, 0.11, 0.95)], [0.45, Color(0.3, 0.29, 0.28, 0.6)], [1.0, Color(0.55, 0.55, 0.55, 0)]])
	pm.angle_min = -180; pm.angle_max = 180
	pm.angular_velocity_min = -20; pm.angular_velocity_max = 20
	var p := _particles(22, 4.5, _quad(3.4), _mat(smoke_tex, false), pm)
	p.explosiveness = 0.75
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
	pm.scale_min = 0.5
	pm.scale_max = 1.0
	pm.particle_flag_align_y = true
	var q := QuadMesh.new()
	q.size = Vector2(0.07, 0.45)
	var p := _particles(28, 1.0, q, m, pm)
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
	return _particles(6, 0.08, _quad(1.4), m, pm)

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
			var dmg: int = e["dmg"]
			if dmg == 10:
				_nuke(pos, e["size"])
				return
			if not _near_camera(pos): return
			var size: float = max(e["size"], 1.5)
			if dmg in EXPLOSIVE:
				var s: float = clamp(size / 3.0, 0.6, 3.5)
				_play("fire", pos + Vector3.UP * 0.5, s)
				_play("smoke", pos, s * 0.9)
				_play("sparks", pos, 1.0)
				_flash(pos, 6.0 * s, 10.0 + size * 2.0)
				_scorch(pos, 2.5 + s * 2.5)
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
			elif t == 3:
				_play("fire", pos, 0.9)
				_play("smoke", pos, 0.8)
				_play("sparks", pos, 1.4)
				if audio: audio._play3d(audio._load("aircraft_breakup"), pos, -1.0)
			elif t == 4:
				_play("fire", pos + Vector3.UP * 2, 1.6)
				_play("smoke", pos + Vector3.UP * 2, 2.0)
				_play("splash", pos, 2.5)
				if audio: audio._play3d(audio._load("ship_sinking"), pos, 0.0, 0.02)
			elif t == 1:
				var s: float = clamp(e["size"] / 3.0, 0.8, 3.0)
				_play("fire", pos + Vector3.UP, s * 1.4)
				_play("smoke", pos, s * 1.5)
				_play("sparks", pos, 1.0)
				_flash(pos, 10.0, 16.0)
				_scorch(pos, 4.0 + s * 2.0)
				if audio: audio.play_explosion(s * 1.3, pos)
		"wreck_trail":
			if _near_camera(pos):
				_play("smoke", pos, 0.65)
				_play("fire", pos, 0.45)
		"wreck_foam":
			if _near_camera(pos):
				_play("splash", pos, clampf(e["size"] / 3.0, 0.7, 2.2))
				_play("smoke", pos + Vector3.UP, 0.65)
		"wreck_splash":
			if _near_camera(pos):
				_play("splash", pos, 3.0)
				_play("smoke", pos, 1.3)
				if audio: audio.play_splash(pos)
		"wreck_impact":
			if _near_camera(pos):
				_play("fire", pos + Vector3.UP, 2.2)
				_play("smoke", pos, 2.5)
				_play("dust", pos, 2.0)
				_play("sparks", pos, 2.0)
				_flash(pos, 8.0, 22.0)
				_scorch(pos, 8.0)
				if audio: audio.play_explosion(2.0, pos)
		"trail":
			if randf() < 0.35 and _near_camera(pos, 180.0):
				_play("puff", pos, 0.8)
		"contrail":
			if _near_camera(pos, 220.0):
				_play("puff", pos, 0.32)
		"move_marker":
			_marker(pos, Color(1, 0.3, 0.25) if e["mine"] else Color(0.4, 1.0, 0.5))
		_:
			if audio: audio.on_event(e)

func _emissive(col: Color, energy: float, alpha := 1.0) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(col, alpha)
	m.emission_enabled = true
	m.emission = col
	m.emission_energy_multiplier = energy
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.disable_receive_shadows = true
	return m

func _blast_smoke_texture(seed_value: int) -> Texture2D:
	var noise := FastNoiseLite.new()
	noise.seed = seed_value
	noise.frequency = 0.052
	noise.fractal_octaves = 5
	var img := Image.create(128, 128, false, Image.FORMAT_RGBA8)
	for y in 128:
		for x in 128:
			var p := Vector2(x - 64, y - 64) / 62.0
			var n := noise.get_noise_2d(x, y) * 0.5 + 0.5
			var detail := noise.get_noise_2d(x * 2.7 + 90, y * 2.7) * 0.5 + 0.5
			var edge := clampf((1.0 - p.length() + (n - 0.5) * 0.32) * 3.4, 0.0, 1.0)
			var alpha := edge * edge * (0.48 + n * 0.5)
			var shade := clampf(0.22 + n * 0.62 + detail * 0.18 - p.y * 0.13, 0.08, 1.0)
			img.set_pixel(x, y, Color(shade, shade, shade, alpha))
	return ImageTexture.create_from_image(img)

func _nuclear_cloud(pos: Vector3, radius: float) -> void:
	if get_tree().get_nodes_in_group("nuclear_cloud").size() >= 3: return
	# Layered textured smoke has soft, broken edges instead of solid sphere lobes.
	var stem := Node3D.new()
	var cap := Node3D.new()
	cap.add_to_group("nuclear_cloud")
	add_child(stem)
	add_child(cap)
	stem.global_position = pos
	cap.global_position = pos
	stem.scale = Vector3.ONE * 0.02
	cap.scale = Vector3.ONE * 0.02
	var materials: Array[StandardMaterial3D] = []
	for i in 8:
		var mat := StandardMaterial3D.new()
		mat.albedo_texture = blast_smoke_textures[i % 4]
		mat.albedo_color = Color(0.82, 0.78, 0.69, 0.95) if i < 4 else Color(0.42, 0.38, 0.32, 0.95)
		mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		mat.billboard_mode = BaseMaterial3D.BILLBOARD_ENABLED
		mat.billboard_keep_scale = true
		mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		mat.disable_receive_shadows = true
		materials.append(mat)
	for i in 90:
		var puff := MeshInstance3D.new()
		puff.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var mesh := QuadMesh.new()
		var a := i * 2.39996
		if i < 30:
			var h := float(i) / 29.0
			mesh.size = Vector2.ONE * radius * (0.35 + 0.12 * sin(i * 1.7))
			puff.mesh = mesh
			puff.material_override = materials[4 + i % 4]
			stem.add_child(puff)
			puff.position = Vector3(cos(a) * radius * 0.08, h * radius * 1.85, sin(a) * radius * 0.08)
		else:
			var spread := sqrt(float(i - 30) / 60.0)
			mesh.size = Vector2(1.0, 0.72) * radius * (0.42 + 0.18 * sin(i * 1.3) * sin(i * 1.3))
			puff.mesh = mesh
			puff.material_override = materials[i % 4]
			cap.add_child(puff)
			puff.position = Vector3(cos(a) * spread * radius * 0.65, radius * (0.15 * cos(i * 1.2) + 0.16 * (1.0 - spread)), sin(a) * spread * radius * 0.65)
	var rise := create_tween().set_parallel(true)
	rise.tween_property(stem, "scale", Vector3.ONE, 12.0).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)
	rise.tween_property(cap, "scale", Vector3.ONE, 12.0).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)
	rise.tween_property(cap, "global_position", pos + Vector3.UP * radius * 2.0, 12.0).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)
	var expansion := create_tween()
	expansion.tween_interval(12.0)
	expansion.tween_property(cap, "scale", Vector3(1.3, 1.1, 1.3), 28.0)
	expansion.parallel().tween_property(cap, "global_position", pos + Vector3.UP * radius * 2.5 + Vector3(radius * 0.4, 0, 0), 28.0)
	expansion.parallel().tween_property(stem, "scale", Vector3(1.1, 1.2, 1.1), 28.0)
	# the cap rolls and the stem twists as it climbs
	var drift := create_tween().set_parallel(true)
	drift.tween_property(cap, "rotation:y", 1.1, 40.0)
	drift.tween_property(stem, "rotation:y", -0.7, 40.0)
	for i in materials.size():
		var mat := materials[i]
		var base := mat.albedo_color
		# lit from inside by the fireball at first, then cooling to ash grey
		mat.albedo_color = Color(1.0, 0.62, 0.32, base.a) if i < 4 else Color(0.85, 0.42, 0.2, base.a)
		var cool := create_tween()
		cool.tween_property(mat, "albedo_color", base, 11.0).set_trans(Tween.TRANS_SINE)
		var fade := create_tween()
		fade.tween_interval(26.0)
		fade.tween_property(mat, "albedo_color:a", 0.0, 20.0)
	get_tree().create_timer(47.0).timeout.connect(func(): stem.queue_free(); cap.queue_free())

func _nuclear_screen_flash(pos: Vector3) -> void:
	if rig == null or not _near_camera(pos, 450.0): return
	# Keep one overlay: simultaneous impacts refresh the flash without stacking
	# screen-copy passes or obscuring the interface.
	for old in get_tree().get_nodes_in_group("blast_overlay"):
		old.queue_free()
	var layer := CanvasLayer.new()
	layer.layer = 8
	layer.add_to_group("blast_overlay")
	add_child(layer)
	var screen := ColorRect.new()
	screen.set_anchors_preset(Control.PRESET_FULL_RECT)
	screen.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var mat := ShaderMaterial.new()
	mat.shader = load("res://shaders/blast_screen.gdshader")
	var viewport_size := get_viewport().get_visible_rect().size
	var center: Vector2 = rig.cam.unproject_position(pos) / viewport_size
	mat.set_shader_parameter("center", center)
	mat.set_shader_parameter("aspect", viewport_size.x / viewport_size.y)
	mat.set_shader_parameter("flash", 1.0)
	mat.set_shader_parameter("strength", clampf(1.1 - pos.distance_to(rig.target) / 700.0, 0.12, 1.0))
	screen.material = mat
	layer.add_child(screen)
	var flash := layer.create_tween()
	flash.tween_interval(0.45)
	flash.tween_method(func(v: float): mat.set_shader_parameter("flash", v), 1.0, 0.7, 0.3)
	flash.tween_method(func(v: float): mat.set_shader_parameter("flash", v), 0.7, 0.9, 0.15)
	flash.tween_method(func(v: float): mat.set_shader_parameter("flash", v), 0.9, 0.0, 4.0).set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_OUT)
	var wave := layer.create_tween()
	wave.tween_interval(0.6)
	wave.tween_method(func(v: float): mat.set_shader_parameter("wave", v), 0.0, 1.0, 4.5)
	wave.tween_callback(layer.queue_free)

func _nuclear_dust_front(pos: Vector3, radius: float) -> void:
	var pm := ParticleProcessMaterial.new()
	pm.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_RING
	pm.emission_ring_axis = Vector3.UP
	pm.emission_ring_radius = radius * 0.18
	pm.emission_ring_inner_radius = radius * 0.12
	pm.emission_ring_height = 0.6
	pm.direction = Vector3.UP
	pm.initial_velocity_min = 0.5
	pm.initial_velocity_max = 1.5
	pm.radial_velocity_min = radius / 3.5
	pm.radial_velocity_max = radius / 3.5
	pm.gravity = Vector3(0, 0.35, 0)
	pm.scale_min = 1.0
	pm.scale_max = 2.8
	pm.scale_curve = _curve([[0.0, 0.15], [0.3, 0.7], [1.0, 1.5]])
	pm.color_ramp = _ramp([[0.0, Color(0.7, 0.65, 0.52, 0)], [0.08, Color(0.65, 0.58, 0.45, 0.8)], [0.65, Color(0.45, 0.4, 0.32, 0.5)], [1.0, Color(0.5, 0.46, 0.38, 0)]])
	var dust := _particles(260, 6.0, _quad(6.0), _mat(smoke_tex, false), pm)
	dust.explosiveness = 1.0
	dust.visibility_aabb = AABB(Vector3(-radius * 4, -10, -radius * 4), Vector3(radius * 8, radius * 3, radius * 8))
	add_child(dust)
	dust.global_position = pos + Vector3.UP
	dust.restart()
	get_tree().create_timer(7.0).timeout.connect(dust.queue_free)

func _nuke(pos: Vector3, radius: float) -> void:
	last_nuclear_msec = Time.get_ticks_msec()
	var r: float = max(radius, 20.0)
	_nuclear_screen_flash(pos)
	# blinding light, held, then a long decay
	var flash := OmniLight3D.new()
	flash.light_color = Color(1.0, 0.95, 0.85)
	flash.omni_range = r * 8.0
	flash.light_energy = 60.0
	add_child(flash)
	flash.global_position = pos + Vector3.UP * 20.0
	var tw := create_tween()
	tw.tween_interval(0.5)
	tw.tween_property(flash, "light_energy", 10.0, 1.2)
	tw.tween_property(flash, "light_energy", 0.0, 4.0)
	tw.tween_callback(flash.queue_free)
	# fireball: swells fast, then climbs slowly into the cap, cooling as it goes
	var fb := MeshInstance3D.new()
	var sm := SphereMesh.new()
	sm.radius = 1.0
	sm.height = 2.0
	fb.mesh = sm
	var fmat := _emissive(Color(1.0, 0.62, 0.22), 9.0)
	fmat.albedo_texture = smoke_tex
	fb.material_override = fmat
	fb.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(fb)
	fb.global_position = pos
	var glow := OmniLight3D.new()
	glow.light_color = Color(1.0, 0.5, 0.18)
	glow.omni_range = r * 3.0
	glow.light_energy = 14.0
	fb.add_child(glow)
	var t2 := create_tween().set_parallel(true)
	t2.tween_property(fb, "scale", Vector3.ONE * r * 0.42, 2.4).set_ease(Tween.EASE_OUT).set_trans(Tween.TRANS_EXPO)
	t2.tween_property(fb, "global_position", pos + Vector3.UP * r * 1.9, 13.0).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	t2.tween_property(fmat, "emission_energy_multiplier", 1.2, 7.0)
	t2.tween_property(glow, "light_energy", 0.0, 12.0)
	t2.chain().tween_property(fmat, "albedo_color", Color(0.25, 0.2, 0.18, 0.0), 5.0)
	t2.chain().tween_callback(fb.queue_free)
	_nuclear_cloud(pos, r)
	_nuclear_dust_front(pos, r)
	# condensation ring riding the pressure front (same 3.5 s as the simulation)
	var ring := MeshInstance3D.new()
	var tm := TorusMesh.new()
	tm.inner_radius = 0.97
	tm.outer_radius = 1.0
	ring.mesh = tm
	var rmat := _emissive(Color(0.92, 0.9, 0.86), 0.4, 0.55)
	ring.material_override = rmat
	add_child(ring)
	ring.global_position = pos + Vector3.UP * 1.5
	var t4 := create_tween().set_parallel(true)
	t4.tween_property(ring, "scale", Vector3(r, 3.0, r), 3.5).set_ease(Tween.EASE_OUT).set_trans(Tween.TRANS_SINE)
	t4.tween_property(rmat, "albedo_color:a", 0.0, 3.5)
	t4.chain().tween_callback(ring.queue_free)
	# the pressure front keeps travelling past the kill radius and shakes distant trees
	var front_speed := r / 3.5
	var shake := create_tween()
	shake.tween_method(func(f: float):
		var k: float = clampf(1.0 - f / (r * 4.0), 0.0, 1.0)
		RenderingServer.global_shader_parameter_set("nuke_blast", Vector4(pos.x, pos.z, f, k)),
		0.0, r * 4.0, r * 4.0 / front_speed)
	shake.tween_callback(func(): RenderingServer.global_shader_parameter_set("nuke_blast", Vector4(0, 0, -1000, 0)))
	for k in 8:
		var a := k * TAU / 8.0
		_play("dust", pos + Vector3(cos(a), 0.3, sin(a)) * r * 0.8, 3.0)
		_play("fire", pos + Vector3(cos(a), 0.3, sin(a)) * r * 0.4, 3.0)
		_play("smoke", pos + Vector3(cos(a), 0.0, sin(a)) * r * 0.6, 3.0)
	# ground keeps burning for a while
	for k in 14:
		var delay := 1.5 + k * 0.8
		var a2 := randf() * TAU
		var d2 := randf_range(0.15, 0.7) * r
		get_tree().create_timer(delay).timeout.connect(func():
			var p2 := pos + Vector3(cos(a2) * d2, 0.5, sin(a2) * d2)
			_play("fire", p2, 1.6)
			_play("smoke", p2, 2.5))
	_scorch(pos, r * 2.2)
	if audio:
		audio.nuclear_blast(pos)
	# the shock reaches the camera a moment later: wind, shake, then falling debris
	var cam_dist := 0.0
	if rig:
		cam_dist = Vector2(rig.target.x, rig.target.z).distance_to(Vector2(pos.x, pos.z))
	var arrive := clampf(cam_dist / front_speed, 0.3, 6.0)
	get_tree().create_timer(arrive).timeout.connect(func():
		if rig and rig.has_method("shake"):
			rig.shake(2.2)
		if audio:
			audio._play3d(audio._load("nuclear_wind"), rig.target if rig else pos, 6.0, 0.02))
	get_tree().create_timer(arrive + 3.0).timeout.connect(func():
		if audio:
			audio._play3d(audio._load("debris_rain"), pos, 0.0, 0.02))

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
