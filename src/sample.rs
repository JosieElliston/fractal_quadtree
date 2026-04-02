use eframe::egui::Color32;

use crate::{
    camera::{Square, Window},
    fixed::*,
};

pub(crate) struct Sample {
    depth: f32,
}
impl Sample {
    // const MAX_DEPTH: u32 = 8192;
    const MAX_DEPTH: u32 = 131072;

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

    // TODO: consider using fixed point for this
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

pub(crate) const WIDTH: usize = 128;
fn deepest_on_grid((z0_real, z0_imag): (Real, Imag), window: Window) -> Sample {
    let mut deepest: f32 = 0.0;
    for line in window.grid(WIDTH, WIDTH) {
        for (c_real, c_imag) in line {
            let sample = quadratic_map((z0_real, z0_imag), (c_real, c_imag));
            // metajulia
            // let sample = mandelbrot_sample(c_real, c_imag, z0_real, z0_imag);
            if sample.depth == Sample::MAX_DEPTH as f32 {
                return sample;
            }
            deepest = deepest.max(sample.depth);
        }
    }
    Sample { depth: deepest }
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

    deepest_on_grid((z0_real, z0_imag), window)
}
