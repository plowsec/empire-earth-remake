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
}

impl Session {
    pub fn new(config: MatchConfig, local_players: Vec<u8>, transport: Box<dyn Transport>) -> Session {
        Session {
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

    pub fn make_replay(&self) -> Replay {
        Replay {
            seed: self.world.config.seed,
            players: self.world.players.len(),
            data_hash: ee_sim::data().hash,
            ticks: self.replay.clone(),
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
