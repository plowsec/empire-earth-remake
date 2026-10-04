//! LAN multiplayer: lobby + lockstep transport over TCP, host-relayed (star).
//!
//! Every machine runs the whole simulation; only command bundles travel. The host
//! accepts connections, owns the lobby (slots, settings), runs the computer players and
//! relays each client's per-tick bundle to the other clients. Games are announced on the
//! local network by UDP broadcast. A client that drops is resigned on every machine at
//! the same agreed tick, so the game goes on deterministically.
use crate::{AiSetup, Transport};
use ee_sim::command::{Command, CommandKind, TickCommands};
use ee_sim::world::{MatchConfig, PlayerConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const GAME_PORT: u16 = 47777;
pub const DISCOVERY_PORT: u16 = 47778;
const MAGIC: &str = "EE-ATOMIC-1";
pub const MAX_PLAYERS: usize = 4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SlotKind {
    Human,
    Ai { difficulty: i32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Slot {
    pub name: String,
    pub kind: SlotKind,
    /// alliance at start: same team = allies
    #[serde(default)]
    pub team: u8,
    /// which connection holds this human slot (0 = the host itself)
    pub peer: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LobbySettings {
    pub map_size: u8,
    pub resources: u32,
    pub start_res: i32,
    pub pop_limit: i32,
    pub reveal: bool,
}

impl Default for LobbySettings {
    fn default() -> Self {
        // the house rules: deathmatch, 3000 pop, high resources, large islands, revealed
        LobbySettings { map_size: 2, resources: 150, start_res: 10000, pop_limit: 3000, reveal: true }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LobbyState {
    pub host_name: String,
    pub slots: Vec<Slot>,
    pub settings: LobbySettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Msg {
    Hello { name: String, data_hash: u64, sim_version: String },
    Welcome { slot: u8 },
    Reject { reason: String },
    Lobby(LobbyState),
    Start { config: MatchConfig, ais: Vec<AiSetup>, input_delay: u32, you: u8 },
    Tick { player: u8, tc: TickCommands },
    Checksum { tick: u32, hash: u64 },
    /// `player` dropped: absent after tick `after`, resigned at `resign_at`
    Left { player: u8, after: u32, resign_at: u32 },
    Desync { tick: u32 },
    Chat { from: String, text: String },
}

// ------------------------------------------------------------------ framing

fn write_msg(s: &mut TcpStream, m: &Msg) -> std::io::Result<()> {
    let body = ron::to_string(m).map_err(|e| std::io::Error::other(e.to_string()))?;
    let b = body.as_bytes();
    s.write_all(&(b.len() as u32).to_le_bytes())?;
    s.write_all(b)?;
    s.flush()
}

fn read_msg(s: &mut TcpStream) -> std::io::Result<Msg> {
    let mut len = [0u8; 4];
    s.read_exact(&mut len)?;
    let n = u32::from_le_bytes(len) as usize;
    if n > 16 << 20 {
        return Err(std::io::Error::other("frame too large"));
    }
    let mut buf = vec![0u8; n];
    s.read_exact(&mut buf)?;
    let text = String::from_utf8(buf).map_err(|e| std::io::Error::other(e.to_string()))?;
    ron::from_str(&text).map_err(|e| std::io::Error::other(e.to_string()))
}

enum Event {
    Msg(u32, Msg),
    Closed(u32),
}

struct Peer {
    id: u32,
    out: Arc<Mutex<TcpStream>>,
    slot: Option<u8>,
    alive: bool,
}

fn spawn_reader(id: u32, mut s: TcpStream, tx: Sender<Event>) {
    std::thread::spawn(move || loop {
        match read_msg(&mut s) {
            Ok(m) => {
                if tx.send(Event::Msg(id, m)).is_err() {
                    return;
                }
            }
            Err(_) => {
                let _ = tx.send(Event::Closed(id));
                return;
            }
        }
    });
}

fn send_to(p: &Peer, m: &Msg) -> bool {
    p.alive && p.out.lock().map(|mut s| write_msg(&mut s, m).is_ok()).unwrap_or(false)
}

// ------------------------------------------------------------------ discovery

/// A game announced on the LAN.
#[derive(Clone, Debug)]
pub struct Announced {
    pub name: String,
    pub addr: SocketAddr,
    pub players: usize,
    pub seen: Instant,
}

/// Listens for host announcements.
pub struct Discovery {
    found: Arc<Mutex<BTreeMap<String, Announced>>>,
}

impl Discovery {
    pub fn start() -> std::io::Result<Discovery> {
        let sock = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT))?;
        sock.set_read_timeout(Some(Duration::from_millis(500)))?;
        let found: Arc<Mutex<BTreeMap<String, Announced>>> = Default::default();
        let f = found.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            loop {
                if Arc::strong_count(&f) == 1 {
                    return; // Discovery dropped
                }
                if let Ok((n, from)) = sock.recv_from(&mut buf) {
                    let s = String::from_utf8_lossy(&buf[..n]).to_string();
                    let parts: Vec<&str> = s.split('|').collect();
                    if parts.len() == 4 && parts[0] == MAGIC {
                        if let (Ok(port), Ok(players)) = (parts[2].parse::<u16>(), parts[3].parse::<usize>()) {
                            let addr = SocketAddr::new(from.ip(), port);
                            f.lock().unwrap().insert(addr.to_string(), Announced { name: parts[1].to_string(), addr, players, seen: Instant::now() });
                        }
                    }
                }
            }
        });
        Ok(Discovery { found })
    }

    /// Games heard from in the last 4 seconds.
    pub fn games(&self) -> Vec<Announced> {
        let mut m = self.found.lock().unwrap();
        m.retain(|_, a| a.seen.elapsed() < Duration::from_secs(4));
        m.values().cloned().collect()
    }
}

// ------------------------------------------------------------------ host

struct HostShared {
    peers: Vec<Peer>,
    lobby: LobbyState,
    started: bool,
    next_peer: u32,
}

/// The hosting side of a LAN game, from lobby to match.
pub struct LanHost {
    shared: Arc<Mutex<HostShared>>,
    events: Arc<Mutex<Receiver<Event>>>,
    port: u16,
    pub chat: Vec<String>,
}

impl LanHost {
    pub fn open(name: &str, port: u16) -> std::io::Result<LanHost> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        let port = listener.local_addr()?.port();
        let (tx, rx) = channel();
        let shared = Arc::new(Mutex::new(HostShared {
            peers: Vec::new(),
            lobby: LobbyState {
                host_name: name.to_string(),
                slots: vec![
                    Slot { name: name.to_string(), kind: SlotKind::Human, peer: 0, team: 0 },
                    Slot { name: "AI".into(), kind: SlotKind::Ai { difficulty: 3 }, peer: 0, team: 1 },
                ],
                settings: LobbySettings::default(),
            },
            started: false,
            next_peer: 1,
        }));
        // accept connections until the match starts
        let sh = shared.clone();
        listener.set_nonblocking(true)?;
        std::thread::spawn(move || {
            loop {
                // stop listening once the match starts or the host is closed
                if Arc::strong_count(&sh) == 1 || sh.lock().unwrap().started {
                    return;
                }
                let s = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                };
                let _ = s.set_nonblocking(false);
                let _ = s.set_nodelay(true);
                let mut g = sh.lock().unwrap();
                if g.started {
                    return;
                }
                let id = g.next_peer;
                g.next_peer += 1;
                let Ok(rs) = s.try_clone() else { continue };
                g.peers.push(Peer { id, out: Arc::new(Mutex::new(s)), slot: None, alive: true });
                drop(g);
                spawn_reader(id, rs, tx.clone());
            }
        });
        // announce on the LAN once a second while in the lobby
        let sh = shared.clone();
        std::thread::spawn(move || {
            let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)) else { return };
            let _ = sock.set_broadcast(true);
            loop {
                let (msg, started) = {
                    let g = sh.lock().unwrap();
                    let humans = g.lobby.slots.len();
                    (format!("{MAGIC}|{}|{port}|{humans}", g.lobby.host_name), g.started)
                };
                if started || Arc::strong_count(&sh) == 1 {
                    return;
                }
                let _ = sock.send_to(msg.as_bytes(), ("255.255.255.255", DISCOVERY_PORT));
                let _ = sock.send_to(msg.as_bytes(), ("127.0.0.1", DISCOVERY_PORT));
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        Ok(LanHost { shared, events: Arc::new(Mutex::new(rx)), port, chat: Vec::new() })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn lobby(&self) -> LobbyState {
        self.shared.lock().unwrap().lobby.clone()
    }

    fn broadcast_lobby(g: &mut HostShared) {
        let m = Msg::Lobby(g.lobby.clone());
        for p in &g.peers {
            if p.slot.is_some() {
                send_to(p, &m);
            }
        }
    }

    /// Handle joins / leaves / chat while in the lobby. Call every frame.
    pub fn pump(&mut self) {
        let rx = self.events.lock().unwrap();
        while let Ok(ev) = rx.try_recv() {
            let mut g = self.shared.lock().unwrap();
            match ev {
                Event::Msg(id, Msg::Hello { name, data_hash, sim_version }) => {
                    let reject = if data_hash != ee_sim::data().hash || sim_version != ee_sim::SIM_VERSION {
                        Some("different game version".to_string())
                    } else if g.lobby.slots.len() >= MAX_PLAYERS && !g.lobby.slots.iter().any(|s| matches!(s.kind, SlotKind::Ai { .. })) {
                        Some("game is full".to_string())
                    } else {
                        None
                    };
                    let Some(pi) = g.peers.iter().position(|p| p.id == id) else { continue };
                    if let Some(reason) = reject {
                        send_to(&g.peers[pi], &Msg::Reject { reason });
                        g.peers[pi].alive = false;
                        continue;
                    }
                    // take a free seat: replace the last AI if the table is full
                    let slot = if g.lobby.slots.len() < MAX_PLAYERS {
                        let team = g.lobby.slots.len() as u8;
                        g.lobby.slots.push(Slot { name: name.clone(), kind: SlotKind::Human, peer: id, team });
                        g.lobby.slots.len() - 1
                    } else {
                        let k = g.lobby.slots.iter().rposition(|s| matches!(s.kind, SlotKind::Ai { .. })).unwrap();
                        g.lobby.slots[k] = Slot { name: name.clone(), kind: SlotKind::Human, peer: id, team: k as u8 };
                        k
                    };
                    g.peers[pi].slot = Some(slot as u8);
                    send_to(&g.peers[pi], &Msg::Welcome { slot: slot as u8 });
                    Self::broadcast_lobby(&mut g);
                    self.chat.push(format!("{name} joined"));
                }
                Event::Msg(_, Msg::Chat { from, text }) => {
                    self.chat.push(format!("{from}: {text}"));
                    let m = Msg::Chat { from, text };
                    for p in &g.peers {
                        send_to(p, &m);
                    }
                }
                Event::Closed(id) => {
                    if let Some(pi) = g.peers.iter().position(|p| p.id == id) {
                        g.peers[pi].alive = false;
                        if let Some(k) = g.peers[pi].slot {
                            let name = g.lobby.slots[k as usize].name.clone();
                            g.lobby.slots.remove(k as usize);
                            // seats after it shift down
                            for p in g.peers.iter_mut() {
                                if let Some(s) = p.slot.as_mut() {
                                    if *s > k {
                                        *s -= 1;
                                    }
                                }
                            }
                            self.chat.push(format!("{name} left"));
                        }
                        g.peers.remove(pi);
                        Self::broadcast_lobby(&mut g);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn add_ai(&mut self, difficulty: i32) {
        let mut g = self.shared.lock().unwrap();
        if g.lobby.slots.len() < MAX_PLAYERS {
            let n = g.lobby.slots.iter().filter(|s| matches!(s.kind, SlotKind::Ai { .. })).count() + 1;
            let team = g.lobby.slots.len() as u8;
            g.lobby.slots.push(Slot { name: format!("AI {n}"), kind: SlotKind::Ai { difficulty }, peer: 0, team });
            Self::broadcast_lobby(&mut g);
        }
    }

    /// Remove an AI seat (humans leave on their own).
    pub fn remove_ai(&mut self, slot: usize) {
        let mut g = self.shared.lock().unwrap();
        if slot < g.lobby.slots.len() && matches!(g.lobby.slots[slot].kind, SlotKind::Ai { .. }) && g.lobby.slots.len() > 2 {
            g.lobby.slots.remove(slot);
            for p in g.peers.iter_mut() {
                if let Some(s) = p.slot.as_mut() {
                    if *s as usize > slot {
                        *s -= 1;
                    }
                }
            }
            Self::broadcast_lobby(&mut g);
        }
    }

    pub fn set_ai_difficulty(&mut self, slot: usize, difficulty: i32) {
        let mut g = self.shared.lock().unwrap();
        if let Some(s) = g.lobby.slots.get_mut(slot) {
            if let SlotKind::Ai { .. } = s.kind {
                s.kind = SlotKind::Ai { difficulty };
            }
        }
        Self::broadcast_lobby(&mut g);
    }

    pub fn set_team(&mut self, slot: usize, team: u8) {
        let mut g = self.shared.lock().unwrap();
        if let Some(s) = g.lobby.slots.get_mut(slot) {
            s.team = team;
        }
        Self::broadcast_lobby(&mut g);
    }

    pub fn set_settings(&mut self, settings: LobbySettings) {
        let mut g = self.shared.lock().unwrap();
        g.lobby.settings = settings;
        Self::broadcast_lobby(&mut g);
    }

    pub fn say(&mut self, text: &str) {
        let g = self.shared.lock().unwrap();
        let m = Msg::Chat { from: g.lobby.host_name.clone(), text: text.to_string() };
        for p in &g.peers {
            send_to(p, &m);
        }
        self.chat.push(format!("{}: {text}", g.lobby.host_name));
    }

    /// Lock the lobby and send everyone the match. Returns what the host needs to run it.
    pub fn launch(self, seed: u64) -> (MatchConfig, Vec<AiSetup>, u8, LanTransport) {
        let input_delay = 3;
        let (config, ais, slots) = {
            let mut g = self.shared.lock().unwrap();
            g.started = true;
            let lobby = g.lobby.clone();
            let mut cfg = MatchConfig::skirmish(seed, lobby.slots.len());
            let s = &lobby.settings;
            cfg.map_size = s.map_size;
            cfg.resources = s.resources;
            cfg.pop_limit = s.pop_limit;
            cfg.reveal = s.reveal;
            let sr = s.start_res;
            cfg.start_res = [sr, sr, sr * 6 / 10, sr * 2 / 3, sr * 2 / 3];
            cfg.players = lobby
                .slots
                .iter()
                .enumerate()
                .map(|(i, sl)| PlayerConfig { name: sl.name.clone(), team: sl.team, color: i as u8, is_ai: !matches!(sl.kind, SlotKind::Human) })
                .collect();
            let ais: Vec<AiSetup> = lobby
                .slots
                .iter()
                .enumerate()
                .filter_map(|(i, sl)| match sl.kind {
                    SlotKind::Ai { difficulty } => Some(AiSetup { player: i as u8, difficulty, seed: seed ^ (i as u64 * 7919) }),
                    _ => None,
                })
                .collect();
            for p in &g.peers {
                if let Some(k) = p.slot {
                    send_to(p, &Msg::Start { config: cfg.clone(), ais: ais.clone(), input_delay, you: k });
                }
            }
            (cfg, ais, lobby.slots)
        };
        let humans: Vec<u8> = slots.iter().enumerate().filter(|(_, s)| s.kind == SlotKind::Human).map(|(i, _)| i as u8).collect();
        let transport = LanTransport {
            me: 0,
            host: true,
            remote: humans.into_iter().filter(|&p| p != 0).collect(),
            shared: Some(self.shared.clone()),
            client_out: None,
            events: self.events.clone(),
            last_from: BTreeMap::new(),
            pending: Vec::new(),
            my_hashes: BTreeMap::new(),
            peer_hashes: Vec::new(),
            desync: None,
            disconnected: Vec::new(),
            input_delay,
            host_lost: false,
        };
        (config, ais, 0, transport)
    }
}

// ------------------------------------------------------------------ client

/// The joining side of a LAN game.
pub struct LanClient {
    out: Arc<Mutex<TcpStream>>,
    events: Arc<Mutex<Receiver<Event>>>,
    pub slot: Option<u8>,
    pub lobby: Option<LobbyState>,
    pub rejected: Option<String>,
    pub host_gone: bool,
    pub chat: Vec<String>,
    name: String,
    start: Option<(MatchConfig, Vec<AiSetup>, u32, u8)>,
}

impl LanClient {
    pub fn connect(addr: &str, name: &str) -> std::io::Result<LanClient> {
        let addr = if addr.contains(':') { addr.to_string() } else { format!("{addr}:{GAME_PORT}") };
        let s = TcpStream::connect_timeout(&addr.parse().map_err(|_| std::io::Error::other("bad address"))?, Duration::from_secs(4))?;
        s.set_nodelay(true)?;
        let (tx, rx) = channel();
        spawn_reader(0, s.try_clone()?, tx);
        let out = Arc::new(Mutex::new(s));
        write_msg(&mut out.lock().unwrap(), &Msg::Hello { name: name.to_string(), data_hash: ee_sim::data().hash, sim_version: ee_sim::SIM_VERSION.to_string() })?;
        Ok(LanClient { out, events: Arc::new(Mutex::new(rx)), slot: None, lobby: None, rejected: None, host_gone: false, chat: Vec::new(), name: name.to_string(), start: None })
    }

    /// Process lobby traffic. Returns true once the host has started the match.
    pub fn pump(&mut self) -> bool {
        let rx = self.events.lock().unwrap();
        while let Ok(ev) = rx.try_recv() {
            match ev {
                Event::Msg(_, Msg::Welcome { slot }) => self.slot = Some(slot),
                Event::Msg(_, Msg::Reject { reason }) => self.rejected = Some(reason),
                Event::Msg(_, Msg::Lobby(l)) => self.lobby = Some(l),
                Event::Msg(_, Msg::Chat { from, text }) => self.chat.push(format!("{from}: {text}")),
                Event::Msg(_, Msg::Start { config, ais, input_delay, you }) => {
                    self.start = Some((config, ais, input_delay, you));
                    return true; // later traffic belongs to the match
                }
                Event::Closed(_) => self.host_gone = true,
                _ => {}
            }
        }
        self.start.is_some()
    }

    pub fn say(&mut self, text: &str) {
        let _ = write_msg(&mut self.out.lock().unwrap(), &Msg::Chat { from: self.name.clone(), text: text.to_string() });
    }

    /// The match the host started: (config, AIs to *not* run here, me, transport).
    pub fn take_match(self) -> Option<(MatchConfig, Vec<AiSetup>, u8, LanTransport)> {
        let (config, ais, input_delay, me) = self.start?;
        let n = config.players.len() as u8;
        let transport = LanTransport {
            me,
            host: false,
            remote: (0..n).filter(|&p| p != me).collect(),
            shared: None,
            client_out: Some(self.out),
            events: self.events,
            last_from: BTreeMap::new(),
            pending: Vec::new(),
            my_hashes: BTreeMap::new(),
            peer_hashes: Vec::new(),
            desync: None,
            disconnected: Vec::new(),
            input_delay,
            host_lost: false,
        };
        Some((config, ais, me, transport))
    }
}

// ------------------------------------------------------------------ in-game transport

pub struct LanTransport {
    pub me: u8,
    pub host: bool,
    /// players whose bundles we wait for
    remote: Vec<u8>,
    shared: Option<Arc<Mutex<HostShared>>>,
    client_out: Option<Arc<Mutex<TcpStream>>>,
    events: Arc<Mutex<Receiver<Event>>>,
    /// last tick received from each player
    last_from: BTreeMap<u8, u32>,
    pending: Vec<(u8, TickCommands)>,
    my_hashes: BTreeMap<u32, u64>,
    peer_hashes: Vec<(u32, u64)>,
    /// first tick at which checksums disagreed
    pub desync: Option<u32>,
    /// players that dropped out
    pub disconnected: Vec<u8>,
    pub input_delay: u32,
    /// (clients) the host connection is gone
    pub host_lost: bool,
}

impl LanTransport {
    fn handle(&mut self, from_peer: u32, m: Msg) {
        match m {
            Msg::Tick { player, tc } => {
                self.last_from.insert(player, tc.tick);
                if self.host {
                    // relay to the other clients
                    if let Some(sh) = &self.shared {
                        let g = sh.lock().unwrap();
                        let fwd = Msg::Tick { player, tc: tc.clone() };
                        for p in &g.peers {
                            if p.id != from_peer && p.slot.is_some() {
                                send_to(p, &fwd);
                            }
                        }
                    }
                }
                self.pending.push((player, tc));
            }
            Msg::Checksum { tick, hash } => {
                if self.host {
                    self.peer_hashes.push((tick, hash));
                    self.check_hashes();
                }
            }
            Msg::Desync { tick } => {
                self.desync.get_or_insert(tick);
            }
            Msg::Left { player, after, resign_at } => self.player_left(player, after, resign_at),
            _ => {}
        }
    }

    fn check_hashes(&mut self) {
        let mut bad = None;
        self.peer_hashes.retain(|(t, h)| match self.my_hashes.get(t) {
            Some(mine) => {
                if mine != h {
                    bad.get_or_insert(*t);
                }
                false
            }
            None => true,
        });
        if let Some(t) = bad {
            if self.desync.is_none() {
                self.desync = Some(t);
                if let Some(sh) = &self.shared {
                    let g = sh.lock().unwrap();
                    for p in &g.peers {
                        send_to(p, &Msg::Desync { tick: t });
                    }
                }
            }
        }
    }

    /// Stop waiting for `player` after tick `after`; they resign at `resign_at`.
    fn player_left(&mut self, player: u8, after: u32, resign_at: u32) {
        if self.disconnected.contains(&player) {
            return;
        }
        self.disconnected.push(player);
        let _ = after;
        self.remote.retain(|&p| p != player);
        // everyone applies the same resign command at the same tick
        self.pending.push((player, TickCommands { tick: resign_at, commands: vec![Command { player, kind: CommandKind::Resign }] }));
    }
}

impl Transport for LanTransport {
    fn send(&mut self, player: u8, tc: &TickCommands) {
        let m = Msg::Tick { player, tc: tc.clone() };
        if self.host {
            if let Some(sh) = &self.shared {
                let g = sh.lock().unwrap();
                for p in &g.peers {
                    if p.slot.is_some() {
                        send_to(p, &m);
                    }
                }
            }
        } else if let Some(out) = &self.client_out {
            if out.lock().map(|mut s| write_msg(&mut s, &m).is_err()).unwrap_or(true) {
                self.host_lost = true;
            }
        }
    }

    fn poll(&mut self) -> Vec<(u8, TickCommands)> {
        let evs: Vec<Event> = {
            let rx = self.events.lock().unwrap();
            let mut v = Vec::new();
            while let Ok(e) = rx.try_recv() {
                v.push(e);
            }
            v
        };
        for e in evs {
            match e {
                Event::Msg(from, m) => self.handle(from, m),
                Event::Closed(id) => {
                    if !self.host {
                        self.host_lost = true;
                        continue;
                    }
                    // a client dropped: everyone stops waiting for it and resigns it
                    let slot = self.shared.as_ref().and_then(|sh| {
                        let mut g = sh.lock().unwrap();
                        let s = g.peers.iter().find(|p| p.id == id).and_then(|p| p.slot);
                        if let Some(p) = g.peers.iter_mut().find(|p| p.id == id) {
                            p.alive = false;
                        }
                        s
                    });
                    if let Some(player) = slot {
                        let after = self.last_from.get(&player).copied().unwrap_or(0);
                        let resign_at = after + self.input_delay * 2 + 2;
                        if let Some(sh) = &self.shared {
                            let g = sh.lock().unwrap();
                            for p in &g.peers {
                                send_to(p, &Msg::Left { player, after, resign_at });
                            }
                        }
                        self.player_left(player, after, resign_at);
                    }
                }
            }
        }
        std::mem::take(&mut self.pending)
    }

    fn remote_players(&self) -> Vec<u8> {
        self.remote.clone()
    }

    fn status(&self) -> crate::NetStatus {
        crate::NetStatus { online: true, waiting_for: vec![], desync_tick: self.desync, dropped: self.disconnected.clone(), host_lost: self.host_lost }
    }

    fn report_checksum(&mut self, tick: u32, hash: u64) {
        if self.host {
            self.my_hashes.insert(tick, hash);
            // keep a bounded window
            while self.my_hashes.len() > 200 {
                let k = *self.my_hashes.keys().next().unwrap();
                self.my_hashes.remove(&k);
            }
            self.check_hashes();
        } else if let Some(out) = &self.client_out {
            let _ = write_msg(&mut out.lock().unwrap(), &Msg::Checksum { tick, hash });
        }
    }
}
