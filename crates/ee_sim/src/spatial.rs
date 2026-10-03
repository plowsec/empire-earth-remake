//! Uniform-grid spatial hash over mobile units and buildings (resources excluded).
//! Rebuilt every tick in slot order, so query results are deterministic.
use crate::defs::GameData;
use crate::entity::{Entity, EntityId};
use crate::fixed::{FVec, Fx};

pub const CELL: i32 = 4;

pub struct SpatialHash {
    cw: i32,
    ch: i32,
    cells: Vec<Vec<(EntityId, u32)>>,
}

impl SpatialHash {
    pub fn new(map_w: i32, map_h: i32) -> SpatialHash {
        let cw = map_w / CELL + 1;
        let ch = map_h / CELL + 1;
        SpatialHash { cw, ch, cells: vec![Vec::new(); (cw * ch) as usize] }
    }

    pub fn rebuild(&mut self, entities: &[Entity], data: &GameData) {
        for c in &mut self.cells {
            c.clear();
        }
        for (slot, e) in entities.iter().enumerate() {
            if !e.on_map() || data.def(e.def).is_resource() {
                continue;
            }
            let (tx, ty) = e.pos.tile();
            let cx = (tx / CELL).clamp(0, self.cw - 1);
            let cy = (ty / CELL).clamp(0, self.ch - 1);
            self.cells[(cy * self.cw + cx) as usize].push((e.id, slot as u32));
        }
    }

    /// Visit every entity whose position is within the square of `r` around `p`.
    /// Callers do the exact distance test.
    #[inline]
    pub fn for_each(&self, p: FVec, r: Fx, mut f: impl FnMut(EntityId, usize)) {
        let x0 = ((p.x - r).floor_int() / CELL - 1).clamp(0, self.cw - 1);
        let x1 = ((p.x + r).floor_int() / CELL + 1).clamp(0, self.cw - 1);
        let y0 = ((p.y - r).floor_int() / CELL - 1).clamp(0, self.ch - 1);
        let y1 = ((p.y + r).floor_int() / CELL + 1).clamp(0, self.ch - 1);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                for &(id, slot) in &self.cells[(cy * self.cw + cx) as usize] {
                    f(id, slot as usize);
                }
            }
        }
    }
}
