use eframe::egui::Color32;

use crate::{
    camera::{Square, Window},
    fixed::*,
};

pub(crate) struct Sample {
    depth: f32,
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
        } else {
            // let t = self.depth.ln().fract();
            let t = self.depth.ln().ln().fract();
            rainbow(t)
        }
    }
}

pub(crate) fn quadratic_map(
    (z0_real, z0_imag): (Real, Imag),
    (c_real, c_imag): (Real, Imag),
) -> Sample {
    // const Z_ESCAPE_RAD2: f32 = 4.0;
    const Z_ESCAPE_RAD2: f32 = 64.0;

    let z0_real: f32 = z0_real.into();
    let z0_imag: f32 = z0_imag.into();
    let c_real: f32 = c_real.into();
    let c_imag: f32 = c_imag.into();

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
                depth: depth as f32 + 2.0 - (z_real2 + z_imag2).sqrt().ln().ln() / 2.0f32.ln(),
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
        assert!(z_real.is_finite());
        assert!(z_imag.is_finite());
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
pub(crate) fn distance_estimator(
    (z0_real, z0_imag): (Real, Imag),
    (c_real, c_imag): (Real, Imag),
) -> Option<Fixed> {
    // const Z_ESCAPE_RAD2: f32 = 4.0;
    // TODO: probably make this bigger
    const Z_ESCAPE_RAD2: f32 = 64.0;

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
        Fixed::try_from_f32(2.0 * z_abs * z_abs.ln() / dz_abs)
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

    let z0_real: f32 = z0_real.into();
    let z0_imag: f32 = z0_imag.into();
    let c_real: f32 = c_real.into();
    let c_imag: f32 = c_imag.into();

    let mut z_real = z0_real;
    let mut z_imag = z0_imag;
    let mut old_real = z_real;
    let mut old_imag = z_imag;
    let mut z_real2 = z_real * z_real;
    let mut z_imag2 = z_imag * z_imag;
    // TODO: not sure about this
    let mut dz_real = 1.0;
    // let mut dz_real = 0.0;
    let mut dz_imag = 0.0;
    let mut period_i = 0;
    let mut period_len = 1;
    for depth in 0..Sample::MAX_DEPTH {
        if z_real2 + z_imag2 > Z_ESCAPE_RAD2 {
            return estimate(z_real, z_imag, dz_real, dz_imag);
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
            return Some(0.0.into());
        }

        period_i += 1;
        if period_i > period_len {
            period_i = 0;
            period_len += 1;
            old_real = z_real;
            old_imag = z_imag;
        };
        assert!(z_real.is_finite());
        assert!(z_imag.is_finite());
    }
    estimate(z_real, z_imag, dz_real, dz_imag)
    // 0.0
}

/// returns the estimated distance and gradient of the estimated distance
/// TODO: compute the gradient exactly, not with finite difference
pub(crate) fn distance_estimator_gradient(
    z0: (Real, Imag),
    (c_real, c_imag): (Real, Imag),
) -> Option<(Fixed, (Real, Imag))> {
    let delta = 0.00001.into();
    let distance = distance_estimator(z0, (c_real, c_imag))?;
    let distance_right = distance_estimator(z0, (c_real + delta, c_imag))?;
    let distance_up = distance_estimator(z0, (c_real, c_imag + delta))?;
    let grad_real = Fixed::try_from_f32((distance_right - distance).into_f32() / delta.into_f32())?;
    let grad_imag = Fixed::try_from_f32((distance_up - distance).into_f32() / delta.into_f32())?;
    Some((distance, (grad_real, grad_imag)))
}

/// one iteration of ~gradient descent
/// except that we know how far to step
pub(crate) fn gradient_step(
    (z0_real, z0_imag): (Real, Imag),
    (c_real, c_imag): (Real, Imag),
) -> Option<(Real, Imag)> {
    let (distance, (grad_real, grad_imag)) =
        distance_estimator_gradient((z0_real, z0_imag), (c_real, c_imag))?;
    let distance: f32 = distance.into();
    let grad_real: f32 = grad_real.into();
    let grad_imag: f32 = grad_imag.into();
    let grad_len = (grad_real * grad_real + grad_imag * grad_imag).sqrt();
    if grad_len == 0.0 {
        return None;
    }
    let step_size = distance / grad_len;
    Some((
        (c_real.into_f32() - grad_real * step_size).into(),
        (c_imag.into_f32() - grad_imag * step_size).into(),
    ))
}

pub(crate) const WIDTH: usize = 128;
fn deepest_on_grid((z0_real, z0_imag): (Real, Imag), window: Window) -> ((Real, Imag), Sample) {
    let mut deepest: f32 = 0.0;
    let mut deepest_point = (0.0.into(), 0.0.into());
    for line in window.grid(WIDTH, WIDTH) {
        for (c_real, c_imag) in line {
            let sample = quadratic_map((z0_real, z0_imag), (c_real, c_imag));
            // metajulia
            // let sample = mandelbrot_sample(c_real, c_imag, z0_real, z0_imag);
            if sample.depth == Sample::MAX_DEPTH as f32 {
                return ((c_real, c_imag), sample);
            }
            if sample.depth > deepest {
                deepest = sample.depth;
                deepest_point = (c_real, c_imag);
            }
        }
    }
    (deepest_point, Sample { depth: deepest })
}

#[inline(never)]
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
        let window0 = Window::from_center_size(0.0.into(), 0.0.into(), 4.0.into(), 4.0.into());

        // on the first iteration, the points that were outside of
        // the circle with radius 2 centered at (z0_imag*z0_imag - z0_real*z0_real, -2.0*z0_real*z0_imag)
        // will escape
        let window1 = Window::from_center_size(
            (f64::from(z0_imag) * f64::from(z0_imag) - f64::from(z0_real) * f64::from(z0_real))
                .into(),
            (-2.0 * f64::from(z0_real) * f64::from(z0_imag)).into(),
            4.0.into(),
            4.0.into(),
        );

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

    let (c, sample) = deepest_on_grid((z0_real, z0_imag), window);
    sample
}
