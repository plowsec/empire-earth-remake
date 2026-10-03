//! 16.16 fixed-point math. The simulation never touches floats, so every peer
//! computes bit-identical results regardless of CPU or compiler.
use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

pub const FRAC_BITS: i32 = 16;
pub const ONE: i32 = 1 << FRAC_BITS;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Fx(pub i32);

impl Fx {
    pub const ZERO: Fx = Fx(0);
    pub const ONE: Fx = Fx(ONE);
    pub const HALF: Fx = Fx(ONE / 2);
    #[inline]
    pub const fn from_int(v: i32) -> Fx {
        Fx(v << FRAC_BITS)
    }
    /// `num / den` as fixed point (e.g. centi-tiles: `from_ratio(150, 100)` = 1.5).
    #[inline]
    pub const fn from_ratio(num: i32, den: i32) -> Fx {
        Fx((((num as i64) << FRAC_BITS) / den as i64) as i32)
    }
    #[inline]
    pub const fn floor_int(self) -> i32 {
        self.0 >> FRAC_BITS
    }
    #[inline]
    pub fn mul(self, o: Fx) -> Fx {
        Fx(((self.0 as i64 * o.0 as i64) >> FRAC_BITS) as i32)
    }
    #[inline]
    pub fn div(self, o: Fx) -> Fx {
        if o.0 == 0 {
            return Fx(if self.0 >= 0 { i32::MAX } else { i32::MIN });
        }
        Fx((((self.0 as i64) << FRAC_BITS) / o.0 as i64) as i32)
    }
    #[inline]
    pub fn mul_int(self, v: i32) -> Fx {
        Fx(self.0.wrapping_mul(v))
    }
    #[inline]
    pub fn abs(self) -> Fx {
        Fx(self.0.abs())
    }
    #[inline]
    pub fn min(self, o: Fx) -> Fx {
        if self.0 < o.0 { self } else { o }
    }
    #[inline]
    pub fn max(self, o: Fx) -> Fx {
        if self.0 > o.0 { self } else { o }
    }
    /// Render-side conversion only. Never feed the result back into the sim.
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / ONE as f32
    }
}

impl Add for Fx {
    type Output = Fx;
    #[inline]
    fn add(self, o: Fx) -> Fx {
        Fx(self.0.wrapping_add(o.0))
    }
}
impl Sub for Fx {
    type Output = Fx;
    #[inline]
    fn sub(self, o: Fx) -> Fx {
        Fx(self.0.wrapping_sub(o.0))
    }
}
impl Neg for Fx {
    type Output = Fx;
    #[inline]
    fn neg(self) -> Fx {
        Fx(-self.0)
    }
}
impl AddAssign for Fx {
    fn add_assign(&mut self, o: Fx) {
        self.0 = self.0.wrapping_add(o.0)
    }
}
impl SubAssign for Fx {
    fn sub_assign(&mut self, o: Fx) {
        self.0 = self.0.wrapping_sub(o.0)
    }
}

/// Integer square root (floor) of a u64.
pub fn isqrt_u64(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = 1u64 << ((64 - n.leading_zeros() + 1) / 2);
    loop {
        let y = (x + n / x) >> 1;
        if y >= x {
            break;
        }
        x = y;
    }
    while x * x > n {
        x -= 1;
    }
    x
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FVec {
    pub x: Fx,
    pub y: Fx,
}

impl FVec {
    pub const ZERO: FVec = FVec { x: Fx(0), y: Fx(0) };
    #[inline]
    pub const fn new(x: Fx, y: Fx) -> FVec {
        FVec { x, y }
    }
    /// Center of a tile.
    #[inline]
    pub fn tile_center(tx: i32, ty: i32) -> FVec {
        FVec::new(Fx(tx * ONE + ONE / 2), Fx(ty * ONE + ONE / 2))
    }
    #[inline]
    pub fn tile(self) -> (i32, i32) {
        (self.x.floor_int(), self.y.floor_int())
    }
    /// Squared length in raw units (i64, exact).
    #[inline]
    pub fn len2_raw(self) -> i64 {
        let x = self.x.0 as i64;
        let y = self.y.0 as i64;
        x * x + y * y
    }
    #[inline]
    pub fn len(self) -> Fx {
        Fx(isqrt_u64(self.len2_raw() as u64) as i32)
    }
    #[inline]
    pub fn dist(self, o: FVec) -> Fx {
        (self - o).len()
    }
    #[inline]
    pub fn dist2_raw(self, o: FVec) -> i64 {
        (self - o).len2_raw()
    }
    /// True if within `r` (exact, no sqrt).
    #[inline]
    pub fn within(self, o: FVec, r: Fx) -> bool {
        let rr = r.0 as i64;
        self.dist2_raw(o) <= rr * rr
    }
    /// Scale to the given length (returns zero for zero vectors).
    pub fn with_len(self, l: Fx) -> FVec {
        let cur = self.len();
        if cur.0 == 0 {
            return FVec::ZERO;
        }
        FVec::new(
            Fx(((self.x.0 as i64 * l.0 as i64) / cur.0 as i64) as i32),
            Fx(((self.y.0 as i64 * l.0 as i64) / cur.0 as i64) as i32),
        )
    }
    pub fn normalized(self) -> FVec {
        self.with_len(Fx::ONE)
    }
    #[inline]
    pub fn scale(self, s: Fx) -> FVec {
        FVec::new(self.x.mul(s), self.y.mul(s))
    }
    #[inline]
    pub fn dot_raw(self, o: FVec) -> i64 {
        self.x.0 as i64 * o.x.0 as i64 + self.y.0 as i64 * o.y.0 as i64
    }
    /// Move toward `target` by at most `step`; returns (new_pos, arrived).
    pub fn step_toward(self, target: FVec, step: Fx) -> (FVec, bool) {
        let d = target - self;
        if d.within(FVec::ZERO, step) {
            (target, true)
        } else {
            (self + d.with_len(step), false)
        }
    }
}

impl Add for FVec {
    type Output = FVec;
    #[inline]
    fn add(self, o: FVec) -> FVec {
        FVec::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for FVec {
    type Output = FVec;
    #[inline]
    fn sub(self, o: FVec) -> FVec {
        FVec::new(self.x - o.x, self.y - o.y)
    }
}
impl AddAssign for FVec {
    fn add_assign(&mut self, o: FVec) {
        self.x += o.x;
        self.y += o.y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isqrt_exact() {
        for n in 0..10_000u64 {
            let r = isqrt_u64(n);
            assert!(r * r <= n && (r + 1) * (r + 1) > n);
        }
        assert_eq!(isqrt_u64(u64::MAX >> 2), 2147483647);
    }
    #[test]
    fn vec_len() {
        let v = FVec::new(Fx::from_int(3), Fx::from_int(4));
        assert_eq!(v.len(), Fx::from_int(5));
        let s = FVec::ZERO.step_toward(v, Fx::from_int(1)).0;
        assert_eq!(s.len().floor_int(), 0); // ~0.99999 due to floor
    }
}
