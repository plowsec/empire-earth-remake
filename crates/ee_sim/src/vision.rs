//! Fog of war: per-player visibility grids, part of the sim so the AI plays fair.
use crate::mapgen::GAIA;
use crate::world::{data, World};

impl World {
    pub(crate) fn update_vision(&mut self) {
        let w = self.map.w;
        let h = self.map.h;
        for v in self.vision.iter_mut() {
            for c in v.iter_mut() {
                if *c == 2 {
                    *c = 1;
                }
            }
        }
        let np = self.players.len();
        for e in &self.entities {
            if !e.alive || e.owner == GAIA || (e.owner as usize) >= np {
                continue;
            }
            let d = data().def(e.def);
            if d.is_resource() {
                continue;
            }
            if e.inside != 0 {
                continue;
            }
            let m = self.players[e.owner as usize].mods[e.def as usize];
            let mut r = d.sight_tiles * (100 + m.sight_pct) / 100;
            if d.is_building() && !e.complete {
                r = 2;
            }
            let (cx, cy) = e.pos.tile();
            let rr = r * r + r;
            let v = &mut self.vision[e.owner as usize];
            let y0 = (cy - r).max(0);
            let y1 = (cy + r).min(h - 1);
            for y in y0..=y1 {
                let dy = y - cy;
                // half-width of the circle on this row
                let mut hw = 0;
                while (hw + 1) * (hw + 1) + dy * dy <= rr {
                    hw += 1;
                }
                let x0 = (cx - hw).max(0);
                let x1 = (cx + hw).min(w - 1);
                let row = (y * w) as usize;
                for x in x0..=x1 {
                    v[row + x as usize] = 2;
                }
            }
        }
        // allies share vision
        if np > 1 {
            for a in 0..np {
                for b in 0..np {
                    if a != b && self.players[a].team == self.players[b].team {
                        let (src, dst) = if a < b {
                            let (l, r) = self.vision.split_at_mut(b);
                            (&r[0], &mut l[a])
                        } else {
                            let (l, r) = self.vision.split_at_mut(a);
                            (&l[b], &mut r[0])
                        };
                        for (d, s) in dst.iter_mut().zip(src.iter()) {
                            if *s == 2 {
                                *d = 2;
                            } else if *s == 1 && *d == 0 {
                                *d = 1;
                            }
                        }
                    }
                }
            }
        }
    }
}
