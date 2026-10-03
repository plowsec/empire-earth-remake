//! Lockstep session. Owns the World, schedules commands from every source (local
//! UI, AI, network peers) for `tick + input_delay`, and only advances a tick when
//! every player's commands for it are known.
//!
//! Single player uses `LocalTransport` (nothing to wait for). Online play only
//! needs a `Transport` that ships `TickCommands` between peers; the rest of the
//! game is already deterministic and command-driven.
use ee_sim::command::{Command, CommandKind, TickCommands};
use ee_sim::defs::TICKS_PER_SEC;
use ee_sim::world::{MatchConfig, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Something that produces commands for one player by looking at the world: the AI.
pub trait Controller: Send {
    fn player(&self) -> u8;
    /// Called once per tick before the tick executes. Return commands to issue.
    fn think(&mut self, world: &World) -> Vec<CommandKind>;
    /// Human-readable internal state for debugging/telemetry.
    fn debug(&self) -> String {
        String::new()
    }
    /// Serialized internal state for save games (None = stateless / not saveable).
    fn save_state(&self) -> Option<String> {
        None
    }
}

/// How a computer player was set up, so a replay can recreate it exactly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiSetup {
    pub player: u8,
    pub difficulty: i32,
    pub seed: u64,
}

/// Network seam. A transport carries each peer's per-tick command bundles.
pub trait Transport: Send {
    /// Broadcast the commands local players scheduled for `tc.tick`.
    fn send(&mut self, player: u8, tc: &TickCommands);
    /// Bundles received from remote players since the last poll.
    fn poll(&mut self) -> Vec<(u8, TickCommands)>;
    /// Remote players whose input must arrive before a tick can run.
    fn remote_players(&self) -> Vec<u8>;
    /// Exchange a checksum for desync detection (no-op locally).
    fn report_checksum(&mut self, _tick: u32, _hash: u64) {}
}

pub struct LocalTransport;
impl Transport for LocalTransport {
    fn send(&mut self, _player: u8, _tc: &TickCommands) {}
    fn poll(&mut self) -> Vec<(u8, TickCommands)> {
        vec![]
    }
    fn remote_players(&self) -> Vec<u8> {
        vec![]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Replay {
    pub seed: u64,
    pub players: usize,
    pub data_hash: u64,
    pub ticks: Vec<TickCommands>,
    /// full match setup (absent in old replays)
    #[serde(default)]
    pub config: Option<MatchConfig>,
    #[serde(default)]
    pub ais: Vec<AiSetup>,
    /// (tick, world checksum) samples to detect divergence on re-simulation
    #[serde(default)]
    pub checksums: Vec<(u32, u64)>,
    /// last tick played
    #[serde(default)]
    pub end_tick: u32,
    /// command latency the session ran with (AI commands land this many ticks later)
    #[serde(default = "one")]
    pub input_delay: u32,
    /// simulation build that recorded it (see `ee_sim::SIM_VERSION`)
    #[serde(default)]
    pub sim_version: String,
}

fn one() -> u32 {
    1
}

pub struct Session {
    pub world: World,
    /// players controlled from this machine (UI + local AIs)
    local_players: Vec<u8>,
    pub input_delay: u32,
    /// tick -> commands
    scheduled: BTreeMap<u32, Vec<Command>>,
    /// tick -> remote players that delivered their bundle
    arrived: BTreeMap<u32, Vec<u8>>,
    controllers: Vec<Box<dyn Controller>>,
    transport: Box<dyn Transport>,
    pub replay: Vec<TickCommands>,
    pub record_replay: bool,
    accumulator: f64,
    /// 1.0 = normal
    pub speed: f64,
    pub paused: bool,
    pub checksum_interval: u32,
    pub checksums: Vec<(u32, u64)>,
    /// events from every executed tick since the client last drained them
    pub event_log: Vec<ee_sim::world::SimEvent>,
    pub collect_events: bool,
    /// computer players (recorded in replays)
    pub ai_setup: Vec<AiSetup>,
    /// match setup at tick 0 (a loaded save keeps the original)
    pub start_config: MatchConfig,
}

impl Session {
    pub fn new(config: MatchConfig, local_players: Vec<u8>, transport: Box<dyn Transport>) -> Session {
        Session {
            start_config: config.clone(),
            ai_setup: Vec::new(),
            world: World::new(config),
            local_players,
            input_delay: 2,
            scheduled: BTreeMap::new(),
            arrived: BTreeMap::new(),
            controllers: Vec::new(),
            transport,
            replay: Vec::new(),
            record_replay: true,
            accumulator: 0.0,
            speed: 1.0,
            paused: false,
            checksum_interval: 100,
            checksums: Vec::new(),
            event_log: Vec::new(),
            collect_events: false,
        }
    }

    pub fn single_player(config: MatchConfig) -> Session {
        let mut s = Session::new(config, vec![0], Box::new(LocalTransport));
        s.input_delay = 1;
        s
    }

    pub fn add_controller(&mut self, c: Box<dyn Controller>) {
        if !self.local_players.contains(&c.player()) {
            self.local_players.push(c.player());
        }
        self.controllers.push(c);
    }

    /// Schedule a command for an exact tick (replays).
    pub fn schedule(&mut self, tick: u32, cmd: Command) {
        self.scheduled.entry(tick).or_default().push(cmd);
    }

    /// Saved state of every controller, in order (None for stateless ones).
    pub fn controller_states(&self) -> Vec<Option<String>> {
        self.controllers.iter().map(|c| c.save_state()).collect()
    }

    /// Issue a command for a local player. It executes `input_delay` ticks later.
    pub fn issue(&mut self, player: u8, kind: CommandKind) {
        let tick = self.world.tick + self.input_delay;
        self.scheduled.entry(tick).or_default().push(Command { player, kind });
    }

    fn ready(&self, tick: u32) -> bool {
        let remote = self.transport.remote_players();
        if remote.is_empty() {
            return true;
        }
        let got = self.arrived.get(&tick);
        remote.iter().all(|p| got.map_or(false, |g| g.contains(p)))
    }

    /// Run one tick if inputs are ready. Returns false when waiting on peers.
    pub fn step_once(&mut self) -> bool {
        for (player, tc) in self.transport.poll() {
            self.scheduled.entry(tc.tick).or_default().extend(tc.commands);
            self.arrived.entry(tc.tick).or_default().push(player);
        }
        let tick = self.world.tick;
        // controllers think on the state they can see now; their commands land later
        let mut ctrl = std::mem::take(&mut self.controllers);
        for c in ctrl.iter_mut() {
            for kind in c.think(&self.world) {
                let p = c.player();
                self.issue(p, kind);
            }
        }
        self.controllers = ctrl;
        // ship our bundle for the tick that just got its last local input
        let send_tick = tick + self.input_delay;
        for &p in &self.local_players.clone() {
            let cmds: Vec<Command> = self
                .scheduled
                .get(&send_tick)
                .map(|v| v.iter().filter(|c| c.player == p).cloned().collect())
                .unwrap_or_default();
            self.transport.send(p, &TickCommands { tick: send_tick, commands: cmds });
        }
        if !self.ready(tick) {
            return false;
        }
        let mut tc = TickCommands { tick, commands: self.scheduled.remove(&tick).unwrap_or_default() };
        self.arrived.remove(&tick);
        tc.canonicalize();
        self.world.step(&tc);
        if self.collect_events {
            self.event_log.extend(self.world.events.drain(..));
        }
        if self.record_replay && !tc.commands.is_empty() {
            self.replay.push(tc);
        }
        if self.checksum_interval > 0 && tick % self.checksum_interval == 0 {
            let h = self.world.checksum();
            self.checksums.push((tick, h));
            self.transport.report_checksum(tick, h);
        }
        true
    }

    /// Advance by wall-clock `dt` seconds with a fixed timestep. Returns ticks run.
    pub fn advance(&mut self, dt: f64) -> u32 {
        if self.paused || self.world.game_over {
            return 0;
        }
        let step = 1.0 / TICKS_PER_SEC as f64;
        self.accumulator += dt.min(0.25) * self.speed;
        let mut n = 0;
        while self.accumulator >= step && n < 8 {
            if !self.step_once() {
                break;
            }
            self.accumulator -= step;
            n += 1;
        }
        if n == 8 {
            self.accumulator = self.accumulator.min(step);
        }
        n
    }

    /// Interpolation factor between the previous and current tick for rendering.
    pub fn alpha(&self) -> f32 {
        let step = 1.0 / TICKS_PER_SEC as f64;
        (self.accumulator / step).clamp(0.0, 1.0) as f32
    }

    pub fn controller_debug(&self) -> Vec<String> {
        self.controllers.iter().map(|c| format!("P{}: {}", c.player(), c.debug())).collect()
    }

    pub fn make_replay(&self) -> Replay {
        Replay {
            seed: self.world.config.seed,
            players: self.world.players.len(),
            data_hash: ee_sim::data().hash,
            ticks: self.replay.clone(),
            config: Some(self.start_config.clone()),
            ais: self.ai_setup.clone(),
            checksums: self.checksums.iter().copied().filter(|(t, _)| t % 1200 == 0).collect(),
            end_tick: self.world.tick,
            input_delay: self.input_delay,
            sim_version: ee_sim::SIM_VERSION.to_string(),
        }
    }
}

/// Re-run a recorded match and return the final checksum (determinism check).
pub fn run_replay(config: MatchConfig, replay: &Replay, ticks: u32) -> u64 {
    let mut world = World::new(config);
    let mut idx = 0;
    for t in 0..ticks {
        let tc = if idx < replay.ticks.len() && replay.ticks[idx].tick == t {
            idx += 1;
            replay.ticks[idx - 1].clone()
        } else {
            TickCommands { tick: t, commands: vec![] }
        };
        world.step(&tc);
    }
    world.checksum()
}
