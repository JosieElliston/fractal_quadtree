use eframe::egui::Color32;

use crate::{camera::Square, lerp};

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

fn mandelbrot_sample(z0_real: f32, z0_imag: f32, c_real: f32, c_imag: f32) -> Sample {
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
pub(crate) fn metabrot_sample(z0_real: f32, z0_imag: f32) -> Sample {
    const WIDTH: usize = 128;
    const WINDOW: Square = Square::try_new(-2.0, 2.0, -2.0, 2.0).unwrap();
    let mut deepest = 0;
    for row in 0..WIDTH {
        let c_imag = lerp(
            WINDOW.imag_lo(),
            WINDOW.imag_hi(),
            1.0 - row as f32 / WIDTH as f32,
        );
        for col in 0..WIDTH {
            let c_real = lerp(
                WINDOW.real_lo(),
                WINDOW.real_hi(),
                col as f32 / WIDTH as f32,
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
