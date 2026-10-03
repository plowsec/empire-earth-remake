//! `GameView`: the Godot node that hosts a match. GDScript (UI, camera, input)
//! talks to the game exclusively through the #[func] methods below.
use crate::client::{player_color, Client, StartOptions};
use crate::terrain::{self, TILE};
use ee_sim::command::CommandKind;
use ee_sim::defs::{Class, DefId, NUM_RES, RES_NAMES};
use ee_sim::entity::{EntityId, Order, ProdItem};
use ee_sim::mapgen::GAIA;
use ee_sim::world::data;
use godot::classes::{
    Camera3D, CompressedTexture2DArray, FastNoiseLite, INode3D, MeshInstance3D, Node3D, NoiseTexture2D, PlaneMesh, Shader,
    ShaderMaterial, Texture2D,
};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node3D)]
pub struct GameView {
    base: Base<Node3D>,
    client: Option<Client>,
    camera: Option<Gd<Camera3D>>,
    terrain_mat: Option<Gd<ShaderMaterial>>,
    water_mat: Option<Gd<ShaderMaterial>>,
    place_def: Option<DefId>,
    ghost: Option<Gd<MeshInstance3D>>,
    ghost_mat: Option<Gd<ShaderMaterial>>,
    ghost_tile: (i32, i32),
    ghost_ok: bool,
    noise_tex: Option<Gd<Texture2D>>,
}

#[godot_api]
impl INode3D for GameView {
    fn init(base: Base<Node3D>) -> Self {
        GameView {
            base,
            client: None,
            camera: None,
            terrain_mat: None,
            water_mat: None,
            place_def: None,
            ghost: None,
            ghost_mat: None,
            ghost_tile: (0, 0),
            ghost_ok: false,
            noise_tex: None,
        }
    }

    fn process(&mut self, delta: f64) {
        let cam = self.camera.clone();
        if let Some(c) = self.client.as_mut() {
            c.update(delta, cam.as_ref());
        }
    }
}

fn noise_texture(seed: i32, freq: f32, size: i32, normal: bool) -> Gd<NoiseTexture2D> {
    let mut n = FastNoiseLite::new_gd();
    n.set_seed(seed);
    n.set_frequency(freq);
    n.set_fractal_octaves(5);
    let mut t = NoiseTexture2D::new_gd();
    t.set_width(size);
    t.set_height(size);
    t.set_seamless(true);
    t.set_generate_mipmaps(true);
    t.set_noise(&n);
    if normal {
        t.set_as_normal_map(true);
        t.set_bump_strength(6.0);
    }
    t
}

fn dict() -> VarDictionary {
    VarDictionary::new()
}

fn cost_dict(c: &ee_sim::defs::Cost) -> VarDictionary {
    let mut d = dict();
    for (i, v) in c.arr().iter().enumerate() {
        if *v > 0 {
            d.set(RES_NAMES[i], *v);
        }
    }
    d
}

#[godot_api]
impl GameView {
    #[signal]
    fn game_started();

    /// Start a skirmish. Keys: seed, players, difficulty, map_size, resources, pop_limit, reveal, start_res.
    #[func]
    fn start_game(&mut self, cfg: VarDictionary) {
        let get_i = |k: &str, def: i64| -> i64 { cfg.get(k).and_then(|v| v.try_to::<i64>().ok()).unwrap_or(def) };
        let opt = StartOptions {
            seed: get_i("seed", 7) as u64,
            players: get_i("players", 2) as usize,
            difficulty: get_i("difficulty", 1) as i32,
            map_size: get_i("map_size", 1) as u8,
            resources: get_i("resources", 100) as u32,
            pop_limit: get_i("pop_limit", 300) as i32,
            reveal: get_i("reveal", 0) != 0,
            start_res: get_i("start_res", 1500) as i32,
            ai_self: get_i("ai_self", 0) != 0,
        };
        // clear previous match
        for mut ch in self.base().get_children().iter_shared() {
            ch.queue_free();
        }
        self.client = None;
        let noise: Gd<Texture2D> = noise_texture(11, 0.02, 512, false).upcast();
        self.noise_tex = Some(noise.clone());
        let root = self.base().clone().upcast::<Node>();
        let client = Client::new(root, opt, Some(noise.clone()));
        self.build_world(&client, &noise);
        self.client = Some(client);
        self.signals().game_started().emit();
    }

    fn build_world(&mut self, c: &Client, noise: &Gd<Texture2D>) {
        let map = &c.world().map;
        let size = Vector2::new(map.w as f32 * TILE, map.h as f32 * TILE);
        // terrain material
        let mut tm = ShaderMaterial::new_gd();
        tm.set_shader(&godot::tools::load::<Shader>("res://shaders/terrain.gdshader"));
        tm.set_shader_parameter("albedo_tex", &godot::tools::load::<CompressedTexture2DArray>("res://assets/textures/terrain_albedo_array.png").to_variant());
        tm.set_shader_parameter("normal_tex", &godot::tools::load::<CompressedTexture2DArray>("res://assets/textures/terrain_normal_array.png").to_variant());
        let (ca, cb) = terrain::build_control_maps(map);
        tm.set_shader_parameter("control_a", &ca.to_variant());
        tm.set_shader_parameter("control_b", &cb.to_variant());
        tm.set_shader_parameter("fog_tex", &c.fog_tex.to_variant());
        tm.set_shader_parameter("macro_noise", &noise.to_variant());
        tm.set_shader_parameter("map_size", &size.to_variant());
        // per-layer tints (sand, grass, meadow, forest, dirt, grassrock, rock, seabed)
        let tints = PackedColorArray::from(&[
            Color::from_rgb(1.0, 0.95, 0.85),
            Color::from_rgb(0.62, 0.86, 0.42),
            Color::from_rgb(0.74, 0.92, 0.46),
            Color::from_rgb(0.72, 0.78, 0.6),
            Color::from_rgb(0.86, 0.8, 0.68),
            Color::from_rgb(0.74, 0.86, 0.6),
            Color::from_rgb(0.82, 0.8, 0.78),
            Color::from_rgb(0.85, 0.92, 0.85),
        ][..]);
        tm.set_shader_parameter("layer_tint", &tints.to_variant());
        let chunks_x = (map.w + terrain::CHUNK - 1) / terrain::CHUNK;
        let chunks_y = (map.h + terrain::CHUNK - 1) / terrain::CHUNK;
        let mut tnode = Node3D::new_alloc();
        tnode.set_name("Terrain");
        for cy in 0..chunks_y {
            for cx in 0..chunks_x {
                let mesh = terrain::build_chunk(&c.heights, cx, cy);
                let mut mi = MeshInstance3D::new_alloc();
                mi.set_mesh(&mesh);
                mi.set_material_override(&tm);
                mi.set_cast_shadows_setting(ShadowCastingSetting::ON);
                tnode.add_child(&mi);
            }
        }
        let apron = terrain::build_apron(&c.heights, -14.0, 2500.0);
        let mut ami = MeshInstance3D::new_alloc();
        ami.set_mesh(&apron);
        ami.set_material_override(&tm);
        tnode.add_child(&ami);
        self.base_mut().add_child(&tnode);

        // water: dense plane over the map, coarse plane to the horizon
        let mut wm = ShaderMaterial::new_gd();
        wm.set_shader(&godot::tools::load::<Shader>("res://shaders/water.gdshader"));
        wm.set_shader_parameter("normal_a", &noise_texture(3, 0.05, 512, true).to_variant());
        wm.set_shader_parameter("normal_b", &noise_texture(5, 0.09, 512, true).to_variant());
        wm.set_shader_parameter("foam_noise", &noise_texture(9, 0.06, 256, false).to_variant());
        wm.set_shader_parameter("fog_tex", &c.fog_tex.to_variant());
        wm.set_shader_parameter("map_size", &size.to_variant());
        let mut plane = PlaneMesh::new_gd();
        plane.set_size(size + Vector2::new(160.0, 160.0));
        plane.set_subdivide_width(300);
        plane.set_subdivide_depth(300);
        let mut wmi = MeshInstance3D::new_alloc();
        wmi.set_name("Water");
        wmi.set_mesh(&plane);
        wmi.set_material_override(&wm);
        wmi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        wmi.set_position(Vector3::new(size.x * 0.5, 0.0, size.y * 0.5));
        self.base_mut().add_child(&wmi);
        for (k, (ox, oz, sx, sz)) in [
            (0.5, -0.5 - 2.0, 6.0, 4.0),
            (0.5, 1.5 + 2.0, 6.0, 4.0),
            (-0.5 - 2.0, 0.5, 4.0, 1.0 + 0.4),
            (1.5 + 2.0, 0.5, 4.0, 1.0 + 0.4),
        ]
        .iter()
        .enumerate()
        {
            let mut far = PlaneMesh::new_gd();
            far.set_size(Vector2::new(size.x * sx, size.y * sz));
            far.set_subdivide_width(40);
            far.set_subdivide_depth(40);
            let mut fmi = MeshInstance3D::new_alloc();
            fmi.set_name(&format!("WaterFar{k}"));
            fmi.set_mesh(&far);
            fmi.set_material_override(&wm);
            fmi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            fmi.set_position(Vector3::new(size.x * ox, -0.05, size.y * oz));
            self.base_mut().add_child(&fmi);
        }
        self.terrain_mat = Some(tm);
        self.water_mat = Some(wm);
    }

    #[func]
    fn set_camera(&mut self, cam: Gd<Camera3D>) {
        self.camera = Some(cam);
    }

    #[func]
    fn is_running(&self) -> bool {
        self.client.is_some()
    }

    #[func]
    fn map_size(&self) -> Vector2 {
        match &self.client {
            Some(c) => Vector2::new(c.world().map.w as f32 * TILE, c.world().map.h as f32 * TILE),
            None => Vector2::ZERO,
        }
    }

    #[func]
    fn home_position(&self) -> Vector3 {
        let Some(c) = &self.client else { return Vector3::ZERO };
        let (x, y) = c.world().starts[c.me as usize];
        let wx = (x as f32 + 0.5) * TILE;
        let wz = (y as f32 + 0.5) * TILE;
        Vector3::new(wx, c.heights.at(wx, wz), wz)
    }

    #[func]
    fn height_at(&self, x: f32, z: f32) -> f32 {
        self.client.as_ref().map_or(0.0, |c| c.heights.at(x, z).max(0.0))
    }

    #[func]
    fn set_speed(&mut self, s: f64) {
        if let Some(c) = self.client.as_mut() {
            c.session.speed = s;
        }
    }

    /// Fast-forward the simulation by `ticks` (demo/testing).
    #[func]
    fn warp(&mut self, ticks: i64) {
        if let Some(c) = self.client.as_mut() {
            for _ in 0..ticks {
                c.session.step_once();
            }
            c.session.event_log.clear();
        }
    }

    #[func]
    fn set_show_all_bars(&mut self, on: bool) {
        if let Some(c) = self.client.as_mut() {
            c.show_all_bars = on;
        }
    }

    #[func]
    fn set_paused(&mut self, p: bool) {
        if let Some(c) = self.client.as_mut() {
            c.session.paused = p;
        }
    }

    #[func]
    fn is_paused(&self) -> bool {
        self.client.as_ref().map_or(false, |c| c.session.paused)
    }

    #[func]
    fn set_reveal(&mut self, r: bool) {
        if let Some(c) = self.client.as_mut() {
            c.reveal = r;
            c.session.world.config.reveal = r;
        }
        let f = if r { 0.0f32 } else { 1.0 };
        if let Some(m) = self.terrain_mat.as_mut() {
            m.set_shader_parameter("fog_enabled", &f.to_variant());
        }
        if let Some(m) = self.water_mat.as_mut() {
            m.set_shader_parameter("fog_enabled", &f.to_variant());
        }
    }

    #[func]
    fn minimap_texture(&self) -> Option<Gd<Texture2D>> {
        self.client.as_ref().map(|c| c.minimap_tex.clone().upcast())
    }

    /// Player HUD state.
    #[func]
    fn player_state(&self) -> VarDictionary {
        let mut d = dict();
        let Some(c) = &self.client else { return d };
        let w = c.world();
        let p = &w.players[c.me as usize];
        let mut res = VarArray::new();
        for r in 0..NUM_RES {
            res.push(&p.res[r].to_variant());
        }
        d.set("res", &res);
        d.set("pop", p.pop);
        d.set("pop_cap", p.pop_cap);
        d.set("tick", w.tick as i64);
        d.set("seconds", (w.tick / 20) as i64);
        d.set("game_over", w.game_over);
        d.set("defeated", p.defeated);
        d.set("won", w.game_over && w.winner_team == Some(p.team));
        d.set("speed", c.session.speed);
        d
    }

    #[func]
    fn players(&self) -> VarArray {
        let mut out = VarArray::new();
        let Some(c) = &self.client else { return out };
        for p in &c.world().players {
            let mut d = dict();
            d.set("id", p.id as i64);
            d.set("name", p.name.as_str());
            d.set("color", player_color(p.color));
            d.set("defeated", p.defeated);
            d.set("kills", p.stats.kills as i64);
            d.set("lost", p.stats.lost as i64);
            d.set("built", p.stats.built as i64);
            d.set("razed", p.stats.razed as i64);
            d.set("trained", p.stats.trained as i64);
            let gathered: i64 = p.stats.gathered.iter().sum();
            d.set("gathered", gathered);
            out.push(&d.to_variant());
        }
        out
    }

    // ------------------------------------------------------------------ input

    #[func]
    fn screen_to_ground(&self, p: Vector2) -> Vector3 {
        match (&self.client, &self.camera) {
            (Some(c), Some(cam)) => c.ground_at(cam, p).unwrap_or(Vector3::new(-1.0, -1.0, -1.0)),
            _ => Vector3::ZERO,
        }
    }

    #[func]
    fn hover(&mut self, p: Vector2) -> VarDictionary {
        let mut d = dict();
        let (Some(c), Some(cam)) = (self.client.as_mut(), self.camera.as_ref()) else { return d };
        let id = c.pick(cam, p);
        c.hover = id;
        if let Some(e) = c.world().get(id) {
            let dd = data().def(e.def);
            d.set("id", id as i64);
            d.set("name", dd.data.name.as_str());
            d.set("own", e.owner == c.me);
            d.set("enemy", c.world().is_enemy(c.me, e.owner));
            d.set("resource", dd.is_resource());
            if dd.is_resource() {
                d.set("amount", e.amount);
            }
        }
        d
    }

    #[func]
    fn select_rect(&mut self, a: Vector2, b: Vector2, add: bool) {
        if let (Some(c), Some(cam)) = (self.client.as_mut(), self.camera.as_ref()) {
            c.select_rect(cam, a, b, add);
        }
    }

    #[func]
    fn select_click(&mut self, p: Vector2, add: bool, same_type: bool) {
        if let (Some(c), Some(cam)) = (self.client.as_mut(), self.camera.as_ref()) {
            c.select_click(cam, p, add, same_type);
        }
    }

    #[func]
    fn select_ids(&mut self, ids: PackedInt64Array) {
        if let Some(c) = self.client.as_mut() {
            c.selection = ids.as_slice().iter().map(|&i| i as EntityId).collect();
        }
    }

    #[func]
    fn select_idle_citizen(&mut self) -> Vector3 {
        let Some(c) = self.client.as_mut() else { return Vector3::ZERO };
        let w = c.world();
        let cur = c.selection.first().copied().unwrap_or(0);
        let idle: Vec<&ee_sim::entity::Entity> = w
            .entities
            .iter()
            .filter(|e| e.alive && e.owner == c.me && e.inside == 0 && data().def(e.def).class() == Class::Citizen && e.order == Order::Idle)
            .collect();
        if idle.is_empty() {
            return Vector3::new(-1.0, -1.0, -1.0);
        }
        let i = idle.iter().position(|e| e.id == cur).map_or(0, |i| (i + 1) % idle.len());
        let e = idle[i];
        let (x, z) = crate::client::to_world2(e.pos);
        let id = e.id;
        c.selection = vec![id];
        Vector3::new(x, 0.0, z)
    }

    /// Right click at a screen point.
    #[func]
    fn right_click(&mut self, p: Vector2, queue: bool) {
        let (Some(c), Some(cam)) = (self.client.as_mut(), self.camera.as_ref()) else { return };
        let target = c.pick(cam, p);
        let ground = c.ground_at(cam, p);
        c.command_at(target, ground, queue, false);
    }

    #[func]
    fn attack_move_click(&mut self, p: Vector2, queue: bool) {
        let (Some(c), Some(cam)) = (self.client.as_mut(), self.camera.as_ref()) else { return };
        let target = c.pick(cam, p);
        let enemy = c.world().get(target).map_or(false, |e| c.world().is_enemy(c.me, e.owner));
        let ground = c.ground_at(cam, p);
        if enemy {
            let units = c.my_selected_units();
            c.issue(CommandKind::Attack { units, target, queue });
        } else {
            c.command_at(0, ground, queue, true);
        }
    }

    /// Minimap right-click: world position.
    #[func]
    fn command_world(&mut self, pos: Vector3, queue: bool) {
        if let Some(c) = self.client.as_mut() {
            c.command_at(0, Some(pos), queue, false);
        }
    }

    #[func]
    fn selection_count(&self) -> i64 {
        self.client.as_ref().map_or(0, |c| c.selection.len() as i64)
    }

    #[func]
    fn selection_center(&self) -> Vector3 {
        let Some(c) = &self.client else { return Vector3::ZERO };
        let w = c.world();
        let mut sum = Vector3::ZERO;
        let mut n = 0.0;
        for &id in &c.selection {
            if let Some(e) = w.get(id) {
                let (x, z) = crate::client::to_world2(e.pos);
                sum += Vector3::new(x, 0.0, z);
                n += 1.0;
            }
        }
        if n > 0.0 { sum / n } else { Vector3::new(-1.0, -1.0, -1.0) }
    }

    #[func]
    fn set_group(&mut self, g: i64) {
        if let Some(c) = self.client.as_mut() {
            let sel = c.selection.clone();
            if let Some(gr) = c.groups.get_mut(g as usize) {
                *gr = sel;
            }
        }
    }

    #[func]
    fn recall_group(&mut self, g: i64) -> bool {
        let Some(c) = self.client.as_mut() else { return false };
        let ids: Vec<EntityId> = c.groups.get(g as usize).cloned().unwrap_or_default();
        let alive: Vec<EntityId> = ids.into_iter().filter(|&id| c.world().get(id).is_some()).collect();
        if alive.is_empty() {
            return false;
        }
        c.selection = alive;
        true
    }

    // ------------------------------------------------------------------ selection panel

    #[func]
    fn selection_info(&self) -> VarArray {
        let mut out = VarArray::new();
        let Some(c) = &self.client else { return out };
        let w = c.world();
        for &id in c.selection.iter().take(64) {
            let Some(e) = w.get(id) else { continue };
            let d = data().def(e.def);
            let mut x = dict();
            x.set("id", id as i64);
            x.set("key", d.data.key.as_str());
            x.set("name", d.data.name.as_str());
            x.set("role", d.data.role.as_str());
            x.set("hp", e.hp);
            x.set("max_hp", w.max_hp(e));
            x.set("own", e.owner == c.me);
            x.set("owner_color", player_color(w.players.get(e.owner as usize).map_or(7, |p| p.color)));
            x.set("class", format!("{:?}", d.class()).as_str());
            x.set("armor", d.data.armor + w.mods(e.owner, e.def).armor);
            if let Some(wp) = d.weapons.first() {
                x.set("attack", wp.damage * (100 + w.mods(e.owner, e.def).attack_pct) / 100 * wp.burst);
                x.set("range", wp.range.to_f32());
            }
            if d.is_resource() {
                x.set("amount", e.amount);
            }
            if e.carry > 0 {
                x.set("carry", e.carry);
                x.set("carry_res", RES_NAMES[(e.carry_res as usize).min(4)]);
            }
            if d.data.cargo > 0 || d.data.airport {
                x.set("cargo", e.cargo.len() as i64);
                x.set("cargo_max", d.data.cargo as i64);
            }
            if d.fuel_ticks > 0 {
                x.set("fuel", e.fuel as f32 / d.fuel_ticks as f32);
            }
            if d.is_building() {
                x.set("complete", e.complete);
                x.set("progress", e.progress as f32 / d.build_ticks.max(1) as f32);
            }
            x.set("kills", e.kills as i64);
            x.set("order", format!("{:?}", e.order).split(' ').next().unwrap_or("").trim_end_matches('{').to_string().as_str());
            out.push(&x.to_variant());
        }
        out
    }

    /// Production queue of the first selected own building.
    #[func]
    fn production_queue(&self) -> VarArray {
        let mut out = VarArray::new();
        let Some(c) = &self.client else { return out };
        let w = c.world();
        let Some(&bid) = c.my_selected_buildings().first() else { return out };
        let b = w.get(bid).unwrap();
        for (i, item) in b.production.iter().enumerate() {
            let mut x = dict();
            let (key, name, ticks) = match item {
                ProdItem::Unit(u) => {
                    let d = data().def(*u);
                    (d.data.key.clone(), d.data.name.clone(), d.build_ticks)
                }
                ProdItem::Tech(t) => {
                    let td = &data().techs[*t as usize];
                    (format!("tech:{}", td.data.key), td.data.name.clone(), td.research_ticks)
                }
            };
            x.set("index", i as i64);
            x.set("key", key.as_str());
            x.set("name", name.as_str());
            x.set("progress", if i == 0 { b.prod_progress as f32 / ticks.max(1) as f32 } else { 0.0 });
            out.push(&x.to_variant());
        }
        out
    }

    /// Buttons for the command card, based on the current selection.
    #[func]
    fn command_card(&self) -> VarArray {
        let mut out = VarArray::new();
        let Some(c) = &self.client else { return out };
        let w = c.world();
        let p = &w.players[c.me as usize];
        let units = c.my_selected_units();
        let buildings = c.my_selected_buildings();
        let mut button = |action: &str, key: &str, label: &str, hotkey: &str, cost: Option<&ee_sim::defs::Cost>, enabled: bool, tip: &str| {
            let mut b = dict();
            b.set("action", action);
            b.set("key", key);
            b.set("label", label);
            b.set("hotkey", hotkey);
            if let Some(cst) = cost {
                b.set("cost", &cost_dict(cst));
            }
            b.set("enabled", enabled);
            b.set("tooltip", tip);
            out.push(&b.to_variant());
        };
        const HOTKEYS: [&str; 15] = ["Q", "W", "E", "R", "T", "F", "G", "Z", "X", "C", "V", "B", "N", "J", "K"];
        if !units.is_empty() {
            let has_citizen = units.iter().any(|&id| w.class_of(id) == Some(Class::Citizen));
            if has_citizen {
                let cit = data().def(data().id("citizen"));
                for (i, &b) in cit.builds.iter().enumerate() {
                    let d = data().def(b);
                    let afford = w.can_afford(c.me, &d.data.cost);
                    button("build", &d.data.key, &d.data.name, HOTKEYS.get(i).copied().unwrap_or(""), Some(&d.data.cost), afford, &d.data.role);
                }
            }
            let any_transport = units.iter().any(|&id| w.get(id).map_or(false, |e| data().def(e.def).data.cargo > 0 && !e.cargo.is_empty()));
            let any_air = units.iter().any(|&id| w.get(id).map_or(false, |e| data().def(e.def).data.needs_airport));
            let any_combat = units.iter().any(|&id| w.get(id).map_or(false, |e| data().def(e.def).can_attack()));
            if any_combat {
                button("attack_move", "", "Attack-Move", "A", None, true, "Move, engaging enemies on the way (A, then click).");
            }
            button("stop", "", "Stop", "H", None, true, "Halt all orders.");
            if any_transport {
                button("unload", "", "Unload", "U", None, true, "Unload cargo at a shore point (U, then click).");
            }
            if any_air {
                button("rtb", "", "Return to Base", "Y", None, true, "Fly back to the nearest airfield to refuel and rearm.");
            }
            button("delete", "", "Disband", "Delete", None, true, "Destroy the selected units.");
        } else if let Some(&bid) = buildings.first() {
            let b = w.get(bid).unwrap();
            let d = data().def(b.def);
            if b.complete {
                for (i, &u) in d.trains.iter().enumerate() {
                    let ud = data().def(u);
                    let afford = w.can_afford(c.me, &ud.data.cost);
                    let tip = format!("{}\nHP {}  Armor {}{}", ud.data.role, ud.data.hp, ud.data.armor, ud.weapons.first().map(|wp| format!("  Attack {}  Range {:.1}", wp.damage * wp.burst, wp.range.to_f32())).unwrap_or_default());
                    button("train", &ud.data.key, &ud.data.name, HOTKEYS.get(i).copied().unwrap_or(""), Some(&ud.data.cost), afford, &tip);
                }
                let mut k = d.trains.len();
                for t in &data().techs {
                    if t.at != b.def || p.techs[t.id as usize] {
                        continue;
                    }
                    let busy = p.researching[t.id as usize];
                    let afford = w.can_afford(c.me, &t.data.cost) && !busy;
                    button("research", &t.data.key, &t.data.name, HOTKEYS.get(k).copied().unwrap_or(""), Some(&t.data.cost), afford, &t.data.role);
                    k += 1;
                }
                if d.data.airport && !b.cargo.is_empty() {
                    button("launch", "", "Launch Aircraft", "L", None, true, "Select the aircraft parked here.");
                }
            }
            button("delete", "", "Demolish", "Delete", None, true, "Destroy this building.");
        }
        out
    }

    /// Execute a command-card button. Returns a mode string for GDScript
    /// ("place", "attack_move", "unload") or "".
    #[func]
    fn do_action(&mut self, action: GString, key: GString) -> GString {
        let action = action.to_string();
        let key = key.to_string();
        let Some(c) = self.client.as_mut() else { return GString::new() };
        let units = c.my_selected_units();
        let buildings = c.my_selected_buildings();
        match action.as_str() {
            "train" => {
                if let (Some(&b), Some(def)) = (buildings.first(), data().try_id(&key)) {
                    // spread across all selected buildings of this type
                    let bdef = c.world().get(b).map(|e| e.def);
                    let same: Vec<EntityId> = buildings.iter().copied().filter(|&x| c.world().get(x).map(|e| e.def) == bdef).collect();
                    let best = same.iter().copied().min_by_key(|&x| c.world().get(x).map_or(99, |e| e.production.len())).unwrap_or(b);
                    c.issue(CommandKind::Train { building: best, def, count: 1 });
                }
            }
            "research" => {
                if let (Some(&b), Some(t)) = (buildings.first(), data().tech_id(&key)) {
                    c.issue(CommandKind::Research { building: b, tech: t });
                }
            }
            "build" => {
                if let Some(def) = data().try_id(&key) {
                    self.place_def = Some(def);
                    self.make_ghost(def);
                    return "place".into();
                }
            }
            "stop" => c.issue(CommandKind::Stop { units }),
            "rtb" => c.issue(CommandKind::ReturnToBase { units }),
            "attack_move" => return "attack_move".into(),
            "unload" => return "unload".into(),
            "delete" => {
                let mut all = units;
                all.extend(buildings);
                c.issue(CommandKind::Delete { units: all });
            }
            "launch" => {
                if let Some(&b) = buildings.first() {
                    let cargo = c.world().get(b).map(|e| e.cargo.clone()).unwrap_or_default();
                    c.selection = cargo;
                }
            }
            "cancel" => {
                if let Some(&b) = buildings.first() {
                    let idx: u8 = key.parse().unwrap_or(0);
                    c.issue(CommandKind::CancelProduction { building: b, index: idx });
                }
            }
            _ => {}
        }
        GString::new()
    }

    #[func]
    fn unload_click(&mut self, p: Vector2) {
        let (Some(c), Some(cam)) = (self.client.as_mut(), self.camera.as_ref()) else { return };
        if let Some(g) = c.ground_at(cam, p) {
            let units = c.my_selected_units();
            let at = crate::client::from_world(g.x, g.z);
            c.issue(CommandKind::Unload { units, at });
        }
    }

    // ------------------------------------------------------------------ placement

    fn make_ghost(&mut self, def: DefId) {
        if let Some(mut g) = self.ghost.take() {
            g.queue_free();
        }
        let Some(c) = &self.client else { return };
        let model = &c.models.list[c.models.by_def[def as usize]];
        let mut root = MeshInstance3D::new_alloc();
        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&godot::tools::load::<Shader>("res://shaders/ghost.gdshader"));
        for part in &model.parts {
            let mut mi = MeshInstance3D::new_alloc();
            mi.set_mesh(&part.mesh);
            mi.set_transform(part.local);
            mi.set_material_override(&mat);
            mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            root.add_child(&mi);
        }
        self.base_mut().add_child(&root);
        self.ghost = Some(root);
        self.ghost_mat = Some(mat);
    }

    #[func]
    fn placement_update(&mut self, p: Vector2) {
        let (Some(def), Some(c), Some(cam)) = (self.place_def, self.client.as_ref(), self.camera.as_ref()) else { return };
        let Some(g) = c.ground_at(cam, p) else { return };
        let d = data().def(def);
        let (sw, sh) = d.size();
        let tx = (g.x / TILE - sw as f32 * 0.5).round() as i32;
        let ty = (g.z / TILE - sh as f32 * 0.5).round() as i32;
        let ok = c.world().can_place(c.me, def, (tx, ty)).is_ok() && c.world().can_afford(c.me, &d.data.cost);
        let cx = (tx as f32 + sw as f32 * 0.5) * TILE;
        let cz = (ty as f32 + sh as f32 * 0.5) * TILE;
        let cy = c.heights.at(cx, cz).max(0.0);
        self.ghost_tile = (tx, ty);
        self.ghost_ok = ok;
        if let Some(gh) = self.ghost.as_mut() {
            gh.set_position(Vector3::new(cx, cy, cz));
        }
        if let Some(m) = self.ghost_mat.as_mut() {
            let col = if ok { Color::from_rgba(0.3, 1.0, 0.45, 0.45) } else { Color::from_rgba(1.0, 0.25, 0.2, 0.45) };
            m.set_shader_parameter("tint", &col.to_variant());
        }
    }

    /// Place the building. Returns true if placement mode should continue (shift).
    #[func]
    fn placement_confirm(&mut self, keep: bool) -> bool {
        let Some(def) = self.place_def else { return false };
        if !self.ghost_ok {
            if let Some(c) = self.client.as_mut() {
                let reason = c.world().can_place(c.me, def, self.ghost_tile).err().unwrap_or("Not enough resources");
                c.events.push(crate::client::ClientEvent { kind: "notice", pos: Vector3::ZERO, to: Vector3::ZERO, size: 0.0, text: reason.to_string(), dmg: 0, mine: true });
            }
            return true;
        }
        let tile = self.ghost_tile;
        if let Some(c) = self.client.as_mut() {
            let units: Vec<EntityId> = c
                .my_selected_units()
                .into_iter()
                .filter(|&id| c.world().class_of(id) == Some(Class::Citizen))
                .collect();
            if !units.is_empty() {
                c.issue(CommandKind::Build { units, def, tile, queue: keep });
            }
        }
        if !keep {
            self.cancel_placement();
        }
        keep
    }

    #[func]
    fn cancel_placement(&mut self) {
        self.place_def = None;
        if let Some(mut g) = self.ghost.take() {
            g.queue_free();
        }
    }

    /// "" if the current ghost placement is valid, otherwise the reason.
    #[func]
    fn placement_error(&self) -> GString {
        let (Some(def), Some(c)) = (self.place_def, self.client.as_ref()) else { return "not placing".into() };
        match c.world().can_place(c.me, def, self.ghost_tile) {
            Ok(()) if self.ghost_ok => GString::new(),
            Ok(()) => "not enough resources".into(),
            Err(e) => e.into(),
        }
    }

    #[func]
    fn is_placing(&self) -> bool {
        self.place_def.is_some()
    }

    // ------------------------------------------------------------------ events

    /// Drain events for VFX/audio/UI since the last call.
    #[func]
    fn take_events(&mut self) -> VarArray {
        let mut out = VarArray::new();
        let Some(c) = self.client.as_mut() else { return out };
        for e in c.events.drain(..) {
            let mut d = dict();
            d.set("kind", e.kind);
            d.set("pos", e.pos);
            d.set("to", e.to);
            d.set("size", e.size);
            d.set("text", e.text.as_str());
            d.set("dmg", e.dmg as i64);
            d.set("mine", e.mine);
            out.push(&d.to_variant());
        }
        out
    }

    /// Stress test: spawn `n` mixed units per side near the local player's base,
    /// facing each other, and order them to attack-move. Returns the battle center.
    #[func]
    fn debug_spawn_battle(&mut self, n: i64) -> Vector3 {
        let Some(c) = self.client.as_mut() else { return Vector3::ZERO };
        let w = &mut c.session.world;
        let (sx, sy) = w.starts[0];
        let mix = ["rifleman", "rifleman", "machine_gunner", "bazooka", "tank", "rifleman", "mortar", "aa_vehicle", "at_gun", "medic"];
        // find an open area on the island
        let mut center = (sx, sy + 10);
        'search: for r in (6..30).rev() {
            for (dx, dy) in [(0, 1), (1, 0), (0, -1), (-1, 0)] {
                let t = (sx + dx * r, sy + dy * r);
                let ok = (-6..=6).all(|oy| (-10..=10).all(|ox| w.map.passable(t.0 + ox, t.1 + oy, ee_sim::defs::Layer::Land)));
                if ok {
                    center = t;
                    break 'search;
                }
            }
        }
        let mut ids = [Vec::new(), Vec::new()];
        for side in 0..2u8 {
            let owner = side;
            for k in 0..n as i32 {
                let key = mix[(k as usize) % mix.len()];
                let def = data().id(key);
                let row = k / 40;
                let col = k % 40;
                let x = center.0 as i32 * 65536 + (col - 20) * 65536 * 45 / 100;
                let y = center.1 as i32 * 65536 + (if side == 0 { -6 - row } else { 6 + row }) * 65536 * 45 / 100;
                let p = ee_sim::fixed::FVec::new(ee_sim::fixed::Fx(x), ee_sim::fixed::Fx(y));
                if w.map.passable_at(p, ee_sim::defs::Layer::Land) {
                    ids[side as usize].push(w.spawn(def, owner, p));
                }
            }
        }
        let target = ee_sim::fixed::FVec::tile_center(center.0, center.1);
        for side in 0..2u8 {
            let cmd = ee_sim::command::Command {
                player: side,
                kind: CommandKind::Move { units: ids[side as usize].clone(), to: target, attack_move: true, queue: false },
            };
            w.apply_command(&cmd);
        }
        w.recount_pop();
        let (x, z) = crate::client::to_world2(target);
        Vector3::new(x, 0.0, z)
    }

    /// Wall-clock milliseconds the last frame's simulation took (perf HUD).
    #[func]
    fn sim_stats(&self) -> VarDictionary {
        let mut d = dict();
        let Some(c) = &self.client else { return d };
        let w = c.world();
        d.set("units", w.entities.iter().filter(|e| e.alive && e.owner != GAIA && data().def(e.def).is_unit()).count() as i64);
        d.set("entities", w.entities.iter().filter(|e| e.alive).count() as i64);
        d.set("projectiles", w.projectiles.len() as i64);
        d.set("tick", w.tick as i64);
        d.set("sim_ms", c.last_sim_ms);
        d
    }

    /// Number of entities of `key` owned by the local player (tests/UI).
    #[func]
    fn count_owned(&self, key: GString) -> i64 {
        let Some(c) = &self.client else { return 0 };
        let Some(def) = data().try_id(&key.to_string()) else { return 0 };
        c.world().entities.iter().filter(|e| e.alive && e.owner == c.me && e.def == def).count() as i64
    }

    /// Debug: describe the local player's first entity of `key`.
    #[func]
    fn entity_debug(&self, key: GString) -> GString {
        let Some(c) = &self.client else { return GString::new() };
        let Some(def) = data().try_id(&key.to_string()) else { return GString::new() };
        for e in &c.world().entities {
            if e.alive && e.owner == c.me && e.def == def {
                return GString::from(&format!("{} pos {:?} order {:?} action {:?} hp {} progress {} complete {} goal {:?} stuck {}", key, e.pos.tile(), e.order, e.action, e.hp, e.progress, e.complete, e.goal.map(|g| g.tile()), e.stuck));
            }
        }
        GString::from("none")
    }

    /// Debug: what's under a screen point.
    #[func]
    fn debug_pick(&self, p: Vector2) -> GString {
        let (Some(c), Some(cam)) = (&self.client, &self.camera) else { return GString::new() };
        let id = c.pick(cam, p);
        let g = c.ground_at(cam, p);
        GString::from(&format!("pick {} ground {:?}", id, g))
    }

    /// Screen position of the local player's first entity of `key` (tests).
    #[func]
    fn screen_pos_of(&self, key: GString) -> Vector2 {
        let (Some(c), Some(cam)) = (&self.client, &self.camera) else { return Vector2::new(-1.0, -1.0) };
        let Some(def) = data().try_id(&key.to_string()) else { return Vector2::new(-1.0, -1.0) };
        for e in &c.world().entities {
            if e.alive && e.owner == c.me && e.def == def {
                let (x, z) = crate::client::to_world2(e.pos);
                let y = c.heights.at(x, z).max(0.0) + 1.0;
                return cam.unproject_position(Vector3::new(x, y, z));
            }
        }
        Vector2::new(-1.0, -1.0)
    }

    /// Screen position of the nearest resource of `key` to the player's start (tests).
    #[func]
    fn screen_pos_of_resource(&self, key: GString) -> Vector2 {
        let (Some(c), Some(cam)) = (&self.client, &self.camera) else { return Vector2::new(-1.0, -1.0) };
        let Some(def) = data().try_id(&key.to_string()) else { return Vector2::new(-1.0, -1.0) };
        let (sx, sy) = c.world().starts[c.me as usize];
        let home = ee_sim::fixed::FVec::tile_center(sx, sy);
        let best = c.world().entities.iter().filter(|e| e.alive && e.def == def).min_by_key(|e| e.pos.dist2_raw(home));
        match best {
            Some(e) => {
                let (x, z) = crate::client::to_world2(e.pos);
                cam.unproject_position(Vector3::new(x, c.heights.at(x, z).max(0.0) + 0.5, z))
            }
            None => Vector2::new(-1.0, -1.0),
        }
    }

    /// World position of the biggest recent fight (for demo camera).
    #[func]
    fn hotspot(&self) -> Vector3 {
        let Some(c) = &self.client else { return Vector3::ZERO };
        let w = c.world();
        let mut best: Option<(usize, Vector3)> = None;
        for e in &w.entities {
            if !e.alive || e.owner == GAIA || w.tick.wrapping_sub(e.last_fire_tick) > 40 || e.last_fire_tick == 0 {
                continue;
            }
            let (x, z) = crate::client::to_world2(e.pos);
            let p = Vector3::new(x, 0.0, z);
            let n = w.entities.iter().filter(|o| o.alive && o.owner != GAIA && w.tick.wrapping_sub(o.last_fire_tick) < 40 && o.last_fire_tick > 0 && {
                let (ox, oz) = crate::client::to_world2(o.pos);
                (ox - x).abs() < 40.0 && (oz - z).abs() < 40.0
            }).count();
            if best.map_or(true, |(b, _)| n > b) {
                best = Some((n, p));
            }
        }
        best.map_or(Vector3::new(-1.0, -1.0, -1.0), |b| b.1)
    }

    /// Key game stats for the end screen / debugging.
    #[func]
    fn debug_line(&self) -> GString {
        let Some(c) = &self.client else { return GString::new() };
        let w = c.world();
        let units = w.entities.iter().filter(|e| e.alive && e.owner != GAIA && data().def(e.def).is_unit()).count();
        GString::from(&format!("tick {} | units {} | proj {} | {}", w.tick, units, w.projectiles.len(), c.session.controller_debug().join(" | ")))
    }
}
