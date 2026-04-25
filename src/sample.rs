use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use eframe::egui::Color32;

use crate::complex::{Window, fixed::*};

// pub(crate) static SAMPLE_ELAPSED_NANOS: AtomicU64 = AtomicU64::new(0);
// pub(crate) static SAMPLE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub(crate) struct SampleDepth {
    pub(crate) depth: f32,
}
#[derive(Debug, Clone)]
pub(crate) struct SampleMaybeDistance {
    pub(crate) depth: f32,
    pub(crate) distance: Option<Fixed>,
}
#[derive(Debug, Clone)]
struct SampleDistance {
    depth: f32,
    distance: Fixed,
}
// struct SampleDistanceMaybeGradient {
//     depth: f32,
//     distance: Fixed,
//     grad: Option<(Real, Imag)>,
// }
#[derive(Debug, Clone)]
pub(crate) struct SampleDistanceGradient {
    pub(crate) depth: f32,
    pub(crate) distance: Fixed,
    pub(crate) grad_real: Real,
    pub(crate) grad_imag: Imag,
}

// impl From<SampleMaybeDistance> for SampleDepth {
//     fn from(value: SampleMaybeDistance) -> Self {
//         Self { depth: value.depth }
//     }
// }
// impl From<SampleDistance> for SampleMaybeDistance {
//     fn from(value: SampleDistance) -> Self {
//         Self {
//             depth: value.depth,
//             distance: Some(value.distance),
//         }
//     }
// }
// impl From<SampleDistanceMaybeGradient> for SampleDistance {
//     fn from(value: SampleDistanceMaybeGradient) -> Self {
//         Self {
//             depth: value.depth,
//             distance: value.distance,
//         }
//     }
// }
// impl From<SampleDistanceGradient> for SampleDistanceMaybeGradient {
//     fn from(value: SampleDistanceGradient) -> Self {
//         Self {
//             depth: value.depth,
//             distance: value.distance,
//             grad: Some((value.grad_real, value.grad_imag)),
//         }
//     }
// }

impl TryFrom<SampleMaybeDistance> for SampleDistance {
    type Error = ();

    fn try_from(value: SampleMaybeDistance) -> Result<Self, Self::Error> {
        Ok(Self {
            depth: value.depth,
            distance: value.distance.ok_or(())?,
        })
    }
}

impl SampleDepth {
    // const MAX_DEPTH: u32 = 256;
    const MAX_DEPTH: u32 = 8192;
    // const MAX_DEPTH: u32 = 131072;

    // fn color(&self) -> Color32 {
    //     let color = if self.depth == 0 {
    //         255
    //     } else if self.depth == Self::MAX_DEPTH {
    //         0
    //     } else {
    //         (35.0 * (self.depth as f64).ln()).clamp(0.0, 255.0) as u8
    //     };
    //     Color32::from_gray(color)
    // }
    pub(crate) fn color(&self) -> Color32 {
        fn cubehelix(c: [f32; 3]) -> Color32 {
            let h = (c[0] + 120.0) * std::f32::consts::PI / 180.0;
            let l = c[2];
            let a = c[1] * l * (1.0 - l);
            let cosh = h.cos();
            let sinh = h.sin();
            let r = f32::min(1.0, l - a * (0.14861 * cosh - 1.78277 * sinh));
            let g = f32::min(1.0, l - a * (0.29227 * cosh + 0.90649 * sinh));
            let b = f32::min(1.0, l + a * (1.97294 * cosh));
            Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
        }
        fn rainbow(t: f32) -> Color32 {
            let ts = (t - 0.5).abs();
            let h = 360.0 * t - 100.0;
            let s = 1.5 - 1.5 * ts;
            let l = 0.8 - 0.9 * ts;
            cubehelix([h, s, l])
        }
        if self.depth == 0.0 {
            Color32::WHITE
        } else if self.depth == Self::MAX_DEPTH as f32 {
            Color32::BLACK
            // Color32::GRAY
        } else {
            // let t = self.depth.ln().fract();
            let t = self.depth.ln().ln().fract();
            rainbow(t)
        }
    }
}
impl SampleDistanceGradient {
    /// returns the estimated distance and gradient of the estimated distance.
    // TODO: compute the gradient exactly, not with finite difference
    // TODO: maybe use the wirtinger derivative
    // TODO: if we immediately normalize the gradient, we don't have to deal with fixed point domain errors
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn new<const LOGGING: bool>(
        sample_log: &mut Option<SampleLog>,
        z0: (Real, Imag),
        (c_real, c_imag): (Real, Imag),
    ) -> Option<Self> {
        let delta = 0.00001.try_into().unwrap();
        let init: SampleDistance = distance_estimator::<LOGGING>(sample_log, z0, (c_real, c_imag))
            .try_into()
            .ok()?;
        let right: SampleDistance =
            distance_estimator::<LOGGING>(sample_log, z0, (c_real + delta, c_imag))
                .try_into()
                .ok()?;
        let up: SampleDistance =
            distance_estimator::<LOGGING>(sample_log, z0, (c_real, c_imag + delta))
                .try_into()
                .ok()?;
        let grad_real =
            Fixed::try_from_f64((right.distance - init.distance).into_f64() / delta.into_f64())?;
        let grad_imag =
            Fixed::try_from_f64((up.distance - init.distance).into_f64() / delta.into_f64())?;
        Some(Self {
            depth: init.depth,
            distance: init.distance,
            grad_real,
            grad_imag,
        })
    }

    /// one iteration of ~gradient descent,
    /// except that we know how far to step.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn stepped(&self, (c_real, c_imag): (Real, Imag)) -> Option<(Real, Imag)> {
        let distance = self.distance.into_f64();
        let grad_real = self.grad_real.into_f64();
        let grad_imag = self.grad_imag.into_f64();
        let grad_len = (grad_real * grad_real + grad_imag * grad_imag).sqrt();
        if grad_len == 0.0 {
            return None;
        }
        let step_size = distance / grad_len;
        Some((
            (c_real.into_f64() - grad_real * step_size)
                .try_into()
                .ok()?,
            (c_imag.into_f64() - grad_imag * step_size)
                .try_into()
                .ok()?,
        ))
    }

    /// one iteration of ~gradient descent,
    /// except that we know how far to step.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn step<const LOGGING: bool>(
        sample_log: &mut Option<SampleLog>,
        z0: (Real, Imag),
        c: (Real, Imag),
    ) -> Option<(Real, Imag)> {
        Self::new::<LOGGING>(sample_log, z0, c)?.stepped(c)
    }
}

pub(crate) use logging::*;
mod logging {
    use egui::ahash::HashMap;

    use super::*;

    #[derive(Debug, Clone)]
    pub(crate) enum SampleLogEntry {
        // QuadraticMap { z0: (Real, Imag), c: (Real, Imag) },
        // DistanceEstimator { z0: (Real, Imag), c: (Real, Imag) },
        // Complex((Real, Imag)),
        ComplexStatic {
            real: Real,
            imag: Imag,
            rad: f32,
            color: Color32,
        },
        ComplexDynamic {
            real: Real,
            imag: Imag,
            max_rad: f32,
            color: Color32,
        },
        Window {
            window: Window,
            stroke: egui::Stroke,
        },
    }

    /// for drawing what points we sampled
    #[derive(Debug, Default)]
    pub(crate) struct SampleLog {
        pub(crate) entries: Vec<SampleLogEntry>,
        quadratic_map: HashMap<((Real, Imag), (Real, Imag)), SampleDepth>,
        distance_estimator: HashMap<((Real, Imag), (Real, Imag)), SampleMaybeDistance>,
    }
    impl SampleLog {
        pub fn insert_quadratic_map(
            &mut self,
            z0: (Real, Imag),
            c: (Real, Imag),
            depth: SampleDepth,
        ) {
            self.quadratic_map.insert((z0, c), depth);
        }

        pub fn insert_distance_estimator(
            &mut self,
            z0: (Real, Imag),
            c: (Real, Imag),
            depth: SampleMaybeDistance,
        ) {
            self.distance_estimator.insert((z0, c), depth);
        }

        // fn quadratic_map(&mut self, z0: (Real, Imag), c: (Real, Imag), color: Color32) {
        //     self.entries
        //         .push((SampleLogEntry::QuadraticMap { z0, c }, color));
        // }

        // fn distance_estimator(&mut self, z0: (Real, Imag), c: (Real, Imag), color: Color32) {
        //     self.entries
        //         .push((SampleLogEntry::DistanceEstimator { z0, c }, color));
        // }

        // fn complex(&mut self, c: (Real, Imag), color: Color32) {
        //     self.entries.push((SampleLogEntry::Complex(c), color));
        // }

        pub(crate) fn window_primary(&mut self, window: Window) {
            self.entries.push(SampleLogEntry::Window {
                window,
                stroke: egui::Stroke::new(2.0, Color32::WHITE),
            });
        }

        pub(crate) fn window_secondary(&mut self, window: Window) {
            self.entries.push(SampleLogEntry::Window {
                window,
                stroke: egui::Stroke::new(1.0, Color32::LIGHT_GRAY),
            });
        }
    }
}

/// note that this doesn't tell the log to draw anything.
#[cfg_attr(feature = "profiling", inline(never))]
pub(crate) fn quadratic_map<const LOGGING: bool>(
    sample_log: &mut Option<SampleLog>,
    (z0_real, z0_imag): (Real, Imag),
    (c_real, c_imag): (Real, Imag),
) -> SampleDepth {
    assert_eq!(LOGGING, sample_log.is_some());

    // const Z_ESCAPE_RAD2: f32 = 4.0;
    const Z_ESCAPE_RAD2: f32 = 64.0;

    // do it like this so the argument names are nice
    let z0_real_fixed = z0_real;
    let z0_imag_fixed = z0_imag;
    let c_real_fixed = c_real;
    let c_imag_fixed = c_imag;

    let z0_real: f32 = z0_real.into_f64() as f32;
    let z0_imag: f32 = z0_imag.into_f64() as f32;
    let c_real: f32 = c_real.into_f64() as f32;
    let c_imag: f32 = c_imag.into_f64() as f32;

    // TODO: consider using fixed point for all the computation
    let mut z_real = z0_real;
    let mut z_imag = z0_imag;
    let mut old_real = z_real;
    let mut old_imag = z_imag;
    let mut z_real2 = z_real * z_real;
    let mut z_imag2 = z_imag * z_imag;
    let mut period_i = 0;
    let mut period_len = 1;
    for depth in 0..SampleDepth::MAX_DEPTH {
        if z_real2 + z_imag2 > Z_ESCAPE_RAD2 {
            let ret = SampleDepth {
                // TODO: defer computing log unless we actually need it
                // depth
                // depth: depth as f32 + 2.0 - (z_real2 + z_imag2).sqrt().ln().ln() / 2.0f32.ln(),
                depth: depth as f32 + 2.0 - (z_real2 + z_imag2).sqrt().ln().log2(),
            };
            if LOGGING {
                let sample_log = sample_log.as_mut().unwrap();
                sample_log.insert_quadratic_map(
                    (z0_real_fixed, z0_imag_fixed),
                    (c_real_fixed, c_imag_fixed),
                    ret.clone(),
                );
            }
            return ret;
        }

        z_imag = (z_real + z_real).mul_add(z_imag, c_imag);
        z_real = z_real2 - z_imag2 + c_real;
        z_real2 = z_real * z_real;
        z_imag2 = z_imag * z_imag;

        // unsafe {
        //     use std::intrinsics::{fadd_fast, fmul_fast, fsub_fast};
        //     z_imag = fadd_fast(fmul_fast(fadd_fast(z_real, z_real), z_imag), c_imag);
        //     z_real = fadd_fast(fsub_fast(z_real2, z_imag2), c_real);
        //     z_real2 = fmul_fast(z_real, z_real);
        //     z_imag2 = fmul_fast(z_imag, z_imag);
        // }

        if (old_real == z_real) && (old_imag == z_imag) {
            let ret = SampleDepth {
                depth: SampleDepth::MAX_DEPTH as f32,
            };
            if LOGGING {
                let sample_log = sample_log.as_mut().unwrap();
                sample_log.insert_quadratic_map(
                    (z0_real_fixed, z0_imag_fixed),
                    (c_real_fixed, c_imag_fixed),
                    ret.clone(),
                );
            }
            return ret;
        }

        period_i += 1;
        if period_i > period_len {
            period_i = 0;
            period_len += 1;
            old_real = z_real;
            old_imag = z_imag;
        };
        debug_assert!(z_real.is_finite());
        debug_assert!(z_imag.is_finite());
    }

    let ret = SampleDepth {
        depth: SampleDepth::MAX_DEPTH as f32,
    };
    if LOGGING {
        let sample_log = sample_log.as_mut().unwrap();
        sample_log.insert_quadratic_map(
            (z0_real_fixed, z0_imag_fixed),
            (c_real_fixed, c_imag_fixed),
            ret.clone(),
        );
    }
    ret
}

/// fails if the fixed point can't be constructed from the float.
/// note that this doesn't tell the log to draw anything.
#[cfg_attr(feature = "profiling", inline(never))]
pub(crate) fn distance_estimator<const LOGGING: bool>(
    sample_log: &mut Option<SampleLog>,
    (z0_real, z0_imag): (Real, Imag),
    (c_real, c_imag): (Real, Imag),
) -> SampleMaybeDistance {
    assert_eq!(LOGGING, sample_log.is_some());

    // const Z_ESCAPE_RAD2: f32 = 4.0;
    // TODO: probably make this bigger
    // const Z_ESCAPE_RAD2: f32 = 64.0;
    const Z_ESCAPE_RAD2: f32 = 256.0;

    //
    // P_c^0(c) = c
    // P_c^1(c) = c^2 + c
    // z_0 = 0
    // z_1 = c
    // z_2 = c^2 + c
    //
    // z_n = P_c^{n-1}(c)
    // z_{n+1} = P_c^n(c)
    // z_n = P_c^{n}(0)

    /// 2 * |P_c^n(c)| * ln|P^n_c(c)| / |∂/∂c P_c^n(c)|
    /// 2 * Abs[f[c]]*Log[Abs[f[c]]]/Abs[Partial[f[c],c]]
    fn estimate(z_real: f32, z_imag: f32, dz_real: f32, dz_imag: f32) -> Option<Fixed> {
        let z_abs = (z_real * z_real + z_imag * z_imag).sqrt();
        let dz_abs = (dz_real * dz_real + dz_imag * dz_imag).sqrt();
        Fixed::try_from_f64((2.0 * z_abs * z_abs.ln() / dz_abs) as f64)
    }

    // // f(c) = |P_c^n(c)|
    // // g(c) = |∂/∂c P_c^n(c)|
    // // Partial[2*f(c)*Log[f(c)]/g(c),c] = (2 g[c] (1 + Log[f[c]]) f'[c] - 2 f[c] Log[f[c]] g'[c])/g[c]^2
    // /// the derivative of the estimate with respect to c
    // // z_{n+1} = z_n^2 + c
    // // dz_{n+1} = 2 * z_n * dz_n
    // // adz_n = |dz_n| = Real(dz_n)^2 + Imag(dz_n)^2
    // // dadz_n = 2 * Real(dz_n) *
    // g probably isn't differentiable actually
    // fn gradient() -> (Real, Imag) {}

    // do it like this so the argument names are nice
    let z0_real_fixed = z0_real;
    let z0_imag_fixed = z0_imag;
    let c_real_fixed = c_real;
    let c_imag_fixed = c_imag;

    let z0_real: f32 = z0_real.into_f64() as f32;
    let z0_imag: f32 = z0_imag.into_f64() as f32;
    let c_real: f32 = c_real.into_f64() as f32;
    let c_imag: f32 = c_imag.into_f64() as f32;

    let mut z_real = z0_real;
    let mut z_imag = z0_imag;
    let mut old_real = z_real;
    let mut old_imag = z_imag;
    let mut z_real2 = z_real * z_real;
    let mut z_imag2 = z_imag * z_imag;
    let mut dz_real = 1.0;
    let mut dz_imag = 0.0;
    let mut period_i = 0;
    let mut period_len = 1;
    for depth in 0..SampleDepth::MAX_DEPTH {
        if z_real2 + z_imag2 > Z_ESCAPE_RAD2 {
            let ret = SampleMaybeDistance {
                // depth: depth as f32 + 2.0 - (z_real2 + z_imag2).sqrt().ln().ln() / 2.0f32.ln(),
                depth: depth as f32 + 2.0 - (z_real2 + z_imag2).sqrt().ln().log2(),
                distance: estimate(z_real, z_imag, dz_real, dz_imag),
            };
            if LOGGING {
                let sample_log = sample_log.as_mut().unwrap();
                sample_log.insert_distance_estimator(
                    (z0_real_fixed, z0_imag_fixed),
                    (c_real_fixed, c_imag_fixed),
                    ret.clone(),
                );
            }
            return ret;
        }

        // 2 * z * dz + 1
        (dz_real, dz_imag) = (
            // TODO: use `mul_add`
            2.0 * (z_real * dz_real - z_imag * dz_imag) + 1.0,
            2.0 * (z_real * dz_imag + z_imag * dz_real),
        );
        z_imag = (z_real + z_real).mul_add(z_imag, c_imag);
        z_real = z_real2 - z_imag2 + c_real;
        z_real2 = z_real * z_real;
        z_imag2 = z_imag * z_imag;

        if (old_real == z_real) && (old_imag == z_imag) {
            // return estimate(z_real, z_imag, dz_real, dz_imag);
            let ret = SampleMaybeDistance {
                depth: SampleDepth::MAX_DEPTH as f32,
                distance: Some(Fixed::ZERO),
            };
            if LOGGING {
                let sample_log = sample_log.as_mut().unwrap();
                sample_log.insert_distance_estimator(
                    (z0_real_fixed, z0_imag_fixed),
                    (c_real_fixed, c_imag_fixed),
                    ret.clone(),
                );
            }
            return ret;
        }

        period_i += 1;
        if period_i > period_len {
            period_i = 0;
            period_len += 1;
            old_real = z_real;
            old_imag = z_imag;
        };
        debug_assert!(z_real.is_finite());
        debug_assert!(z_imag.is_finite());
    }

    let ret = SampleMaybeDistance {
        depth: SampleDepth::MAX_DEPTH as f32,
        distance: estimate(z_real, z_imag, dz_real, dz_imag),
    };
    if LOGGING {
        let sample_log = sample_log.as_mut().unwrap();
        sample_log.insert_distance_estimator(
            (z0_real_fixed, z0_imag_fixed),
            (c_real_fixed, c_imag_fixed),
            ret.clone(),
        );
    }
    ret
}

pub(crate) fn initial_window((z0_real, z0_imag): (Real, Imag)) -> Option<Window> {
    // TODO: actually these comments are wrong,
    // and the escape circles are more subtle,
    // but the results are probably still correct bc the mandelbrots are a lot smaller than the circles

    // it's also the case that for coloring things far away, we care about how deep they get, which can be far away
    // also the non-fixed window works better

    // points outside of circle with radius 2 centered at the origin escape
    // let window0 = Window::from_mid_rad(
    //     Fixed::ZERO,
    //     Fixed::ZERO,
    //     2.0.try_into().unwrap(),
    //     2.0.try_into().unwrap(),
    // );

    // on the first iteration, the points that were outside of
    // the circle with radius 2 centered at (z0_imag*z0_imag - z0_real*z0_real, -2.0*z0_real*z0_imag)
    // will escape
    let real = z0_imag.mul_checked(z0_imag)? - z0_real.mul_checked(z0_real)?;
    let imag = -z0_real.mul_checked(z0_imag)?.mul2_checked()?;
    let rad = 2.0.try_into().unwrap();
    Window::from_mid_rad(real, imag, rad, rad)

    // let window1 = Window::from_mid_diam(
    //     (f64::from(z0_imag) * f64::from(z0_imag) - f64::from(z0_real) * f64::from(z0_real))
    //         .into(),
    //     (-2.0 * f64::from(z0_real) * f64::from(z0_imag)).into(),
    //     4.0.into(),
    //     4.0.into(),
    // );

    // their intersection gives a tighter bound on the area that can escape
    // which lets us use our samples on a more important area
    // window0.intersect(window1)
}

pub(crate) const WIDTH: usize = 32;
// pub(crate) const WIDTH: usize = 64;
// pub(crate) const GRADIENT_STEPS: usize = 1;
pub(crate) const GRADIENT_STEPS: usize = 0;
#[cfg_attr(feature = "profiling", inline(never))]
pub(crate) fn deepest_via_gradient_steps<const LOGGING: bool>(
    sample_log: &mut Option<SampleLog>,
    z0: (Real, Imag),
    window: Window,
    width: usize,
    height: usize,
    gradient_steps: usize,
) -> ((Real, Imag), SampleDepth) {
    assert_eq!(LOGGING, sample_log.is_some());
    let log_delta = 8;
    let delta = Fixed::ONE.div2_n_floor(log_delta);
    let mut deepest: f32 = 0.0;
    let mut deepest_point = (Fixed::ZERO, Fixed::ZERO);
    for line in window.grid_centers(width, height) {
        for (mut c_real, mut c_imag) in line {
            // TODO: less recomputation
            for _ in 0..gradient_steps {
                let Some(distance_init) = ({
                    let c = (c_real, c_imag);
                    let SampleMaybeDistance { depth, distance } =
                        distance_estimator::<LOGGING>(sample_log, z0, c);
                    if depth > deepest {
                        if depth >= SampleDepth::MAX_DEPTH as f32 {
                            return (c, SampleDepth { depth });
                        }
                        deepest = depth;
                        deepest_point = c;
                    }
                    distance
                }) else {
                    break;
                };

                let Some(distance_right) = ({
                    let c = (c_real + delta, c_imag);
                    let SampleMaybeDistance { depth, distance } =
                        distance_estimator::<LOGGING>(sample_log, z0, c);
                    if depth > deepest {
                        if depth >= SampleDepth::MAX_DEPTH as f32 {
                            return (c, SampleDepth { depth });
                        }
                        deepest = depth;
                        deepest_point = c;
                    }
                    distance
                }) else {
                    break;
                };

                let Some(distance_left) = ({
                    let c = (c_real, c_imag + delta);
                    let SampleMaybeDistance { depth, distance } =
                        distance_estimator::<LOGGING>(sample_log, z0, c);
                    if depth > deepest {
                        if depth >= SampleDepth::MAX_DEPTH as f32 {
                            return (c, SampleDepth { depth });
                        }
                        deepest = depth;
                        deepest_point = c;
                    }
                    distance
                }) else {
                    break;
                };

                // TODO: fixed point
                let grad_real = (distance_right - distance_init).into_f64() / delta.into_f64();
                let grad_imag = (distance_left - distance_init).into_f64() / delta.into_f64();
                let grad_len = (grad_real * grad_real + grad_imag * grad_imag).sqrt();
                let scale = distance_init.into_f64() / grad_len;
                let (Some(c_real_new), Some(c_imag_new)) = (
                    Fixed::try_from_f64(c_real.into_f64() - grad_real * scale),
                    Fixed::try_from_f64(c_imag.into_f64() - grad_imag * scale),
                ) else {
                    break;
                };
                c_real = c_real_new;
                c_imag = c_imag_new;
            }

            let sample = quadratic_map::<LOGGING>(sample_log, z0, (c_real, c_imag));
            // metajulia
            // let sample = mandelbrot_sample(c_real, c_imag, z0_real, z0_imag);
            if sample.depth > deepest {
                if sample.depth >= SampleDepth::MAX_DEPTH as f32 {
                    return ((c_real, c_imag), sample);
                }
                deepest = sample.depth;
                deepest_point = (c_real, c_imag);
            }
        }
    }
    (deepest_point, SampleDepth { depth: deepest })
}

/// for the initial samples
pub(crate) const WIDTH0: usize = 64;
/// for the resamples
pub(crate) const WIDTH1: usize = 8;

/// sample_log should be `Some` iff `LOGGING`
#[cfg_attr(feature = "profiling", inline(never))]
pub(crate) fn metabrot_sample<const LOGGING: bool>(
    sample_log: &mut Option<SampleLog>,
    (z0_real, z0_imag): (Real, Imag),
) -> SampleDepth {
    // pub(crate) const WIDTH: usize = 128;
    // const WIDTH: usize = 512;
    assert_eq!(LOGGING, sample_log.is_some());

    if LOGGING {
        let sample_log = sample_log.as_mut().unwrap();
        assert!(
            sample_log.entries.is_empty(),
            "maybe i should just clear it here"
        );
    }
    // #[cfg(LOGGING)]
    // let sample_log = sample_log.unwrap();

    let Some(window) = initial_window((z0_real, z0_imag)) else {
        // if the windows don't intersect,
        // then we know that all points escape immediately
        return SampleDepth {
            depth: 0.0,
            // this is for debug
            // depth: Sample::MAX_DEPTH,
        };
    };
    if LOGGING {
        let sample_log = sample_log.as_mut().unwrap();
        sample_log.window_primary(window);
    }

    let start = Instant::now();

    let mut deepest: f32 = 0.0;
    let mut deepest_point = (Fixed::ZERO, Fixed::ZERO);

    let (c, sample) = deepest_via_gradient_steps::<LOGGING>(
        sample_log,
        (z0_real, z0_imag),
        window,
        WIDTH,
        WIDTH,
        GRADIENT_STEPS,
    );
    deepest = sample.depth;
    deepest_point = c;

    #[cfg(false)]
    {
        let elapsed = start.elapsed();
        SAMPLE_ELAPSED_NANOS.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
        SAMPLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    }

    return sample;

    // resample around points with a distance estimate < the diameter of each grid cell.
    // note that we don't use the gradient of the distance estimate for this.
    // TODO: use the gradient of the distance estimate
    // to avoid sampling the same mesa-points as our neighbor will,
    // resample with the radius of the cell, not the distance estimate.
    // this could miss points that are farther than the radius
    // but still have a distance estimate smaller than the diameter of the cell,
    // if our neighbor's distance estimate didn't trigger a resample on them.
    // TODO: fix this

    // we want to look through all the points at a coarse grain before resampling
    let mut to_resample = Vec::with_capacity(WIDTH0 * WIDTH0);
    let cell_rad = {
        (window.real_rad().div_f64(WIDTH0 as f64)).max(window.imag_rad().div_f64(WIDTH0 as f64))
    };
    // initial samples
    for (c_real, c_imag) in window.grid_centers(WIDTH0, WIDTH0).flatten() {
        let SampleMaybeDistance { depth, distance } =
            distance_estimator::<LOGGING>(sample_log, (z0_real, z0_imag), (c_real, c_imag));
        if depth > deepest {
            if depth >= SampleDepth::MAX_DEPTH as f32 {
                return SampleDepth { depth };
            }
            deepest = depth;
            deepest_point = (c_real, c_imag);
        }
        if let Some(distance) = distance
            && distance < cell_rad.mul2()
        {
            to_resample.push((c_real, c_imag));
        }
    }
    // TODO: try sorting the vec by distance estimate
    // windows around the points that triggered a resample
    for (c0_real, c0_imag) in to_resample {
        let resample_window = Window::from_mid_rad(c0_real, c0_imag, cell_rad, cell_rad).unwrap();
        for (c_real, c_imag) in resample_window.grid_centers(WIDTH1, WIDTH1).flatten() {
            if (c0_real, c0_imag) == (c_real, c_imag) {
                continue;
            }
            let sample = quadratic_map::<LOGGING>(sample_log, (z0_real, z0_imag), (c_real, c_imag));
            if sample.depth > deepest {
                if sample.depth >= SampleDepth::MAX_DEPTH as f32 {
                    return sample;
                }
                deepest = sample.depth;
                deepest_point = (c_real, c_imag);
            }
        }
    }

    SampleDepth { depth: deepest }
}
