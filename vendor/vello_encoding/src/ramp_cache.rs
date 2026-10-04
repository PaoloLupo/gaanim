// Copyright 2022 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use peniko::color::cache_key::CacheKey;
use peniko::color::Srgb;
use peniko::{ColorStop, ColorStops};

const N_SAMPLES: usize = 512;
/// Gaanim patch: samples computed together by [`write_ramp`].
const LANES: usize = 8;
/// Gaanim patch: the position of each sample along the ramp, computed once.
static SAMPLE_U: [f32; N_SAMPLES] = {
    let mut positions = [0.0; N_SAMPLES];
    let mut i = 0;
    while i < N_SAMPLES {
        positions[i] = (i as f32) / (N_SAMPLES - 1) as f32;
        i += 1;
    }
    positions
};
const RETAINED_COUNT: usize = 64;
/// Gaanim patch: renders a ramp may go unused and still be kept once the
/// cache holds more than [`RETAINED_COUNT`]. A frame can take several
/// renders (shader effects, transitions), so this spans a few frames.
const RETAINED_EPOCHS: u64 = 8;
/// Gaanim patch: rows beyond which a render reuses the rows of ramps it has
/// not drawn instead of adding new ones. Every row is uploaded on every
/// render, and the rows must fit in a texture's height.
const RETAINED_ROWS: usize = 1024;

/// Data and dimensions for a set of resolved gradient ramps.
#[derive(Copy, Clone, Debug, Default)]
pub struct Ramps<'a> {
    pub data: &'a [u32],
    pub width: u32,
    pub height: u32,
}

/// Gaanim patch: upstream keeps only the first [`RETAINED_COUNT`] ramps
/// between renders and recomputes every other one on every render, so a
/// scene with more gradients than that (a smooth-shaded 3D mesh has one per
/// triangle) rebuilt thousands of ramps per frame. Ramps now stay while a
/// recent render used them, and the rows of dropped ramps are reused.
#[derive(Default)]
pub(crate) struct RampCache {
    epoch: u64,
    map: HashMap<CacheKey<ColorStops>, (u32, u64), BuildHasherDefault<FxHasher>>,
    data: Vec<u32>,
    /// Rows of `data` that no ramp uses.
    free: Vec<u32>,
    /// The epoch whose undrawn ramps were last released into `free`.
    released: u64,
}

/// Gaanim patch: FxHash, rustc's hasher, for the ramp map. A lit 3D mesh
/// looks up thousands of ramps per render, and SipHash took most of the
/// time spent outside sampling. Ramps are colors, not untrusted keys, and
/// which row a ramp lands in does not change what is drawn.
#[derive(Default)]
struct FxHasher(u64);

impl FxHasher {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.add(value.into());
    }

    fn write_u16(&mut self, value: u16) {
        self.add(value.into());
    }

    fn write_u32(&mut self, value: u32) {
        self.add(value.into());
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

impl RampCache {
    pub(crate) fn maintain(&mut self) {
        self.epoch += 1;
        if self.map.len() > RETAINED_COUNT {
            let epoch = self.epoch;
            let free = &mut self.free;
            self.map.retain(|_key, &mut (id, used)| {
                let keep = used + RETAINED_EPOCHS >= epoch;
                if !keep {
                    free.push(id);
                }
                keep
            });
        }
        // Free rows at the end of the data go away, so the uploaded ramps
        // shrink with the scene.
        self.free.sort_unstable();
        while self
            .free
            .last()
            .is_some_and(|&id| (id as usize + 1) * N_SAMPLES == self.data.len())
        {
            self.free.pop();
            self.data.truncate(self.data.len() - N_SAMPLES);
        }
    }

    pub(crate) fn add(&mut self, stops: &[ColorStop]) -> u32 {
        if let Some(entry) = self.map.get_mut(&CacheKey(stops.into())) {
            entry.1 = self.epoch;
            return entry.0;
        }
        if self.free.is_empty()
            && self.data.len() >= RETAINED_ROWS * N_SAMPLES
            && self.released != self.epoch
        {
            // Colors that change every frame (shaded 3D) would otherwise
            // keep adding rows for the retained epochs.
            self.released = self.epoch;
            let epoch = self.epoch;
            let free = &mut self.free;
            self.map.retain(|_key, &mut (id, used)| {
                if used < epoch {
                    free.push(id);
                }
                used == epoch
            });
        }
        let id = self.free.pop().unwrap_or_else(|| {
            let id = (self.data.len() / N_SAMPLES) as u32;
            self.data.resize(self.data.len() + N_SAMPLES, 0);
            id
        });
        let start = id as usize * N_SAMPLES;
        write_ramp(stops, &mut self.data[start..start + N_SAMPLES]);
        self.map.insert(CacheKey(stops.into()), (id, self.epoch));
        id
    }

    pub(crate) fn ramps(&self) -> Ramps<'_> {
        Ramps {
            data: &self.data,
            width: N_SAMPLES as u32,
            height: (self.data.len() / N_SAMPLES) as u32,
        }
    }
}

/// `x` in [0, 1] as a byte: `(x * 255. + 0.5).clamp(0.0, 255.0) as u8`.
/// `max` and `min` map NaN to 0 as that cast does, and the clamped value
/// then converts without the cast's saturation checks, so lanes of samples
/// vectorize.
#[inline(always)]
fn byte(x: f32) -> u8 {
    let clamped = (x * 255. + 0.5).max(0.0).min(255.0);
    // SAFETY: `clamped` is finite and within [0, 255].
    unsafe { clamped.to_int_unchecked::<i32>() as u8 }
}

/// Write the ramp of `stops` into `out`, which holds [`N_SAMPLES`] values.
///
/// Gaanim patch: upstream evaluates every sample through
/// `AlphaColor::lerp`, which premultiplies both ends of the segment again
/// for each sample, inside an iterator that also looks for the next stop.
/// Here each run of samples between two stops is a plain loop over the
/// premultiplied ends, which the compiler vectorizes. sRGB has no hue
/// channel to fix up, so every sample is computed exactly as upstream does.
fn write_ramp(stops: &[ColorStop], out: &mut [u32]) {
    let sample_u = |i: usize| SAMPLE_U[i];
    let mut last_u = 0.0;
    let mut this_u = last_u;
    let mut this_c = stops[0].color.to_alpha_color::<Srgb>();
    let mut last_p = this_c.premultiply();
    let mut this_p = last_p;
    let mut j = 0;
    let mut i = 0;
    while i < N_SAMPLES {
        let u = sample_u(i);
        while u > this_u {
            last_u = this_u;
            last_p = this_p;
            if let Some(s) = stops.get(j + 1) {
                this_u = s.offset;
                this_c = s.color.to_alpha_color::<Srgb>();
                this_p = this_c.premultiply();
                j += 1;
            } else {
                break;
            }
        }
        // The samples up to `this_u` share this segment; past the last
        // stop, every remaining one does.
        let end = if u > this_u {
            N_SAMPLES
        } else {
            // The first sample past `this_u`: estimated, then settled
            // against the exact sample positions.
            let estimate = (this_u * (N_SAMPLES - 1) as f32) as usize;
            let mut end = estimate.clamp(i + 1, N_SAMPLES);
            while end > i + 1 && sample_u(end - 1) > this_u {
                end -= 1;
            }
            while end < N_SAMPLES && sample_u(end) <= this_u {
                end += 1;
            }
            end
        };
        let du = this_u - last_u;
        if du < 1e-9 {
            out[i..end].fill(this_c.premultiply().to_rgba8().to_u32());
        } else {
            // `last_p.lerp_rect(this_p, t).un_premultiply().premultiply()
            // .to_rgba8().to_u32()`, spelled out with the same operations so
            // that lanes of samples vectorize.
            let from = last_p.components;
            let span = (this_p - last_p).components;
            let t_at = |u: f32| (u - last_u) / du;
            if from[3] == 1.0 && span[3] == 0.0 {
                // Opaque: alpha is exactly 1 at every sample, so
                // un-premultiplying and premultiplying again multiply by 1.
                for (value, &u) in out[i..end].iter_mut().zip(&SAMPLE_U[i..end]) {
                    let t = t_at(u);
                    *value = u32::from_ne_bytes([
                        byte(from[0] + span[0] * t),
                        byte(from[1] + span[1] * t),
                        byte(from[2] + span[2] * t),
                        255,
                    ]);
                }
                i = end;
                continue;
            }
            let mut start = i;
            for chunk in out[i..end].chunks_mut(LANES) {
                let mut t = [0.0_f32; LANES];
                for (t, &u) in t.iter_mut().zip(&SAMPLE_U[start..]) {
                    *t = t_at(u);
                }
                let channel = |c: usize| t.map(|t| from[c] + span[c] * t);
                let (r, g, b, a) = (channel(0), channel(1), channel(2), channel(3));
                let scale = a.map(|alpha| if alpha == 0.0 { 1.0 } else { 1.0 / alpha });
                for (lane, value) in chunk.iter_mut().enumerate() {
                    let (scale, alpha) = (scale[lane], a[lane]);
                    *value = u32::from_ne_bytes([
                        byte(r[lane] * scale * alpha),
                        byte(g[lane] * scale * alpha),
                        byte(b[lane] * scale * alpha),
                        byte(alpha),
                    ]);
                }
                start += LANES;
            }
        }
        i = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peniko::color::{AlphaColor, HueDirection};

    fn stops(colors: &[[f32; 4]]) -> Vec<ColorStop> {
        let last = (colors.len() - 1).max(1) as f32;
        colors
            .iter()
            .enumerate()
            .map(|(index, &components)| ColorStop {
                offset: index as f32 / last,
                color: AlphaColor::<Srgb>::new(components).into(),
            })
            .collect()
    }

    /// Upstream's sampling, which the patched ramp must reproduce bit for bit.
    fn upstream_ramp(stops: &[ColorStop]) -> Vec<u32> {
        let mut last_u = 0.0;
        let mut last_c = stops[0].color.to_alpha_color::<Srgb>();
        let mut this_u = last_u;
        let mut this_c = last_c;
        let mut j = 0;
        (0..N_SAMPLES)
            .map(|i| {
                let u = (i as f32) / (N_SAMPLES - 1) as f32;
                while u > this_u {
                    last_u = this_u;
                    last_c = this_c;
                    if let Some(s) = stops.get(j + 1) {
                        this_u = s.offset;
                        this_c = s.color.to_alpha_color::<Srgb>();
                        j += 1;
                    } else {
                        break;
                    }
                }
                let du = this_u - last_u;
                let c = if du < 1e-9 {
                    this_c
                } else {
                    last_c.lerp(this_c, (u - last_u) / du, HueDirection::default())
                };
                c.premultiply().to_rgba8().to_u32()
            })
            .collect()
    }

    #[test]
    fn random_ramps_match_upstream_sampling() {
        // A small LCG: reproducible stops of every kind, including opaque
        // ones, alpha 0, values outside [0, 1] and repeated offsets.
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move || {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (state >> 40) as f32 / (1u64 << 24) as f32
        };
        let mut ramp = vec![0; N_SAMPLES];
        for case in 0..4000 {
            let count = 1 + case % 4;
            let mut offsets: Vec<f32> = (0..count).map(|_| next()).collect();
            offsets.sort_by(f32::total_cmp);
            if case % 3 == 0 {
                offsets[0] = 0.0;
                *offsets.last_mut().unwrap() = 1.0;
            }
            let stops: Vec<ColorStop> = offsets
                .iter()
                .map(|&offset| {
                    let mut components = [next(), next(), next(), next()];
                    match case % 5 {
                        0 | 1 => components[3] = 1.0,
                        2 if next() < 0.3 => components[3] = 0.0,
                        3 => components[0] = next() * 1.4 - 0.2,
                        _ => {}
                    }
                    ColorStop {
                        offset,
                        color: AlphaColor::<Srgb>::new(components).into(),
                    }
                })
                .collect();
            write_ramp(&stops, &mut ramp);
            assert_eq!(ramp, upstream_ramp(&stops), "stops {stops:?}");
        }
    }

    #[test]
    fn ramps_match_upstream_sampling() {
        for colors in [
            &[[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]][..],
            &[[0.2, 0.7, 0.1, 0.0], [0.9, 0.3, 0.5, 0.6], [0.1, 0.1, 0.1, 1.0]],
            &[[0.33, 0.66, 0.99, 0.25], [0.5, 0.5, 0.5, 0.5]],
        ] {
            let stops = stops(colors);
            let mut ramp = vec![0; N_SAMPLES];
            write_ramp(&stops, &mut ramp);
            assert_eq!(ramp, upstream_ramp(&stops));
        }
        // Stops that start late, repeat an offset or end early.
        let uneven = [(0.2, [1.0, 0.5, 0.0, 1.0]), (0.2, [0.0, 0.0, 0.0, 0.0]), (0.5, [0.1, 0.9, 0.3, 0.7]), (0.75, [0.6, 0.2, 0.8, 0.9])];
        let stops: Vec<ColorStop> = uneven
            .iter()
            .map(|&(offset, components)| ColorStop {
                offset,
                color: AlphaColor::<Srgb>::new(components).into(),
            })
            .collect();
        let mut ramp = vec![0; N_SAMPLES];
        write_ramp(&stops, &mut ramp);
        assert_eq!(ramp, upstream_ramp(&stops));
        let single = &stops[2..3];
        write_ramp(single, &mut ramp);
        assert_eq!(ramp, upstream_ramp(single));
    }

    #[test]
    #[ignore = "timing; run with --release --ignored --nocapture"]
    fn ramp_timing() {
        for (name, alpha) in [("opaque", 1.0), ("translucent", 0.9)] {
            let all: Vec<Vec<ColorStop>> = (0..20_000)
                .map(|index| stops(&[[index as f32 / 20_000.0, 0.3, 0.6, alpha], [0.1, 0.8, 0.2, 1.0]]))
                .collect();
            let mut ramp = vec![0; N_SAMPLES];
            let started = std::time::Instant::now();
            for stops in &all {
                write_ramp(stops, &mut ramp);
            }
            let patched = started.elapsed();
            let started = std::time::Instant::now();
            let mut sum = 0_u64;
            for stops in &all {
                sum += u64::from(upstream_ramp(stops)[100]);
            }
            let upstream = started.elapsed();
            println!("{name} ramp: patched {:?}, upstream {:?} ({sum})", patched / 20_000, upstream / 20_000);
        }
    }

    #[test]
    fn many_ramps_survive_between_renders() {
        let mut cache = RampCache::default();
        let all: Vec<Vec<ColorStop>> = (0..200)
            .map(|index| stops(&[[index as f32 / 200.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]]))
            .collect();
        cache.maintain();
        let ids: Vec<u32> = all.iter().map(|stops| cache.add(stops)).collect();
        cache.maintain();
        let rows = cache.ramps().height;
        for (stops, id) in all.iter().zip(&ids) {
            assert_eq!(cache.add(stops), *id, "a ramp used last render is kept");
        }
        assert_eq!(cache.ramps().height, rows, "no ramp was computed again");
    }

    #[test]
    fn ramps_that_change_every_render_do_not_add_rows() {
        let mut cache = RampCache::default();
        let mut highest = 0;
        for render in 0..20 {
            cache.maintain();
            for index in 0..1500 {
                let red = (render * 1500 + index) as f32 / 30_000.0;
                cache.add(&stops(&[[red, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]]));
            }
            highest = highest.max(cache.ramps().height);
        }
        assert!(highest <= 1500 + RETAINED_ROWS as u32, "{highest} rows");
    }

    #[test]
    fn unused_ramps_free_their_rows() {
        let mut cache = RampCache::default();
        let first: Vec<Vec<ColorStop>> = (0..100)
            .map(|index| stops(&[[index as f32 / 100.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]]))
            .collect();
        cache.maintain();
        for stops in &first {
            cache.add(stops);
        }
        let rows = cache.ramps().height;
        for _ in 0..=RETAINED_EPOCHS {
            cache.maintain();
        }
        assert_eq!(cache.ramps().height, 0, "ramps unused for a while are dropped");
        let second: Vec<Vec<ColorStop>> = (0..100)
            .map(|index| stops(&[[0.0, index as f32 / 100.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]]))
            .collect();
        for stops in &second {
            cache.add(stops);
        }
        assert_eq!(cache.ramps().height, rows);
        let id = cache.add(&second[7]) as usize;
        let ramps = cache.ramps();
        let row = &ramps.data[id * N_SAMPLES..(id + 1) * N_SAMPLES];
        assert_eq!(row, upstream_ramp(&second[7]).as_slice());
    }
}
