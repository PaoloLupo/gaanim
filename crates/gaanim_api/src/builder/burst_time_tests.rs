use super::*;

#[test]
fn repeated_bursts_fire_at_every_cycle_start() {
    assert_eq!(burst_times(2.0, 0.5, &RateFunc::Linear), [2.0]);
    let repeat = |mode| RateFunc::Repeat {
        inner: Box::new(RateFunc::Smooth),
        count: 3,
        gap: 0.5,
        mode,
    };
    // Three 0.4 s cycles separated by 0.2 s gaps span 1.6 s.
    for mode in [
        gaanim_math::RepeatMode::Cycle,
        gaanim_math::RepeatMode::PingPong,
    ] {
        let times = burst_times(1.0, 1.6, &repeat(mode));
        assert_eq!(times.len(), 3);
        for (time, expected) in times.iter().zip([1.0, 1.6, 2.2]) {
            assert!((time - expected).abs() < 1e-12, "{times:?}");
        }
    }
}
