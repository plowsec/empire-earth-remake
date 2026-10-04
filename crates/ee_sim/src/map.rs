//! Tile grid: terrain, heights, passability and static occupancy.
use crate::defs::Layer;
use crate::fixed::{FVec, Fx};
use serde::{Deserialize, Serialize};

/// Heights are in 1/256 tile units; 0 = sea level.
pub const HEIGHT_UNIT: i32 = 256;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Terrain {
    DeepWater = 0,
    ShallowWater = 1,
    Beach = 2,
    Grass = 3,
    Meadow = 4,
    Forest = 5,
    Dirt = 6,
    Rock = 7,
    Mountain = 8,
}

pub const PASS_LAND: u8 = 1;
/// Water any ship can use.
pub const PASS_WATER: u8 = 2;
/// Deep water with clearance for large ships.
pub const PASS_DEEP: u8 = 4;

#[derive(Clone, Serialize, Deserialize)]
pub struct Map {
    pub w: i32,
    pub h: i32,
    /// (w+1)*(h+1) corner heights
    pub height: Vec<i16>,
    pub terrain: Vec<Terrain>,
    /// natural passability (terrain only)
    pub base_pass: Vec<u8>,
    /// passability after static objects (buildings, trees, mines)
    pub pass: Vec<u8>,
    /// entity id of the static object occupying the tile (0 = none)
    pub occupant: Vec<u32>,
    /// bumps whenever `pass` changes, invalidates cached flow fields
    pub version: u32,
    /// natural land-mass id per tile (u16::MAX = water); derived, not saved
    #[serde(skip, default)]
    pub land_id: Vec<u16>,
}

impl Map {
    pub fn new(w: i32, h: i32) -> Map {
        let n = (w * h) as usize;
        Map {
            w,
            h,
            height: vec![0; ((w + 1) * (h + 1)) as usize],
            terrain: vec![Terrain::DeepWater; n],
            base_pass: vec![0; n],
            pass: vec![0; n],
            occupant: vec![0; n],
            version: 0,
            land_id: Vec::new(),
        }
    }
    #[inline]
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }
    #[inline]
    pub fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }
    #[inline]
    pub fn corner(&self, x: i32, y: i32) -> i32 {
        let x = x.clamp(0, self.w);
        let y = y.clamp(0, self.h);
        self.height[(y * (self.w + 1) + x) as usize] as i32
    }
    pub fn set_corner(&mut self, x: i32, y: i32, v: i32) {
        if x < 0 || y < 0 || x > self.w || y > self.h {
            return;
        }
        let w1 = self.w + 1;
        self.height[(y * w1 + x) as usize] = v.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
    /// Average height of a tile.
    pub fn tile_height(&self, x: i32, y: i32) -> i32 {
        (self.corner(x, y) + self.corner(x + 1, y) + self.corner(x, y + 1) + self.corner(x + 1, y + 1)) / 4
    }
    /// Bilinear height at a fixed-point position (1/256 tile units).
    pub fn height_at(&self, p: FVec) -> i32 {
        let tx = p.x.floor_int();
        let ty = p.y.floor_int();
        let fx = (p.x.0 & 0xffff) as i64;
        let fy = (p.y.0 & 0xffff) as i64;
        let h00 = self.corner(tx, ty) as i64;
        let h10 = self.corner(tx + 1, ty) as i64;
        let h01 = self.corner(tx, ty + 1) as i64;
        let h11 = self.corner(tx + 1, ty + 1) as i64;
        let a = h00 * (65536 - fx) + h10 * fx;
        let b = h01 * (65536 - fx) + h11 * fx;
        ((a * (65536 - fy) + b * fy) >> 32) as i32
    }
    #[inline]
    pub fn terrain_at(&self, x: i32, y: i32) -> Terrain {
        if !self.in_bounds(x, y) {
            return Terrain::DeepWater;
        }
        self.terrain[self.idx(x, y)]
    }
    #[inline]
    pub fn passable(&self, x: i32, y: i32, layer: Layer) -> bool {
        if !self.in_bounds(x, y) {
            return false;
        }
        let p = self.pass[self.idx(x, y)];
        match layer {
            Layer::Land => p & PASS_LAND != 0,
            Layer::Water => p & PASS_WATER != 0,
            Layer::Air => true,
            Layer::None => false,
        }
    }
    pub fn passable_at(&self, pos: FVec, layer: Layer) -> bool {
        let (x, y) = pos.tile();
        self.passable(x, y, layer)
    }
    pub fn is_water(&self, x: i32, y: i32) -> bool {
        matches!(self.terrain_at(x, y), Terrain::DeepWater | Terrain::ShallowWater)
    }
    pub fn is_land(&self, x: i32, y: i32) -> bool {
        self.in_bounds(x, y) && !self.is_water(x, y)
    }

    /// Recompute terrain-only passability from terrain types.
    pub fn compute_base_pass(&mut self) {
        for y in 0..self.h {
            for x in 0..self.w {
                let i = self.idx(x, y);
                let p = match self.terrain[i] {
                    Terrain::DeepWater => PASS_WATER | PASS_DEEP,
                    Terrain::ShallowWater => PASS_WATER,
                    Terrain::Mountain => 0,
                    _ => PASS_LAND,
                };
                self.base_pass[i] = p;
            }
        }
        // big ships need a tile of clearance from land
        let snapshot = self.base_pass.clone();
        for y in 0..self.h {
            for x in 0..self.w {
                let i = self.idx(x, y);
                if snapshot[i] & PASS_DEEP == 0 {
                    continue;
                }
                'n: for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if self.in_bounds(nx, ny) && snapshot[self.idx(nx, ny)] & PASS_WATER == 0 {
                            self.base_pass[i] &= !PASS_DEEP;
                            break 'n;
                        }
                    }
                }
            }
        }
        for i in 0..self.pass.len() {
            self.pass[i] = if self.occupant[i] != 0 { 0 } else { self.base_pass[i] };
        }
        self.version = self.version.wrapping_add(1);
    }

    /// Mark a rectangle as occupied by a static object.
    pub fn occupy(&mut self, x0: i32, y0: i32, w: i32, h: i32, id: u32, walkable: bool) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                if self.in_bounds(x, y) {
                    let i = self.idx(x, y);
                    self.occupant[i] = id;
                    self.pass[i] = if walkable { self.base_pass[i] } else { 0 };
                }
            }
        }
        self.version = self.version.wrapping_add(1);
    }
    pub fn vacate(&mut self, x0: i32, y0: i32, w: i32, h: i32, id: u32) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                if self.in_bounds(x, y) {
                    let i = self.idx(x, y);
                    if self.occupant[i] == id {
                        self.occupant[i] = 0;
                        self.pass[i] = self.base_pass[i];
                    }
                }
            }
        }
        self.version = self.version.wrapping_add(1);
    }

    /// Bresenham line-of-passage between two positions on a layer.
    pub fn line_clear(&self, a: FVec, b: FVec, layer: Layer) -> bool {
        if layer == Layer::Air {
            return true;
        }
        // sample every 1/4 tile: conservative and deterministic
        let d = b - a;
        let len = d.len();
        let steps = (len.0 / (Fx::ONE.0 / 4)).max(1);
        for s in 0..=steps {
            let p = FVec::new(
                a.x + Fx(((d.x.0 as i64 * s as i64) / steps as i64) as i32),
                a.y + Fx(((d.y.0 as i64 * s as i64) / steps as i64) as i32),
            );
            if !self.passable_at(p, layer) {
                return false;
            }
        }
        true
    }

    /// Nearest passable tile to (x,y) on a layer, spiralling outward.
    pub fn nearest_passable(&self, x: i32, y: i32, layer: Layer, max_r: i32) -> Option<(i32, i32)> {
        if self.passable(x, y, layer) {
            return Some((x, y));
        }
        for r in 1..=max_r {
            let mut best: Option<(i32, (i32, i32))> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let (nx, ny) = (x + dx, y + dy);
                    if self.passable(nx, ny, layer) {
                        let d = dx * dx + dy * dy;
                        if best.map_or(true, |(bd, _)| d < bd) {
                            best = Some((d, (nx, ny)));
                        }
                    }
                }
            }
            if let Some((_, t)) = best {
                return Some(t);
            }
        }
        None
    }

    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for &p in &self.pass {
            h ^= p as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

impl Map {
    /// Label every natural land mass (terrain only; buildings and trees don't split them).
    pub fn compute_land_ids(&mut self) {
        let n = (self.w * self.h) as usize;
        let mut ids = vec![u16::MAX; n];
        let mut next: u16 = 0;
        let mut stack = Vec::new();
        for s in 0..n {
            if ids[s] != u16::MAX || self.base_pass[s] & PASS_LAND == 0 {
                continue;
            }
            ids[s] = next;
            stack.push(s);
            while let Some(i) = stack.pop() {
                let (x, y) = ((i as i32) % self.w, (i as i32) / self.w);
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if self.in_bounds(nx, ny) {
                        let j = self.idx(nx, ny);
                        if ids[j] == u16::MAX && self.base_pass[j] & PASS_LAND != 0 {
                            ids[j] = next;
                            stack.push(j);
                        }
                    }
                }
            }
            next = next.saturating_add(1);
        }
        self.land_id = ids;
    }

    /// Land mass of a tile, if it is land.
    pub fn land_at(&self, x: i32, y: i32) -> Option<u16> {
        if !self.in_bounds(x, y) {
            return None;
        }
        self.land_id.get(self.idx(x, y)).copied().filter(|&v| v != u16::MAX)
    }
}
