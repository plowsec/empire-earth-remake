//! Command validation and translation into unit orders. Every command is
//! re-validated here (ownership, affordability, placement) because in multiplayer
//! commands arrive from untrusted peers.
use crate::command::{Command, CommandKind};
use crate::defs::{Class, Layer};
use crate::entity::*;
use crate::fixed::{FVec, Fx};
use crate::mapgen::GAIA;
use crate::path::FlowField;
use crate::world::{data, SimEvent, World};

const MAX_QUEUE: usize = 60;

fn owned_units(w: &World, player: u8, ids: &[EntityId]) -> Vec<EntityId> {
    let mut out: Vec<EntityId> = ids
        .iter()
        .copied()
        .filter(|&id| match w.get(id) {
            Some(e) => e.owner == player && w.def_of(e).is_unit(),
            None => false,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out.truncate(512);
    out
}

fn owned_buildings(w: &World, player: u8, ids: &[EntityId]) -> Vec<EntityId> {
    let mut out: Vec<EntityId> = ids
        .iter()
        .copied()
        .filter(|&id| match w.get(id) {
            Some(e) => e.owner == player && w.def_of(e).is_building(),
            None => false,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Give `order` to a unit, replacing or queueing.
pub fn give(w: &mut World, id: EntityId, order: Order, queue: bool) {
    let Some(e) = w.get_mut(id) else { return };
    if queue && e.order != Order::Idle {
        if e.queue.len() < 32 {
            e.queue.push(order);
        }
        return;
    }
    e.queue.clear();
    set_order(w, id, order);
}

/// Replace the current order, resetting movement state.
pub fn set_order(w: &mut World, id: EntityId, order: Order) {
    let tick = w.tick;
    let Some(e) = w.get_mut(id) else { return };
    e.order = order;
    e.goal = None;
    e.path.clear();
    e.flow = None;
    e.stuck = 0;
    e.repath_cd = 0;
    e.goal_rect = false;
    e.forced_target = matches!(order, Order::Attack { .. });
    if let Order::Attack { target } = order {
        e.target = target;
    }
    // landed aircraft take off for any active order
    if e.inside != 0 && !matches!(order, Order::Idle | Order::ReturnToBase) {
        let inside = e.inside;
        if let Some(home) = w.get(inside) {
            let hp = home.pos;
            let is_airport = data().def(home.def).data.airport;
            if is_airport {
                if let Some(t) = w.get_mut(inside) {
                    t.cargo.retain(|&c| c != id);
                }
                let e = w.get_mut(id).unwrap();
                e.inside = 0;
                e.pos = hp;
                e.prev_pos = hp;
                e.action = Action::Move;
                let _ = tick;
            }
        }
    }
}

/// Battle rank for formations: 0 front (armor) .. 4 rear/centre (support).
fn rank_of(d: &crate::defs::Def) -> i32 {
    use crate::defs::ArmorClass;
    if d.class() == crate::defs::Class::Citizen || d.data.heal > 0 || d.data.cargo > 0 {
        return 4;
    }
    let min_range = d.weapons.iter().map(|w| w.min_range).max().unwrap_or(Fx::ZERO);
    if min_range > Fx::ZERO || d.max_range >= Fx::from_int(10) {
        return 3; // artillery
    }
    if d.data.armor_class == ArmorClass::Heavy {
        return 0;
    }
    if d.weapons.iter().all(|w| !w.vs_ground) || d.max_range >= Fx::from_ratio(700, 100) {
        return 2; // AA, AT guns, snipers
    }
    1
}

/// Intelligent formation: ranks perpendicular to the direction of travel, armor in
/// front, artillery and support behind; within a rank units keep their left/right
/// order so paths don't cross. Slots snap to distinct passable tiles.
pub fn formation_slots(w: &World, units: &[EntityId], to: FVec, layer: Layer, spacing: Fx) -> Vec<(EntityId, FVec)> {
    let n = units.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![(units[0], snap_slot(w, to, layer, &mut Vec::new()))];
    }
    let cx = units.iter().map(|&u| w.get(u).unwrap().pos.x.0 as i64).sum::<i64>() / n as i64;
    let cy = units.iter().map(|&u| w.get(u).unwrap().pos.y.0 as i64).sum::<i64>() / n as i64;
    let centroid = FVec::new(Fx(cx as i32), Fx(cy as i32));
    let mut fwd = (to - centroid).normalized();
    if fwd.len2_raw() == 0 {
        fwd = FVec::new(Fx::ZERO, Fx::ONE);
    }
    let right = FVec::new(-fwd.y, fwd.x);
    // aircraft and ships use a simple block; ground units use ranks
    let ranked: Vec<(i32, i64, EntityId)> = units
        .iter()
        .map(|&u| {
            let e = w.get(u).unwrap();
            let d = data().def(e.def);
            let rank = if layer == Layer::Land { rank_of(d) } else { 1 };
            let lateral = (e.pos - centroid).dot_raw(right) >> 16;
            (rank, lateral, u)
        })
        .collect();
    let cols = ((crate::fixed::isqrt_u64(n as u64 * 2) as usize).max(3)).min(n).min(16);
    let mut out = Vec::with_capacity(n);
    let mut used: Vec<(i32, i32)> = Vec::new();
    let mut depth_rows = 0i32;
    for rank in 0..5 {
        let mut members: Vec<(i64, EntityId)> = ranked.iter().filter(|r| r.0 == rank).map(|r| (r.1, r.2)).collect();
        if members.is_empty() {
            continue;
        }
        members.sort();
        for chunk in members.chunks(cols) {
            let m = chunk.len() as i32;
            for (k, &(_, u)) in chunk.iter().enumerate() {
                let lat = spacing.mul_int(k as i32 * 2 - (m - 1));
                let lat = Fx(lat.0 / 2);
                let back = spacing.mul_int(depth_rows);
                let p = to + right.scale(lat) - fwd.scale(back);
                out.push((u, snap_slot(w, p, layer, &mut used)));
            }
            depth_rows += 1;
        }
    }
    out
}

fn snap_slot(w: &World, p: FVec, layer: Layer, used: &mut Vec<(i32, i32)>) -> FVec {
    let (tx, ty) = p.tile();
    if layer == Layer::Air {
        return p;
    }
    if w.map.passable(tx, ty, layer) && !used.contains(&(tx, ty)) {
        used.push((tx, ty));
        return p;
    }
    for r in 1i32..8 {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let t = (tx + dx, ty + dy);
                if w.map.passable(t.0, t.1, layer) && !used.contains(&t) {
                    used.push(t);
                    return FVec::tile_center(t.0, t.1);
                }
            }
        }
    }
    p
}

/// Square-ish formation offsets, deterministic by index.
pub fn formation_offset(i: usize, n: usize, spacing: Fx) -> FVec {
    if n <= 1 {
        return FVec::ZERO;
    }
    let cols = (crate::fixed::isqrt_u64(n as u64) as usize).max(1);
    let cols = if cols * cols < n { cols + 1 } else { cols };
    let rows = (n + cols - 1) / cols;
    let c = (i % cols) as i32;
    let r = (i / cols) as i32;
    let ox = spacing.mul_int(c * 2 - (cols as i32 - 1));
    let oy = spacing.mul_int(r * 2 - (rows as i32 - 1));
    FVec::new(Fx(ox.0 / 2), Fx(oy.0 / 2))
}

pub fn apply(w: &mut World, c: &Command) {
    let p = c.player;
    if p as usize >= w.players.len() || w.players[p as usize].defeated {
        return;
    }
    match &c.kind {
        CommandKind::Move { units, to, attack_move, queue } => {
            let units = owned_units(w, p, units);
            move_group(w, &units, *to, *attack_move, *queue);
        }
        CommandKind::Target { units, target, queue } => {
            let units = owned_units(w, p, units);
            smart_target(w, p, &units, *target, *queue, false);
        }
        CommandKind::Attack { units, target, queue } => {
            let units = owned_units(w, p, units);
            smart_target(w, p, &units, *target, *queue, true);
        }
        CommandKind::Build { units, def, tile, queue } => {
            let units: Vec<EntityId> = owned_units(w, p, units)
                .into_iter()
                .filter(|&id| {
                    let e = w.get(id).unwrap();
                    w.def_of(e).builds.contains(def)
                })
                .collect();
            if units.is_empty() || *def as usize >= data().defs.len() {
                return;
            }
            if let Err(msg) = w.can_place(p, *def, *tile) {
                w.events.push(SimEvent::Notice { owner: p, text: msg });
                return;
            }
            let cost = data().def(*def).data.cost;
            if !w.pay(p, &cost) {
                w.events.push(SimEvent::Notice { owner: p, text: "Not enough resources" });
                return;
            }
            let site = w.spawn_static(*def, p, *tile, false);
            w.events.push(SimEvent::BuildingPlaced { id: site, def: *def, owner: p });
            for id in units {
                give(w, id, Order::Build { site }, *queue);
            }
        }
        CommandKind::Train { building, def, count } => {
            let Some(b) = w.get(*building) else { return };
            if b.owner != p || !b.complete || !w.def_of(b).trains.contains(def) {
                return;
            }
            let cost = data().def(*def).data.cost;
            for _ in 0..(*count).clamp(1, 10) {
                let len = w.get(*building).unwrap().production.len();
                if len >= MAX_QUEUE {
                    break;
                }
                if !w.pay(p, &cost) {
                    w.events.push(SimEvent::Notice { owner: p, text: "Not enough resources" });
                    break;
                }
                w.get_mut(*building).unwrap().production.push(ProdItem::Unit(*def));
            }
        }
        CommandKind::Research { building, tech } => {
            let d = data();
            let Some(t) = d.techs.get(*tech as usize) else { return };
            let Some(b) = w.get(*building) else { return };
            if b.owner != p || !b.complete || b.def != t.at || b.production.len() >= MAX_QUEUE {
                return;
            }
            let pl = &w.players[p as usize];
            if pl.techs[*tech as usize] || pl.researching[*tech as usize] {
                return;
            }
            if !t.requires.iter().all(|r| pl.techs[*r as usize]) {
                return;
            }
            if !w.pay(p, &t.data.cost) {
                w.events.push(SimEvent::Notice { owner: p, text: "Not enough resources" });
                return;
            }
            w.players[p as usize].researching[*tech as usize] = true;
            w.get_mut(*building).unwrap().production.push(ProdItem::Tech(*tech));
        }
        CommandKind::CancelProduction { building, index } => {
            let Some(b) = w.get(*building) else { return };
            if b.owner != p || *index as usize >= b.production.len() {
                return;
            }
            let item = b.production[*index as usize];
            let b = w.get_mut(*building).unwrap();
            b.production.remove(*index as usize);
            if *index == 0 {
                b.prod_progress = 0;
            }
            match item {
                ProdItem::Unit(u) => {
                    let cost = data().def(u).data.cost;
                    w.refund(p, &cost);
                }
                ProdItem::Tech(t) => {
                    let cost = data().techs[t as usize].data.cost;
                    w.refund(p, &cost);
                    w.players[p as usize].researching[t as usize] = false;
                }
            }
            w.recount_pop();
        }
        CommandKind::SetRally { buildings, to, target } => {
            for b in owned_buildings(w, p, buildings) {
                let e = w.get_mut(b).unwrap();
                if *to == e.pos && *target == 0 {
                    e.rally = None;
                    e.rally_target = 0;
                } else {
                    e.rally = Some(*to);
                    e.rally_target = *target;
                }
            }
        }
        CommandKind::Stop { units } => {
            for id in owned_units(w, p, units) {
                give(w, id, Order::Idle, false);
                if let Some(e) = w.get_mut(id) {
                    e.target = 0;
                    e.sortie = None;
                    e.patrol.clear();
                }
            }
        }
        CommandKind::Scout { units } => {
            let units = owned_units(w, p, units);
            scout(w, &units);
        }
        CommandKind::RebuildFarms { building } => {
            rebuild_farms(w, p, *building);
        }
        CommandKind::Unload { units, at } => {
            for id in owned_units(w, p, units) {
                let e = w.get(id).unwrap();
                if w.def_of(e).data.cargo > 0 {
                    give(w, id, Order::Unload { at: *at }, false);
                }
            }
        }
        CommandKind::ReturnToBase { units } => {
            for id in owned_units(w, p, units) {
                let e = w.get(id).unwrap();
                if w.def_of(e).data.needs_airport {
                    give(w, id, Order::ReturnToBase, false);
                    if let Some(e) = w.get_mut(id) {
                        e.sortie = None;
                    }
                }
            }
        }
        CommandKind::Delete { units } => {
            let mut ids = owned_units(w, p, units);
            ids.extend(owned_buildings(w, p, units));
            for id in ids {
                w.kill(id, GAIA);
            }
            w.recount_pop();
        }
        CommandKind::Resign => {
            w.resign(p);
        }
    }
}

pub fn move_group(w: &mut World, units: &[EntityId], to: FVec, attack_move: bool, queue: bool) {
    if units.is_empty() {
        return;
    }
    // group per layer so ships and tanks ordered together each get a sane formation
    for layer in [Layer::Land, Layer::Water, Layer::Air] {
        let group: Vec<EntityId> = units
            .iter()
            .copied()
            .filter(|&id| w.def_of(w.get(id).unwrap()).layer == layer)
            .collect();
        if group.is_empty() {
            continue;
        }
        // largest radius sets spacing
        let maxr = group.iter().map(|&id| w.def_of(w.get(id).unwrap()).radius).max().unwrap_or(Fx::HALF);
        let spacing = match layer {
            // hulls and airframes look much bigger than their hit radius
            Layer::Water => maxr.mul_int(4) + Fx::from_ratio(50, 100),
            Layer::Air => maxr.mul_int(5),
            _ => maxr.mul_int(2) + Fx::from_ratio(30, 100),
        }
        .max(Fx::from_ratio(70, 100));
        // order units by distance to target so the closest take the front slots
        let mut sorted: Vec<(i64, EntityId)> =
            group.iter().map(|&id| (w.get(id).unwrap().pos.dist2_raw(to), id)).collect();
        sorted.sort();
        let n = sorted.len();
        let use_flow = n >= 8 && layer != Layer::Air;
        let (gx, gy) = to.tile();
        if use_flow {
            let ok = w.map.passable(gx, gy, layer);
            let goal = if ok { Some((gx, gy)) } else { w.map.nearest_passable(gx, gy, layer, 8) };
            if let Some(g) = goal {
                let li = layer as u8;
                if !w.flows.iter().any(|f| f.layer as u8 == li && f.goal == g && f.version == w.map.version) {
                    let ff = FlowField::build(&w.map, g, layer, w.tick);
                    w.flows.push(ff);
                    if w.flows.len() > 24 {
                        // evict least recently used
                        let (oldest, _) = w.flows.iter().enumerate().min_by_key(|(_, f)| f.last_used).unwrap();
                        w.flows.remove(oldest);
                    }
                }
            }
        }
        let slots = formation_slots(w, &sorted.iter().map(|x| x.1).collect::<Vec<_>>(), to, layer, spacing);
        for (i, (_, id)) in sorted.iter().enumerate() {
            let _ = i;
            let dest = slots.iter().find(|(u, _)| u == id).map(|x| x.1).unwrap_or(to);
            if layer == Layer::Air && w.def_of(w.get(*id).unwrap()).data.needs_airport {
                give(w, *id, Order::Patrol { at: dest }, queue);
                if let Some(e) = w.get_mut(*id) {
                    e.sortie = Some(dest);
                }
                continue;
            }
            give(w, *id, Order::Move { to: dest, attack_move }, queue);
            if use_flow && !queue {
                let g = if w.map.passable(gx, gy, layer) { Some((gx, gy)) } else { w.map.nearest_passable(gx, gy, layer, 8) };
                if let (Some(g), Some(e)) = (g, w.get_mut(*id)) {
                    e.flow = Some((layer as u8, g.0, g.1));
                }
            }
        }
    }
}

fn smart_target(w: &mut World, p: u8, units: &[EntityId], target: EntityId, queue: bool, force_attack: bool) {
    let Some(t) = w.get(target) else { return };
    let tdef = w.def_of(t);
    let towner = t.owner;
    let tpos = t.pos;
    let enemy = w.is_enemy(p, towner);
    let own = towner == p;
    let t_complete = t.complete;
    let t_damaged = t.hp < w.max_hp(t);
    let mut plan: Vec<(EntityId, Option<Order>)> = Vec::new();
    for &id in units {
        let e = w.get(id).unwrap();
        let t = w.get(target).unwrap();
        let d = w.def_of(e);
        let order = if enemy || force_attack {
            if crate::combat::can_attack(w, e, t) {
                Some(Order::Attack { target })
            } else {
                None
            }
        } else if tdef.is_resource() {
            let r = tdef.data.resource.unwrap() as usize;
            let is_fish = w.fish.contains(&target);
            let can = d.gather_rate[r] > 0 && ((d.layer == Layer::Water) == is_fish);
            if can { Some(Order::Gather { node: target }) } else { None }
        } else if own && tdef.is_building() {
            if !t_complete && !d.builds.is_empty() {
                Some(Order::Build { site: target })
            } else if tdef.data.walkable && d.gather_rate[0] > 0 {
                Some(Order::Gather { node: target })
            } else if e.carry > 0 && tdef.data.dropsite.iter().any(|r| *r as u8 == e.carry_res) {
                Some(Order::ReturnCargo)
            } else if t_damaged && !d.builds.is_empty() {
                Some(Order::Repair { target })
            } else if tdef.data.airport && d.data.needs_airport {
                Some(Order::ReturnToBase)
            } else {
                None
            }
        } else if own && tdef.data.cargo > 0 && d.layer == Layer::Land {
            Some(Order::Board { transport: target })
        } else {
            None
        };
        plan.push((id, order));
    }
    let mut movers = Vec::new();
    for (id, order) in plan {
        match order {
            Some(o) => {
                give(w, id, o, queue);
                if o == Order::ReturnToBase {
                    if let Some(e) = w.get_mut(id) {
                        e.home = target;
                    }
                }
            }
            None => movers.push(id),
        }
    }
    if !movers.is_empty() {
        move_group(w, &movers, tpos, false, queue);
    }
    // landing ships sail to the beach nearest the units waiting to board
    if own && tdef.data.cargo > 0 {
        let boarders: Vec<FVec> = units
            .iter()
            .filter_map(|&u| w.get(u))
            .filter(|e| matches!(e.order, Order::Board { .. }))
            .map(|e| e.pos)
            .collect();
        if !boarders.is_empty() {
            let n = boarders.len() as i64;
            let cx = boarders.iter().map(|p| p.x.0 as i64).sum::<i64>() / n;
            let cy = boarders.iter().map(|p| p.y.0 as i64).sum::<i64>() / n;
            let center = FVec::new(Fx(cx as i32), Fx(cy as i32));
            if let Some(shore) = shore_water_near(w, center, 24) {
                let t = w.get(target).unwrap();
                if matches!(t.order, Order::Idle | Order::Move { .. }) {
                    give(w, target, Order::Move { to: shore, attack_move: false }, false);
                }
            }
        }
    }
    let _ = Class::Building;
}

/// Water tile (usable by ships) next to land, nearest to `p`.
pub fn shore_water_near(w: &World, p: FVec, max_r: i32) -> Option<FVec> {
    let (px, py) = p.tile();
    for r in 0..=max_r {
        let mut best: Option<(i32, (i32, i32))> = None;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let (x, y) = (px + dx, py + dy);
                if !w.map.passable(x, y, Layer::Water) {
                    continue;
                }
                let coast = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(ox, oy)| w.map.is_land(x + ox, y + oy));
                if coast {
                    let d = dx * dx + dy * dy;
                    if best.map_or(true, |(b, _)| d < b) {
                        best = Some((d, (x, y)));
                    }
                }
            }
        }
        if let Some((_, (x, y))) = best {
            return Some(FVec::tile_center(x, y));
        }
    }
    None
}

/// Land tiles of the island containing (x, y) (or nearest island), capped.
fn island_tiles(w: &World, x: i32, y: i32) -> Vec<(i32, i32)> {
    let start = if w.map.is_land(x, y) {
        Some((x, y))
    } else {
        let mut f = None;
        'o: for r in 1..40 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if (dx as i32).abs() != r && (dy as i32).abs() != r {
                        continue;
                    }
                    if w.map.is_land(x + dx, y + dy) {
                        f = Some((x + dx, y + dy));
                        break 'o;
                    }
                }
            }
        }
        f
    };
    let Some(s) = start else { return vec![] };
    let mut seen = std::collections::BTreeSet::new();
    let mut q = std::collections::VecDeque::new();
    seen.insert(s);
    q.push_back(s);
    let mut out = Vec::new();
    while let Some((cx, cy)) = q.pop_front() {
        out.push((cx, cy));
        if out.len() > 60000 {
            break;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (cx + dx, cy + dy);
            if w.map.is_land(n.0, n.1) && !seen.contains(&n) {
                seen.insert(n);
                q.push_back(n);
            }
        }
    }
    out
}

/// Give each unit a looping route around its island and set it scouting.
pub fn scout(w: &mut World, units: &[EntityId]) {
    if units.is_empty() {
        return;
    }
    for layer in [Layer::Land, Layer::Water, Layer::Air] {
        let group: Vec<EntityId> = units.iter().copied().filter(|&id| w.def_of(w.get(id).unwrap()).layer == layer).collect();
        if group.is_empty() {
            continue;
        }
        let n = group.len() as i64;
        let cx = group.iter().map(|&id| w.get(id).unwrap().pos.x.0 as i64).sum::<i64>() / n;
        let cy = group.iter().map(|&id| w.get(id).unwrap().pos.y.0 as i64).sum::<i64>() / n;
        let (tx, ty) = FVec::new(Fx(cx as i32), Fx(cy as i32)).tile();
        let tiles = island_tiles(w, tx, ty);
        if tiles.is_empty() {
            continue;
        }
        let ix = tiles.iter().map(|t| t.0 as i64).sum::<i64>() / tiles.len() as i64;
        let iy = tiles.iter().map(|t| t.1 as i64).sum::<i64>() / tiles.len() as i64;
        // radius of a disc with the island's area
        let rad = crate::fixed::isqrt_u64((tiles.len() as u64 * 100) / 314) as i32;
        let ring = match layer {
            Layer::Land => rad * 70 / 100,
            Layer::Water => rad * 125 / 100 + 3,
            _ => rad * 85 / 100,
        }
        .max(4);
        let mut route = Vec::new();
        for k in 0..12 {
            let (c, s) = crate::mapgen::sincos_deg(k * 30);
            let (px, py) = (ix as i32 + c * ring / 1024, iy as i32 + s * ring / 1024);
            let snapped = match layer {
                Layer::Air => Some((px.clamp(1, w.map.w - 2), py.clamp(1, w.map.h - 2))),
                l => w.map.nearest_passable(px, py, l, 8),
            };
            if let Some((sx, sy)) = snapped {
                route.push(FVec::tile_center(sx, sy));
            }
        }
        if route.len() < 3 {
            continue;
        }
        for &id in &group {
            let pos = w.get(id).unwrap().pos;
            let start = route.iter().enumerate().min_by_key(|(_, p)| p.dist2_raw(pos)).map(|(i, _)| i).unwrap_or(0);
            give(w, id, Order::Scout { idx: start as u8 }, false);
            if let Some(e) = w.get_mut(id) {
                e.patrol = route.clone();
                e.sortie = None;
            }
        }
    }
}

/// Granary: farm foundations on every free plot of its ring, citizens assigned.
pub fn rebuild_farms(w: &mut World, p: u8, building: EntityId) {
    let d = data();
    let farm = d.id("farm");
    let Some(g) = w.get(building) else { return };
    if g.owner != p || !g.complete || g.def != d.id("granary") {
        return;
    }
    let (gx, gy) = g.tile;
    let gpos = g.pos;
    let slots = [
        (gx - 3, gy), (gx + 3, gy), (gx, gy - 3), (gx, gy + 3),
        (gx - 3, gy - 3), (gx + 3, gy - 3), (gx - 3, gy + 3), (gx + 3, gy + 3),
    ];
    let cost = d.def(farm).data.cost;
    let mut sites = Vec::new();
    for t in slots {
        if w.can_place(p, farm, t).is_ok() {
            if !w.pay(p, &cost) {
                w.events.push(SimEvent::Notice { owner: p, text: "Not enough wood for more fields" });
                break;
            }
            let id = w.spawn_static(farm, p, t, false);
            w.events.push(SimEvent::BuildingPlaced { id, def: farm, owner: p });
            sites.push(id);
        }
    }
    if sites.is_empty() {
        return;
    }
    // one citizen per field: idle ones first, then food gatherers, then the nearest
    let cit = d.id("citizen");
    let mut pool: Vec<(i64, EntityId)> = w
        .entities
        .iter()
        .filter(|e| e.alive && e.owner == p && e.def == cit && e.inside == 0 && !matches!(e.order, Order::Build { .. }))
        .map(|e| {
            let pri: i64 = match e.order {
                Order::Idle => 0,
                Order::Gather { .. } if e.last_res == 0 => 1 << 40,
                _ => 2 << 40,
            };
            (pri + e.pos.dist2_raw(gpos) / 65536, e.id)
        })
        .collect();
    pool.sort();
    for (site, (_, c)) in sites.iter().zip(pool.iter()) {
        give(w, *c, Order::Build { site: *site }, false);
    }
}
