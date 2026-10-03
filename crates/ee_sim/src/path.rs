//! Pathfinding on the tile grid: A* for individual units, shared flow fields for
//! large groups. Integer costs (10 orthogonal / 14 diagonal), deterministic tie-breaks.
use crate::defs::Layer;
use crate::fixed::FVec;
use crate::map::Map;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const DIRS: [(i32, i32, u32); 8] = [
    (1, 0, 10),
    (-1, 0, 10),
    (0, 1, 10),
    (0, -1, 10),
    (1, 1, 14),
    (-1, 1, 14),
    (1, -1, 14),
    (-1, -1, 14),
];

/// Goal region: inclusive tile rectangle.
#[derive(Clone, Copy, Debug)]
pub struct GoalRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}
impl GoalRect {
    pub fn tile(x: i32, y: i32) -> GoalRect {
        GoalRect { x0: x, y0: y, x1: x, y1: y }
    }
    #[inline]
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
    #[inline]
    fn h(&self, x: i32, y: i32) -> u32 {
        let dx = if x < self.x0 { self.x0 - x } else if x > self.x1 { x - self.x1 } else { 0 } as u32;
        let dy = if y < self.y0 { self.y0 - y } else if y > self.y1 { y - self.y1 } else { 0 } as u32;
        let (a, b) = if dx > dy { (dx, dy) } else { (dy, dx) };
        10 * a + 4 * b
    }
}

/// Reusable search buffers. Not part of the game state.
#[derive(Default)]
pub struct PathScratch {
    g: Vec<u32>,
    parent: Vec<u32>,
    stamp: Vec<u32>,
    cur: u32,
    heap: BinaryHeap<Reverse<(u32, u32, u32)>>,
}

impl PathScratch {
    fn reset(&mut self, n: usize) {
        if self.g.len() != n {
            self.g = vec![0; n];
            self.parent = vec![0; n];
            self.stamp = vec![0; n];
            self.cur = 0;
        }
        self.cur = self.cur.wrapping_add(1);
        if self.cur == 0 {
            self.stamp.iter_mut().for_each(|s| *s = 0);
            self.cur = 1;
        }
        self.heap.clear();
    }
}

#[inline]
fn can_step(map: &Map, x: i32, y: i32, dx: i32, dy: i32, layer: Layer) -> bool {
    let nx = x + dx;
    let ny = y + dy;
    if !map.passable(nx, ny, layer) {
        return false;
    }
    if dx != 0 && dy != 0 {
        // no corner cutting
        return map.passable(x + dx, y, layer) && map.passable(x, y + dy, layer);
    }
    true
}

/// A* from `start` tile to any tile in `goal`. Returns tile waypoints (start excluded),
/// or a path to the closest reachable tile if the goal can't be reached.
pub fn astar(
    map: &Map,
    s: &mut PathScratch,
    start: (i32, i32),
    goal: GoalRect,
    layer: Layer,
    max_nodes: u32,
) -> Vec<(i32, i32)> {
    let w = map.w;
    let n = (map.w * map.h) as usize;
    s.reset(n);
    if !map.in_bounds(start.0, start.1) {
        return vec![];
    }
    let si = (start.1 * w + start.0) as u32;
    s.g[si as usize] = 0;
    s.parent[si as usize] = si;
    s.stamp[si as usize] = s.cur;
    let mut counter: u32 = 0;
    s.heap.push(Reverse((goal.h(start.0, start.1), counter, si)));
    let mut best = si;
    let mut best_h = goal.h(start.0, start.1);
    let mut expanded = 0;
    let mut found = None;
    while let Some(Reverse((_f, _c, ci))) = s.heap.pop() {
        let cx = (ci % w as u32) as i32;
        let cy = (ci / w as u32) as i32;
        if goal.contains(cx, cy) {
            found = Some(ci);
            break;
        }
        expanded += 1;
        if expanded > max_nodes {
            break;
        }
        let cg = s.g[ci as usize];
        for &(dx, dy, cost) in &DIRS {
            if !can_step(map, cx, cy, dx, dy, layer) {
                // still allow stepping INTO the goal (e.g. a blocked building tile)
                let (nx, ny) = (cx + dx, cy + dy);
                if !(goal.contains(nx, ny) && map.in_bounds(nx, ny) && (dx == 0 || dy == 0)) {
                    continue;
                }
            }
            let nx = cx + dx;
            let ny = cy + dy;
            let ni = (ny * w + nx) as usize;
            let ng = cg + cost;
            if s.stamp[ni] == s.cur && s.g[ni] <= ng {
                continue;
            }
            s.stamp[ni] = s.cur;
            s.g[ni] = ng;
            s.parent[ni] = ci;
            let h = goal.h(nx, ny);
            if h < best_h {
                best_h = h;
                best = ni as u32;
            }
            counter += 1;
            s.heap.push(Reverse((ng + h, counter, ni as u32)));
        }
    }
    let end = found.unwrap_or(best);
    let mut out = Vec::new();
    let mut c = end;
    while c != si {
        out.push(((c % w as u32) as i32, (c / w as u32) as i32));
        let p = s.parent[c as usize];
        if p == c {
            break;
        }
        c = p;
    }
    out.reverse();
    out
}

/// Convert a tile path to smoothed world waypoints (string pulling with line checks).
/// Returned in reverse order (next waypoint at the end) for cheap popping.
pub fn smooth(map: &Map, from: FVec, tiles: &[(i32, i32)], final_pos: Option<FVec>, layer: Layer) -> Vec<FVec> {
    let mut pts: Vec<FVec> = tiles.iter().map(|&(x, y)| FVec::tile_center(x, y)).collect();
    if let (Some(fp), Some(last)) = (final_pos, pts.last_mut()) {
        // end exactly on the requested point if it lies in the last tile
        if fp.tile() == last.tile() {
            *last = fp;
        }
    }
    let mut out = Vec::new();
    let mut anchor = from;
    let mut i = 0;
    while i < pts.len() {
        // furthest point visible from anchor
        let mut j = i;
        let lim = (i + 24).min(pts.len() - 1);
        let mut k = lim;
        while k > i {
            if map.line_clear(anchor, pts[k], layer) {
                j = k;
                break;
            }
            k -= 1;
        }
        out.push(pts[j]);
        anchor = pts[j];
        i = j + 1;
    }
    out.reverse();
    out
}

pub const FLOW_UNREACHABLE: u16 = u16::MAX;

/// Integer Dijkstra distance field from a goal tile; units descend it.
pub struct FlowField {
    pub layer: Layer,
    pub goal: (i32, i32),
    pub version: u32,
    pub cost: Vec<u16>,
    pub last_used: u32,
}

impl FlowField {
    pub fn build(map: &Map, goal: (i32, i32), layer: Layer, tick: u32) -> FlowField {
        let w = map.w;
        let n = (map.w * map.h) as usize;
        let mut cost = vec![FLOW_UNREACHABLE; n];
        let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
        if map.in_bounds(goal.0, goal.1) {
            let gi = (goal.1 * w + goal.0) as usize;
            cost[gi] = 0;
            heap.push(Reverse((0, gi as u32)));
        }
        while let Some(Reverse((c, i))) = heap.pop() {
            if c > cost[i as usize] as u32 {
                continue;
            }
            let x = (i % w as u32) as i32;
            let y = (i / w as u32) as i32;
            for &(dx, dy, sc) in &DIRS {
                // reverse search: step from neighbour into (x,y)
                let nx = x + dx;
                let ny = y + dy;
                if !map.passable(nx, ny, layer) {
                    continue;
                }
                if dx != 0 && dy != 0 && !(map.passable(x + dx, y, layer) && map.passable(x, y + dy, layer)) {
                    continue;
                }
                let ni = (ny * w + nx) as usize;
                let nc = c + sc;
                if nc < cost[ni] as u32 && nc < FLOW_UNREACHABLE as u32 {
                    cost[ni] = nc as u16;
                    heap.push(Reverse((nc, ni as u32)));
                }
            }
        }
        FlowField { layer, goal, version: map.version, cost, last_used: tick }
    }

    /// Best neighbouring tile to step to from (x,y), or None at goal / unreachable.
    pub fn next(&self, map: &Map, x: i32, y: i32) -> Option<(i32, i32)> {
        if !map.in_bounds(x, y) {
            return None;
        }
        let w = map.w;
        let here = self.cost[(y * w + x) as usize];
        if here == 0 {
            return None;
        }
        let mut best: Option<(u16, (i32, i32))> = None;
        for &(dx, dy, _) in &DIRS {
            let nx = x + dx;
            let ny = y + dy;
            if !map.in_bounds(nx, ny) {
                continue;
            }
            if dx != 0 && dy != 0 && !(map.passable(x + dx, y, self.layer) && map.passable(x, y + dy, self.layer)) {
                continue;
            }
            let c = self.cost[(ny * w + nx) as usize];
            if c == FLOW_UNREACHABLE {
                continue;
            }
            if best.map_or(true, |(bc, _)| c < bc) {
                best = Some((c, (nx, ny)));
            }
        }
        match best {
            Some((c, t)) if here == FLOW_UNREACHABLE || c < here => Some(t),
            _ => None,
        }
    }

    pub fn reachable(&self, map: &Map, x: i32, y: i32) -> bool {
        map.in_bounds(x, y) && self.cost[(y * map.w + x) as usize] != FLOW_UNREACHABLE
    }
}
