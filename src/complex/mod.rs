mod cameras;
mod domain;
pub(crate) mod fixed;
mod square;
mod window;

pub(crate) use cameras::{Camera, CameraMap, Pixel};
pub(crate) use domain::Domain;
// pub(crate) use fixed::{Fixed, Imag, Real};
pub(crate) use square::Square;
pub(crate) use window::Window;

pub(crate) fn lerp(lo: f64, hi: f64, t: f64) -> f64 {
    assert!(lo < hi);
    // assert!((0.0..=1.0).contains(&t));
    // lo * (1.0 - t) + hi * t
    lo + (hi - lo) * t
}

pub(crate) fn inv_lerp(lo: f64, hi: f64, x: f64) -> f64 {
    assert!(lo < hi);
    // assert!((lo..=hi).contains(&x));
    (x - lo) / (hi - lo)
}
