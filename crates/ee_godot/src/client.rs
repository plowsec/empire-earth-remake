//! Client-side game state: owns the lockstep session, renders the world through
//! MultiMesh batches, maintains fog/minimap textures, selection, and translates
//! UI intent into sim commands. Nothing here mutates the World directly.
use crate::batch::Batch;
use crate::models::{Models, Role};
use crate::terrain::{self, Heights, TILE};
use ee_ai::{Ai, Difficulty};
use ee_net::Session;
use ee_sim::command::CommandKind;
use ee_sim::defs::{Class, DamageType, DefId, Layer};
use ee_sim::entity::{Action, Entity, EntityId, Order, ProdItem};
use ee_sim::fixed::{FVec, Fx};
use ee_sim::mapgen::GAIA;
use ee_sim::world::{data, MatchConfig, SimEvent, World};
use godot::classes::image::Format;
use godot::classes::{Camera3D, Image, ImageTexture, Mesh, Node, QuadMesh, Shader, ShaderMaterial, Texture2D, BoxMesh};
use godot::prelude::*;
use std::collections::{BTreeMap, HashMap};

pub const PLAYER_COLORS: [(f32, f32, f32); 8] = [
    (0.16, 0.42, 0.95),
    (0.90, 0.16, 0.14),
    (0.20, 0.75, 0.25),
    (0.95, 0.80, 0.15),
    (0.15, 0.80, 0.85),
    (0.62, 0.25, 0.85),
    (0.98, 0.52, 0.12),
    (0.92, 0.92, 0.92),
];

pub fn player_color(c: u8) -> Color {
    let (r, g, b) = PLAYER_COLORS[(c as usize) % 8];
    Color::from_rgb(r, g, b)
}

#[inline]
pub fn to_world2(p: FVec) -> (f32, f32) {
    (p.x.to_f32() * TILE, p.y.to_f32() * TILE)
}

pub fn from_world(x: f32, z: f32) -> FVec {
    FVec::new(Fx((x / TILE * 65536.0) as i32), Fx((z / TILE * 65536.0) as i32))
}

const AIR_ALT: f32 = 16.0;

#[derive(Clone)]
struct Remembered {
    def: DefId,
    owner: u8,
    pos: FVec,
    progress: f32,
}

struct Corpse {
    model: usize,
    xf: Transform3D,
    color: Color,
    t: f32,
    life: f32,
    kind: u8, // 0 infantry, 1 vehicle wreck, 2 building collapse, 3 aircraft, 4 ship
    vel: Vector3,
    impacted: bool,
    fx_at: f32,
    roll: f32,
}

struct Tracer {
    a: Vector3,
    b: Vector3,
    t: f32,
    life: f32,
    color: Color,
}

pub struct ClientEvent {
    pub kind: &'static str,
    pub pos: Vector3,
    pub to: Vector3,
    pub size: f32,
    pub text: String,
    pub dmg: i32,
    pub mine: bool,
}

pub struct Client {
    pub session: Session,
    pub me: u8,
    pub heights: Heights,
    pub models: Models,
    batches: HashMap<(usize, usize), Batch>,
    static_batches: HashMap<(usize, usize), Batch>,
    static_dirty: bool,
    ring_batch: Batch,
    team_batch: Batch,
    bar_batch: Batch,
    tracer_batch: Batch,
    proj_batch: Batch,
    root: Gd<Node>,
    pub fog_tex: Gd<ImageTexture>,
    fog_w: i32,
    fog_h: i32,
    fog_buf: Vec<u8>,
    last_fog_tick: u32,
    pub minimap_tex: Gd<ImageTexture>,
    minimap_base: Vec<[u8; 3]>,
    minimap_buf: Vec<u8>,
    minimap_timer: f32,
    pub selection: Vec<EntityId>,
    pub groups: Vec<Vec<EntityId>>,
    pub hover: EntityId,
    remembered: BTreeMap<EntityId, Remembered>,
    corpses: Vec<Corpse>,
    tracers: Vec<Tracer>,
    proj_prev: HashMap<u32, Vector3>,
    /// last drawn position of each ICBM (for mid-air interceptions)
    icbm_pos: HashMap<EntityId, Vector3>,
    /// rendered position of every aircraft this frame (shots start/aim there)
    air_pos: HashMap<EntityId, Vector3>,
    /// launch height of each projectile
    proj_start: HashMap<u32, f32>,
    /// next time (s) each vehicle may play its engine sound
    engine_next: HashMap<EntityId, f64>,
    pub events: Vec<ClientEvent>,
    pub time: f64,
    last_attack_notice: f64,
    palm_model: usize,
    pine_model: usize,
    pub reveal: bool,
    shot_budget: i32,
    deco_batches: Vec<Batch>,
    pub last_sim_ms: f64,
    pub show_all_bars: bool,
    sapling_timer: f32,
    pub attack_marks: Vec<(Vector3, f64)>,
    labels: Vec<Gd<godot::classes::Label3D>>,
    label_timer: f32,
    flag_pole: Batch,
    flag_cloth: Batch,
    /// replay file kept up to date while playing (None = not recording)
    pub replay_path: Option<String>,
    replay_timer: f32,
    replay_final: bool,
    /// folder for rotating autosaves (None = off, e.g. the menu backdrop)
    pub autosave_dir: Option<String>,
    autosave_next: u32,
    autosave_slot: u32,
    autosave_final: bool,
    /// per-player stats every 30 s of game time (end screen charts)
    pub history: Vec<crate::stats::Sample>,
    /// networked match: no pausing, no speed changes, no saves
    pub lan: bool,
}

/// Autosave interval: 5 minutes of game time.
const AUTOSAVE_TICKS: u32 = 20 * 60 * 5;

pub struct StartOptions {
    pub seed: u64,
    pub players: usize,
    pub difficulty: i32,
    pub map_size: u8,
    pub resources: u32,
    pub pop_limit: i32,
    pub reveal: bool,
    pub start_res: i32,
    /// AI also controls the local player (demo / screenshots)
    pub ai_self: bool,
    /// 0 free-for-all, 1 you + AI 1 vs the rest, 2 you vs all AIs
    pub teams: i32,
}

impl Client {
    pub fn new(root: Gd<Node>, opt: StartOptions, noise: Option<Gd<Texture2D>>) -> Client {
        let mut cfg = MatchConfig::skirmish(opt.seed, opt.players.clamp(2, 8));
        cfg.map_size = opt.map_size;
        cfg.resources = opt.resources;
        cfg.pop_limit = opt.pop_limit;
        let sr = opt.start_res;
        cfg.start_res = [sr, sr, sr * 6 / 10, sr * 2 / 3, sr * 2 / 3];
        cfg.players[0].name = "You".into();
        let np = cfg.players.len();
        for (i, p) in cfg.players.iter_mut().enumerate() {
            p.team = match opt.teams {
                1 => if i <= 1 { 0 } else { 1 },
                2 => if i == 0 { 0 } else { 1 },
                _ => i as u8,
            };
        }
        let _ = np;
        // part of the match setup (recorded in replays): the sim's visibility depends on it
        cfg.reveal = opt.reveal;
        for (i, p) in cfg.players.iter_mut().enumerate().skip(1) {
            p.name = format!("AI {}", i);
        }
        let mut session = Session::single_player(cfg);
        let add_ai = |s: &mut Session, p: u8, diff: i32, seed: u64| {
            s.add_controller(Box::new(Ai::new(p, Difficulty::from_index(diff), seed)));
            s.ai_setup.push(ee_net::AiSetup { player: p, difficulty: diff, seed });
        };
        if opt.ai_self {
            add_ai(&mut session, 0, opt.difficulty.max(2), opt.seed ^ 0x55);
        }
        for p in 1..opt.players.clamp(2, 8) {
            add_ai(&mut session, p as u8, opt.difficulty, opt.seed);
        }
        Client::from_session(root, session, opt.reveal, noise)
    }

    /// Build the presentation around a running session (new match or loaded save).
    pub fn from_session(mut root: Gd<Node>, mut session: Session, reveal: bool, noise: Option<Gd<Texture2D>>) -> Client {
        session.collect_events = true;
        let heights = Heights::from_map(&session.world.map);
        let map = &session.world.map;
        let fog_w = map.w * 2;
        let fog_h = map.h * 2;
        let fog_buf = vec![0u8; (fog_w * fog_h * 2) as usize];
        let fog_img = Image::create_from_data(fog_w, fog_h, false, Format::RG8, &PackedByteArray::from(&fog_buf[..])).unwrap();
        let fog_tex = ImageTexture::create_from_image(&fog_img).unwrap();
        let map_size = Vector2::new(map.w as f32 * TILE, map.h as f32 * TILE);
        let mut models = Models::new(noise, Some((fog_tex.clone().upcast(), map_size)));
        models.load_all(&data().defs);
        let tree_def = data().def(data().id("tree"));
        let palm_model = models.variant("tree_palm", tree_def);
        let pine_model = models.variant("tree_pine", tree_def);
        let deco_models: Vec<usize> = ["deco_grass", "deco_flowers", "deco_rock", "deco_bush", "deco_reeds"]
            .iter()
            .map(|n| models.variant(n, tree_def))
            .collect();

        let quad = {
            let mut q = QuadMesh::new_gd();
            q.set_size(Vector2::new(1.0, 1.0));
            q.set_orientation(godot::classes::plane_mesh::Orientation::Y);
            q
        };
        let ring_mat = shader_mat("res://shaders/ring.gdshader");
        let ring_batch = Batch::new(&mut root, &quad.clone().upcast(), false, Some(&ring_mat));
        let team_mat = shader_mat("res://shaders/team_disc.gdshader");
        let team_batch = Batch::new(&mut root, &quad.clone().upcast(), false, Some(&team_mat));
        let bar_quad = {
            let mut q = QuadMesh::new_gd();
            q.set_size(Vector2::new(1.0, 0.14));
            q
        };
        let bar_mat = shader_mat("res://shaders/bar.gdshader");
        let bar_batch = Batch::new(&mut root, &bar_quad.upcast(), false, Some(&bar_mat));
        let tracer_mat = shader_mat("res://shaders/tracer.gdshader");
        let tracer_quad = {
            let mut q = QuadMesh::new_gd();
            q.set_size(Vector2::new(1.0, 1.0));
            q.set_center_offset(Vector3::new(0.5, 0.0, 0.0));
            q
        };
        let tracer_batch = Batch::new(&mut root, &tracer_quad.upcast(), false, Some(&tracer_mat));
        let proj_mesh = {
            let mut b = BoxMesh::new_gd();
            b.set_size(Vector3::new(0.12, 0.12, 0.7));
            b
        };
        let proj_batch = Batch::new(&mut root, &proj_mesh.upcast(), false, Some(&tracer_mat));
        let flag_mat = shader_mat("res://shaders/unit.gdshader");
        let mut fm = flag_mat.clone();
        fm.set_shader_parameter("team_mask", &1.0f32.to_variant());
        fm.set_shader_parameter("albedo", &Color::from_rgb(0.9, 0.9, 0.9).to_variant());
        let pole_mesh = {
            let mut b = BoxMesh::new_gd();
            b.set_size(Vector3::new(0.12, 3.6, 0.12));
            b
        };
        let cloth_mesh = {
            let mut b = BoxMesh::new_gd();
            b.set_size(Vector3::new(0.05, 0.9, 1.4));
            b
        };
        let mut pm = shader_mat("res://shaders/unit.gdshader");
        pm.set_shader_parameter("albedo", &Color::from_rgb(0.25, 0.25, 0.27).to_variant());
        let flag_pole = Batch::new(&mut root, &pole_mesh.upcast(), true, Some(&pm));
        let flag_cloth = Batch::new(&mut root, &cloth_mesh.upcast(), true, Some(&fm));

        let map = &session.world.map;
        let minimap_base = terrain::minimap_base(map);
        let minimap_buf = vec![0u8; (map.w * map.h * 4) as usize];
        let mm_img = Image::create_from_data(map.w, map.h, false, Format::RGBA8, &PackedByteArray::from(&minimap_buf[..])).unwrap();
        let minimap_tex = ImageTexture::create_from_image(&mm_img).unwrap();

        let mut c = Client {
            session,
            me: 0,
            heights,
            models,
            batches: HashMap::new(),
            static_batches: HashMap::new(),
            static_dirty: true,
            ring_batch,
            team_batch,
            bar_batch,
            tracer_batch,
            proj_batch,
            root,
            fog_tex,
            fog_w,
            fog_h,
            fog_buf,
            last_fog_tick: u32::MAX,
            minimap_tex,
            minimap_base,
            minimap_buf,
            minimap_timer: 0.0,
            selection: Vec::new(),
            groups: vec![Vec::new(); 10],
            hover: 0,
            remembered: BTreeMap::new(),
            corpses: Vec::new(),
            tracers: Vec::new(),
            proj_prev: HashMap::new(),
            icbm_pos: HashMap::new(),
            air_pos: HashMap::new(),
            proj_start: HashMap::new(),
            engine_next: HashMap::new(),
            events: Vec::new(),
            time: 0.0,
            last_attack_notice: -100.0,
            palm_model,
            pine_model,
            reveal,
            shot_budget: 0,
            deco_batches: Vec::new(),
            last_sim_ms: 0.0,
            show_all_bars: false,
            sapling_timer: 0.0,
            attack_marks: Vec::new(),
            labels: Vec::new(),
            label_timer: 0.0,
            flag_pole,
            flag_cloth,
            replay_path: None,
            replay_timer: 30.0,
            replay_final: false,
            autosave_dir: None,
            autosave_next: 0,
            autosave_slot: 0,
            autosave_final: false,
            history: Vec::new(),
            lan: false,
        };
        c.autosave_next = c.session.world.tick + AUTOSAVE_TICKS;
        c.build_decorations(&deco_models);
        c.session.world.config.reveal = reveal;
        c.update_fog(true);
        c
    }

    /// Save the match as `<autosave_dir>/<name>.eesave`. Serializes now, writes on a
    /// background thread (`background`) so autosaves don't stall the frame.
    pub fn save_to(&mut self, name: &str, background: bool) -> Result<String, String> {
        let dir = self.autosave_dir.clone().ok_or("saving is off")?;
        let text = crate::save::save(&self.session, self.reveal, &self.history)?;
        let path = format!("{dir}/{name}.eesave");
        let tmp = format!("{path}.tmp");
        let write = {
            let path = path.clone();
            move || std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())
        };
        if background {
            std::thread::spawn(move || {
                let _ = write();
            });
            self.events.push(ClientEvent { kind: "autosaved", pos: Vector3::ZERO, to: Vector3::ZERO, size: 0.0, text: name.to_string(), dmg: 0, mine: true });
        } else {
            write()?;
        }
        self.write_replay();
        Ok(path)
    }

    pub fn write_replay(&self) {
        let Some(path) = &self.replay_path else { return };
        match crate::save::replay_text(&self.session) {
            Ok(text) => {
                // write then rename: never leave a half-written file behind
                let tmp = format!("{path}.tmp");
                if std::fs::write(&tmp, text).is_ok() {
                    let _ = std::fs::rename(&tmp, path);
                }
            }
            Err(e) => {
                godot_warn!("replay not written: {e}");
            }
        }
    }

    /// Play as `me` (networked games) and refresh what that player can see.
    pub fn set_me(&mut self, me: u8) {
        self.me = me;
        self.update_fog(true);
    }

    pub fn world(&self) -> &World {
        &self.session.world
    }

    pub fn issue(&mut self, kind: CommandKind) {
        let me = self.me;
        self.session.issue(me, kind);
    }

    // ------------------------------------------------------------------ frame

    pub fn update(&mut self, dt: f64, cam: Option<&Gd<Camera3D>>) {
        self.time += dt;
        let t0 = std::time::Instant::now();
        self.session.advance(dt);
        self.last_sim_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let evs = std::mem::take(&mut self.session.event_log);
        self.shot_budget = 60;
        for e in &evs {
            self.handle_event(e);
        }
        let w = &self.session.world;
        let tick = w.tick;
        if tick != self.last_fog_tick && (tick % 4 == 1 || self.last_fog_tick == u32::MAX) {
            self.update_fog(false);
        }
        self.selection.retain(|&id| self.session.world.get(id).map_or(false, |e| e.inside == 0 || data().def(e.def).is_building()));
        self.render(dt as f32, cam);
        // saplings visibly grow: refresh static scenery every couple of seconds
        self.sapling_timer -= dt as f32;
        if self.sapling_timer <= 0.0 {
            self.sapling_timer = 2.0;
            let sap = data().id("sapling");
            if self.session.world.entities.iter().any(|e| e.alive && e.def == sap) {
                self.static_dirty = true;
            }
        }
        // stats history for the end screen
        let tick = self.session.world.tick;
        if self.history.last().map_or(true, |s| tick >= s.tick + 600) || (self.session.world.game_over && self.history.last().map_or(false, |s| s.tick != tick)) {
            self.history.push(crate::stats::sample(&self.session.world));
        }
        // rotating autosaves every 5 minutes of game time, and one when the match ends
        let over = self.session.world.game_over;
        if self.autosave_dir.is_some() && (tick >= self.autosave_next || (over && !self.autosave_final)) {
            self.autosave_next = tick + AUTOSAVE_TICKS;
            let name = if over {
                self.autosave_final = true;
                "Autosave (end of game)".to_string()
            } else {
                self.autosave_slot = self.autosave_slot % 3 + 1;
                format!("Autosave {}", self.autosave_slot)
            };
            if let Err(e) = self.save_to(&name, true) {
                godot_warn!("autosave failed: {e}");
            }
        }
        // keep the replay file current so a crash or quit still leaves a full record
        self.replay_timer -= dt as f32;
        let over = self.session.world.game_over;
        if self.replay_timer <= 0.0 || (over && !self.replay_final) {
            self.replay_timer = 30.0;
            self.replay_final = over;
            self.write_replay();
        }
        self.label_timer -= dt as f32;
        if self.label_timer <= 0.0 {
            self.label_timer = 0.3;
            self.update_work_labels(cam);
        }
        self.minimap_timer -= dt as f32;
        if self.minimap_timer <= 0.0 {
            self.minimap_timer = 0.25;
            self.update_minimap();
        }
    }

    pub fn wreck_counts(&self) -> (usize, usize, usize) {
        (self.corpses.iter().filter(|c| c.kind == 3).count(),
         self.corpses.iter().filter(|c| c.kind == 4).count(), self.corpses.len())
    }

    fn sees(&self, x: i32, y: i32) -> bool {
        self.session.world.visible(self.me, x, y)
    }

    fn world_pos3(&self, p: FVec, layer: Layer, id: u32) -> Vector3 {
        let (x, z) = to_world2(p);
        let y = match layer {
            Layer::Water => (self.time as f32 * 1.3 + id as f32 * 0.7).sin() * 0.08,
            Layer::Air => self.air_pos.get(&id).map_or(AIR_ALT, |v| v.y),
            _ => self.heights.at(x, z).max(-0.3),
        };
        Vector3::new(x, y, z)
    }

    fn handle_event(&mut self, ev: &SimEvent) {
        let me = self.me;
        match ev {
            SimEvent::Shot { from, to, from_pos, to_pos, weapon } => {
                let (fx, fy) = from_pos.tile();
                let (tx, ty) = to_pos.tile();
                if !(self.sees(fx, fy) || self.sees(tx, ty)) {
                    return;
                }
                let w = &self.session.world;
                let Some(src) = w.get(*from) else { return };
                let sd = data().def(src.def);
                let Some(wp) = sd.weapons.get(*weapon as usize) else { return };
                let mut a = self.world_pos3(*from_pos, sd.layer, *from);
                let tlayer = w.get(*to).map(|t| data().def(t.def).layer).unwrap_or(Layer::Land);
                let mut b = self.world_pos3(*to_pos, tlayer, *to);
                let sh = self.models.list[self.models.by_def[src.def as usize]].height;
                a.y += if sd.is_building() { sh * 0.8 } else { sh * 0.55 };
                b.y += 0.8;
                let instant = matches!(wp.projectile, ee_sim::defs::Projectile::Instant);
                if instant && self.shot_budget > 0 {
                    self.shot_budget -= 1;
                    let col = match wp.dmg_type {
                        DamageType::Flak => Color::from_rgba(1.0, 0.75, 0.35, 1.0),
                        _ => Color::from_rgba(1.0, 0.85, 0.5, 0.9),
                    };
                    for k in 0..wp.burst.min(4) {
                        let jitter = Vector3::new(((k * 37 % 7) as f32 - 3.0) * 0.08, 0.0, ((k * 53 % 5) as f32 - 2.0) * 0.08);
                        self.tracers.push(Tracer { a, b: b + jitter, t: -(k as f32) * 0.05, life: 0.09, color: col });
                    }
                }
                self.events.push(ClientEvent {
                    kind: "shot",
                    pos: a,
                    to: b,
                    size: wp.damage as f32,
                    text: sd.data.key.clone(),
                    dmg: wp.dmg_type as i32,
                    mine: src.owner == me,
                });
            }
            SimEvent::Impact { pos, dmg_type, splash, .. } => {
                let (tx, ty) = pos.tile();
                if !self.sees(tx, ty) {
                    return;
                }
                let water = self.session.world.map.is_water(tx, ty);
                let p = self.world_pos3(*pos, if water { Layer::Water } else { Layer::Land }, 0);
                self.events.push(ClientEvent {
                    kind: if water && *dmg_type != DamageType::Nuclear as u8 { "splash" } else { "impact" },
                    pos: p,
                    to: p,
                    size: splash.to_f32() * TILE,
                    text: String::new(),
                    dmg: *dmg_type as i32,
                    mine: false,
                });
            }
            SimEvent::Died { id, def, owner, pos, facing, velocity, inside, .. } => {
                let air_at = self.air_pos.remove(id);
                // Cargo and parked aircraft disappear with their carrier/airfield.
                if *inside != 0 { return; }
                let d = data().def(*def);
                let (tx, ty) = pos.tile();
                if *owner == me && d.is_unit() {
                    // notified via UI counters only
                }
                if !self.sees(tx, ty) && *owner != me && !d.data.icbm {
                    self.remembered.remove(id);
                    return;
                }
                self.remembered.remove(id);
                if d.data.icbm {
                    // intercepted high up (or detonated: the nuke covers that)
                    if let Some(p) = self.icbm_pos.remove(id) {
                        if p.y > 20.0 {
                            self.events.push(ClientEvent { kind: "intercept", pos: p, to: p, size: 0.0, text: String::new(), dmg: 0, mine: *owner == me });
                        }
                    }
                    return;
                }
                let p = air_at.unwrap_or_else(|| self.world_pos3(*pos, d.layer, *id));
                let model = self.models.by_def[*def as usize];
                let w = &self.session.world;
                let color = w.players.get(*owner as usize).map(|pl| player_color(pl.color)).unwrap_or(Color::WHITE);
                // Preserve the flight/course direction at the instant of destruction.
                let yaw = if d.is_building() { 0.0 } else { facing.x.to_f32().atan2(facing.y.to_f32()) };
                let xf = Transform3D::new(Basis::from_axis_angle(Vector3::UP, yaw), p);
                let kind = match d.class() {
                    Class::Citizen | Class::Infantry => 0,
                    Class::Building => 2,
                    Class::Aircraft => 3,
                    Class::Ship => 4,
                    _ => 1,
                };
                let life = match kind {
                    0 => 6.0,
                    1 => 9.0,
                    2 => 5.0,
                    3 => 12.0,
                    _ => 18.0,
                };
                if !d.is_resource() {
                    let vel = Vector3::new(velocity.x.to_f32() * TILE, 0.0, velocity.y.to_f32() * TILE);
                    self.corpses.push(Corpse { model, xf, color, t: 0.0, life, kind, vel,
                        impacted: false, fx_at: 0.0, roll: if id % 2 == 0 { 1.0 } else { -1.0 } });
                    self.events.push(ClientEvent {
                        kind: "death",
                        pos: p,
                        to: p,
                        size: match kind {
                            0 => 0.0,
                            2 => (d.size().0 as f32) * TILE,
                            _ => d.radius.to_f32() * TILE * 2.0,
                        },
                        text: d.data.key.clone(),
                        dmg: kind as i32,
                        mine: *owner == me,
                    });
                } else {
                    self.static_dirty = true;
                }
            }
            SimEvent::TreeFelled { id, def, pos, away } => {
                let (tx, ty) = pos.tile();
                if !self.sees(tx, ty) { return; }
                let model = self.models.by_def[*def as usize];
                let p = self.world_pos3(*pos, Layer::Land, *id);
                let yaw = away.x.to_f32().atan2(away.y.to_f32());
                self.corpses.push(Corpse { model, xf: Transform3D::new(Basis::from_axis_angle(Vector3::UP, yaw), p),
                    color: Color::WHITE, t: 0.0, life: 4.0, kind: 5, vel: Vector3::ZERO,
                    impacted: false, fx_at: 0.0, roll: 1.0 });
                self.static_dirty = true;
            }
            SimEvent::ResourceDepleted { .. } | SimEvent::Grown { .. } => {
                self.static_dirty = true;
            }
            SimEvent::BuildingPlaced { owner, .. } if *owner == me => {
                self.events.push(ClientEvent { kind: "placed", pos: Vector3::ZERO, to: Vector3::ZERO, size: 0.0, text: String::new(), dmg: 0, mine: true });
            }
            SimEvent::BuildingComplete { owner, def, id } if *owner == me => {
                let p = self.session.world.get(*id).map(|e| self.world_pos3(e.pos, Layer::Land, 0)).unwrap_or(Vector3::ZERO);
                self.events.push(ClientEvent {
                    kind: "complete",
                    pos: p,
                    to: p,
                    size: 0.0,
                    text: data().def(*def).data.name.clone(),
                    dmg: 0,
                    mine: true,
                });
            }
            SimEvent::Spawned { owner, def, .. } if *owner == me => {
                self.events.push(ClientEvent { kind: "trained", pos: Vector3::ZERO, to: Vector3::ZERO, size: 0.0, text: data().def(*def).data.key.clone(), dmg: 0, mine: true });
            }
            SimEvent::ResearchComplete { owner, tech } if *owner == me => {
                self.events.push(ClientEvent {
                    kind: "research",
                    pos: Vector3::ZERO,
                    to: Vector3::ZERO,
                    size: 0.0,
                    text: data().techs[*tech as usize].data.name.clone(),
                    dmg: 0,
                    mine: true,
                });
            }
            SimEvent::Damaged { owner, pos, .. } if *owner == me => {
                let p = self.world_pos3(*pos, Layer::Land, 0);
                let now = self.time;
                let fresh_mark = !self.attack_marks.iter().any(|(m, t)| now - t < 8.0 && m.distance_to(p) < 40.0);
                if fresh_mark {
                    // a new attack site: alert only if nothing nearby was reported recently
                    let new_site = !self.attack_marks.iter().any(|(m, t)| now - t < 30.0 && m.distance_to(p) < 90.0);
                    self.attack_marks.push((p, now));
                    if new_site && now - self.last_attack_notice > 4.0 {
                        self.last_attack_notice = now;
                        self.events.push(ClientEvent { kind: "under_attack", pos: p, to: p, size: 0.0, text: String::new(), dmg: 0, mine: true });
                    }
                }
                self.attack_marks.retain(|(_, t)| now - t < 30.0);
            }
            SimEvent::MissileLaunch { owner, from, to, .. } => {
                let (fx, fy) = from.tile();
                let detected = *owner == me || self.session.world.players[me as usize].radar;
                if self.sees(fx, fy) || detected {
                    let p = self.world_pos3(*from, Layer::Land, 0);
                    let t = self.world_pos3(*to, Layer::Land, 0);
                    self.events.push(ClientEvent { kind: "missile_launch", pos: p, to: t, size: 0.0, text: String::new(), dmg: 0, mine: *owner == me });
                }
                if *owner != me && detected {
                    let t = self.world_pos3(*to, Layer::Land, 0);
                    self.events.push(ClientEvent { kind: "nuke_alarm", pos: t, to: t, size: 0.0, text: String::new(), dmg: 0, mine: false });
                }
            }
            SimEvent::Notice { owner, text } if *owner == me => {
                self.events.push(ClientEvent { kind: "notice", pos: Vector3::ZERO, to: Vector3::ZERO, size: 0.0, text: text.to_string(), dmg: 0, mine: true });
            }
            SimEvent::PlayerDefeated { player } => {
                let name = self.session.world.players[*player as usize].name.clone();
                self.events.push(ClientEvent { kind: "defeated", pos: Vector3::ZERO, to: Vector3::ZERO, size: 0.0, text: name, dmg: *player as i32, mine: *player == me });
            }
            SimEvent::Diplomacy { from, to, kind } if *from == me || *to == me => {
                let w = &self.session.world;
                let other = if *from == me { *to } else { *from };
                let name = w.players[other as usize].name.clone();
                let text = match (kind, *from == me) {
                    (0, true) => format!("Alliance offered to {name}"),
                    (0, false) => format!("{name} offers an alliance (Diplomacy to accept)"),
                    (1, _) => format!("Alliance formed with {name}"),
                    (_, true) => format!("You are now at war with {name}"),
                    _ => format!("{name} has declared war on you!"),
                };
                self.events.push(ClientEvent { kind: "diplomacy", pos: Vector3::ZERO, to: Vector3::ZERO, size: 0.0, text, dmg: *kind as i32, mine: *from == me });
            }
            SimEvent::Tribute { from, to, res } if *to == me || *from == me => {
                let w = &self.session.world;
                const N: [&str; 5] = ["food", "wood", "stone", "gold", "iron"];
                let parts: Vec<String> = res.iter().enumerate().filter(|(_, a)| **a > 0).map(|(r, a)| format!("{a} {}", N[r])).collect();
                let text = if *to == me {
                    format!("{} sent you {}", w.players[*from as usize].name, parts.join(", "))
                } else {
                    format!("Sent {} to {}", parts.join(", "), w.players[*to as usize].name)
                };
                self.events.push(ClientEvent { kind: "diplomacy", pos: Vector3::ZERO, to: Vector3::ZERO, size: 0.0, text, dmg: 3, mine: *from == me });
            }
            SimEvent::GameOver { .. } => {
                // you win if you're still standing when the last enemies fall
                let won = !self.session.world.players[me as usize].defeated;
                self.events.push(ClientEvent { kind: "game_over", pos: Vector3::ZERO, to: Vector3::ZERO, size: 0.0, text: if won { "victory".into() } else { "defeat".into() }, dmg: 0, mine: won });
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ fog & minimap

    fn update_fog(&mut self, force: bool) {
        let w = &self.session.world;
        if !force && w.tick == self.last_fog_tick {
            return;
        }
        self.last_fog_tick = w.tick;
        let mw = w.map.w;
        let mh = w.map.h;
        let fw = self.fog_w;
        let fh = self.fog_h;
        let vis = &w.vision[self.me as usize];
        let reveal = self.reveal;
        // 2x upsample with a 3x3 tent filter for soft edges
        for y in 0..fh {
            for x in 0..fw {
                let mut sv = 0u32;
                let mut se = 0u32;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let tx = ((x + dx) / 2).clamp(0, mw - 1);
                        let ty = ((y + dy) / 2).clamp(0, mh - 1);
                        let v = if reveal { 2 } else { vis[(ty * mw + tx) as usize] };
                        let wgt = if dx == 0 && dy == 0 { 4 } else if dx == 0 || dy == 0 { 2 } else { 1 };
                        if v == 2 {
                            sv += 255 * wgt;
                        }
                        if v >= 1 {
                            se += 255 * wgt;
                        }
                    }
                }
                let i = ((y * fw + x) * 2) as usize;
                self.fog_buf[i] = (sv / 16) as u8;
                self.fog_buf[i + 1] = (se / 16) as u8;
            }
        }
        if let Some(img) = Image::create_from_data(fw, fh, false, Format::RG8, &PackedByteArray::from(&self.fog_buf[..])) {
            self.fog_tex.update(&img);
        }
    }

    fn update_minimap(&mut self) {
        let w = &self.session.world;
        let mw = w.map.w;
        let mh = w.map.h;
        let vis = &w.vision[self.me as usize];
        for i in 0..(mw * mh) as usize {
            let v = if self.reveal { 2 } else { vis[i] };
            let c = self.minimap_base[i];
            let k = match v {
                2 => 255u32,
                1 => 130,
                _ => 0,
            };
            self.minimap_buf[i * 4] = (c[0] as u32 * k / 255) as u8;
            self.minimap_buf[i * 4 + 1] = (c[1] as u32 * k / 255) as u8;
            self.minimap_buf[i * 4 + 2] = (c[2] as u32 * k / 255) as u8;
            self.minimap_buf[i * 4 + 3] = 255;
        }
        let plot = |buf: &mut Vec<u8>, x: i32, y: i32, c: Color, r: i32| {
            for dy in -r..=r {
                for dx in -r..=r {
                    let (px, py) = (x + dx, y + dy);
                    if px >= 0 && py >= 0 && px < mw && py < mh {
                        let i = ((py * mw + px) * 4) as usize;
                        buf[i] = (c.r * 255.0) as u8;
                        buf[i + 1] = (c.g * 255.0) as u8;
                        buf[i + 2] = (c.b * 255.0) as u8;
                    }
                }
            }
        };
        let mut units: Vec<(i32, i32, Color)> = Vec::new();
        for e in &w.entities {
            if !e.alive || e.inside != 0 {
                continue;
            }
            let d = data().def(e.def);
            let (tx, ty) = e.pos.tile();
            if d.is_resource() {
                if d.size().0 > 1 && w.explored(self.me, tx, ty) {
                    let c = match d.data.key.as_str() {
                        "gold_mine" => Color::from_rgb(1.0, 0.85, 0.2),
                        "iron_mine" => Color::from_rgb(0.75, 0.45, 0.35),
                        _ => Color::from_rgb(0.8, 0.8, 0.8),
                    };
                    plot(&mut self.minimap_buf, tx, ty, c, 1);
                }
                continue;
            }
            if e.owner == GAIA {
                continue;
            }
            let radar_track = d.data.icbm && w.players[self.me as usize].radar;
            if e.owner != self.me && !w.can_see(self.me, e) && !radar_track {
                continue;
            }
            let c = player_color(w.players[e.owner as usize].color);
            if d.is_building() {
                plot(&mut self.minimap_buf, tx, ty, c, d.size().0 / 2);
            } else {
                units.push((tx, ty, if d.data.icbm { Color::WHITE } else { c }));
            }
        }
        // units last and bold (outlined 3x3 blips) so armies and fleets stand out
        for &(tx, ty, _) in &units {
            plot(&mut self.minimap_buf, tx, ty, Color::from_rgb(0.05, 0.05, 0.05), 2);
        }
        for &(tx, ty, c) in &units {
            let bright = Color::from_rgb((c.r * 0.8 + 0.2).min(1.0), (c.g * 0.8 + 0.2).min(1.0), (c.b * 0.8 + 0.2).min(1.0));
            plot(&mut self.minimap_buf, tx, ty, bright, 1);
        }
        for (_, r) in &self.remembered {
            let (tx, ty) = r.pos.tile();
            let c = player_color(w.players[r.owner as usize].color) * 0.7;
            plot(&mut self.minimap_buf, tx, ty, c, 1);
        }
        if let Some(img) = Image::create_from_data(mw, mh, false, Format::RGBA8, &PackedByteArray::from(&self.minimap_buf[..])) {
            self.minimap_tex.update(&img);
        }
    }

    // ------------------------------------------------------------------ rendering

    fn batch(&mut self, model: usize, part: usize, stat: bool) -> &mut Batch {
        let map = if stat { &mut self.static_batches } else { &mut self.batches };
        if !map.contains_key(&(model, part)) {
            let mesh: Gd<Mesh> = self.models.list[model].parts[part].mesh.clone();
            let shadows = self.models.list[model].shadows;
            let mut b = Batch::new(&mut self.root, &mesh, shadows, None);
            b.begin();
            map.insert((model, part), b);
        }
        map.get_mut(&(model, part)).unwrap()
    }

    fn push_model(&mut self, model: usize, xf: Transform3D, color: Color, custom: [f32; 4], turret_yaw: f32, stat: bool) {
        let nparts = self.models.list[model].parts.len();
        let t = self.time as f32;
        for pi in 0..nparts {
            let (role, local, pivot, rest_inv) = {
                let p = &self.models.list[model].parts[pi];
                (p.role, p.local, p.pivot, p.rest_inv)
            };
            let sc = self.models.list[model].scale;
            let xf = if sc != 1.0 { Transform3D::new(xf.basis.scaled(Vector3::splat(sc)), xf.origin) } else { xf };
            let pxf = match role {
                Role::Body => xf * local,
                Role::Turret => {
                    let r = Transform3D::new(Basis::from_axis_angle(Vector3::UP, turret_yaw) * rest_inv, Vector3::ZERO);
                    let to_p = Transform3D::new(Basis::IDENTITY, pivot);
                    let from_p = Transform3D::new(Basis::IDENTITY, -pivot);
                    xf * to_p * r * from_p * local
                }
                Role::Rotor | Role::RotorX => {
                    let axis = if role == Role::Rotor { Vector3::UP } else { Vector3::RIGHT };
                    let spin = if custom[1] > 5.5 && custom[1] < 6.5 { 0.0 } else { t * 40.0 };
                    let r = Transform3D::new(Basis::from_axis_angle(axis, spin), Vector3::ZERO);
                    let to_p = Transform3D::new(Basis::IDENTITY, pivot);
                    let from_p = Transform3D::new(Basis::IDENTITY, -pivot);
                    xf * to_p * r * from_p * local
                }
            };
            self.batch(model, pi, stat).push(&pxf, color, custom);
        }
    }

    fn render(&mut self, dt: f32, cam: Option<&Gd<Camera3D>>) {
        let alpha = self.session.alpha();
        for b in self.batches.values_mut() {
            b.begin();
        }
        self.ring_batch.begin();
        self.team_batch.begin();
        self.bar_batch.begin();
        self.tracer_batch.begin();
        self.proj_batch.begin();
        let me = self.me;
        let t = self.time as f32;

        // camera culling: only draw what's near the view
        let cam_pos = cam.map(|c| c.get_global_position()).unwrap_or(Vector3::ZERO);
        let lod_dist2 = 95.0f32 * 95.0;
        let (cull_c, cull_r) = match cam {
            Some(c) => {
                let o = c.get_global_position();
                let f = -c.get_global_transform().basis.col_c();
                let k = if f.y < -0.05 { -o.y / f.y } else { 200.0 };
                let gp = o + f * k;
                (Vector2::new(gp.x, gp.z), (o.y * 2.2).max(160.0))
            }
            None => (Vector2::ZERO, 1e9),
        };

        let n = self.session.world.entities.len();
        let sel: std::collections::HashSet<EntityId> = self.selection.iter().copied().collect();
        // remember enemy buildings we can see; forget ones proven gone
        {
            let w = &self.session.world;
            let mut seen_now: Vec<(EntityId, Remembered)> = Vec::new();
            for e in &w.entities {
                if !e.alive || e.owner == GAIA || e.owner == me {
                    continue;
                }
                let d = data().def(e.def);
                if d.is_building() && w.can_see(me, e) {
                    let bt = d.build_ticks.max(1) as f32;
                    seen_now.push((e.id, Remembered { def: e.def, owner: e.owner, pos: e.pos, progress: if e.complete { 1.0 } else { e.progress as f32 / bt } }));
                }
            }
            for (id, r) in seen_now {
                self.remembered.insert(id, r);
            }
            let gone: Vec<EntityId> = self
                .remembered
                .iter()
                .filter(|(id, r)| {
                    let (tx, ty) = r.pos.tile();
                    w.visible(me, tx, ty) && w.get(**id).is_none()
                })
                .map(|(id, _)| *id)
                .collect();
            for g in gone {
                self.remembered.remove(&g);
            }
        }

        for i in 1..n {
            let (def, owner, pos, prev, facing, action, id, hp, complete, progress, target, inside, carry, home) = {
                let e = &self.session.world.entities[i];
                if !e.alive {
                    continue;
                }
                (e.def, e.owner, e.pos, e.prev_pos, e.facing, e.action, e.id, e.hp, e.complete, e.progress, e.target, e.inside, e.carry, e.home)
            };
            let d = data().def(def);
            if d.is_resource() || inside != 0 {
                continue;
            }
            let w = &self.session.world;
            let visible = owner == me || w.can_see(me, &w.entities[i]) || (d.data.icbm && w.players[me as usize].radar);
            if !visible {
                continue;
            }
            // interpolate
            let ip = FVec::new(
                Fx(prev.x.0 + ((pos.x.0 - prev.x.0) as f32 * alpha) as i32),
                Fx(prev.y.0 + ((pos.y.0 - prev.y.0) as f32 * alpha) as i32),
            );
            let mut p = self.world_pos3(ip, d.layer, id);
            if Vector2::new(p.x, p.z).distance_to(cull_c) > cull_r {
                continue;
            }
            let model = self.models.by_def[def as usize];
            let fx = facing.x.to_f32();
            let fz = facing.y.to_f32();
            let yaw = if d.is_building() { 0.0 } else { fx.atan2(fz) };
            let mut basis = Basis::from_axis_angle(Vector3::UP, yaw);
            if d.data.icbm {
                // ballistic arc from the silo to the aim point, nose along the trajectory
                let e = &w.entities[i];
                let (alt, slope) = icbm_arc(e.sortie, e.goal.or(match e.order { ee_sim::entity::Order::Strike { at } => Some(at), _ => None }), ip);
                p.y = self.heights.at(p.x, p.z).max(0.0) + alt;
                basis = basis * Basis::from_axis_angle(Vector3::RIGHT, -slope.atan());
                self.icbm_pos.insert(id, p);
                self.air_pos.insert(id, p);
                let back = Vector3::new(-fx, -slope, -fz).normalized() * 4.5;
                self.events.push(ClientEvent { kind: "missile_trail", pos: p + back, to: p, size: alt, text: String::new(), dmg: 0, mine: false });
            } else if d.layer == Layer::Air {
                // bank into turns, bob gently
                let turn = ((facing.x.0 as f32 * 0.0) + (pos.x.0 - prev.x.0) as f32 * fz - (pos.y.0 - prev.y.0) as f32 * fx) / 65536.0;
                basis = basis * Basis::from_axis_angle(Vector3::FORWARD, (turn * 25.0).clamp(-0.42, 0.42));
                // cruise altitude by type, staggered per plane so they never share a level
                let cruise = match d.data.key.as_str() {
                    "helicopter" => 11.0,
                    "bomber" => 30.0,
                    "nuke_bomber" => 34.0,
                    _ => 22.0,
                } + ((id.wrapping_mul(37) % 7) as f32 - 3.0) * 1.3;
                // climb out of / descend into the home airfield
                let mut alt = cruise;
                if let Some(h) = self.session.world.get(home) {
                    let (hx, hz) = to_world2(h.pos);
                    let hd = Vector2::new(p.x - hx, p.z - hz).length();
                    let ground = self.heights.at(hx, hz).max(0.0);
                    let k = ((hd - 6.0) / 55.0).clamp(0.0, 1.0);
                    let k = k * k * (3.0 - 2.0 * k);
                    alt = ground + 1.2 + (cruise - ground - 1.2) * k;
                    // nose up while climbing, down while descending
                    if k < 0.99 && !d.data.hover {
                        let vel = Vector2::new((pos.x.0 - prev.x.0) as f32, (pos.y.0 - prev.y.0) as f32);
                        let outbound = vel.dot(Vector2::new(p.x - hx, p.z - hz)) > 0.0;
                        let pitch = if outbound { -0.18 } else { 0.1 } * (1.0 - k);
                        basis = basis * Basis::from_axis_angle(Vector3::RIGHT, pitch);
                    }
                }
                p.y = alt + (t * 1.7 + id as f32).sin() * 0.4;
                self.air_pos.insert(id, p);
                // engine trails behind moving jets
                if !d.data.hover && (pos.x.0 != prev.x.0 || pos.y.0 != prev.y.0) && ((t * 12.0) as u32 + id) % 2 == 0 {
                    let back = Vector3::new(-fx, 0.0, -fz) * (self.models.list[model].radius * 0.9);
                    self.events.push(ClientEvent { kind: "contrail", pos: p + back, to: p, size: 0.0, text: String::new(), dmg: 0, mine: false });
                }
            } else if d.layer == Layer::Water {
                let roll = (t * 1.1 + id as f32 * 0.37).sin() * 0.03;
                basis = basis * Basis::from_axis_angle(Vector3::BACK, roll);
            } else if d.class() == Class::Vehicle {
                // align to terrain slope
                let nrm = self.heights.normal(p.x, p.z);
                let up = Vector3::UP;
                let axis = up.cross(nrm);
                let ang = up.angle_to(nrm);
                if axis.length() > 1e-4 {
                    basis = Basis::from_axis_angle(axis.normalized(), ang * 0.8) * basis;
                }
            }
            // engines: jets roar past, rotors thump, ships chug (near the camera only)
            let moving = pos.x.0 != prev.x.0 || pos.y.0 != prev.y.0;
            if moving && !d.data.icbm && matches!(d.class(), Class::Aircraft | Class::Ship) && Vector2::new(p.x, p.z).distance_to(cull_c) < 120.0 {
                let now = self.time;
                let next = self.engine_next.get(&id).copied().unwrap_or(0.0);
                if now >= next {
                    let (kind, every) = if d.class() == Class::Ship { ("ship", 6.0) } else if d.data.hover { ("heli", 2.0) } else { ("jet", 5.0) };
                    self.engine_next.insert(id, now + every + (id % 7) as f64 * 0.3);
                    self.events.push(ClientEvent { kind: "engine", pos: p, to: p, size: 0.0, text: kind.into(), dmg: 0, mine: false });
                }
            }
            let xf = Transform3D::new(basis, p);
            let color = player_color(w.players.get(owner as usize).map_or(7, |pl| pl.color));
            let prog = if d.is_building() && !complete { progress as f32 / d.build_ticks.max(1) as f32 } else { 1.0 };
            let mut flags = 0.0;
            if self.hover == id {
                flags += 2.0;
            }
            if sel.contains(&id) {
                flags += 1.0;
            }
            let act = if carry > 0 && action == Action::Move { Action::Carry } else { action };
            let custom = [t + (id % 97) as f32 * 0.31, act as u8 as f32, prog, flags];
            // turret: aim at the current target
            let mut turret_yaw = 0.0;
            if target != 0 && (action == Action::Attack || d.is_building()) {
                if let Some(te) = w.get(target) {
                    let (tx, tz) = to_world2(te.pos);
                    let world_yaw = (tx - p.x).atan2(tz - p.z);
                    turret_yaw = world_yaw - yaw;
                }
            }
            let draw_model = match self.models.list[model].lod1 {
                Some(l) if (p - cam_pos).length_squared() > lod_dist2 => l,
                _ => model,
            };
            self.push_model(draw_model, xf, color, custom, turret_yaw, false);

            // selection ring + health bar
            let w = &self.session.world;
            let maxhp = w.max_hp(&w.entities[i]).max(1);
            let selected = sel.contains(&id);
            let r = self.models.list[model].radius;
            if d.is_unit() && !d.data.icbm {
                let gy = if d.layer == Layer::Air { self.heights.at(p.x, p.z).max(0.0) + 0.1 } else { p.y + 0.08 };
                let tr = if matches!(d.class(), Class::Citizen | Class::Infantry) { (r * 1.25).max(0.9) } else { r * 0.95 };
                let txf = Transform3D::new(Basis::from_scale(Vector3::new(tr * 2.0, 1.0, tr * 2.0)), Vector3::new(p.x, gy, p.z));
                let a = if d.layer == Layer::Air { 0.5 } else { 1.0 };
                self.team_batch.push(&txf, Color::from_rgba(color.r, color.g, color.b, a), [0.0; 4]);
            }
            if selected || self.hover == id {
                let ring_col = if owner == me {
                    Color::from_rgba(0.35, 1.0, 0.45, if selected { 1.0 } else { 0.5 })
                } else if w.is_enemy(me, owner) {
                    Color::from_rgba(1.0, 0.25, 0.2, 0.8)
                } else {
                    Color::from_rgba(1.0, 0.9, 0.3, 0.8)
                };
                let gy = if d.layer == Layer::Air { self.heights.at(p.x, p.z).max(0.0) + 0.15 } else { p.y + 0.12 };
                let rxf = Transform3D::new(Basis::from_scale(Vector3::new(r * 2.0, 1.0, r * 2.0)), Vector3::new(p.x, gy, p.z));
                self.ring_batch.push(&rxf, ring_col, [0.0; 4]);
            }
            let damaged = hp < maxhp;
            if selected || (self.hover == id && d.is_unit()) || (self.show_all_bars && damaged) || (damaged && d.is_building() && owner == me) || (!complete && d.is_building()) {
                let h = self.models.list[model].height;
                let bw = (r * 1.2).clamp(1.2, 4.5);
                let bxf = Transform3D::new(Basis::from_scale(Vector3::new(bw, bw, bw)), Vector3::new(p.x, p.y + h + 0.8, p.z));
                let sec = if !complete { prog } else { 0.0 };
                self.bar_batch.push(&bxf, Color::WHITE, [hp as f32 / maxhp as f32, sec, 0.0, 0.0]);
            }
        }

        // remembered enemy buildings under fog
        let rem: Vec<(EntityId, Remembered)> = self.remembered.iter().map(|(k, v)| (*k, v.clone())).collect();
        for (id, r) in rem {
            let w = &self.session.world;
            if let Some(e) = w.get(id) {
                if w.can_see(me, e) {
                    continue;
                }
            }
            let p = self.world_pos3(r.pos, Layer::Land, 0);
            if Vector2::new(p.x, p.z).distance_to(cull_c) > cull_r {
                continue;
            }
            let model = self.models.by_def[r.def as usize];
            let color = player_color(w.players[r.owner as usize].color);
            self.push_model(model, Transform3D::new(Basis::IDENTITY, p), color, [0.0, 0.0, r.progress, 4.0], 0.0, false);
        }

        // corpses / wrecks
        let mut corpses = std::mem::take(&mut self.corpses);
        for c in corpses.iter_mut() {
            c.t += dt;
            let k = c.t / c.life;
            let mut xf = c.xf;
            let (action, prog, flags) = match c.kind {
                0 => {
                    // fall, then sink into the ground
                    if k > 0.7 {
                        xf.origin.y -= (k - 0.7) / 0.3 * 1.2;
                    }
                    (7.0, 1.0, 0.0)
                }
                1 => {
                    xf.origin.y -= (k - 0.8).max(0.0) / 0.2 * 2.0;
                    (0.0, 1.0, 4.0)
                }
                2 => {
                    // collapse: sink and tilt
                    xf.origin.y -= k * k * self.models.list[c.model].height * 0.9;
                    xf.basis = xf.basis * Basis::from_axis_angle(Vector3::RIGHT, k * 0.12);
                    (0.0, 1.0, 4.0)
                }
                3 => {
                    if !c.impacted {
                        // Ballistic descent preserves momentum; compute from elapsed
                        // time so the trajectory does not depend on render framerate.
                        xf.origin += c.vel * c.t + Vector3::DOWN * (2.5 * c.t + 4.9 * c.t * c.t);
                        let gy = self.heights.at(xf.origin.x, xf.origin.z).max(0.0);
                        xf.basis = xf.basis
                            * Basis::from_axis_angle(Vector3::RIGHT, (c.t * 0.32).min(1.1))
                            * Basis::from_axis_angle(Vector3::BACK, c.roll * c.t * 1.5);
                        if xf.origin.y <= gy + 0.3 {
                            xf.origin.y = gy + 0.3;
                            c.impacted = true;
                            c.xf = xf;
                            let water = self.heights.at(xf.origin.x, xf.origin.z) < 0.0;
                            c.kind = if water { 4 } else { 1 };
                            c.vel = Vector3::ZERO;
                            c.t = 0.0;
                            c.life = if water { 7.0 } else { 10.0 };
                            self.events.push(ClientEvent { kind: if water { "wreck_splash" } else { "wreck_impact" },
                                pos: xf.origin, to: xf.origin, size: self.models.list[c.model].radius.max(2.0),
                                text: String::new(), dmg: 0, mine: false });
                        }
                    }
                    (0.0, 1.0, 16.0)
                }
                4 => {
                    // Lose headway, list, then settle below the waterline. A ship
                    // keeps its full silhouette until the hull actually submerges.
                    xf.origin += c.vel * ((1.0 - (-c.t * 0.5).exp()) * 2.0);
                    let settle = k * k;
                    xf.origin.y -= settle * (self.models.list[c.model].radius * 2.0 + self.models.list[c.model].height + 3.0);
                    xf.basis = xf.basis
                        * Basis::from_axis_angle(Vector3::BACK, c.roll * k.powf(0.7) * 1.25)
                        * Basis::from_axis_angle(Vector3::RIGHT, k * 0.38);
                    (0.0, 1.0, 16.0)
                }
                5 => {
                    let fall = (c.t / 0.85).min(1.0);
                    xf.basis = xf.basis * Basis::from_axis_angle(Vector3::RIGHT, fall * fall * 1.5);
                    xf.origin.y -= (c.t - 2.0).max(0.0) * 1.2;
                    (0.0, 1.0, 16.0)
                }
                _ => (0.0, 1.0, 16.0),
            };
            let custom = [if c.kind == 0 { c.t * 1.5 } else { 0.0 }, action, prog, flags];
            if matches!(c.kind, 3 | 4) && c.t >= c.fx_at && k < 0.7
                && Vector2::new(xf.origin.x, xf.origin.z).distance_to(cull_c) < cull_r {
                c.fx_at = c.t + if c.kind == 3 { 0.12 } else { 0.65 };
                let mut pos = xf.origin;
                if c.kind == 4 { pos.y = 0.15; }
                self.events.push(ClientEvent { kind: if c.kind == 3 { "wreck_trail" } else { "wreck_foam" },
                    pos, to: pos, size: self.models.list[c.model].radius.max(2.0),
                    text: String::new(), dmg: 0, mine: false });
            }
            self.push_model(c.model, xf, c.color, custom, 0.0, false);
        }
        corpses.retain(|c| c.t < c.life);
        self.corpses = corpses;

        // projectiles
        let mut seen = HashMap::new();
        {
            let w = &self.session.world;
            for pr in &w.projectiles {
                let (tx, ty) = pr.pos.tile();
                if !w.visible(me, tx, ty) {
                    continue;
                }
                let (x, z) = to_world2(pr.pos);
                let (sx, sz) = to_world2(pr.start);
                let (ax, az) = to_world2(pr.aim);
                let total = ((ax - sx).powi(2) + (az - sz).powi(2)).sqrt().max(0.01);
                let done = ((x - sx).powi(2) + (z - sz).powi(2)).sqrt() / total;
                let src_air = data().def(pr.src_def).layer == Layer::Air;
                let torpedo = pr.dmg_type == DamageType::Torpedo;
                // launch height: the shooter's real altitude / muzzle height
                let start_y = *self.proj_start.entry(pr.id).or_insert_with(|| {
                    if src_air {
                        self.air_pos.get(&pr.src).map_or(AIR_ALT, |v| v.y - 1.0)
                    } else if torpedo {
                        -0.4
                    } else {
                        self.heights.at(sx, sz).max(0.0) + 2.0
                    }
                });
                // end height: an aircraft target's altitude, else the ground/water
                let tgt_air = w.get(pr.target).map_or(false, |t| data().def(t.def).layer == Layer::Air);
                let end_y = if tgt_air {
                    self.air_pos.get(&pr.target).map_or(AIR_ALT, |v| v.y)
                } else if torpedo {
                    -0.4
                } else {
                    self.heights.at(ax, az).max(0.0) + 1.0
                };
                let base_y = start_y + (end_y - start_y) * done.min(1.0);
                // ballistic arc for shells
                let arc = if pr.homing || src_air { 0.0 } else { (done * std::f32::consts::PI).sin() * (total * 0.22).min(40.0) };
                let pos = Vector3::new(x, base_y + arc, z);
                let prevp = self.proj_prev.get(&pr.id).copied().unwrap_or(Vector3::new(sx, base_y, sz));
                seen.insert(pr.id, pos);
                let dir = pos - prevp;
                let len = dir.length();
                if len > 1e-3 {
                    let fwd = dir / len;
                    let up = if fwd.y.abs() > 0.95 { Vector3::RIGHT } else { Vector3::UP };
                    let right = up.cross(fwd).normalized();
                    let up2 = fwd.cross(right);
                    let basis = Basis::from_cols(right, up2, fwd);
                    // size per munition: thick glowing shells, long missiles, dark bombs
                    let (thick, long, col) = match pr.dmg_type {
                        DamageType::NavalGun => (5.0, 4.5, Color::from_rgba(1.0, 0.75, 0.35, 1.0)),
                        DamageType::Cannon | DamageType::Explosive => (3.0, 2.6, Color::from_rgba(1.0, 0.8, 0.4, 1.0)),
                        DamageType::Missile | DamageType::Flak => (2.8, 3.2, Color::from_rgba(1.0, 0.62, 0.25, 1.0)),
                        DamageType::Interceptor => (3.5, 5.0, Color::from_rgba(1.0, 0.95, 0.8, 1.0)),
                        DamageType::AirGun => (1.8, 6.0, Color::from_rgba(1.0, 0.85, 0.45, 1.0)),
                        DamageType::Bomb | DamageType::Nuclear => (6.0, 2.6, Color::from_rgba(0.25, 0.25, 0.22, 1.0)),
                        DamageType::Torpedo => (2.5, 3.0, Color::from_rgba(0.5, 0.6, 0.65, 0.5)),
                        _ => (1.5, 2.0, Color::from_rgba(1.0, 0.8, 0.4, 1.0)),
                    };
                    let basis = Basis::from_cols(basis.col_a() * thick, basis.col_b() * thick, basis.col_c() * long);
                    let xf = Transform3D::new(basis, pos);
                    self.proj_batch.push(&xf, col, [1.0, 0.0, 0.0, 0.0]);
                    if pr.dmg_type == DamageType::NavalGun {
                        // a battleship fires a full turret salvo
                        let side = basis.col_a().normalized() * 1.8;
                        for k in [-1.0f32, 1.0] {
                            let o = pos + side * k + Vector3::new(0.0, (pr.id as f32 * k).sin() * 0.6, 0.0);
                            self.proj_batch.push(&Transform3D::new(basis, o), col, [1.0, 0.0, 0.0, 0.0]);
                        }
                    }
                    let kind = match pr.dmg_type {
                        DamageType::Torpedo => "wake",
                        DamageType::Interceptor => "missile_trail",
                        DamageType::NavalGun | DamageType::Cannon => "shell_trail",
                        _ if pr.homing => "trail",
                        _ => "",
                    };
                    if !kind.is_empty() {
                        let wpos = if torpedo { Vector3::new(pos.x, 0.12, pos.z) } else { pos };
                        self.events.push(ClientEvent { kind, pos: wpos, to: prevp, size: 0.0, text: String::new(), dmg: 0, mine: false });
                    }
                }
            }
        }
        self.proj_start.retain(|k, _| seen.contains_key(k));
        self.proj_prev = seen;

        // tracers
        let mut tr = std::mem::take(&mut self.tracers);
        for tc in tr.iter_mut() {
            tc.t += dt;
            if tc.t < 0.0 {
                continue;
            }
            let k = (tc.t / tc.life).clamp(0.0, 1.0);
            let dir = tc.b - tc.a;
            let len = dir.length().max(0.01);
            let fwd = dir / len;
            // streak travels along the line
            let seg = (len * 0.35).min(6.0);
            let start = tc.a + fwd * ((len - seg) * k);
            let cam_up = Vector3::UP;
            let side = cam_up.cross(fwd).normalized();
            let up = fwd.cross(side);
            let basis = Basis::from_cols(fwd * seg, up, side * 0.22);
            let xf = Transform3D::new(basis, start);
            self.tracer_batch.push(&xf, tc.color, [1.0 - k * 0.5, 0.0, 0.0, 0.0]);
        }
        tr.retain(|tc| tc.t < tc.life);
        self.tracers = tr;

        // rally flags for selected production buildings
        self.flag_pole.begin();
        self.flag_cloth.begin();
        {
            let w = &self.session.world;
            let mycol = player_color(w.players[me as usize].color);
            let mut flags = Vec::new();
            for &id in &self.selection {
                if let Some(e) = w.get(id) {
                    if e.owner == me && data().def(e.def).is_building() {
                        if let Some(r) = e.rally {
                            flags.push(r);
                        }
                    }
                }
            }
            for r in flags {
                let (x, z) = to_world2(r);
                let y = self.heights.at(x, z).max(0.0);
                let wave = (t * 3.0).sin() * 0.15;
                self.flag_pole.push(&Transform3D::new(Basis::IDENTITY, Vector3::new(x, y + 1.8, z)), Color::WHITE, [0.0, 0.0, 1.0, 0.0]);
                self.flag_cloth.push(&Transform3D::new(Basis::from_axis_angle(Vector3::UP, wave), Vector3::new(x, y + 3.1, z + 0.75)), mycol, [0.0, 0.0, 1.0, 0.0]);
            }
        }
        self.flag_pole.finish();
        self.flag_cloth.finish();
        for b in self.batches.values_mut() {
            b.finish();
        }
        self.ring_batch.finish();
        self.team_batch.finish();
        self.bar_batch.finish();
        self.tracer_batch.finish();
        self.proj_batch.finish();

        if self.static_dirty {
            self.static_dirty = false;
            self.render_static();
        }
    }

    /// Trees, mines, berries: rebuilt only when resources or fog change.
    fn render_static(&mut self) {
        for b in self.static_batches.values_mut() {
            b.begin();
        }
        let n = self.session.world.entities.len();
        let tree = data().id("tree");
        for i in 1..n {
            let (def, pos, id, amount, progress, complete) = {
                let e = &self.session.world.entities[i];
                if !e.alive {
                    continue;
                }
                (e.def, e.pos, e.id, e.amount, e.progress, e.complete)
            };
            let d = data().def(def);
            if !d.is_resource() || d.layer == Layer::Water || self.session.world.fish.contains(&id) {
                continue;
            }
            let (tx, ty) = pos.tile();
            let w = &self.session.world;
            let fog = 0.0;
            let (x, z) = to_world2(pos);
            // jitter trees inside their tile so forests don't look gridded
            let hsh = (id.wrapping_mul(2654435761)) >> 8;
            let (jx, jz) = if def == tree || d.data.plantable {
                (((hsh & 0xff) as f32 / 255.0 - 0.5) * TILE * 0.55, (((hsh >> 8) & 0xff) as f32 / 255.0 - 0.5) * TILE * 0.55)
            } else {
                (0.0, 0.0)
            };
            let wx = x + jx;
            let wz = z + jz;
            let y = self.heights.at(wx, wz);
            let yaw = ((hsh >> 16) & 0xff) as f32 / 255.0 * std::f32::consts::TAU;
            let mut s = if def == tree || d.data.plantable { 0.8 + ((hsh >> 4) & 0x3f) as f32 / 63.0 * 0.5 } else { 1.0 };
            if d.data.plantable {
                // grows from a seedling to a full tree
                let g = if complete { progress as f32 / (d.build_ticks + d.grow_ticks).max(1) as f32 } else { 0.0 };
                s *= 0.12 + 0.75 * g.clamp(0.0, 1.0);
            }
            let model = if def == tree {
                let t = w.map.terrain_at(tx, ty);
                if t == ee_sim::map::Terrain::Beach {
                    self.palm_model
                } else if matches!(t, ee_sim::map::Terrain::Rock | ee_sim::map::Terrain::Mountain) || (hsh % 3 == 0) {
                    self.pine_model
                } else {
                    self.models.by_def[def as usize]
                }
            } else {
                self.models.by_def[def as usize]
            };
            // deplete visually: mines shrink as they're mined out
            let depl = if d.data.amount > 0 && def != tree && d.data.regrow == 0 { (amount as f32 / d.data.amount.max(1) as f32).clamp(0.35, 1.0).sqrt() } else { 1.0 };
            let basis = Basis::from_axis_angle(Vector3::UP, yaw).scaled(Vector3::new(s, s * depl, s));
            let xf = Transform3D::new(basis, Vector3::new(wx, y - 0.05, wz));
            let nparts = self.models.list[model].parts.len();
            for pi in 0..nparts {
                let local = self.models.list[model].parts[pi].local;
                let b = self.batch(model, pi, true);
                let v = ((hsh >> 3) & 0xff) as f32 / 255.0;
                let tint = if def == tree { Color::from_rgb(0.85 + v * 0.3, 0.9 + v * 0.15, 0.8 + (1.0 - v) * 0.25) } else { Color::WHITE };
                b.push(&(xf * local), tint, [0.0, 0.0, 1.0, fog]);
            }
        }
        for b in self.static_batches.values_mut() {
            b.finish();
        }
    }

    /// Grass, flowers, pebbles and bushes: client-only scenery, built once.
    fn build_decorations(&mut self, deco: &[usize]) {
        use ee_sim::map::Terrain;
        let w = &self.session.world;
        let map = &w.map;
        let mut inst: Vec<(usize, Transform3D, Color)> = Vec::new();
        let hash = |x: i32, y: i32, k: u32| -> u32 {
            let mut h = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b) ^ k.wrapping_mul(0xc2b2_ae35);
            h ^= h >> 13;
            h = h.wrapping_mul(0x27d4_eb2d);
            h ^= h >> 15;
            h
        };
        for ty in 0..map.h {
            for tx in 0..map.w {
                let i = map.idx(tx, ty);
                if map.occupant[i] != 0 {
                    continue;
                }
                let t = map.terrain[i];
                let near_water = (-1..=1).any(|dy| (-1..=1).any(|dx| map.is_water(tx + dx, ty + dy)));
                let (count, pick): (u32, &[(usize, u32)]) = match t {
                    Terrain::Grass => (3, &[(0, 70), (1, 12), (2, 8), (3, 10)]),
                    Terrain::Meadow => (4, &[(0, 50), (1, 40), (3, 10)]),
                    Terrain::Forest => (2, &[(0, 40), (3, 40), (2, 20)]),
                    Terrain::Dirt => (1, &[(2, 50), (0, 50)]),
                    Terrain::Rock => (2, &[(2, 70), (0, 30)]),
                    Terrain::Beach if near_water => (1, &[(4, 40), (2, 60)]),
                    _ => (0, &[]),
                };
                for k in 0..count {
                    let h = hash(tx, ty, k + 1);
                    if h % 100 > 55 {
                        continue;
                    }
                    let roll = (h >> 8) % 100;
                    let mut acc = 0;
                    let mut which = pick[0].0;
                    for &(m, p) in pick {
                        acc += p;
                        if roll < acc {
                            which = m;
                            break;
                        }
                    }
                    let fx = ((h >> 16) & 0xff) as f32 / 255.0;
                    let fz = ((h >> 24) & 0xff) as f32 / 255.0;
                    let x = (tx as f32 + fx) * TILE;
                    let z = (ty as f32 + fz) * TILE;
                    let y = self.heights.at(x, z);
                    if y < 0.05 && which != 4 {
                        continue;
                    }
                    let yaw = ((h >> 4) & 0xff) as f32 / 255.0 * std::f32::consts::TAU;
                    let s = 0.7 + ((h >> 12) & 0x3f) as f32 / 63.0 * 0.8;
                    let v = ((h >> 20) & 0xff) as f32 / 255.0;
                    let tint = Color::from_rgb(0.8 + v * 0.35, 0.85 + v * 0.2, 0.7 + (1.0 - v) * 0.3);
                    inst.push((deco[which], Transform3D::new(Basis::from_axis_angle(Vector3::UP, yaw).scaled(Vector3::splat(s)), Vector3::new(x, y - 0.03, z)), tint));
                }
            }
        }
        let mut batches: HashMap<(usize, usize), Batch> = HashMap::new();
        for (model, xf, tint) in inst {
            let nparts = self.models.list[model].parts.len();
            for pi in 0..nparts {
                let b = batches.entry((model, pi)).or_insert_with(|| {
                    let mesh: Gd<Mesh> = self.models.list[model].parts[pi].mesh.clone();
                    let mut b = Batch::new(&mut self.root, &mesh, false, None);
                    b.begin();
                    b
                });
                let local = self.models.list[model].parts[pi].local;
                b.push(&(xf * local), tint, [0.0, 0.0, 1.0, 0.0]);
            }
        }
        for b in batches.values_mut() {
            b.finish();
        }
        self.deco_batches = batches.into_values().collect();
    }

    /// Floating "workers / capacity" labels over resources and granaries we work.
    fn update_work_labels(&mut self, cam: Option<&Gd<Camera3D>>) {
        use godot::classes::label_3d::AlphaCutMode;
        use godot::classes::base_material_3d::BillboardMode;
        let w = &self.session.world;
        let me = self.me;
        let cam_pos = cam.map(|c| c.get_global_position()).unwrap_or(Vector3::ZERO);
        let mut workers: std::collections::BTreeMap<EntityId, i32> = std::collections::BTreeMap::new();
        for e in &w.entities {
            if !e.alive || e.owner != me {
                continue;
            }
            if let Order::Gather { node } = e.order {
                *workers.entry(node).or_insert(0) += 1;
            }
        }
        // (world pos, text, color)
        let mut shown: Vec<(Vector3, String, Color)> = Vec::new();
        let granary = data().id("granary");
        let farm = data().id("farm");
        for (&node, &n) in &workers {
            let Some(e) = w.get(node) else { continue };
            let d = data().def(e.def);
            // only mines and fish shoals: trees/berries would clutter the view
            if d.data.walkable || (d.size() == (1, 1) && !w.fish.contains(&node)) {
                continue;
            }
            let cap = ee_sim::world::gather_cap(d);
            let p = self.world_pos3(e.pos, if w.fish.contains(&node) { Layer::Water } else { Layer::Land }, 0);
            let h = self.models.list[self.models.by_def[e.def as usize]].height;
            let full = n >= cap;
            let text = if full { format!("FULL {}/{}", n.min(cap), cap) } else { format!("{}/{}", n, cap) };
            let col = if full { Color::from_rgb(0.45, 1.0, 0.45) } else { Color::from_rgb(1.0, 0.85, 0.45) };
            shown.push((p + Vector3::UP * (h + 1.5), text, col));
        }
        for g in w.entities.iter().filter(|e| e.alive && e.owner == me && e.def == granary && e.complete) {
            let (gx, gy) = g.tile;
            let mut fields = 0;
            let mut farmed = 0;
            for e in w.entities.iter().filter(|e| e.alive && e.owner == me && e.def == farm && e.complete) {
                let (fx, fy) = e.tile;
                if (fx - gx).abs() <= 3 && (fy - gy).abs() <= 3 {
                    fields += 1;
                    if workers.get(&e.id).copied().unwrap_or(0) > 0 {
                        farmed += 1;
                    }
                }
            }
            if fields == 0 {
                continue;
            }
            let full = fields == 8 && farmed == 8;
            let text = if full { "Fields FULL".to_string() } else { format!("Fields {}/{}", farmed, fields) };
            let col = if full { Color::from_rgb(0.45, 1.0, 0.45) } else if farmed == fields { Color::from_rgb(0.8, 1.0, 0.6) } else { Color::from_rgb(1.0, 0.85, 0.45) };
            let p = self.world_pos3(g.pos, Layer::Land, 0);
            shown.push((p + Vector3::UP * 9.5, text, col));
        }
        shown.retain(|(p, _, _)| p.distance_to(cam_pos) < 260.0);
        shown.truncate(80);
        while self.labels.len() < shown.len() {
            let mut l = godot::classes::Label3D::new_alloc();
            l.set_billboard_mode(BillboardMode::ENABLED);
            l.set_font_size(30);
            l.set_outline_size(10);
            l.set_pixel_size(0.00055);
            l.set_draw_flag(godot::classes::label_3d::DrawFlags::FIXED_SIZE, true);
            l.set_draw_flag(godot::classes::label_3d::DrawFlags::DISABLE_DEPTH_TEST, true);
            l.set_alpha_cut_mode(AlphaCutMode::DISABLED);
            l.set_outline_modulate(Color::from_rgba(0.0, 0.0, 0.0, 0.85));
            self.root.add_child(&l);
            self.labels.push(l);
        }
        for (i, l) in self.labels.iter_mut().enumerate() {
            if let Some((p, text, col)) = shown.get(i) {
                l.set_visible(true);
                l.set_position(*p);
                l.set_text(text.as_str());
                l.set_modulate(*col);
            } else {
                l.set_visible(false);
            }
        }
    }

    // ------------------------------------------------------------------ picking

    pub fn ground_at(&self, cam: &Gd<Camera3D>, screen: Vector2) -> Option<Vector3> {
        let o = cam.project_ray_origin(screen);
        let dir = cam.project_ray_normal(screen);
        let surf = |p: Vector3| -> f32 { self.heights.at(p.x, p.z).max(0.0) };
        let mut t = 0.0f32;
        let step = 2.0;
        let mut prev = o;
        for _ in 0..1500 {
            let p = o + dir * t;
            if p.y <= surf(p) {
                // refine
                let (mut a, mut b) = (t - step, t);
                for _ in 0..12 {
                    let m = (a + b) * 0.5;
                    let pm = o + dir * m;
                    if pm.y <= surf(pm) {
                        b = m;
                    } else {
                        a = m;
                    }
                }
                let hit = o + dir * b;
                return Some(hit);
            }
            prev = p;
            t += step;
            if p.y < -50.0 {
                break;
            }
        }
        let _ = prev;
        None
    }

    fn entity_screen(&self, cam: &Gd<Camera3D>, e: &Entity) -> Option<(Vector2, f32)> {
        let d = data().def(e.def);
        let p = self.world_pos3(e.pos, d.layer, e.id);
        let model = &self.models.list[self.models.by_def[e.def as usize]];
        let c = p + Vector3::UP * (model.height * 0.45);
        if cam.is_position_behind(c) {
            return None;
        }
        let s = cam.unproject_position(c);
        let edge = cam.unproject_position(c + cam.get_global_transform().basis.col_a() * model.radius);
        Some((s, (edge - s).length().max(10.0)))
    }

    /// Entity under the cursor (visible only).
    pub fn pick(&self, cam: &Gd<Camera3D>, screen: Vector2) -> EntityId {
        let w = &self.session.world;
        let ground = self.ground_at(cam, screen);
        let mut best: Option<(f32, EntityId)> = None;
        let center = ground.map(|g| Vector2::new(g.x, g.z));
        for e in &w.entities {
            if !e.alive || e.inside != 0 {
                continue;
            }
            let d = data().def(e.def);
            if d.is_resource() && d.layer == Layer::Water && !w.fish.contains(&e.id) {
                continue;
            }
            if let Some(c) = center {
                let (x, z) = to_world2(e.pos);
                let lim = if d.layer == Layer::Air { 60.0 } else if d.is_building() { 30.0 } else { 14.0 };
                if Vector2::new(x, z).distance_to(c) > lim {
                    continue;
                }
            }
            if e.owner != self.me && !d.is_resource() && !w.can_see(self.me, e) {
                continue;
            }
            if d.is_resource() {
                let (tx, ty) = e.pos.tile();
                if !w.explored(self.me, tx, ty) {
                    continue;
                }
            }
            // buildings: footprint test against the ground point
            if (d.is_building() || d.size().0 > 1) && ground.is_some() {
                let g = ground.unwrap();
                let (sw, sh) = d.size();
                let x0 = e.tile.0 as f32 * TILE;
                let z0 = e.tile.1 as f32 * TILE;
                if g.x >= x0 && g.x <= x0 + sw as f32 * TILE && g.z >= z0 && g.z <= z0 + sh as f32 * TILE {
                    let score = 30.0;
                    if best.map_or(true, |(b, _)| score < b) {
                        best = Some((score, e.id));
                    }
                    continue;
                }
            }
            if let Some((s, r)) = self.entity_screen(cam, e) {
                let dist = s.distance_to(screen);
                if dist < r {
                    let score = dist / r * 20.0 + if d.is_resource() { 15.0 } else { 0.0 };
                    if best.map_or(true, |(b, _)| score < b) {
                        best = Some((score, e.id));
                    }
                }
            }
        }
        best.map_or(0, |b| b.1)
    }

    pub fn select_rect(&mut self, cam: &Gd<Camera3D>, a: Vector2, b: Vector2, add: bool) {
        let min = a.coord_min(b);
        let max = a.coord_max(b);
        let w = &self.session.world;
        let mut units = Vec::new();
        let mut buildings = Vec::new();
        for e in &w.entities {
            if !e.alive || e.owner != self.me || e.inside != 0 {
                continue;
            }
            let d = data().def(e.def);
            if d.is_resource() {
                continue;
            }
            if let Some((s, _)) = self.entity_screen(cam, e) {
                if s.x >= min.x && s.x <= max.x && s.y >= min.y && s.y <= max.y {
                    if d.is_unit() {
                        units.push(e.id);
                    } else {
                        buildings.push(e.id);
                    }
                }
            }
        }
        // prefer combat units over citizens over buildings
        let combat: Vec<EntityId> = units.iter().copied().filter(|&id| w.class_of(id) != Some(Class::Citizen)).collect();
        let chosen = if !combat.is_empty() && units.len() > combat.len() && combat.len() * 4 >= units.len() {
            combat
        } else if !units.is_empty() {
            units
        } else {
            buildings
        };
        if !add {
            self.selection.clear();
        }
        for id in chosen {
            if !self.selection.contains(&id) {
                self.selection.push(id);
            }
        }
    }

    pub fn select_click(&mut self, cam: &Gd<Camera3D>, screen: Vector2, add: bool, same_type: bool) {
        let id = self.pick(cam, screen);
        if !add {
            self.selection.clear();
        }
        if id == 0 {
            return;
        }
        let w = &self.session.world;
        let e = w.get(id).unwrap();
        if same_type && e.owner == self.me {
            let def = e.def;
            // all of this type on screen
            // only what's actually on screen
            let vp = cam.get_viewport().map(|v| v.get_visible_rect()).unwrap_or(Rect2::new(Vector2::ZERO, Vector2::new(1e6, 1e6)));
            let vp_ids: Vec<EntityId> = w
                .entities
                .iter()
                .filter(|o| o.alive && o.owner == self.me && o.def == def && o.inside == 0)
                .filter_map(|o| self.entity_screen(cam, o).filter(|(s, _)| vp.contains_point(*s)).map(|_| o.id))
                .collect();
            for v in vp_ids {
                if !self.selection.contains(&v) {
                    self.selection.push(v);
                }
            }
            return;
        }
        if add {
            if let Some(i) = self.selection.iter().position(|&s| s == id) {
                self.selection.remove(i);
                return;
            }
        }
        self.selection.push(id);
    }

    pub fn my_selected_units(&self) -> Vec<EntityId> {
        let w = &self.session.world;
        self.selection
            .iter()
            .copied()
            .filter(|&id| w.get(id).map_or(false, |e| e.owner == self.me && data().def(e.def).is_unit()))
            .collect()
    }

    pub fn my_selected_buildings(&self) -> Vec<EntityId> {
        let w = &self.session.world;
        self.selection
            .iter()
            .copied()
            .filter(|&id| w.get(id).map_or(false, |e| e.owner == self.me && data().def(e.def).is_building()))
            .collect()
    }

    /// Right-click: smart command for units, rally point for buildings.
    pub fn command_at(&mut self, target: EntityId, ground: Option<Vector3>, queue: bool, attack_move: bool) {
        let units = self.my_selected_units();
        let buildings = self.my_selected_buildings();
        let w = &self.session.world;
        if !units.is_empty() {
            if target != 0 && !attack_move {
                // transports right-clicking land = unload there
                self.issue(CommandKind::Target { units, target, queue });
                self.events.push(ClientEvent { kind: "ack", pos: Vector3::ZERO, to: Vector3::ZERO, size: 0.0, text: "target".into(), dmg: 0, mine: true });
            } else if let Some(g) = ground {
                let to = from_world(g.x, g.z);
                let (tx, ty) = to.tile();
                // loaded transports clicking on land: unload
                let transports: Vec<EntityId> = units
                    .iter()
                    .copied()
                    .filter(|&id| w.get(id).map_or(false, |e| data().def(e.def).data.cargo > 0 && !e.cargo.is_empty()))
                    .collect();
                let others: Vec<EntityId> = units.iter().copied().filter(|id| !transports.contains(id)).collect();
                if !transports.is_empty() && w.map.is_land(tx, ty) {
                    self.issue(CommandKind::Unload { units: transports, at: to });
                } else if !transports.is_empty() {
                    self.issue(CommandKind::Move { units: transports, to, attack_move, queue });
                }
                if !others.is_empty() {
                    self.issue(CommandKind::Move { units: others, to, attack_move, queue });
                }
                self.events.push(ClientEvent { kind: "move_marker", pos: g, to: g, size: 0.0, text: String::new(), dmg: 0, mine: attack_move });
            }
        } else if !buildings.is_empty() {
            if let Some(g) = ground {
                let to = from_world(g.x, g.z);
                if self.launch_at(to) {
                    return;
                }
                self.issue(CommandKind::SetRally { buildings, to, target });
                self.events.push(ClientEvent { kind: "move_marker", pos: g, to: g, size: 0.0, text: String::new(), dmg: 0, mine: false });
            }
        }
    }
}

impl Client {
    /// Fire one ICBM from the first selected silo that has one. Returns false if none.
    pub fn launch_at(&mut self, at: FVec) -> bool {
        let w = self.world();
        let silo = self.my_selected_buildings().into_iter().find(|&b| {
            w.get(b).map_or(false, |e| data().def(e.def).trains.iter().any(|&u| data().def(u).data.icbm) && !e.cargo.is_empty())
        });
        match silo {
            Some(b) => {
                self.issue(CommandKind::Launch { building: b, at });
                true
            }
            None => false,
        }
    }
}

/// ICBM height above ground (m) and climb slope at `at`, on a parabola from `from` to `to`.
pub fn icbm_arc(from: Option<FVec>, to: Option<FVec>, at: FVec) -> (f32, f32) {
    let (Some(a), Some(b)) = (from, to) else { return (40.0, 0.0) };
    let (ax, az) = to_world2(a);
    let (bx, bz) = to_world2(b);
    let (px, pz) = to_world2(at);
    let total = Vector2::new(bx - ax, bz - az).length().max(1.0);
    let t = (Vector2::new(px - ax, pz - az).length() / total).clamp(0.0, 1.0);
    let h = (total * 0.32).clamp(70.0, 260.0);
    (4.0 * h * t * (1.0 - t) + 2.0, 4.0 * h * (1.0 - 2.0 * t) / total)
}

fn shader_mat(path: &str) -> Gd<ShaderMaterial> {
    let mut m = ShaderMaterial::new_gd();
    m.set_shader(&godot::tools::load::<Shader>(path));
    m
}

#[allow(dead_code)]
fn _unused(_: ProdItem, _: Order) {}
