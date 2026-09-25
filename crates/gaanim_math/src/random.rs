//! Seeded randomness and coherent noise shared by authoring and runtime.
//!
//! Everything here is a pure function of its seed and inputs, so a scene
//! authored in Python and evaluated in Rust produces identical values on
//! every platform, preview, seek, and export.

/// SplitMix64: a small, fast, statistically solid PRNG with a stable stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeededRng {
    state: u64,
}

impl SeededRng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)` with 53 bits of precision.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in `[low, high)`.
    pub fn uniform(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next_f64()
    }

    /// Uniform integer in `[low, high)`; `low` when the range is empty.
    pub fn integer(&mut self, low: i64, high: i64) -> i64 {
        if high <= low {
            return low;
        }
        let span = (high as i128 - low as i128) as u128;
        // Lemire's multiply-shift keeps the draw unbiased enough for authoring.
        let value = ((self.next_u64() as u128 * span) >> 64) as i128;
        (low as i128 + value) as i64
    }

    /// Normal deviate (Box–Muller, one value per call).
    pub fn gauss(&mut self, mean: f64, std_dev: f64) -> f64 {
        let u1 = self.next_f64().max(f64::MIN_POSITIVE);
        let u2 = self.next_f64();
        mean + std_dev * (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// Fisher–Yates shuffle in place.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for index in (1..items.len()).rev() {
            let other = self.integer(0, index as i64 + 1) as usize;
            items.swap(index, other);
        }
    }
}

/// Seeded 2D simplex noise with fractal Brownian motion.
#[derive(Debug, Clone, PartialEq)]
pub struct Noise {
    permutation: [u8; 512],
    pub frequency: f64,
    pub amplitude: f64,
    pub octaves: u32,
    /// Frequency multiplier between octaves.
    pub lacunarity: f64,
    /// Amplitude multiplier between octaves.
    pub gain: f64,
}

impl Noise {
    pub fn new(seed: u64, frequency: f64, amplitude: f64, octaves: u32) -> Self {
        let mut table: [u8; 256] = std::array::from_fn(|index| index as u8);
        SeededRng::new(seed).shuffle(&mut table);
        let permutation = std::array::from_fn(|index| table[index & 255]);
        Self {
            permutation,
            frequency,
            amplitude,
            octaves: octaves.max(1),
            lacunarity: 2.0,
            gain: 0.5,
        }
    }

    /// fBm sample at `(x, y)`, within `[-amplitude, amplitude]`.
    pub fn sample(&self, x: f64, y: f64) -> f64 {
        let mut total = 0.0;
        let mut weight = 1.0;
        let mut norm = 0.0;
        let mut frequency = self.frequency;
        for _ in 0..self.octaves {
            total += weight * self.simplex(x * frequency, y * frequency);
            norm += weight;
            weight *= self.gain;
            frequency *= self.lacunarity;
        }
        self.amplitude * total / norm
    }

    /// Noise as a function of time; `channel` decorrelates parallel signals.
    pub fn at_time(&self, time: f64, channel: u32) -> f64 {
        self.sample(time, channel as f64 * 31.7 + 0.5)
    }

    fn simplex(&self, x: f64, y: f64) -> f64 {
        const F2: f64 = 0.366_025_403_784_438_6; // (sqrt(3) - 1) / 2
        const G2: f64 = 0.211_324_865_405_187_1; // (3 - sqrt(3)) / 6
        let skew = (x + y) * F2;
        let i = (x + skew).floor();
        let j = (y + skew).floor();
        let unskew = (i + j) * G2;
        let x0 = x - (i - unskew);
        let y0 = y - (j - unskew);
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - i1 as f64 + G2;
        let y1 = y0 - j1 as f64 + G2;
        let x2 = x0 - 1.0 + 2.0 * G2;
        let y2 = y0 - 1.0 + 2.0 * G2;
        let ii = (i as i64).rem_euclid(256) as usize;
        let jj = (j as i64).rem_euclid(256) as usize;
        let hash = |di: usize, dj: usize| {
            self.permutation[ii + di + self.permutation[jj + dj] as usize] as usize
        };
        let corner = |hash: usize, x: f64, y: f64| {
            let t = 0.5 - x * x - y * y;
            if t <= 0.0 {
                return 0.0;
            }
            const GRADIENTS: [(f64, f64); 8] = [
                (1.0, 1.0),
                (-1.0, 1.0),
                (1.0, -1.0),
                (-1.0, -1.0),
                (1.0, 0.0),
                (-1.0, 0.0),
                (0.0, 1.0),
                (0.0, -1.0),
            ];
            let (gx, gy) = GRADIENTS[hash & 7];
            let t2 = t * t;
            t2 * t2 * (gx * x + gy * y)
        };
        let n =
            corner(hash(0, 0), x0, y0) + corner(hash(i1, j1), x1, y1) + corner(hash(1, 1), x2, y2);
        // Scales the theoretical peak (~0.0141) to roughly [-1, 1].
        (70.0 * n).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_rng_is_reproducible_and_in_range() {
        let mut a = SeededRng::new(42);
        let mut b = SeededRng::new(42);
        for _ in 0..1000 {
            let value = a.uniform(-2.0, 3.0);
            assert_eq!(value, b.uniform(-2.0, 3.0));
            assert!((-2.0..3.0).contains(&value));
            let integer = a.integer(-5, 5);
            assert_eq!(integer, b.integer(-5, 5));
            assert!((-5..5).contains(&integer));
        }
        assert_ne!(SeededRng::new(1).next_u64(), SeededRng::new(2).next_u64());
        let mut items: Vec<u32> = (0..20).collect();
        SeededRng::new(7).shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..20).collect::<Vec<_>>());
        assert_ne!(items, sorted);
        let mean = (0..20_000).map(|_| a.gauss(1.0, 2.0)).sum::<f64>() / 20_000.0;
        assert!((mean - 1.0).abs() < 0.08);
    }

    #[test]
    fn noise_is_seeded_bounded_and_continuous() {
        let noise = Noise::new(5, 0.6, 0.3, 3);
        assert_eq!(noise, Noise::new(5, 0.6, 0.3, 3));
        assert_ne!(
            noise.at_time(1.3, 0),
            Noise::new(6, 0.6, 0.3, 3).at_time(1.3, 0)
        );
        let mut previous = noise.at_time(0.0, 0);
        let mut spread = (f64::MAX, f64::MIN);
        for step in 1..=4000 {
            let value = noise.at_time(step as f64 * 0.005, 0);
            assert!(value.abs() <= 0.3 + 1e-12);
            assert!((value - previous).abs() < 0.05, "jump at step {step}");
            spread = (spread.0.min(value), spread.1.max(value));
            previous = value;
        }
        assert!(spread.1 - spread.0 > 0.2, "noise should vary: {spread:?}");
        assert_ne!(noise.at_time(2.0, 0), noise.at_time(2.0, 1));
    }
}
