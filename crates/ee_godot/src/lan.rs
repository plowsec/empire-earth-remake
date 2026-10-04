//! LAN lobby for the menu: discovery, host/join, seats, settings, chat. When the match
//! starts, everything the game scene needs is parked in `PENDING` and picked up by
//! `GameView::start_lan`.
use ee_net::lan::{Discovery, LanClient, LanHost, LobbySettings, SlotKind};
use ee_net::AiSetup;
use ee_sim::world::MatchConfig;
use godot::prelude::*;
use std::sync::Mutex;

pub struct PendingMatch {
    pub config: MatchConfig,
    pub ais: Vec<AiSetup>,
    pub me: u8,
    pub host: bool,
    pub transport: ee_net::lan::LanTransport,
}

pub static PENDING: Mutex<Option<PendingMatch>> = Mutex::new(None);

#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct Lan {
    host: Option<LanHost>,
    client: Option<LanClient>,
    discovery: Option<Discovery>,
    error: String,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for Lan {
    fn init(base: Base<RefCounted>) -> Self {
        Lan { host: None, client: None, discovery: None, error: String::new(), base }
    }
}

fn s(v: &str) -> GString {
    GString::from(v)
}

#[godot_api]
impl Lan {
    /// Games announced on the local network: [{name, addr, players}]
    #[func]
    fn games(&mut self) -> VarArray {
        if self.discovery.is_none() {
            match Discovery::start() {
                Ok(d) => self.discovery = Some(d),
                Err(e) => self.error = format!("discovery unavailable: {e}"),
            }
        }
        let mut out = VarArray::new();
        if let Some(d) = &self.discovery {
            for g in d.games() {
                let mut x = VarDictionary::new();
                x.set("name", g.name.as_str());
                x.set("addr", g.addr.to_string().as_str());
                x.set("players", g.players as i64);
                out.push(&x.to_variant());
            }
        }
        out
    }

    /// Open a game on this machine. Returns "" or an error.
    #[func]
    fn host(&mut self, name: GString) -> GString {
        self.leave();
        match LanHost::open(&name.to_string(), ee_net::lan::GAME_PORT) {
            Ok(h) => {
                self.host = Some(h);
                GString::new()
            }
            Err(e) => s(&format!("could not open port {}: {e}", ee_net::lan::GAME_PORT)),
        }
    }

    /// Join a game at "ip" or "ip:port". Returns "" or an error.
    #[func]
    fn join(&mut self, addr: GString, name: GString) -> GString {
        self.leave();
        match LanClient::connect(&addr.to_string(), &name.to_string()) {
            Ok(c) => {
                self.client = Some(c);
                GString::new()
            }
            Err(e) => s(&format!("could not connect: {e}")),
        }
    }

    #[func]
    fn leave(&mut self) {
        self.host = None;
        self.client = None;
    }

    /// Process network traffic and describe the lobby:
    /// {role, slots: [{name, human, difficulty, me}], settings, chat, error, started}
    #[func]
    fn pump(&mut self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let mut started = false;
        let (role, lobby, chat, me) = if let Some(h) = self.host.as_mut() {
            h.pump();
            ("host", Some(h.lobby()), h.chat.clone(), Some(0u8))
        } else if let Some(c) = self.client.as_mut() {
            started = c.pump();
            if let Some(r) = &c.rejected {
                self.error = format!("rejected: {r}");
            }
            if c.host_gone && !started {
                self.error = "the host closed the game".into();
            }
            ("client", c.lobby.clone(), c.chat.clone(), c.slot)
        } else {
            ("none", None, vec![], None)
        };
        d.set("role", role);
        if let Some(l) = lobby {
            let mut slots = VarArray::new();
            for (i, sl) in l.slots.iter().enumerate() {
                let mut x = VarDictionary::new();
                x.set("name", sl.name.as_str());
                x.set("human", sl.kind == SlotKind::Human);
                x.set("difficulty", match sl.kind { SlotKind::Ai { difficulty } => difficulty as i64, _ => -1 });
                x.set("me", me == Some(i as u8));
                x.set("team", sl.team as i64);
                slots.push(&x.to_variant());
            }
            d.set("slots", &slots);
            let st = &l.settings;
            let mut sd = VarDictionary::new();
            sd.set("map_size", st.map_size as i64);
            sd.set("resources", st.resources as i64);
            sd.set("start_res", st.start_res as i64);
            sd.set("pop_limit", st.pop_limit as i64);
            sd.set("reveal", st.reveal);
            d.set("settings", &sd);
            d.set("host_name", l.host_name.as_str());
        }
        let mut ch = PackedStringArray::new();
        for line in chat.iter().rev().take(8).rev() {
            ch.push(line.as_str());
        }
        d.set("chat", &ch);
        d.set("error", self.error.as_str());
        if started {
            if let Some(c) = self.client.take() {
                if let Some((config, ais, me, transport)) = c.take_match() {
                    *PENDING.lock().unwrap() = Some(PendingMatch { config, ais, me, host: false, transport });
                }
            }
        }
        d.set("started", started);
        d
    }

    #[func]
    fn add_ai(&mut self, difficulty: i64) {
        if let Some(h) = self.host.as_mut() {
            h.add_ai(difficulty as i32);
        }
    }

    #[func]
    fn remove_ai(&mut self, slot: i64) {
        if let Some(h) = self.host.as_mut() {
            h.remove_ai(slot as usize);
        }
    }

    #[func]
    fn set_team(&mut self, slot: i64, team: i64) {
        if let Some(h) = self.host.as_mut() {
            h.set_team(slot as usize, team as u8);
        }
    }

    #[func]
    fn set_ai_difficulty(&mut self, slot: i64, difficulty: i64) {
        if let Some(h) = self.host.as_mut() {
            h.set_ai_difficulty(slot as usize, difficulty as i32);
        }
    }

    /// Host only: map_size, resources, start_res, pop_limit, reveal.
    #[func]
    fn set_setting(&mut self, key: GString, value: i64) {
        let Some(h) = self.host.as_mut() else { return };
        let mut st: LobbySettings = h.lobby().settings;
        match key.to_string().as_str() {
            "map_size" => st.map_size = value as u8,
            "resources" => st.resources = value as u32,
            "start_res" => st.start_res = value as i32,
            "pop_limit" => st.pop_limit = value as i32,
            "reveal" => st.reveal = value != 0,
            _ => return,
        }
        h.set_settings(st);
    }

    #[func]
    fn say(&mut self, text: GString) {
        if let Some(h) = self.host.as_mut() {
            h.say(&text.to_string());
        } else if let Some(c) = self.client.as_mut() {
            c.say(&text.to_string());
        }
    }

    /// Host: start the match for everyone.
    #[func]
    fn launch(&mut self, seed: i64) -> bool {
        let Some(h) = self.host.take() else { return false };
        let (config, ais, me, transport) = h.launch(seed as u64);
        *PENDING.lock().unwrap() = Some(PendingMatch { config, ais, me, host: true, transport });
        true
    }
}
