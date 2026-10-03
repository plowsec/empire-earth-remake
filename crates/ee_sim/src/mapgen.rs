//! "Big Islands" random map generator. Pure integer math: the same seed produces
//! the same map on every machine.
use crate::map::{Map, Terrain};
use crate::rng::SimRng;

pub const GAIA: u8 = 255;

#[derive(Clone, Debug)]
pub struct Placement {
    pub key: &'static str,
    pub owner: u8,
    /// top-left tile for multi-tile objects, tile for single-tile ones
    pub x: i32,
    pub y: i32,
}

pub struct GenResult {
    pub map: Map,
    pub objects: Vec<Placement>,
    /// player start tile (capitol center)
    pub starts: Vec<(i32, i32)>,
}

#[derive(Clone, Debug)]
pub struct MapParams {
    pub seed: u64,
    pub players: usize,
    /// 0 small .. 2 large
    pub size: u8,
    /// resource abundance percent (100 = standard)
    pub resources: u32,
}

fn hash2(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x27d4_eb2d)
        ^ (y as u32).wrapping_mul(0x1656_67b1)
        ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    h
}

/// Smooth value noise, output 0..=65535.
fn vnoise(x: i32, y: i32, cell: i32, seed: u32) -> i32 {
    let cx = x.div_euclid(cell);
    let cy = y.div_euclid(cell);
    let fx = (x.rem_euclid(cell) as i64 * 65536 / cell as i64) as i64;
    let fy = (y.rem_euclid(cell) as i64 * 65536 / cell as i64) as i64;
    let s = |t: i64| -> i64 { (t * t >> 16) * (3 * 65536 - 2 * t) >> 16 };
    let sx = s(fx);
    let sy = s(fy);
    let v = |ix: i32, iy: i32| -> i64 { (hash2(ix, iy, seed) & 0xffff) as i64 };
    let a = v(cx, cy) * (65536 - sx) + v(cx + 1, cy) * sx;
    let b = v(cx, cy + 1) * (65536 - sx) + v(cx + 1, cy + 1) * sx;
    ((a * (65536 - sy) + b * sy) >> 32) as i32
}

/// Fractal value noise, 0..=65535.
fn fbm(x: i32, y: i32, base_cell: i32, octaves: i32, seed: u32) -> i32 {
    let mut sum: i64 = 0;
    let mut wsum: i64 = 0;
    let mut cell = base_cell;
    let mut w: i64 = 256;
    for o in 0..octaves {
        if cell < 1 {
            break;
        }
        sum += vnoise(x, y, cell, seed.wrapping_add(o as u32 * 7919)) as i64 * w;
        wsum += w;
        cell /= 2;
        w /= 2;
    }
    (sum / wsum.max(1)) as i32
}

fn isqrt(v: i64) -> i64 {
    crate::fixed::isqrt_u64(v.max(0) as u64) as i64
}

struct Island {
    cx: i32,
    cy: i32,
    r: i32,
    /// stretch along a random axis (1000 = round) and that axis' angle in degrees
    stretch: i32,
    axis: i32,
}

pub fn generate(p: &MapParams) -> GenResult {
    let n = p.players.clamp(1, 8);
    let base = match n {
        1 | 2 => 264,
        3 => 300,
        4 => 350,
        _ => 410,
    };
    let size = base + p.size as i32 * 32;
    let mut map = Map::new(size, size);
    let mut rng = SimRng::new(p.seed, 0x15_1a_4d);
    let seed = rng.next_u32();

    // ---- island layout: players around a circle, small neutral isles between
    let mut islands: Vec<Island> = Vec::new();
    let c = size / 2;
    let ring = size * 31 / 100;
    let rot = rng.below(360) as i32;
    // adjacent player centres are 2*ring*sin(pi/n) apart; keep a generous channel
    let adj = if n >= 2 { 2 * ring * sincos_deg(180 / n as i32).1 / 1024 } else { size };
    let player_r = (size * 16 / 100).min(adj * 33 / 100);
    for i in 0..n {
        let ang = rot + (i as i32) * 360 / n as i32;
        let (sx, sy) = sincos_deg(ang);
        islands.push(Island {
            cx: c + ring * sx / 1024,
            cy: c + ring * sy / 1024,
            r: player_r,
            stretch: 1000,
            axis: 0,
        });
    }
    if n == 1 {
        islands[0] = Island { cx: c - size / 5, cy: c, r: player_r, stretch: 1000, axis: 0 };
    }
    // archipelago: many small resource islands scattered across the open sea,
    // each big enough for a forward base (town center + airfield)
    let mut neutral: Vec<Island> = Vec::new();
    let want = n.max(2) * 3 + 2;
    let mut tries = 0;
    while neutral.len() < want && tries < 1500 {
        tries += 1;
        let r = rng.range(9, 14) + size / 120;
        let x = rng.range(r * 2 + 6, size - r * 2 - 6);
        let y = rng.range(r * 2 + 6, size - r * 2 - 6);
        let ok_players = islands.iter().all(|is| {
            let d2 = ((x - is.cx).pow(2) + (y - is.cy).pow(2)) as i64;
            let min = (is.r * 135 / 100 + r * 120 / 100 + 9) as i64;
            d2 >= min * min
        });
        let ok_neutral = neutral.iter().all(|is: &Island| {
            let d2 = ((x - is.cx).pow(2) + (y - is.cy).pow(2)) as i64;
            let min = (is.r * 120 / 100 + r * 120 / 100 + 8) as i64;
            d2 >= min * min
        });
        if ok_players && ok_neutral {
            let stretch = rng.range(1000, 1700);
            let axis = rng.below(180) as i32;
            neutral.push(Island { cx: x, cy: y, r, stretch, axis });
        }
    }
    let all: Vec<&Island> = islands.iter().chain(neutral.iter()).collect();

    // ---- heights
    for y in 0..=size {
        for x in 0..=size {
            // island mask in 1/1000: >0 inside
            let mut mask: i64 = -1_000_000;
            for is in &all {
                let mut dx = (x - is.cx) as i64;
                let mut dy = (y - is.cy) as i64;
                if is.stretch != 1000 {
                    // elongated islands: squash distance along the stretch axis
                    let (ca, sa) = sincos_deg(is.axis);
                    let u = (dx * ca as i64 + dy * sa as i64) / 1024;
                    let v = (-dx * sa as i64 + dy * ca as i64) / 1024;
                    dx = u * 1000 / is.stretch as i64;
                    dy = v * is.stretch as i64 / 1300;
                }
                let d = isqrt((dx * dx + dy * dy) * 1_000_000);
                // coastline wobble: radius varies 72%..128%
                let wx = x + (fbm(x, y, 24, 2, seed ^ 0x123) - 32768) * 10 / 32768;
                let wy = y + (fbm(x, y, 24, 2, seed ^ 0x456) - 32768) * 10 / 32768;
                let wob = fbm(wx, wy, 28, 4, seed ^ 0xa11) as i64;
                // small islands wobble less so they stay usable
                let r = if is.r < 20 {
                    is.r as i64 * (780 + wob * 420 / 65536)
                } else {
                    is.r as i64 * (600 + wob * 750 / 65536)
                };
                let m = (r - d) * 1000 / r.max(1);
                if m > mask {
                    mask = m;
                }
            }
            let mask = mask.max(-1500);
            let mut h: i64;
            if mask > 0 {
                // land: rises from the coast, with rolling hills
                let rise = (mask.min(350) * 1000 / 350) as i64; // 0..1000
                let hills = fbm(x, y, 24, 4, seed ^ 0xb22) as i64 - 26000;
                h = 30 + rise * 200 / 1000 + hills.max(0) * rise / 1000 * 260 / 39535;
                // mountain ridges deep inland
                let ridge = 65535 - (fbm(x, y, 40, 3, seed ^ 0xc33) as i64 - 32768).abs() * 2;
                if mask > 420 && ridge > 58500 {
                    h += (ridge - 58500) * 1100 / 7035 * (mask - 420).min(200) / 200;
                }
            } else {
                // sea floor: shelf then deep
                h = mask * 600 / 1000 - 20;
                let wob = fbm(x, y, 16, 2, seed ^ 0xd44) as i64 - 32768;
                h += wob * 40 / 32768;
            }
            map.set_corner(x, y, h as i32);
        }
    }

    // ---- flatten player start plateaus
    let mut starts = Vec::new();
    for is in &islands {
        let (sx, sy) = (is.cx, is.cy);
        let mut target = 0;
        let mut cnt = 0;
        for y in sy - 3..=sy + 3 {
            for x in sx - 3..=sx + 3 {
                target += map.corner(x, y);
                cnt += 1;
            }
        }
        let target = (target / cnt).clamp(90, 220);
        for y in sy - 20..=sy + 20 {
            for x in sx - 20..=sx + 20 {
                let d = isqrt(((x - sx) * (x - sx) + (y - sy) * (y - sy)) as i64 * 256) as i32; // *16
                let w = if d <= 8 * 16 { 256 } else if d >= 20 * 16 { 0 } else {
                    let t = (20 * 16 - d) * 256 / (12 * 16);
                    t * t / 256 * (768 - 2 * t) / 256
                };
                let cur = map.corner(x, y);
                map.set_corner(x, y, cur + (target - cur) * w / 256);
            }
        }
        starts.push((sx, sy));
    }

    // ---- terrain classes
    for y in 0..size {
        for x in 0..size {
            let h = map.tile_height(x, y);
            let mut hi = i32::MIN;
            let mut lo = i32::MAX;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let v = map.corner(x + dx, y + dy);
                hi = hi.max(v);
                lo = lo.min(v);
            }
            let slope = hi - lo;
            let moist = fbm(x, y, 20, 3, seed ^ 0xe55);
            let t = if h < -110 {
                Terrain::DeepWater
            } else if h < 0 {
                Terrain::ShallowWater
            } else if h < 34 {
                Terrain::Beach
            } else if slope > 170 || h > 620 {
                Terrain::Mountain
            } else if slope > 110 || h > 470 {
                Terrain::Rock
            } else if moist > 40000 {
                Terrain::Meadow
            } else if moist < 22000 {
                Terrain::Dirt
            } else {
                Terrain::Grass
            };
            let i = map.idx(x, y);
            map.terrain[i] = t;
        }
    }
    map.compute_base_pass();

    // ---- object placement
    let mut objects: Vec<Placement> = Vec::new();
    let mut reserved = vec![false; (size * size) as usize];
    let idx = |x: i32, y: i32| (y * size + x) as usize;

    let can_place = |map: &Map, reserved: &Vec<bool>, x0: i32, y0: i32, w: i32, h: i32, pad: i32| -> bool {
        for y in y0 - pad..y0 + h + pad {
            for x in x0 - pad..x0 + w + pad {
                if !map.in_bounds(x, y) {
                    return false;
                }
                let inner = x >= x0 && x < x0 + w && y >= y0 && y < y0 + h;
                if reserved[idx(x, y)] {
                    return false;
                }
                if inner {
                    let t = map.terrain_at(x, y);
                    if matches!(t, Terrain::DeepWater | Terrain::ShallowWater | Terrain::Mountain) {
                        return false;
                    }
                }
            }
        }
        true
    };
    let reserve = |reserved: &mut Vec<bool>, x0: i32, y0: i32, w: i32, h: i32| {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                if x >= 0 && y >= 0 && x < size && y < size {
                    reserved[idx(x, y)] = true;
                }
            }
        }
    };

    // starting capitol + citizens; keep the core clear
    for (pi, &(sx, sy)) in starts.iter().enumerate() {
        objects.push(Placement { key: "capitol", owner: pi as u8, x: sx - 2, y: sy - 2 });
        reserve(&mut reserved, sx - 2, sy - 2, 4, 4);
        for k in 0..8 {
            let ang = k * 45 + 20;
            let (cx, cy) = sincos_deg(ang);
            objects.push(Placement {
                key: "citizen",
                owner: pi as u8,
                x: sx + cx * 4 / 1024,
                y: sy + cy * 4 / 1024,
            });
        }
    }
    let mut clear_core = vec![false; (size * size) as usize];
    for &(sx, sy) in &starts {
        for y in sy - 8..=sy + 8 {
            for x in sx - 8..=sx + 8 {
                if map.in_bounds(x, y) && (x - sx).pow(2) + (y - sy).pow(2) <= 64 {
                    clear_core[idx(x, y)] = true;
                }
            }
        }
    }

    // find a spot at distance [dmin,dmax] from (sx,sy) for a w×h object
    let find_spot = |map: &Map, reserved: &Vec<bool>, rng: &mut SimRng, sx: i32, sy: i32, dmin: i32, dmax: i32, w: i32, h: i32, pad: i32| -> Option<(i32, i32)> {
        for _ in 0..200 {
            let ang = rng.below(360) as i32;
            let d = rng.range(dmin, dmax);
            let (cx, cy) = sincos_deg(ang);
            let x = sx + cx * d / 1024 - w / 2;
            let y = sy + cy * d / 1024 - h / 2;
            if can_place(map, reserved, x, y, w, h, pad) {
                return Some((x, y));
            }
        }
        None
    };

    let res_scale = p.resources.clamp(50, 300) as i32;
    for (pi, &(sx, sy)) in starts.iter().enumerate() {
        let _ = pi;
        // berries: two clusters of 6
        for (dmin, dmax) in [(7, 9), (14, 20)] {
            if let Some((bx, by)) = find_spot(&map, &reserved, &mut rng, sx, sy, dmin, dmax, 3, 2, 1) {
                for k in 0..6 {
                    let (x, y) = (bx + k % 3, by + k / 3);
                    objects.push(Placement { key: "berries", owner: GAIA, x, y });
                    reserve(&mut reserved, x, y, 1, 1);
                }
            }
        }
        let mines: &[(&'static str, i32, i32)] = &[
            ("gold_mine", 10, 15),
            ("gold_mine", 14, 20),
            ("gold_mine", 18, 26),
            ("gold_mine", 24, 34),
            ("stone_mine", 10, 15),
            ("stone_mine", 15, 22),
            ("stone_mine", 20, 30),
            ("iron_mine", 10, 15),
            ("iron_mine", 14, 20),
            ("iron_mine", 18, 24),
            ("iron_mine", 20, 28),
            ("iron_mine", 26, 34),
        ];
        for &(key, dmin, dmax) in mines {
            let count = if res_scale >= 150 { 2 } else { 1 };
            for _ in 0..count {
                if let Some((x, y)) = find_spot(&map, &reserved, &mut rng, sx, sy, dmin, dmax, 2, 2, 2) {
                    objects.push(Placement { key, owner: GAIA, x, y });
                    reserve(&mut reserved, x, y, 2, 2);
                }
            }
        }
        // home forest: a dense patch a short walk away
        if let Some((fx, fy)) = find_spot(&map, &reserved, &mut rng, sx, sy, 12, 15, 1, 1, 0) {
            for y in fy - 4..=fy + 4 {
                for x in fx - 5..=fx + 5 {
                    let dd = (x - fx).pow(2) * 9 / 25 + (y - fy).pow(2);
                    let near_hard = objects.iter().any(|o| {
                        let (w, h) = match o.key {
                            "capitol" => (4, 4),
                            "gold_mine" | "stone_mine" | "iron_mine" => (2, 2),
                            "berries" => (1, 1),
                            _ => return false,
                        };
                        x >= o.x - 2 && x < o.x + w + 2 && y >= o.y - 2 && y < o.y + h + 2
                    });
                    if dd <= 16 && map.in_bounds(x, y) && !clear_core[idx(x, y)] && !near_hard
                        && can_place(&map, &reserved, x, y, 1, 1, 0) && rng.chance(80)
                    {
                        objects.push(Placement { key: "tree", owner: GAIA, x, y });
                        reserve(&mut reserved, x, y, 1, 1);
                    }
                }
            }
        }
    }
    // archipelago riches: 2-4 mines each, biased to gold and iron (air power is expensive)
    for is in &neutral {
        let count = 2 + (is.r >= 11) as i32 + (is.r >= 13) as i32;
        for k in 0..count {
            let key = match (rng.below(10), k) {
                (_, 0) => "gold_mine",
                (_, 1) => "iron_mine",
                (0..=3, _) => "gold_mine",
                (4..=7, _) => "iron_mine",
                _ => "stone_mine",
            };
            if let Some((x, y)) = find_spot(&map, &reserved, &mut rng, is.cx, is.cy, 0, is.r * 55 / 100, 2, 2, 2) {
                objects.push(Placement { key, owner: GAIA, x, y });
                reserve(&mut reserved, x, y, 2, 2);
            }
        }
    }

    // "hard" objects (mines, berries, buildings) get a 2-tile tree-free margin
    let mut hard = vec![false; (size * size) as usize];
    for o in &objects {
        let (w, h) = match o.key {
            "capitol" => (4, 4),
            "gold_mine" | "stone_mine" | "iron_mine" => (2, 2),
            "berries" => (1, 1),
            _ => continue,
        };
        for y in o.y - 2..o.y + h + 2 {
            for x in o.x - 2..o.x + w + 2 {
                if map.in_bounds(x, y) {
                    hard[idx(x, y)] = true;
                }
            }
        }
    }
    // scattered forests from noise
    for y in 1..size - 1 {
        for x in 1..size - 1 {
            if clear_core[idx(x, y)] || reserved[idx(x, y)] {
                continue;
            }
            let t = map.terrain_at(x, y);
            let f = fbm(x, y, 14, 3, seed ^ 0xf66);
            let dense = match t {
                Terrain::Grass | Terrain::Meadow | Terrain::Dirt => f > 39000,
                Terrain::Rock => f > 45000,
                Terrain::Beach => (hash2(x, y, seed ^ 0x77) & 0xff) < 6, // lone palms
                _ => false,
            };
            if !dense {
                continue;
            }
            // keep a 2-tile margin around mines, berries and buildings so they stay reachable
            if hard[idx(x, y)] {
                continue;
            }
            let density = if t == Terrain::Beach { 100 } else { 72 };
            if (hash2(x, y, seed ^ 0x99) % 100) < density {
                objects.push(Placement { key: "tree", owner: GAIA, x, y });
                reserved[idx(x, y)] = true;
                let i = map.idx(x, y);
                if t != Terrain::Beach && t != Terrain::Rock {
                    map.terrain[i] = Terrain::Forest;
                }
            }
        }
    }

    // fish: shallow/deep water tiles near each start's coast, plus around neutral isles
    let mut fish_sites: Vec<(i32, i32, i32)> = Vec::new(); // (x, y, owner-island index)
    let centers: Vec<(i32, i32, i32)> = islands
        .iter()
        .map(|i| (i.cx, i.cy, i.r))
        .chain(neutral.iter().map(|i| (i.cx, i.cy, i.r)))
        .collect();
    for (ci, &(cx, cy, r)) in centers.iter().enumerate() {
        let want = if ci < islands.len() { 9 } else { 3 };
        let mut placed = 0;
        let mut tries = 0;
        while placed < want && tries < 400 {
            tries += 1;
            let ang = rng.below(360) as i32;
            let (dx, dy) = sincos_deg(ang);
            // walk outward until we hit water, then a couple of tiles further
            let mut d = r / 2;
            let mut hit = None;
            while d < r * 2 {
                let x = cx + dx * d / 1024;
                let y = cy + dy * d / 1024;
                if !map.in_bounds(x, y) {
                    break;
                }
                if map.is_water(x, y) {
                    let x2 = cx + dx * (d + 2) / 1024;
                    let y2 = cy + dy * (d + 2) / 1024;
                    if map.in_bounds(x2, y2) && map.is_water(x2, y2) {
                        hit = Some((x2, y2));
                    }
                    break;
                }
                d += 1;
            }
            if let Some((x, y)) = hit {
                if fish_sites.iter().all(|&(fx, fy, _)| (fx - x).pow(2) + (fy - y).pow(2) > 25) {
                    fish_sites.push((x, y, ci as i32));
                    objects.push(Placement { key: "fish", owner: GAIA, x, y });
                    placed += 1;
                }
            }
        }
    }

    GenResult { map, objects, starts }
}

/// Integer (cos, sin) * 1024 for whole degrees, via a quarter-wave table.
pub fn sincos_deg(deg: i32) -> (i32, i32) {
    const T: [i32; 91] = [
        0, 18, 36, 54, 71, 89, 107, 125, 143, 160, 178, 195, 213, 230, 248, 265, 282, 299, 316, 333,
        350, 367, 384, 400, 416, 433, 449, 465, 481, 496, 512, 527, 543, 558, 573, 587, 602, 616,
        630, 644, 658, 672, 685, 698, 711, 724, 737, 749, 761, 773, 784, 796, 807, 818, 828, 839,
        849, 859, 868, 878, 887, 896, 904, 912, 920, 928, 935, 943, 949, 956, 962, 968, 974, 979,
        984, 989, 994, 998, 1002, 1005, 1008, 1011, 1014, 1016, 1018, 1020, 1022, 1023, 1023,
        1024, 1024,
    ];
    let d = deg.rem_euclid(360);
    let sin = |a: i32| -> i32 {
        match a {
            0..=90 => T[a as usize],
            91..=180 => T[(180 - a) as usize],
            181..=270 => -T[(a - 180) as usize],
            _ => -T[(360 - a) as usize],
        }
    };
    (sin((d + 90) % 360), sin(d))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_and_sane() {
        let p = MapParams { seed: 42, players: 2, size: 1, resources: 100 };
        let a = generate(&p);
        let b = generate(&p);
        assert_eq!(a.map.height, b.map.height);
        assert_eq!(a.objects.len(), b.objects.len());
        assert_eq!(a.starts.len(), 2);
        for &(sx, sy) in &a.starts {
            assert!(a.map.is_land(sx, sy), "start must be on land");
        }
        let trees = a.objects.iter().filter(|o| o.key == "tree").count();
        let fish = a.objects.iter().filter(|o| o.key == "fish").count();
        assert!(trees > 500, "trees={trees}");
        assert!(fish >= 8, "fish={fish}");
        // start islands must be separated by water
        let (ax, ay) = a.starts[0];
        let (bx, by) = a.starts[1];
        let mut crossed_water = false;
        for s in 0..=100 {
            let x = ax + (bx - ax) * s / 100;
            let y = ay + (by - ay) * s / 100;
            if a.map.is_water(x, y) {
                crossed_water = true;
            }
        }
        assert!(crossed_water);
    }
}
