use ee_sim::map::Terrain;
use ee_sim::mapgen::{generate, MapParams};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("map") => {
            let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
            let players: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2);
            let out = args.get(4).cloned().unwrap_or("map.ppm".into());
            let g = generate(&MapParams { seed, players, size: 1, resources: 100 });
            let m = &g.map;
            let mut img = vec![0u8; (m.w * m.h * 3) as usize];
            for y in 0..m.h {
                for x in 0..m.w {
                    let h = m.tile_height(x, y);
                    let c: [u8; 3] = match m.terrain_at(x, y) {
                        Terrain::DeepWater => [20, 50, 110],
                        Terrain::ShallowWater => [40, 110, 160],
                        Terrain::Beach => [220, 200, 140],
                        Terrain::Grass => [90, 150, 60],
                        Terrain::Meadow => [120, 170, 70],
                        Terrain::Forest => [30, 90, 30],
                        Terrain::Dirt => [140, 120, 70],
                        Terrain::Rock => [130, 120, 110],
                        Terrain::Mountain => [90, 85, 80],
                    };
                    let shade = (h.clamp(0, 600) / 12) as i32;
                    let i = ((y * m.w + x) * 3) as usize;
                    for k in 0..3 {
                        img[i + k] = (c[k] as i32 + shade).clamp(0, 255) as u8;
                    }
                }
            }
            for o in &g.objects {
                let col: [u8; 3] = match o.key {
                    "tree" => [10, 60, 10],
                    "gold_mine" => [255, 215, 0],
                    "stone_mine" => [200, 200, 200],
                    "iron_mine" => [160, 60, 40],
                    "berries" => [200, 0, 120],
                    "fish" => [0, 255, 255],
                    "capitol" => [255, 0, 0],
                    "citizen" => [255, 255, 255],
                    _ => [255, 0, 255],
                };
                let (w, h) = if o.key.ends_with("_mine") { (2, 2) } else if o.key == "capitol" { (4, 4) } else { (1, 1) };
                for y in o.y..o.y + h {
                    for x in o.x..o.x + w {
                        if m.in_bounds(x, y) {
                            let i = ((y * m.w + x) * 3) as usize;
                            img[i..i + 3].copy_from_slice(&col);
                        }
                    }
                }
            }
            let mut f = std::fs::File::create(&out).unwrap();
            write!(f, "P6\n{} {}\n255\n", m.w, m.h).unwrap();
            f.write_all(&img).unwrap();
            println!("wrote {out} ({}x{}, {} objects)", m.w, m.h, g.objects.len());
        }
        _ => eprintln!("usage: ee_headless map <seed> <players> <out.ppm>"),
    }
}
