/// xorshift64*: small, seedable, and reproducible in tests and simulations.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// Uniform in 0..1.
    pub fn unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    pub fn chance(&mut self, probability: f64) -> bool {
        self.unit() < probability
    }

    pub fn below(&mut self, n: u8) -> u8 {
        ((self.unit() * f64::from(n)) as u8).min(n.saturating_sub(1))
    }
}
