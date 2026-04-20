use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use eframe::egui::Color32;

use crate::complex::{Window, fixed::*};

pub(crate) static SAMPLE_ELAPSED_NANOS: AtomicU64 = AtomicU64::new(0);
pub(crate) static SAMPLE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) struct Sample {
    pub(crate) depth: f32,
}
impl Sample {
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

#[cfg_attr(feature = "profiling", inline(never))]
pub(crate) fn quadratic_map(
    (z0_real, z0_imag): (Real, Imag),
    (c_real, c_imag): (Real, Imag),
) -> Sample {
    // const Z_ESCAPE_RAD2: f32 = 4.0;
    const Z_ESCAPE_RAD2: f32 = 64.0;

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
    for depth in 0..Sample::MAX_DEPTH {
        if z_real2 + z_imag2 > Z_ESCAPE_RAD2 {
            return Sample {
                // depth
                // depth: depth as f32 + 2.0 - (z_real2 + z_imag2).sqrt().ln().ln() / 2.0f32.ln(),
                depth: depth as f32 + 2.0 - (z_real2 + z_imag2).sqrt().ln().log2(),
            };
        }

        z_imag = (z_real + z_real) * z_imag + c_imag;
        z_real = z_real2 - z_imag2 + c_real;
        z_real2 = z_real * z_real;
        z_imag2 = z_imag * z_imag;

        if (old_real == z_real) && (old_imag == z_imag) {
            return Sample {
                depth: Sample::MAX_DEPTH as f32,
            };
            // return MandelbrotSample::new(
            //     BoundedState::Bounded,
            //     depth,
            //     Complex {
            //         real: z_real,
            //         imag: z_imag,
            //     },
            // );
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
    Sample {
        depth: Sample::MAX_DEPTH as f32,
    }
    // MandelbrotSample::new(
    //     BoundedState::Unknown,
    //     MAX_DEPTH,
    //     Complex {
    //         real: z_real,
    //         imag: z_imag,
    //     },
    // )
}

/// fails if the fixed point can't be constructed from the float
#[cfg_attr(feature = "profiling", inline(never))]
pub(crate) fn distance_estimator(
    (z0_real, z0_imag): (Real, Imag),
    (c_real, c_imag): (Real, Imag),
) -> (Sample, Option<Fixed>) {
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
    for depth in 0..Sample::MAX_DEPTH {
        if z_real2 + z_imag2 > Z_ESCAPE_RAD2 {
            return (
                Sample {
                    // depth: depth as f32 + 2.0 - (z_real2 + z_imag2).sqrt().ln().ln() / 2.0f32.ln(),
                    depth: depth as f32 + 2.0 - (z_real2 + z_imag2).sqrt().ln().log2(),
                },
                estimate(z_real, z_imag, dz_real, dz_imag),
            );
        }

        // 2 * z * dz + 1
        (dz_real, dz_imag) = (
            2.0 * (z_real * dz_real - z_imag * dz_imag) + 1.0,
            2.0 * (z_real * dz_imag + z_imag * dz_real),
        );
        z_imag = (z_real + z_real) * z_imag + c_imag;
        z_real = z_real2 - z_imag2 + c_real;
        z_real2 = z_real * z_real;
        z_imag2 = z_imag * z_imag;

        if (old_real == z_real) && (old_imag == z_imag) {
            // return estimate(z_real, z_imag, dz_real, dz_imag);
            return (
                Sample {
                    depth: Sample::MAX_DEPTH as f32,
                },
                Some(Fixed::ZERO),
            );
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
    (
        Sample {
            depth: Sample::MAX_DEPTH as f32,
        },
        estimate(z_real, z_imag, dz_real, dz_imag),
    )
    // 0.0
}

/// returns the estimated distance and gradient of the estimated distance
/// TODO: compute the gradient exactly, not with finite difference
// TODO: if we immediate;y normalize the gradient, we don't have to deal with fixed point domain errors
#[cfg_attr(feature = "profiling", inline(never))]
pub(crate) fn distance_estimator_gradient(
    z0: (Real, Imag),
    (c_real, c_imag): (Real, Imag),
) -> Option<(Fixed, (Real, Imag))> {
    let delta = 0.00001.try_into().unwrap();
    let distance_init = distance_estimator(z0, (c_real, c_imag)).1?;
    let distance_right = distance_estimator(z0, (c_real + delta, c_imag)).1?;
    let distance_up = distance_estimator(z0, (c_real, c_imag + delta)).1?;
    let grad_real =
        Fixed::try_from_f64((distance_right - distance_init).into_f64() / delta.into_f64())?;
    let grad_imag =
        Fixed::try_from_f64((distance_up - distance_init).into_f64() / delta.into_f64())?;
    Some((distance_init, (grad_real, grad_imag)))
}

/// one iteration of ~gradient descent
/// except that we know how far to step
#[cfg_attr(feature = "profiling", inline(never))]
pub(crate) fn gradient_step(
    (z0_real, z0_imag): (Real, Imag),
    (c_real, c_imag): (Real, Imag),
) -> Option<(Real, Imag)> {
    let (distance, (grad_real, grad_imag)) =
        distance_estimator_gradient((z0_real, z0_imag), (c_real, c_imag))?;
    let distance = distance.into_f64();
    let grad_real = grad_real.into_f64();
    let grad_imag = grad_imag.into_f64();
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

pub(crate) const WIDTH: usize = 32;
// pub(crate) const WIDTH: usize = 64;
// pub(crate) const GRADIENT_STEPS: usize = 1;
pub(crate) const GRADIENT_STEPS: usize = 0;
#[cfg_attr(feature = "profiling", inline(never))]
pub(crate) fn deepest_on_grid(
    (z0_real, z0_imag): (Real, Imag),
    window: Window,
    width: usize,
    height: usize,
    gradient_steps: usize,
) -> ((Real, Imag), Sample) {
    let log_delta = 8;
    let delta = Fixed::ONE.div2_n_floor(log_delta);
    let mut deepest: f32 = 0.0;
    let mut deepest_point = (Fixed::ZERO, Fixed::ZERO);
    for line in window.grid_centers(width, height) {
        for (mut c_real, mut c_imag) in line {
            // TODO: less recomputation
            for _ in 0..gradient_steps {
                let Some(distance_init) = ({
                    let (sample_init, distance_init) =
                        distance_estimator((z0_real, z0_imag), (c_real, c_imag));
                    if sample_init.depth > deepest {
                        if sample_init.depth >= Sample::MAX_DEPTH as f32 {
                            return ((c_real, c_imag), sample_init);
                        }
                        deepest = sample_init.depth;
                        deepest_point = (c_real, c_imag);
                    }
                    distance_init
                }) else {
                    break;
                };

                let Some(distance_right) = ({
                    let (sample_right, distance_right) =
                        distance_estimator((z0_real, z0_imag), (c_real + delta, c_imag));
                    if sample_right.depth > deepest {
                        if sample_right.depth >= Sample::MAX_DEPTH as f32 {
                            return ((c_real + delta, c_imag), sample_right);
                        }
                        deepest = sample_right.depth;
                        deepest_point = (c_real + delta, c_imag);
                    }
                    distance_right
                }) else {
                    break;
                };

                let Some(distance_left) = ({
                    let (sample_left, distance_left) =
                        distance_estimator((z0_real, z0_imag), (c_real, c_imag + delta));
                    if sample_left.depth > deepest {
                        if sample_left.depth >= Sample::MAX_DEPTH as f32 {
                            return ((c_real, c_imag + delta), sample_left);
                        }
                        deepest = sample_left.depth;
                        deepest_point = (c_real, c_imag + delta);
                    }
                    distance_left
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

            let sample = quadratic_map((z0_real, z0_imag), (c_real, c_imag));
            // metajulia
            // let sample = mandelbrot_sample(c_real, c_imag, z0_real, z0_imag);
            if sample.depth > deepest {
                if sample.depth >= Sample::MAX_DEPTH as f32 {
                    return ((c_real, c_imag), sample);
                }
                deepest = sample.depth;
                deepest_point = (c_real, c_imag);
            }
        }
    }
    (deepest_point, Sample { depth: deepest })
}

/// for the initial samples
pub(crate) const WIDTH0: usize = 64;
/// for the resamples
pub(crate) const WIDTH1: usize = 8;

#[cfg_attr(feature = "profiling", inline(never))]
pub(crate) fn metabrot_sample((z0_real, z0_imag): (Real, Imag)) -> Sample {
    // pub(crate) const WIDTH: usize = 128;
    // const WIDTH: usize = 512;

    let Some(window) = ({
        // TODO: actually these comments are wrong,
        // and the escape circles are more subtle,
        // but the results are probably still correct bc the mandelbrots are a lot smaller than the circles

        // it's also the case that for coloring things far away, we care about how deep they get, which can be far away
        // also the non-fixed window works better

        // points outside of circle with radius 2 centered at the origin escape
        let window0 = Window::from_mid_rad(
            Fixed::ZERO,
            Fixed::ZERO,
            2.0.try_into().unwrap(),
            2.0.try_into().unwrap(),
        );

        // on the first iteration, the points that were outside of
        // the circle with radius 2 centered at (z0_imag*z0_imag - z0_real*z0_real, -2.0*z0_real*z0_imag)
        // will escape
        let Some(window1) = ({
            let real = z0_imag.mul(z0_imag) - z0_real.mul(z0_real);
            let imag = -z0_real.mul(z0_imag).mul2();
            let rad = 2.0.try_into().unwrap();
            // if !Fixed::in_domain(2.0 * (real.into_f64() - rad))
            //     || !Fixed::in_domain(2.0 * (real.into_f64() + rad))
            //     || !Fixed::in_domain(2.0 * (imag.into_f64() - rad))
            //     || !Fixed::in_domain(2.0 * (imag.into_f64() + rad))
            // {
            //     return Sample { depth: 0.0 };
            // }
            Window::from_mid_rad(real, imag, rad, rad)
        }) else {
            return Sample { depth: 0.0 };
        };
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

        // don't use the fancy stuff,
        // wait until i can have debug tools for comparing fractals
        // Some(window0)
        Some(window1)
    }) else {
        // if the windows don't intersect,
        // then we know that all points escape immediately
        return Sample {
            depth: 0.0,
            // this is for debug
            // depth: Sample::MAX_DEPTH,
        };
    };

    let start = Instant::now();

    let mut deepest: f32 = 0.0;
    let mut deepest_point = (Fixed::ZERO, Fixed::ZERO);

    let (c, sample) = deepest_on_grid((z0_real, z0_imag), window, WIDTH, WIDTH, GRADIENT_STEPS);
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
        let (sample, distance) = distance_estimator((z0_real, z0_imag), (c_real, c_imag));
        if sample.depth > deepest {
            if sample.depth >= Sample::MAX_DEPTH as f32 {
                return sample;
            }
            deepest = sample.depth;
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
            let sample = quadratic_map((z0_real, z0_imag), (c_real, c_imag));
            if sample.depth > deepest {
                if sample.depth >= Sample::MAX_DEPTH as f32 {
                    return sample;
                }
                deepest = sample.depth;
                deepest_point = (c_real, c_imag);
            }
        }
    }

    Sample { depth: deepest }
}
