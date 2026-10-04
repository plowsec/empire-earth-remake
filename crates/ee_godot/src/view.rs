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
    field_ghosts: Vec<(Gd<MeshInstance3D>, Gd<ShaderMaterial>)>,
}

/// Farm plots around a granary (relative to its top-left tile; same as RebuildFarms).
const FIELD_SLOTS: [(i32, i32); 8] = [(-3, 0), (3, 0), (0, -3), (0, 3), (-3, -3), (3, -3), (-3, 3), (3, 3)];

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
            field_ghosts: Vec::new(),
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
        let demo = opt.ai_self;
        let mut client = Client::new(root, opt, Some(noise.clone()));
        if !demo {
            client.replay_path = Some(new_replay_path(client.world().config.seed));
            client.autosave_dir = Some(user_dir("saves"));
        }
        self.build_world(&client, &noise);
        self.client = Some(client);
        self.signals().game_started().emit();
    }

    /// Save the running match to user://saves/<name>.eesave. Returns "" or an error.
    #[func]
    fn save_game(&mut self, name: GString) -> GString {
        let Some(c) = self.client.as_mut() else { return "no game running".into() };
        let safe: String = name.to_string().chars().map(|ch| if ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == ' ' || ch == '(' || ch == ')' { ch } else { '_' }).collect();
        let safe = if safe.trim().is_empty() { "Quicksave".to_string() } else { safe.trim().to_string() };
        if c.autosave_dir.is_none() {
            c.autosave_dir = Some(user_dir("saves"));
        }
        match c.save_to(&safe, false) {
            Ok(_) => GString::new(),
            Err(e) => GString::from(&format!("save failed: {e}")),
        }
    }

    /// Delete a save file (from the save dialog / load menu).
    #[func]
    fn delete_save(&self, path: GString) -> bool {
        let p = path.to_string();
        p.ends_with(".eesave") && std::fs::remove_file(&p).is_ok()
    }

    /// Saved games, newest first: [{name, path, time}].
    #[func]
    fn list_saves(&self) -> VarArray {
        let dir = user_dir("saves");
        let mut found: Vec<(std::time::SystemTime, String, String)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("eesave") {
                    continue;
                }
                let t = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                let name = p.file_stem().and_then(|x| x.to_str()).unwrap_or("?").to_string();
                found.push((t, name, p.to_string_lossy().to_string()));
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0));
        let mut out = VarArray::new();
        for (t, name, path) in found {
            let mut d = VarDictionary::new();
            d.set("name", name);
            d.set("path", path);
            let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
            d.set("time", godot::classes::Time::singleton().get_datetime_string_from_unix_time(secs).to_string().replace('T', " "));
            out.push(&d.to_variant());
        }
        out
    }

    /// Resume a saved match. Returns "" or an error.
    #[func]
    fn load_game(&mut self, path: GString) -> GString {
        let text = match std::fs::read_to_string(path.to_string()) {
            Ok(t) => t,
            Err(e) => return GString::from(&format!("could not read save: {e}")),
        };
        let (session, reveal, history) = match crate::save::load(&text) {
            Ok(x) => x,
            Err(e) => return GString::from(&e),
        };
        for mut ch in self.base().get_children().iter_shared() {
            ch.queue_free();
        }
        self.client = None;
        let noise: Gd<Texture2D> = noise_texture(11, 0.02, 512, false).upcast();
        self.noise_tex = Some(noise.clone());
        let root = self.base().clone().upcast::<Node>();
        let mut client = Client::from_session(root, session, reveal, Some(noise.clone()));
        client.replay_path = Some(new_replay_path(client.world().config.seed));
        client.autosave_dir = Some(user_dir("saves"));
        client.history = history;
        self.build_world(&client, &noise);
        self.client = Some(client);
        self.signals().game_started().emit();
        GString::new()
    }

    /// Flush the replay file (call when leaving a match).
    #[func]
    fn write_replay(&self) {
        if let Some(c) = &self.client {
            c.write_replay();
        }
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
            Color::from_rgb(1.0, 0.95, 0.86),
            Color::from_rgb(0.7, 0.8, 0.58),
            Color::from_rgb(0.78, 0.82, 0.62),
            Color::from_rgb(0.56, 0.62, 0.5),
            Color::from_rgb(0.84, 0.78, 0.68),
            Color::from_rgb(0.74, 0.78, 0.66),
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
                let t = c.session.world.tick;
                if c.history.last().map_or(true, |s| t >= s.tick + 600) {
                    c.history.push(crate::stats::sample(&c.session.world));
                }
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

    /// Everything the end screen shows: per-player totals + the sampled history.
    #[func]
    fn end_stats(&self) -> VarDictionary {
        let mut d = dict();
        let Some(c) = &self.client else { return d };
        let w = c.world();
        let mut players = VarArray::new();
        for pl in &w.players {
            let (m, e, t) = crate::stats::score_parts(w, pl.id);
            let mut x = dict();
            x.set("id", pl.id as i64);
            x.set("name", pl.name.as_str());
            x.set("color", player_color(pl.color));
            x.set("me", pl.id == c.me);
            x.set("won", w.game_over && w.winner_team == Some(pl.team));
            x.set("defeated", pl.defeated);
            x.set("score", m + e + t);
            x.set("military", m);
            x.set("economy", e);
            x.set("technology", t);
            x.set("kills", pl.stats.kills as i64);
            x.set("lost", pl.stats.lost as i64);
            x.set("trained", pl.stats.trained as i64);
            x.set("built", pl.stats.built as i64);
            x.set("razed", pl.stats.razed as i64);
            let mut g = VarArray::new();
            for r in 0..NUM_RES {
                g.push(&pl.stats.gathered[r].to_variant());
            }
            x.set("gathered", &g);
            x.set("techs", pl.techs.iter().filter(|t| **t).count() as i64);
            // peaks from the history
            let peak = |k: usize| c.history.iter().filter_map(|s| s.values.get(pl.id as usize)).map(|v| v[k] as i64).max().unwrap_or(0);
            x.set("peak_pop", peak(1));
            x.set("peak_army", peak(2));
            players.push(&x.to_variant());
        }
        d.set("players", &players);
        d.set("seconds", (w.tick / 20) as i64);
        let mut times = VarArray::new();
        for s in &c.history {
            times.push(&((s.tick / 20) as i64).to_variant());
        }
        d.set("times", &times);
        let mut series = dict();
        for (k, name) in crate::stats::METRICS.iter().enumerate() {
            let mut per = VarArray::new();
            for p in 0..w.players.len() {
                let mut vals = PackedFloat32Array::new();
                for s in &c.history {
                    vals.push(s.values.get(p).map_or(0.0, |v| v[k]));
                }
                per.push(&vals.to_variant());
            }
            series.set(*name, &per);
        }
        d.set("series", &series);
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

    /// What a right-click would do at this screen point, for the mouse cursor:
    /// "attack", "gather", "build", "repair", "board", "land", "move", "rally" or "".
    #[func]
    fn cursor_context(&self, p: Vector2) -> GString {
        let (Some(c), Some(cam)) = (&self.client, &self.camera) else { return GString::new() };
        let w = c.world();
        let units = c.my_selected_units();
        let buildings = c.my_selected_buildings();
        if units.is_empty() && buildings.is_empty() {
            return GString::new();
        }
        let target = c.pick(cam, p);
        if units.is_empty() {
            let silo = buildings.iter().any(|&b| w.get(b).map_or(false, |e| data().def(e.def).trains.iter().any(|&u| data().def(u).data.icbm) && !e.cargo.is_empty()));
            if silo {
                return "attack".into();
            }
            return if target != 0 && w.get(target).map_or(false, |t| data().def(t.def).is_resource()) { "gather".into() } else { "rally".into() };
        }
        let Some(t) = w.get(target) else { return "move".into() };
        let td = data().def(t.def);
        let any = |f: &dyn Fn(&ee_sim::entity::Entity, &ee_sim::defs::Def) -> bool| units.iter().filter_map(|&u| w.get(u)).any(|e| f(e, data().def(e.def)));
        if w.is_enemy(c.me, t.owner) {
            if any(&|e, _| ee_sim::combat::can_attack(w, e, t)) {
                return "attack".into();
            }
            return "move".into();
        }
        if td.is_resource() {
            if let Some(r) = td.data.resource {
                if any(&|_, d| d.gather_rate[r.idx()] > 0) {
                    return "gather".into();
                }
            }
            return "move".into();
        }
        if t.owner == c.me {
            if td.is_building() && !t.complete && any(&|_, d| !d.builds.is_empty()) {
                return "build".into();
            }
            if (td.data.walkable || td.data.key == "granary") && any(&|_, d| d.gather_rate[0] > 0) {
                return "gather".into();
            }
            if td.is_building() && t.hp < w.max_hp(t) && any(&|_, d| !d.builds.is_empty()) {
                return "repair".into();
            }
            if td.data.airport && any(&|_, d| d.data.needs_airport) {
                return "land".into();
            }
            if td.data.cargo > 0 && any(&|_, d| d.layer == ee_sim::defs::Layer::Land) {
                return "board".into();
            }
        }
        "move".into()
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

    /// Select the next idle citizen and the `n - 1` idle citizens nearest to it.
    #[func]
    fn select_idle_citizens(&mut self, n: i64) -> Vector3 {
        let focus = self.select_idle_citizen();
        let Some(c) = self.client.as_mut() else { return focus };
        let Some(&first) = c.selection.first() else { return focus };
        let w = c.world();
        let Some(fp) = w.get(first).map(|e| e.pos) else { return focus };
        let mut idle: Vec<(i64, EntityId)> = w
            .entities
            .iter()
            .filter(|e| e.alive && e.owner == c.me && e.inside == 0 && e.id != first && data().def(e.def).class() == Class::Citizen && e.order == Order::Idle)
            .map(|e| (e.pos.dist2_raw(fp), e.id))
            .collect();
        idle.sort();
        let more: Vec<EntityId> = idle.into_iter().take((n.max(1) - 1) as usize).map(|x| x.1).collect();
        c.selection.extend(more);
        focus
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
            if d.data.plantable {
                let total = (d.build_ticks + d.grow_ticks).max(1);
                x.set("growth", e.progress as f32 / total as f32);
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
            if any_combat {
                button("scout", "", "Scout", "S", None, true, "Patrol around the island, attacking any enemy encountered.");
            }
            if any_air {
                button("rtb", "", "Return to Base", "Y", None, true, "Fly back to the nearest airfield to refuel and rearm. Right-click a point to set a patrol: aircraft circle it, engage anything in reach and return after refuelling.");
            }
            button("delete", "", "Disband", "Delete", None, true, "Disband the selected units (frees population; useful for stuck or surplus units).");
            // construction options after the unit commands, so those are always visible
            if has_citizen {
                let cit = data().def(data().id("citizen"));
                for (i, &b) in cit.builds.iter().enumerate() {
                    let d = data().def(b);
                    let afford = w.can_afford(c.me, &d.data.cost);
                    button("build", &d.data.key, &d.data.name, HOTKEYS.get(i).copied().unwrap_or(""), Some(&d.data.cost), afford, &d.data.role);
                }
            }
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
                if d.data.garrison {
                    button("ungarrison", "", &format!("Release Garrison ({})", b.cargo.len()), "U", None, !b.cargo.is_empty(), "Send the stored units back out (they walk to the rally point if set). Garrisoned units don't count toward population.");
                }
                if d.trains.iter().any(|&u| data().def(u).data.icbm) {
                    button("icbm", "", &format!("Launch ICBM ({}/{})", b.cargo.len(), d.data.cargo), "L", None, !b.cargo.is_empty(), "Click the target point (or right-click it, also on the minimap). Enemy ABM sites backed by an early warning radar can shoot it down.");
                }
                if d.data.key == "granary" {
                    let fc = data().def(data().id("farm")).data.cost;
                    button("fields", "", "Rebuild Fields", "N", Some(&fc), true, "Lay out farms on every free plot around this granary (cost per field) and send citizens to work them.");
                }
                if !d.trains.is_empty() {
                    let tip = if d.data.airport {
                        "Right-click the map to set the patrol point: new and refuelled aircraft fly there, circle and attack anything in reach."
                    } else {
                        "Right-click the map to set where new units go. Right-click a mine, tree or fish to send new workers straight to work."
                    };
                    button("rally_clear", "", "Clear Rally", "", None, b.rally.is_some(), tip);
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
            "train_mass" => {
                // 10 per selected building that can train it
                if let Some(def) = data().try_id(&key) {
                    for &b in &buildings {
                        let ok = c.world().get(b).map_or(false, |e| data().def(e.def).trains.contains(&def));
                        if ok {
                            c.issue(CommandKind::Train { building: b, def, count: 10 });
                        }
                    }
                }
            }
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
            "scout" => c.issue(CommandKind::Scout { units }),
            "fields" => {
                if let Some(&b) = buildings.first() {
                    c.issue(CommandKind::RebuildFarms { building: b });
                }
            }
            "rally_clear" => {
                // a rally at the building's own position means "none"
                for &b in &buildings {
                    if let Some(e) = c.world().get(b) {
                        let pos = e.pos;
                        c.issue(CommandKind::SetRally { buildings: vec![b], to: pos, target: 0 });
                    }
                }
            }
            "rtb" => c.issue(CommandKind::ReturnToBase { units }),
            "attack_move" => return "attack_move".into(),
            "unload" | "ungarrison" => {
                if units.is_empty() && !buildings.is_empty() {
                    c.issue(CommandKind::Unload { units: buildings, at: ee_sim::fixed::FVec::ZERO });
                    return GString::new();
                }
                return "unload".into();
            }
            "icbm" => return "nuke_target".into(),
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
    fn launch_click(&mut self, p: Vector2) {
        let (Some(c), Some(cam)) = (self.client.as_mut(), self.camera.as_ref()) else { return };
        if let Some(g) = c.ground_at(cam, p) {
            let at = crate::client::from_world(g.x, g.z);
            c.launch_at(at);
        }
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
        // granaries preview their ring of fields so several can be tiled efficiently
        self.field_ghosts.clear();
        if data().def(def).data.key == "granary" {
            let farm = data().id("farm");
            let fmodel = &c.models.list[c.models.by_def[farm as usize]];
            for (ox, oy) in FIELD_SLOTS {
                let mut fm = ShaderMaterial::new_gd();
                fm.set_shader(&godot::tools::load::<Shader>("res://shaders/ghost.gdshader"));
                let mut slot = MeshInstance3D::new_alloc();
                for part in &fmodel.parts {
                    let mut mi = MeshInstance3D::new_alloc();
                    mi.set_mesh(&part.mesh);
                    mi.set_transform(part.local);
                    mi.set_material_override(&fm);
                    mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
                    slot.add_child(&mi);
                }
                slot.set_position(Vector3::new(ox as f32 * TILE, 0.0, oy as f32 * TILE));
                root.add_child(&slot);
                self.field_ghosts.push((slot, fm));
            }
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
            if d.data.plantable {
                gh.set_scale(Vector3::splat(0.45));
            }
        }
        if let Some(m) = self.ghost_mat.as_mut() {
            let col = if ok { Color::from_rgba(0.3, 1.0, 0.45, 0.45) } else { Color::from_rgba(1.0, 0.25, 0.2, 0.45) };
            m.set_shader_parameter("tint", &col.to_variant());
        }
        // field slots: green where a farm would fit, red where it's blocked
        if !self.field_ghosts.is_empty() {
            let farm = data().id("farm");
            let me = c.me;
            let fits: Vec<bool> = FIELD_SLOTS
                .iter()
                .map(|&(ox, oy)| {
                    let t = (tx + ox, ty + oy);
                    // the granary itself isn't placed yet: test terrain/occupancy only
                    (0..3).all(|dy| (0..3).all(|dx| {
                        let (x, y) = (t.0 + dx, t.1 + dy);
                        c.world().map.in_bounds(x, y)
                            && c.world().map.occupant[c.world().map.idx(x, y)] == 0
                            && c.world().map.base_pass[c.world().map.idx(x, y)] & ee_sim::map::PASS_LAND != 0
                            && c.world().explored(me, x, y)
                    })) && farm > 0
                })
                .collect();
            for ((slot, fm), fit) in self.field_ghosts.iter_mut().zip(fits) {
                let col = if fit { Color::from_rgba(0.55, 0.95, 0.35, 0.28) } else { Color::from_rgba(1.0, 0.3, 0.2, 0.22) };
                fm.set_shader_parameter("tint", &col.to_variant());
                let _ = slot;
            }
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
                if data().def(def).data.plantable {
                    // plant a 3x3 grove: every free tile around the cursor
                    let mut first = true;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let t = (tile.0 + dx, tile.1 + dy);
                            if c.world().can_place(c.me, def, t).is_ok() {
                                c.issue(CommandKind::Build { units: units.clone(), def, tile: t, queue: keep || !first });
                                first = false;
                            }
                        }
                    }
                } else {
                    c.issue(CommandKind::Build { units, def, tile, queue: keep });
                }
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
        self.field_ghosts.clear();
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

    /// Reproducible scenes for the opt-in feature verification harness.
    #[func]
    fn debug_feature_scene(&mut self, scenario: GString) -> Vector3 {
        use ee_sim::{command::Command, defs::Layer, fixed::{FVec, Fx}};
        let Some(c) = self.client.as_mut() else { return Vector3::ZERO };
        let w = &mut c.session.world;
        let s = w.starts[0];
        let base = FVec::tile_center(s.0, s.1);
        let mut destroy = Vec::new();
        let center = match scenario.to_string().as_str() {
            "airfield" => {
                let def = data().id("airport");
                let tile = (6i32..24).find_map(|r| (-r..=r).find_map(|dx| {
                    [(s.0 + dx, s.1 + r), (s.0 + dx, s.1 - r)].into_iter()
                        .find(|&t| w.can_place(0, def, t).is_ok())
                })).expect("fixture needs an airfield plot");
                let home = w.spawn_static(def, 0, tile, true);
                let at = w.get(home).unwrap().pos;
                w.players[0].res = [10000; 5];
                c.selection = vec![home];
                at
            }
            sc if sc.starts_with("show_") => {
                // a few of one model near the capitol (ships on the nearest water)
                let key = &sc[5..];
                let def = data().id(key);
                let d = data().def(def);
                let at = if d.layer == Layer::Water {
                    ee_sim::orders::shore_water_near(w, base, 60).unwrap_or(base)
                } else {
                    base + FVec::new(Fx::from_int(0), Fx::from_int(9))
                };
                if d.is_building() {
                    let (x, y) = at.tile();
                    for k in 0..3 {
                        let t = (x - 6 + k * 5, y);
                        if w.can_place(0, def, t).is_ok() {
                            w.spawn_static(def, (k % 2) as u8, t, true);
                        }
                    }
                } else {
                    for k in 0..3 {
                        w.spawn(def, (k % 2) as u8, at + FVec::new(Fx::from_int(k * 4 - 4), Fx::ZERO));
                    }
                }
                at
            }
            "icbm" | "icbm_abm" => {
                // a silo with a missile in flight toward a target 40 tiles away
                let def = data().id("missile_silo");
                let tile = (6i32..24).find_map(|r| (-r..=r).find_map(|dx| {
                    [(s.0 + dx, s.1 + r), (s.0 + dx, s.1 - r)].into_iter().find(|&t| w.can_place(0, def, t).is_ok())
                })).expect("silo plot");
                let silo = w.spawn_static(def, 0, tile, true);
                let spos = w.get(silo).unwrap().pos;
                let m = w.spawn(data().id("icbm"), 0, spos);
                if let Some(e) = w.get_mut(m) { e.inside = silo; }
                w.get_mut(silo).unwrap().cargo.push(m);
                let e1 = w.starts[1];
                let dir = (FVec::tile_center(e1.0, e1.1) - spos).normalized();
                let at = spos + dir.scale(Fx::from_int(40));
                if scenario.to_string() == "icbm_abm" {
                    let (ax, ay) = at.tile();
                    w.spawn_static(data().id("abm_site"), 1, (ax - 6, ay - 6), true);
                    w.spawn_static(data().id("radar_station"), 1, (ax + 6, ay + 6), true);
                    w.recount_pop();
                }
                w.apply_command(&Command { player: 0, kind: ee_sim::command::CommandKind::Launch { building: silo, at } });
                let mid = FVec::new(Fx((spos.x.0 + at.x.0) / 2), Fx((spos.y.0 + at.y.0) / 2));
                mid
            }
            "landing" => {
                let shore = ee_sim::orders::shore_water_near(w, base, 50).unwrap();
                let (x, y) = shore.tile();
                let land = w.map.nearest_passable(x, y, Layer::Land, 3).unwrap();
                let offshore = (-10..=10).flat_map(|dy| (-10..=10).map(move |dx| (x + dx, y + dy)))
                    .find(|&(tx, ty)| w.map.passable(tx, ty, Layer::Water)
                        && FVec::tile_center(tx, ty).within(shore, Fx::from_int(10))
                        && !FVec::tile_center(tx, ty).within(shore, Fx::from_int(7))).unwrap();
                let ship = w.spawn(data().id("transport"), 0, FVec::tile_center(offshore.0, offshore.1));
                let soldier = w.spawn(data().id("rifleman"), 0, FVec::tile_center(land.0, land.1));
                c.selection = vec![soldier];
                // The harness issues the boarding order through the normal right-click UI.
                let _ = ship;
                shore
            }
            "wrecks" => {
                let shore = ee_sim::orders::shore_water_near(w, base, 50).unwrap();
                let (x, y) = shore.tile();
                let water = w.map.nearest_passable(x - 4, y - 4, Layer::Water, 12).unwrap();
                let land = w.map.nearest_passable(x + 6, y + 6, Layer::Land, 12).unwrap();
                destroy.push(w.spawn(data().id("battleship"), 0, FVec::tile_center(water.0, water.1)));
                destroy.push(w.spawn(data().id("fighter"), 0, FVec::tile_center(land.0, land.1)));
                destroy.push(w.spawn(data().id("bomber"), 0, FVec::tile_center(water.0 + 3, water.1)));
                for &id in &destroy {
                    let e = w.get_mut(id).unwrap();
                    e.facing = FVec::new(Fx::ONE, Fx::ZERO);
                }
                shore
            }
            "planes" => {
                let def = data().id("airport");
                let tile = (6i32..24).find_map(|r| (-r..=r).find_map(|dx| {
                    [(s.0 + dx, s.1 + r), (s.0 + dx, s.1 - r)].into_iter()
                        .find(|&t| w.can_place(0, def, t).is_ok())
                })).expect("fixture needs an airfield plot");
                let home = w.spawn_static(def, 0, tile, true);
                let hp = w.get(home).unwrap().pos;
                let patrol = base + FVec::new(Fx::from_int(10), Fx::from_int(-6));
                let mut air = Vec::new();
                for (k, key) in ["fighter", "fighter", "strike_fighter", "helicopter", "bomber"].iter().enumerate() {
                    let id = w.spawn(data().id(key), 0, hp + FVec::new(Fx::from_int(k as i32 * 2), Fx::ZERO));
                    if let Some(e) = w.get_mut(id) { e.home = home; }
                    air.push(id);
                }
                for k in 0..6 {
                    w.spawn(data().id(if k % 2 == 0 { "tank" } else { "rifleman" }), 1, patrol + FVec::new(Fx::from_int(k % 3), Fx::from_int(k / 3)));
                }
                w.apply_command(&Command { player: 0, kind: CommandKind::Move { units: air, to: patrol, attack_move: true, queue: false } });
                patrol
            }
            "nuclear" => {
                let pos = w.entities.iter().filter(|e| e.alive && e.def == data().id("tree"))
                    .min_by_key(|e| e.pos.dist2_raw(base)).unwrap().pos;
                let victim = w.spawn(data().id("tank"), 1, pos);
                let bomber = w.spawn(data().id("nuke_bomber"), 0, pos + FVec::new(Fx::from_int(3), Fx::ZERO));
                w.apply_command(&Command { player: 0, kind: CommandKind::Attack { units: vec![bomber], target: victim, queue: false } });
                // An actual hit on a local unit also exercises minimap and audio alerts.
                w.spawn(data().id("tank"), 0, pos + FVec::new(Fx::from_int(4), Fx::ZERO));
                c.selection = vec![bomber];
                pos
            }
            _ => base,
        };
        w.recount_pop();
        let (x, z) = crate::client::to_world2(center);
        if !destroy.is_empty() { c.issue(CommandKind::Delete { units: destroy }); }
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
        d.set("checksum", format!("{:016x}", w.checksum()));
        d.set("replay", c.replay_path.clone().unwrap_or_default());
        d.set("tick", w.tick as i64);
        d.set("sim_ms", c.last_sim_ms);
        let (air, ships, total) = c.wreck_counts();
        d.set("falling_aircraft", air as i64);
        d.set("sinking_ships", ships as i64);
        d.set("wrecks", total as i64);
        d
    }

    /// Select all of the local player's entities of `key` (tests/screenshots).
    #[func]
    fn select_all_of(&mut self, key: GString, max: i64) {
        let Some(c) = self.client.as_mut() else { return };
        let Some(def) = data().try_id(&key.to_string()) else { return };
        let me = c.me;
        c.selection = c.world().entities.iter().filter(|e| e.alive && e.owner == me && e.def == def && e.inside == 0).map(|e| e.id).take(max as usize).collect();
    }

    #[func]
    fn idle_citizen_count(&self) -> i64 {
        let Some(c) = &self.client else { return 0 };
        c.world()
            .entities
            .iter()
            .filter(|e| e.alive && e.owner == c.me && e.inside == 0 && data().def(e.def).class() == Class::Citizen && e.order == Order::Idle)
            .count() as i64
    }

    #[func]
    fn select_all_idle_citizens(&mut self) {
        let Some(c) = self.client.as_mut() else { return };
        let me = c.me;
        c.selection = c
            .world()
            .entities
            .iter()
            .filter(|e| e.alive && e.owner == me && e.inside == 0 && data().def(e.def).class() == Class::Citizen && e.order == Order::Idle)
            .map(|e| e.id)
            .collect();
    }

    /// Control groups for the HUD bar: [{group, count, key}]
    #[func]
    fn group_info(&self) -> VarArray {
        let mut out = VarArray::new();
        let Some(c) = &self.client else { return out };
        for (g, ids) in c.groups.iter().enumerate() {
            let alive: Vec<&ee_sim::entity::Entity> = ids.iter().filter_map(|&id| c.world().get(id)).collect();
            if alive.is_empty() {
                continue;
            }
            let mut counts = std::collections::BTreeMap::new();
            for e in &alive {
                *counts.entry(e.def).or_insert(0) += 1;
            }
            let top = counts.iter().max_by_key(|(_, n)| **n).map(|(d, _)| *d).unwrap();
            let mut d = dict();
            d.set("group", g as i64);
            d.set("count", alive.len() as i64);
            d.set("key", data().def(top).data.key.as_str());
            out.push(&d.to_variant());
        }
        out
    }

    /// Recent attack sites for the minimap: Array of Vector4(x, z, age_seconds, 0).
    #[func]
    fn attack_markers(&self) -> PackedVector3Array {
        let mut out = PackedVector3Array::new();
        let Some(c) = &self.client else { return out };
        for (p, t) in &c.attack_marks {
            let age = (c.time - t) as f32;
            if age < 15.0 {
                out.push(Vector3::new(p.x, age, p.z));
            }
        }
        out
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
                return GString::from(&format!("{} pos {:?} order {:?} action {:?} hp {} progress {} complete {} goal {:?} stuck {} inside {} rally {:?} sortie {:?} cargo {}", key, e.pos.tile(), e.order, e.action, e.hp, e.progress, e.complete, e.goal.map(|g| g.tile()), e.stuck, e.inside, e.rally, e.sortie, e.cargo.len()));
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
    fn screen_pos_of_resource(&self, key: GString, nth: i64) -> Vector2 {
        let (Some(c), Some(cam)) = (&self.client, &self.camera) else { return Vector2::new(-1.0, -1.0) };
        let Some(def) = data().try_id(&key.to_string()) else { return Vector2::new(-1.0, -1.0) };
        let (sx, sy) = c.world().starts[c.me as usize];
        let home = ee_sim::fixed::FVec::tile_center(sx, sy);
        let mut all: Vec<&ee_sim::entity::Entity> = c.world().entities.iter().filter(|e| e.alive && e.def == def).collect();
        all.sort_by_key(|e| e.pos.dist2_raw(home));
        let best = all.get(nth as usize).copied();
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

/// Absolute path of a user:// subfolder, created on demand.
fn user_dir(sub: &str) -> String {
    let p = godot::classes::ProjectSettings::singleton().globalize_path(&format!("user://{sub}")).to_string();
    let _ = std::fs::create_dir_all(&p);
    p
}

fn new_replay_path(seed: u64) -> String {
    let stamp = godot::classes::Time::singleton().get_datetime_string_from_system().to_string().replace([':', 'T'], "-");
    format!("{}/{stamp}-seed{seed}.eerep", user_dir("replays"))
}
