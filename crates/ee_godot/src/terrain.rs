//! Terrain mesh, splat control maps and the smooth height function shared by
//! terrain rendering and unit placement.
use ee_sim::map::{Map, Terrain};
use godot::classes::image::Format;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::{ArrayMesh, Image, ImageTexture};
use godot::prelude::*;

/// World meters per tile.
pub const TILE: f32 = 4.0;
/// Meters per sim height unit (1/256 tile) including visual exaggeration.
pub const HEIGHT_M: f32 = TILE / 256.0 * 1.6;
/// Terrain vertices per tile edge.
pub const SUB: i32 = 2;

pub struct Heights {
    pub w: i32,
    pub h: i32,
    /// (w+1)*(h+1) corner heights in meters, lightly smoothed
    pub c: Vec<f32>,
}

impl Heights {
    pub fn from_map(map: &Map) -> Heights {
        let w = map.w;
        let h = map.h;
        let mut c = vec![0f32; ((w + 1) * (h + 1)) as usize];
        for y in 0..=h {
            for x in 0..=w {
                c[(y * (w + 1) + x) as usize] = map.corner(x, y) as f32 * HEIGHT_M;
            }
        }
        // one gentle smoothing pass: softens the tile grid without moving coastlines much
        let src = c.clone();
        for y in 1..h {
            for x in 1..w {
                let i = (y * (w + 1) + x) as usize;
                let n = src[i - 1] + src[i + 1] + src[i - (w + 1) as usize] + src[i + (w + 1) as usize];
                c[i] = src[i] * 0.6 + n * 0.1;
            }
        }
        Heights { w, h, c }
    }

    #[inline]
    fn corner(&self, x: i32, y: i32) -> f32 {
        let x = x.clamp(0, self.w);
        let y = y.clamp(0, self.h);
        self.c[(y * (self.w + 1) + x) as usize]
    }

    /// Catmull-Rom interpolated height at world (x, z) in meters.
    pub fn at(&self, wx: f32, wz: f32) -> f32 {
        let gx = wx / TILE;
        let gz = wz / TILE;
        let x0 = gx.floor() as i32;
        let z0 = gz.floor() as i32;
        let fx = gx - x0 as f32;
        let fz = gz - z0 as f32;
        let cr = |p0: f32, p1: f32, p2: f32, p3: f32, t: f32| -> f32 {
            let t2 = t * t;
            let t3 = t2 * t;
            0.5 * ((2.0 * p1) + (-p0 + p2) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
        };
        let mut rows = [0f32; 4];
        for (k, dz) in (-1..=2).enumerate() {
            rows[k] = cr(
                self.corner(x0 - 1, z0 + dz),
                self.corner(x0, z0 + dz),
                self.corner(x0 + 1, z0 + dz),
                self.corner(x0 + 2, z0 + dz),
                fx,
            );
        }
        cr(rows[0], rows[1], rows[2], rows[3], fz)
    }

    pub fn normal(&self, wx: f32, wz: f32) -> Vector3 {
        let e = 0.6;
        let hl = self.at(wx - e, wz);
        let hr = self.at(wx + e, wz);
        let hd = self.at(wx, wz - e);
        let hu = self.at(wx, wz + e);
        Vector3::new(hl - hr, 2.0 * e, hd - hu).normalized()
    }
}

/// Build terrain chunks (CHUNK×CHUNK tiles each) as ArrayMeshes. Includes a skirt of
/// sea floor beyond the map edge so the coast never shows a cliff into the void.
pub const CHUNK: i32 = 32;

pub fn build_chunk(hs: &Heights, cx: i32, cy: i32) -> Gd<ArrayMesh> {
    let x0 = cx * CHUNK;
    let y0 = cy * CHUNK;
    let n = CHUNK * SUB + 1;
    let step = TILE / SUB as f32;
    let mut verts = PackedVector3Array::new();
    let mut norms = PackedVector3Array::new();
    let mut uvs = PackedVector2Array::new();
    let mut idx = PackedInt32Array::new();
    for j in 0..n {
        for i in 0..n {
            let wx = x0 as f32 * TILE + i as f32 * step;
            let wz = y0 as f32 * TILE + j as f32 * step;
            let h = hs.at(wx, wz);
            verts.push(Vector3::new(wx, h, wz));
            norms.push(hs.normal(wx, wz));
            uvs.push(Vector2::new(wx / (hs.w as f32 * TILE), wz / (hs.h as f32 * TILE)));
        }
    }
    for j in 0..n - 1 {
        for i in 0..n - 1 {
            let a = j * n + i;
            let b = a + 1;
            let c = a + n;
            let d = c + 1;
            // alternate the diagonal to avoid directional artifacts
            if (i + j) % 2 == 0 {
                idx.extend_array(&PackedInt32Array::from(&[a, b, d, a, d, c][..]));
            } else {
                idx.extend_array(&PackedInt32Array::from(&[a, b, c, b, d, c][..]));
            }
        }
    }
    let mut arrays = VarArray::new();
    arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
    arrays.set(ArrayType::VERTEX.ord() as usize, &verts.to_variant());
    arrays.set(ArrayType::NORMAL.ord() as usize, &norms.to_variant());
    arrays.set(ArrayType::TEX_UV.ord() as usize, &uvs.to_variant());
    arrays.set(ArrayType::INDEX.ord() as usize, &idx.to_variant());
    let mut mesh = ArrayMesh::new_gd();
    mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
    mesh
}

/// Flat sea-floor apron around the map (4 quads) so the horizon is ocean.
pub fn build_apron(hs: &Heights, depth: f32, extent: f32) -> Gd<ArrayMesh> {
    let w = hs.w as f32 * TILE;
    let h = hs.h as f32 * TILE;
    let e = extent;
    let y = depth;
    let quads = [
        (-e, -e, w + e, 0.0),
        (-e, h, w + e, h + e),
        (-e, 0.0, 0.0, h),
        (w, 0.0, w + e, h),
    ];
    let mut verts = PackedVector3Array::new();
    let mut norms = PackedVector3Array::new();
    let mut idx = PackedInt32Array::new();
    for (k, (ax, az, bx, bz)) in quads.iter().enumerate() {
        let base = (k * 4) as i32;
        for (x, z) in [(*ax, *az), (*bx, *az), (*ax, *bz), (*bx, *bz)] {
            verts.push(Vector3::new(x, y, z));
            norms.push(Vector3::UP);
        }
        idx.extend_array(&PackedInt32Array::from(&[base, base + 1, base + 3, base, base + 3, base + 2][..]));
    }
    let mut arrays = VarArray::new();
    arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
    arrays.set(ArrayType::VERTEX.ord() as usize, &verts.to_variant());
    arrays.set(ArrayType::NORMAL.ord() as usize, &norms.to_variant());
    arrays.set(ArrayType::INDEX.ord() as usize, &idx.to_variant());
    let mut mesh = ArrayMesh::new_gd();
    mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
    mesh
}

fn hash2(x: i32, y: i32, s: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x27d4_eb2d) ^ (y as u32).wrapping_mul(0x1656_67b1) ^ s.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h
}

/// Two RGBA control maps (8 layer weights) at RES texels per tile, blurred.
pub const CTRL_RES: i32 = 4;

pub fn build_control_maps(map: &Map) -> (Gd<ImageTexture>, Gd<ImageTexture>) {
    let w = map.w * CTRL_RES;
    let h = map.h * CTRL_RES;
    let mut wts = vec![[0f32; 8]; (w * h) as usize];
    for ty in 0..map.h {
        for tx in 0..map.w {
            let t = map.terrain_at(tx, ty);
            let near_water = (-1..=1).any(|dy| (-1..=1).any(|dx| map.is_water(tx + dx, ty + dy)));
            let hsh = hash2(tx / 3, ty / 3, 7) % 100;
            let mut l = [0f32; 8];
            match t {
                Terrain::Beach => l[0] = 1.0,
                Terrain::Grass => {
                    l[1] = 1.0;
                    if hsh < 25 {
                        l[2] = 0.6;
                    }
                }
                Terrain::Meadow => {
                    l[2] = 1.0;
                    l[1] = 0.3;
                }
                Terrain::Forest => {
                    l[3] = 1.0;
                    l[1] = 0.2;
                }
                Terrain::Dirt => {
                    l[4] = 1.0;
                    l[1] = 0.25;
                }
                Terrain::Rock => {
                    l[5] = 1.0;
                    l[4] = 0.2;
                }
                Terrain::Mountain => l[6] = 1.0,
                Terrain::ShallowWater => {
                    l[0] = 0.5;
                    l[7] = 0.5;
                }
                Terrain::DeepWater => l[7] = 1.0,
            }
            if near_water && !map.is_water(tx, ty) && t != Terrain::Beach {
                l[0] += 0.35;
            }
            for sy in 0..CTRL_RES {
                for sx in 0..CTRL_RES {
                    let i = ((ty * CTRL_RES + sy) * w + tx * CTRL_RES + sx) as usize;
                    wts[i] = l;
                }
            }
        }
    }
    // separable box blur, 2 passes, radius 3 texels (~0.75 tile)
    let r = 3i32;
    for _ in 0..2 {
        let src = wts.clone();
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0f32; 8];
                for dx in -r..=r {
                    let xx = (x + dx).clamp(0, w - 1);
                    let s = src[(y * w + xx) as usize];
                    for k in 0..8 {
                        acc[k] += s[k];
                    }
                }
                wts[(y * w + x) as usize] = acc;
            }
        }
        let src = wts.clone();
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0f32; 8];
                for dy in -r..=r {
                    let yy = (y + dy).clamp(0, h - 1);
                    let s = src[(yy * w + x) as usize];
                    for k in 0..8 {
                        acc[k] += s[k];
                    }
                }
                wts[(y * w + x) as usize] = acc;
            }
        }
    }
    let mut a = vec![0u8; (w * h * 4) as usize];
    let mut b = vec![0u8; (w * h * 4) as usize];
    for (i, l) in wts.iter().enumerate() {
        let sum: f32 = l.iter().sum::<f32>().max(1e-5);
        for k in 0..4 {
            a[i * 4 + k] = (l[k] / sum * 255.0).round() as u8;
            b[i * 4 + k] = (l[k + 4] / sum * 255.0).round() as u8;
        }
    }
    let img_a = Image::create_from_data(w, h, false, Format::RGBA8, &PackedByteArray::from(&a[..])).unwrap();
    let img_b = Image::create_from_data(w, h, false, Format::RGBA8, &PackedByteArray::from(&b[..])).unwrap();
    (ImageTexture::create_from_image(&img_a).unwrap(), ImageTexture::create_from_image(&img_b).unwrap())
}

/// Minimap base colors per tile.
pub fn minimap_base(map: &Map) -> Vec<[u8; 3]> {
    let mut out = Vec::with_capacity((map.w * map.h) as usize);
    for y in 0..map.h {
        for x in 0..map.w {
            let hgt = map.tile_height(x, y);
            let shade = (hgt.clamp(0, 600) / 20) as i32;
            let c: [i32; 3] = match map.terrain_at(x, y) {
                Terrain::DeepWater => [18, 52, 92],
                Terrain::ShallowWater => [36, 104, 136],
                Terrain::Beach => [206, 190, 140],
                Terrain::Grass => [86, 132, 58],
                Terrain::Meadow => [110, 150, 66],
                Terrain::Forest => [44, 88, 40],
                Terrain::Dirt => [128, 112, 72],
                Terrain::Rock => [120, 116, 100],
                Terrain::Mountain => [104, 98, 92],
            };
            out.push([
                (c[0] + shade).clamp(0, 255) as u8,
                (c[1] + shade).clamp(0, 255) as u8,
                (c[2] + shade).clamp(0, 255) as u8,
            ]);
        }
    }
    out
}
