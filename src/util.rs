pub(crate) fn lerp(lo: f32, hi: f32, t: f32) -> f32 {
    assert!(lo < hi);
    // assert!((0.0..=1.0).contains(&t));
    // lo * (1.0 - t) + hi * t
    lo + (hi - lo) * t
}

pub(crate) fn inv_lerp(lo: f32, hi: f32, x: f32) -> f32 {
    assert!(lo < hi);
    // assert!((lo..=hi).contains(&x));
    (x - lo) / (hi - lo)
}
