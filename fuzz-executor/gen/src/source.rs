//! Where the generators draw their choices from: a seeded generator for the Mollusk suite, so a
//! failing case replays from its seed, or the bytes libFuzzer mutates, so coverage guidance can
//! steer every decision.

/// A stream of random choices.
pub trait Source {
    fn next_u64(&mut self) -> u64;

    /// One small choice. A byte source spends one byte on it, so most decisions cost one byte.
    fn next_u8(&mut self) -> u8 {
        self.next_u64() as u8
    }
}

/// SplitMix64: tiny, fast and good enough for test generation. A case's seed replays it exactly.
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl Source for SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// Choices read from a byte string, front to back, then zeros once it runs out. libFuzzer's
/// mutations of one byte change one decision, which keeps its coverage feedback local.
#[derive(Clone, Debug)]
pub struct ByteSource<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> ByteSource<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    /// Bytes drawn so far, including any past the end.
    pub fn consumed(&self) -> usize {
        self.position
    }
}

impl Source for ByteSource<'_> {
    fn next_u8(&mut self) -> u8 {
        let byte = self.data.get(self.position).copied().unwrap_or(0);
        self.position += 1;
        byte
    }

    fn next_u64(&mut self) -> u64 {
        let mut bytes = [0u8; 8];
        for byte in &mut bytes {
            *byte = self.next_u8();
        }
        u64::from_le_bytes(bytes)
    }
}

/// Convenience draws over a [`Source`].
pub struct Gen<'s> {
    source: &'s mut dyn Source,
}

impl<'s> Gen<'s> {
    pub fn new(source: &'s mut dyn Source) -> Self {
        Self { source }
    }

    pub fn u8(&mut self) -> u8 {
        self.source.next_u8()
    }

    pub fn u64(&mut self) -> u64 {
        self.source.next_u64()
    }

    /// A value in `0..bound`, or 0 for an empty range.
    pub fn below(&mut self, bound: usize) -> usize {
        match bound {
            0 | 1 => 0,
            2..=256 => self.source.next_u8() as usize % bound,
            _ => (self.source.next_u64() % bound as u64) as usize,
        }
    }

    /// A value in `low..=high`.
    pub fn range(&mut self, low: usize, high: usize) -> usize {
        if high <= low {
            low
        } else {
            low + self.below(high - low + 1)
        }
    }

    /// True `numerator` times in `denominator` (at most 256).
    pub fn chance(&mut self, numerator: u32, denominator: u32) -> bool {
        (self.source.next_u8() as u32 % denominator.max(1)) < numerator
    }

    pub fn pick<T: Clone>(&mut self, items: &[T]) -> Option<T> {
        if items.is_empty() {
            None
        } else {
            Some(items[self.below(items.len())].clone())
        }
    }

    /// Picks an index by weight.
    pub fn weighted(&mut self, weights: &[u32]) -> usize {
        let total: u32 = weights.iter().sum();
        if total == 0 {
            return 0;
        }
        let mut roll = (self.source.next_u64() % total as u64) as u32;
        for (index, weight) in weights.iter().enumerate() {
            if roll < *weight {
                return index;
            }
            roll -= weight;
        }
        weights.len() - 1
    }

    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.source.next_u8()).collect()
    }

    /// A `u64` that is often at a boundary: zero, one, small, a power of two, near the maximum.
    pub fn interesting_u64(&mut self) -> u64 {
        match self.below(10) {
            0 => 0,
            1 => 1,
            2 | 3 => self.below(17) as u64,
            4 => 1u64 << self.below(64),
            5 => u64::MAX - self.below(3) as u64,
            6 => (1u64 << self.below(64)).wrapping_sub(1),
            7 => self.below(1_000_000) as u64,
            _ => self.source.next_u64(),
        }
    }

    pub fn interesting_i64(&mut self) -> i64 {
        match self.below(8) {
            0 => 0,
            1 => -1,
            2 => i64::MIN,
            3 => i64::MAX,
            4 | 5 => self.below(33) as i64 - 16,
            _ => self.source.next_u64() as i64,
        }
    }

    pub fn interesting_u128(&mut self) -> u128 {
        match self.below(6) {
            0 => 0,
            1 => u128::MAX,
            2 => 1u128 << self.below(128),
            3 => self.interesting_u64() as u128,
            _ => (self.source.next_u64() as u128) << 64 | self.source.next_u64() as u128,
        }
    }
}
