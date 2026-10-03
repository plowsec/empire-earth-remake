//! PCG32: tiny, fast, deterministic. Part of the world state.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SimRng {
    state: u64,
    inc: u64,
}

impl SimRng {
    pub fn new(seed: u64, stream: u64) -> SimRng {
        let mut r = SimRng { state: 0, inc: (stream << 1) | 1 };
        r.next_u32();
        r.state = r.state.wrapping_add(seed);
        r.next_u32();
        r
    }
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6364136223846793005).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }
    /// Uniform in [0, n). n must be > 0.
    pub fn below(&mut self, n: u32) -> u32 {
        ((self.next_u32() as u64 * n as u64) >> 32) as u32
    }
    /// Uniform in [lo, hi] inclusive.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.below((hi - lo + 1) as u32) as i32
    }
    /// True with probability pct/100.
    pub fn chance(&mut self, pct: u32) -> bool {
        self.below(100) < pct
    }
    pub fn state_hash(&self) -> u64 {
        self.state ^ self.inc.rotate_left(17)
    }
}
