//! Island-aware idle selection and mine staffing, shared by the UI and simulation.
use crate::defs::{Class, Layer, Res};
use crate::entity::{EntityId, Order};
use crate::fixed::FVec;
use crate::world::{data, gather_cap, World};

impl World {
    /// Prefer the island under the camera; otherwise use the closest idle worker's
    /// island. Never combine islands in one batch.
    pub fn idle_citizens_near(&self, player: u8, near: FVec, count: usize) -> Vec<EntityId> {
        let island = |pos: FVec| { let (x, y) = pos.tile(); self.map.land_at(x, y) };
        let focus = island(near);
        let mut idle: Vec<_> = self.entities.iter()
            .filter(|e| e.on_map() && e.owner == player && data().def(e.def).class() == Class::Citizen
                && e.order == Order::Idle && e.queue.is_empty() && island(e.pos).is_some())
            .collect();
        idle.sort_by_key(|e| (focus.is_none() || island(e.pos) != focus, e.pos.dist2_raw(near), e.id));
        let Some(first) = idle.first() else { return vec![] };
        let home = island(first.pos);
        idle.into_iter().filter(|e| island(e.pos) == home).take(count).map(|e| e.id).collect()
    }

    /// Up to eight idle citizens fill real vacancies on explored mines. Walkability
    /// includes buildings and walls, not just natural island connectivity.
    pub fn staff_mines(&mut self, player: u8, near: FVec, count: u8) -> usize {
        let mut idle = self.idle_citizens_near(player, near, usize::MAX);
        let Some(&first) = idle.first() else { return 0 };
        let origin = self.get(first).unwrap().pos;
        let mut mines: Vec<_> = self.entities.iter().filter(|e| {
            let d = data().def(e.def);
            let (x, y) = e.pos.tile();
            e.on_map() && e.amount > 0 && d.is_resource()
                && matches!(d.data.resource, Some(Res::Stone | Res::Gold | Res::Iron))
                && self.explored(player, x, y) && self.reachable(origin, e)
        }).map(|e| (e.pos.dist2_raw(near), e.id)).collect();
        mines.sort();
        let regions = crate::path::connected_regions(&self.map, Layer::Land);
        let region = |pos: FVec| {
            let (x, y) = pos.tile();
            if self.map.in_bounds(x, y) { regions[self.map.idx(x, y)] } else { 0 }
        };
        let worker_regions: std::collections::BTreeMap<_, _> = idle.iter()
            .map(|&id| (id, region(self.get(id).unwrap().pos))).collect();
        let mut assigned = 0;
        let limit = count.min(8) as usize;
        for (_, node) in mines {
            if assigned >= limit || idle.is_empty() { break; }
            let mine = self.get(node).unwrap();
            // Count orders directly: cached gatherer counts can lag behind commands.
            let occupied = self.entities.iter().filter(|e| e.alive &&
                (e.order == Order::Gather { node } || e.queue.contains(&Order::Gather { node }))).count();
            let free = (gather_cap(data().def(mine.def)) as usize).saturating_sub(occupied);
            if free == 0 { continue; }
            let pos = mine.pos;
            let (mx, my) = mine.tile;
            let (sw, sh) = data().def(mine.def).size();
            let mut sides = Vec::new();
            for y in my - 1..=my + sh {
                for x in mx - 1..=mx + sw {
                    if self.map.passable(x, y, Layer::Land) {
                        sides.push(regions[self.map.idx(x, y)]);
                    }
                }
            }
            let mut candidates: Vec<_> = idle.iter().filter(|id| {
                let r = worker_regions[id];
                r != 0 && sides.contains(&r)
            }).map(|&id| (self.get(id).unwrap().pos.dist2_raw(pos), id)).collect();
            candidates.sort();
            for (_, id) in candidates.into_iter().take(free.min(limit - assigned)) {
                crate::orders::give(self, id, Order::Gather { node }, false);
                idle.retain(|u| *u != id);
                assigned += 1;
            }
        }
        assigned
    }
}
