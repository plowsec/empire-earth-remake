//! Static game data, loaded from `data/*.ron` at compile time. Units in the data
//! files are human-friendly integers (centi-tiles, deciseconds); they are converted
//! once here into sim units (fixed-point tiles, ticks).
use crate::fixed::Fx;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const TICKS_PER_SEC: i32 = 20;
pub const NUM_RES: usize = 5;
pub const RES_NAMES: [&str; NUM_RES] = ["food", "wood", "stone", "gold", "iron"];

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Res {
    Food = 0,
    Wood = 1,
    Stone = 2,
    Gold = 3,
    Iron = 4,
}
impl Res {
    pub const ALL: [Res; NUM_RES] = [Res::Food, Res::Wood, Res::Stone, Res::Gold, Res::Iron];
    pub fn idx(self) -> usize {
        self as usize
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cost {
    pub food: i32,
    pub wood: i32,
    pub stone: i32,
    pub gold: i32,
    pub iron: i32,
}
impl Cost {
    pub fn arr(&self) -> [i32; NUM_RES] {
        [self.food, self.wood, self.stone, self.gold, self.iron]
    }
    pub fn total(&self) -> i32 {
        self.arr().iter().sum()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Class {
    Citizen,
    Infantry,
    Vehicle,
    Aircraft,
    Ship,
    Building,
    Resource,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArmorClass {
    Infantry = 0,
    Light = 1,
    Heavy = 2,
    Building = 3,
    Air = 4,
    Ship = 5,
    Sub = 6,
    None = 7,
}
pub const NUM_ARMOR: usize = 8;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DamageType {
    Gun = 0,
    Cannon = 1,
    Explosive = 2,
    Flak = 3,
    Flame = 4,
    Torpedo = 5,
    NavalGun = 6,
    Bomb = 7,
    AirGun = 8,
    Missile = 9,
}
pub const NUM_DMG: usize = 10;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Layer {
    Land,
    Water,
    Air,
    None,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Projectile {
    /// Hitscan: damage applied immediately (tracer is a client VFX).
    Instant,
    /// Shell/mortar/bomb: flies to the aimed point, can miss moving targets, may splash.
    Ballistic { speed: i32 },
    /// Missile/torpedo: follows the target.
    Homing { speed: i32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WeaponData {
    pub dmg_type: DamageType,
    pub damage: i32,
    /// centi-tiles
    pub range: i32,
    pub min_range: i32,
    /// deciseconds between shots
    pub reload: i32,
    pub projectile: Projectile,
    /// centi-tiles, 0 = single target
    pub splash: i32,
    pub vs_ground: bool,
    pub vs_air: bool,
    pub vs_water: bool,
    /// bombers: weapon only fires when directly overhead, then needs rearming
    pub ammo: i32,
    /// shots per salvo (machine guns fire bursts: VFX + damage split)
    pub burst: i32,
}
impl Default for WeaponData {
    fn default() -> Self {
        WeaponData {
            dmg_type: DamageType::Gun,
            damage: 10,
            range: 100,
            min_range: 0,
            reload: 20,
            projectile: Projectile::Instant,
            splash: 0,
            vs_ground: true,
            vs_air: false,
            vs_water: true,
            ammo: 0,
            burst: 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UnitData {
    pub key: String,
    pub name: String,
    pub class: Class,
    pub armor_class: ArmorClass,
    pub hp: i32,
    pub armor: i32,
    /// centi-tiles per second
    pub speed: i32,
    /// centi-tiles
    pub radius: i32,
    pub sight: i32,
    pub cost: Cost,
    /// deciseconds
    pub build_time: i32,
    pub pop: i32,
    pub weapons: Vec<WeaponData>,
    /// buildings: unit keys trainable here
    pub trains: Vec<String>,
    /// citizens: building keys constructible
    pub builds: Vec<String>,
    /// building footprint in tiles
    pub size: (i32, i32),
    pub provides_pop: i32,
    pub dropsite: Vec<Res>,
    /// citizens: centi-units gathered per second per resource
    pub gather: Option<Cost>,
    pub carry: i32,
    /// resource nodes
    pub resource: Option<Res>,
    pub amount: i32,
    /// transports
    pub cargo: i32,
    /// space taken in a transport
    pub cargo_size: i32,
    /// aircraft: seconds of fuel; 0 = unlimited
    pub fuel: i32,
    /// heals nearby friendly infantry, hp per second
    pub heal: i32,
    pub heal_range: i32,
    /// buildings that must sit on the coast
    pub coastal: bool,
    /// farms: walkable building that is also an infinite food node
    pub walkable: bool,
    /// can only be built adjacent to this building key (farms next to granaries)
    pub near: Option<String>,
    /// aircraft that must rearm at an airport
    pub needs_airport: bool,
    /// can land aircraft
    pub airport: bool,
    /// is a hover/heli aircraft (stays put while attacking)
    pub hover: bool,
    /// player-facing role line in tooltips
    pub role: String,
    /// render model key (defaults to key)
    pub model: String,
}
impl Default for UnitData {
    fn default() -> Self {
        UnitData {
            key: String::new(),
            name: String::new(),
            class: Class::Infantry,
            armor_class: ArmorClass::Infantry,
            hp: 100,
            armor: 0,
            speed: 0,
            radius: 25,
            sight: 600,
            cost: Cost::default(),
            build_time: 100,
            pop: 1,
            weapons: vec![],
            trains: vec![],
            builds: vec![],
            size: (1, 1),
            provides_pop: 0,
            dropsite: vec![],
            gather: None,
            carry: 0,
            resource: None,
            amount: 0,
            cargo: 0,
            cargo_size: 1,
            fuel: 0,
            heal: 0,
            heal_range: 0,
            coastal: false,
            walkable: false,
            near: None,
            needs_airport: false,
            airport: false,
            hover: false,
            role: String::new(),
            model: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TechData {
    pub key: String,
    pub name: String,
    pub cost: Cost,
    pub research_time: i32,
    pub at: String,
    /// effects: (target class or unit key or "*", stat, delta percent or absolute)
    pub effects: Vec<TechEffect>,
    pub requires: Vec<String>,
    pub role: String,
}
impl Default for TechData {
    fn default() -> Self {
        TechData {
            key: String::new(),
            name: String::new(),
            cost: Cost::default(),
            research_time: 300,
            at: String::new(),
            effects: vec![],
            requires: vec![],
            role: String::new(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stat {
    /// percent bonus to weapon damage
    Attack,
    /// flat armor bonus
    Armor,
    /// percent bonus to hp
    Hp,
    /// percent bonus to speed
    Speed,
    /// percent bonus to gather rate for one resource
    Gather(Res),
    /// flat bonus to weapon range (centi-tiles)
    Range,
    /// percent bonus to sight
    Sight,
    /// percent faster build/train
    BuildSpeed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TechEffect {
    /// class name ("Infantry"), armor class name ("Heavy"), unit key, or "*"
    pub target: String,
    pub stat: Stat,
    pub value: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataFile {
    pub damage_table: BTreeMap<DamageType, [i32; NUM_ARMOR]>,
    pub units: Vec<UnitData>,
    pub techs: Vec<TechData>,
}

pub type DefId = u16;

/// Weapon in sim units.
#[derive(Clone, Debug)]
pub struct Weapon {
    pub dmg_type: DamageType,
    pub damage: i32,
    pub range: Fx,
    pub min_range: Fx,
    pub reload_ticks: i32,
    pub projectile: Projectile,
    /// projectile speed in fixed tiles per tick
    pub proj_speed: Fx,
    pub splash: Fx,
    pub vs_ground: bool,
    pub vs_air: bool,
    pub vs_water: bool,
    pub ammo: i32,
    pub burst: i32,
}

/// A unit/building/resource definition in sim units.
#[derive(Clone, Debug)]
pub struct Def {
    pub id: DefId,
    pub data: UnitData,
    pub layer: Layer,
    pub speed: Fx, // tiles per tick
    pub radius: Fx,
    pub sight_tiles: i32,
    pub build_ticks: i32,
    pub weapons: Vec<Weapon>,
    pub trains: Vec<DefId>,
    pub builds: Vec<DefId>,
    pub gather_rate: [i32; NUM_RES], // centi-units per tick ×100 (i.e. units*10000/tick)
    pub max_range: Fx,
    pub fuel_ticks: i32,
    pub near: Option<DefId>,
}
impl Def {
    pub fn key(&self) -> &str {
        &self.data.key
    }
    pub fn class(&self) -> Class {
        self.data.class
    }
    pub fn is_building(&self) -> bool {
        self.data.class == Class::Building
    }
    pub fn is_unit(&self) -> bool {
        !matches!(self.data.class, Class::Building | Class::Resource)
    }
    pub fn is_resource(&self) -> bool {
        self.data.class == Class::Resource
    }
    pub fn can_attack(&self) -> bool {
        !self.weapons.is_empty()
    }
    pub fn size(&self) -> (i32, i32) {
        self.data.size
    }
}

#[derive(Clone, Debug)]
pub struct Tech {
    pub id: u16,
    pub data: TechData,
    pub at: DefId,
    pub research_ticks: i32,
    pub requires: Vec<u16>,
}

pub struct GameData {
    pub defs: Vec<Def>,
    pub techs: Vec<Tech>,
    pub damage: [[i32; NUM_ARMOR]; NUM_DMG],
    by_key: BTreeMap<String, DefId>,
    tech_by_key: BTreeMap<String, u16>,
    /// FNV hash of the raw data text: part of the multiplayer handshake.
    pub hash: u64,
}

pub const DATA_RON: &str = include_str!("../data/units.ron");

fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl GameData {
    pub fn load() -> GameData {
        Self::from_str(DATA_RON).expect("embedded data/units.ron is invalid")
    }

    pub fn from_str(text: &str) -> Result<GameData, String> {
        let file: DataFile = ron::from_str(text).map_err(|e| e.to_string())?;
        let mut by_key = BTreeMap::new();
        for (i, u) in file.units.iter().enumerate() {
            if by_key.insert(u.key.clone(), i as DefId).is_some() {
                return Err(format!("duplicate unit key {}", u.key));
            }
        }
        let lookup = |k: &String| -> Result<DefId, String> {
            by_key.get(k).copied().ok_or_else(|| format!("unknown unit key '{k}'"))
        };
        let tps = TICKS_PER_SEC;
        let mut defs = Vec::new();
        for (i, u) in file.units.iter().enumerate() {
            let layer = match u.class {
                Class::Aircraft => Layer::Air,
                Class::Ship => Layer::Water,
                Class::Building | Class::Resource => Layer::None,
                _ => Layer::Land,
            };
            let weapons: Vec<Weapon> = u
                .weapons
                .iter()
                .map(|w| {
                    let ps = match w.projectile {
                        Projectile::Instant => 0,
                        Projectile::Ballistic { speed } | Projectile::Homing { speed } => speed,
                    };
                    Weapon {
                        dmg_type: w.dmg_type,
                        damage: w.damage,
                        range: Fx::from_ratio(w.range, 100),
                        min_range: Fx::from_ratio(w.min_range, 100),
                        reload_ticks: (w.reload * tps / 10).max(1),
                        projectile: w.projectile,
                        proj_speed: Fx::from_ratio(ps, 100 * tps),
                        splash: Fx::from_ratio(w.splash, 100),
                        vs_ground: w.vs_ground,
                        vs_air: w.vs_air,
                        vs_water: w.vs_water,
                        ammo: w.ammo,
                        burst: w.burst.max(1),
                    }
                })
                .collect();
            let max_range = weapons.iter().map(|w| w.range).max().unwrap_or(Fx::ZERO);
            let mut gather_rate = [0; NUM_RES];
            if let Some(g) = &u.gather {
                for (r, v) in g.arr().iter().enumerate() {
                    // v is centi-units per second -> units*10000 per tick
                    gather_rate[r] = v * 100 / tps;
                }
            }
            defs.push(Def {
                id: i as DefId,
                data: u.clone(),
                layer,
                speed: Fx::from_ratio(u.speed, 100 * tps),
                radius: Fx::from_ratio(u.radius, 100),
                sight_tiles: (u.sight + 50) / 100,
                build_ticks: (u.build_time * tps / 10).max(1),
                weapons,
                trains: u.trains.iter().map(&lookup).collect::<Result<_, _>>()?,
                builds: u.builds.iter().map(&lookup).collect::<Result<_, _>>()?,
                gather_rate,
                max_range,
                fuel_ticks: u.fuel * tps,
                near: match &u.near {
                    Some(k) => Some(lookup(k)?),
                    None => None,
                },
            });
            if defs[i].data.model.is_empty() {
                defs[i].data.model = u.key.clone();
            }
        }
        let mut tech_by_key = BTreeMap::new();
        for (i, t) in file.techs.iter().enumerate() {
            tech_by_key.insert(t.key.clone(), i as u16);
        }
        let mut techs = Vec::new();
        for (i, t) in file.techs.iter().enumerate() {
            techs.push(Tech {
                id: i as u16,
                data: t.clone(),
                at: lookup(&t.at)?,
                research_ticks: t.research_time * tps / 10,
                requires: t
                    .requires
                    .iter()
                    .map(|k| tech_by_key.get(k).copied().ok_or_else(|| format!("unknown tech '{k}'")))
                    .collect::<Result<_, _>>()?,
            });
        }
        let mut damage = [[100; NUM_ARMOR]; NUM_DMG];
        for (dt, row) in &file.damage_table {
            damage[*dt as usize] = *row;
        }
        Ok(GameData {
            defs,
            techs,
            damage,
            by_key,
            tech_by_key,
            hash: fnv64(text.as_bytes()),
        })
    }

    pub fn id(&self, key: &str) -> DefId {
        *self.by_key.get(key).unwrap_or_else(|| panic!("unknown def {key}"))
    }
    pub fn try_id(&self, key: &str) -> Option<DefId> {
        self.by_key.get(key).copied()
    }
    pub fn tech_id(&self, key: &str) -> Option<u16> {
        self.tech_by_key.get(key).copied()
    }
    #[inline]
    pub fn def(&self, id: DefId) -> &Def {
        &self.defs[id as usize]
    }
    pub fn mult(&self, dt: DamageType, ac: ArmorClass) -> i32 {
        self.damage[dt as usize][ac as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn data_loads() {
        let d = GameData::load();
        assert!(d.defs.len() > 20);
        let cap = d.def(d.id("capitol"));
        assert!(cap.is_building());
        assert!(!cap.trains.is_empty());
    }
}
