use eframe::egui::Color32;

use crate::{camera::Square, fixed::*, lerp};

pub(crate) struct Sample {
    depth: u32,
}
impl Sample {
    const MAX_DEPTH: u32 = 8192;

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
        if self.depth == 0 {
            Color32::WHITE
        } else if self.depth == Self::MAX_DEPTH {
            Color32::BLACK
        } else {
            let t = (self.depth as f32).ln().fract();
            rainbow(t)
        }
    }
}

fn mandelbrot_sample(z0_real: Real, z0_imag: Real, c_real: Real, c_imag: Real) -> Sample {
    // TODO: consider using fixed point for this
    let z0_real: f32 = z0_real.into();
    let z0_imag: f32 = z0_imag.into();
    let c_real: f32 = c_real.into();
    let c_imag: f32 = c_imag.into();
    const Z_ESCAPE_RAD2: f32 = 4.0;
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
            return Sample { depth };
            // return MandelbrotSample::new(
            //     BoundedState::Diverge,
            //     depth,
            //     Complex {
            //         real: z_real,
            //         imag: z_imag,
            //     },
            // );
        }

        z_imag = (z_real + z_real) * z_imag + c_imag;
        z_real = z_real2 - z_imag2 + c_real;
        z_real2 = z_real * z_real;
        z_imag2 = z_imag * z_imag;

        if (old_real == z_real) && (old_imag == z_imag) {
            return Sample {
                depth: Sample::MAX_DEPTH,
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
        depth: Sample::MAX_DEPTH,
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

#[inline(never)]
pub(crate) fn metabrot_sample((z0_real, z0_imag): (Real, Imag)) -> Sample {
    const WIDTH: usize = 128;
    let window: Square =
        Square::new_exact((-2.0).into(), 2.0.into(), (-2.0).into(), 2.0.into()).unwrap();
    let mut deepest = 0;
    for row in 0..WIDTH {
        let c_imag = Fixed::lerp(
            window.imag_lo(),
            window.imag_hi(),
            1.0 - row as f64 / WIDTH as f64,
        );
        for col in 0..WIDTH {
            let c_real = Fixed::lerp(
                window.real_lo(),
                window.real_hi(),
                col as f64 / WIDTH as f64,
            );
            let sample = mandelbrot_sample(z0_real, z0_imag, c_real, c_imag);
            if sample.depth == Sample::MAX_DEPTH {
                return sample;
            }
            deepest = deepest.max(sample.depth);
        }
    }
    Sample { depth: deepest }
}
