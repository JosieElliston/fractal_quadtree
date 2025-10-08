#![feature(portable_simd)]

use std::{
    cmp::Ordering,
    hint::black_box,
    iter::Sum,
    ops::{Add, AddAssign},
    simd::{
        Mask, Simd,
        cmp::{SimdPartialEq, SimdPartialOrd},
        num::SimdFloat,
    },
    time::{Duration, Instant},
};

use eframe::egui::{self, Color32, Key, Pos2, Rect, RichText, Vec2};
use rand::seq::SliceRandom;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> eframe::Result {
    // std::env::set_var("RUST_BACKTRACE", "1");
    // env_logger::init();

    // bench();
    // panic!();

    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        "fractal",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

fn bench() {
    let start = Instant::now();
    let mut tree = Tree::new_leaf(Square::try_new(-4.0, 4.0, -4.0, 4.0).unwrap());
    let stride = 8;
    let camera = Camera::new(0.0, 0.0, 2.0);
    let camera_map = CameraMap::new(
        Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0)),
        camera,
    );
    for (_, pixel) in camera_map.pixels(stride) {
        tree.ensure_pixel_safe(pixel);
    }
    for _ in 0..600 {
        for (_, pixel) in camera_map.pixels(stride) {
            black_box(tree.color(pixel));
        }
    }
    black_box(tree);
    println!("time: {:?}", start.elapsed());
}

struct Sample {
    depth: u32,
}
impl Sample {
    const MAX_DEPTH: u32 = 8192;
    fn color(&self) -> Color32 {
        let color = if self.depth == 0 {
            255
        } else if self.depth == Self::MAX_DEPTH {
            0
        } else {
            (35.0 * (self.depth as f64).ln()).clamp(0.0, 255.0) as u8
        };
        Color32::from_gray(color)
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
fn metabrot_sample(z0_real: f32, z0_imag: f32) -> Sample {
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

// TODO: make this a mapping from egui rect to complex window
#[derive(Debug, Clone, Copy)]
struct Camera {
    real_mid: f32,
    imag_mid: f32,
    real_rad: f32,
}
impl Camera {
    fn new(real_mid: f32, imag_mid: f32, real_rad: f32) -> Self {
        assert!(real_rad > 0.0);
        Self {
            real_mid,
            imag_mid,
            real_rad,
        }
    }

    fn real_lo(self) -> f32 {
        self.real_mid - self.real_rad
    }

    fn real_hi(self) -> f32 {
        self.real_mid + self.real_rad
    }

    // fn imag_lo(self) -> f32 {}

    // fn imag_hi(self) -> f32 {}
}
impl std::ops::AddAssign<(f32, f32)> for Camera {
    fn add_assign(&mut self, rhs: (f32, f32)) {
        self.real_mid += rhs.0;
        self.imag_mid += rhs.1;
    }
}

fn lerp(lo: f32, hi: f32, t: f32) -> f32 {
    assert!(lo < hi);
    // assert!((0.0..=1.0).contains(&t));
    // lo * (1.0 - t) + hi * t
    lo + (hi - lo) * t
}

fn inv_lerp(lo: f32, hi: f32, x: f32) -> f32 {
    assert!(lo < hi);
    // assert!((lo..=hi).contains(&x));
    (x - lo) / (hi - lo)
}

struct CameraMap {
    rect: Rect,
    camera: Camera,
}
impl CameraMap {
    fn new(rect: Rect, camera: Camera) -> Self {
        assert!(rect.min.x < rect.max.x);
        assert!(rect.min.y < rect.max.y);
        Self { rect, camera }
    }

    fn imag_rad(&self) -> f32 {
        self.camera.real_rad * self.rect.height() / self.rect.width()
    }

    fn imag_lo(&self) -> f32 {
        self.camera.imag_mid - self.imag_rad()
    }

    fn imag_hi(&self) -> f32 {
        self.camera.imag_mid + self.imag_rad()
    }

    fn x_to_real(&self, x: f32) -> f32 {
        // -2.0 * x / self.rect.size().x * self.camera.real_rad
        lerp(
            self.camera.real_lo(),
            self.camera.real_hi(),
            inv_lerp(self.rect.min.x, self.rect.max.x, x),
        )
    }

    fn y_to_imag(&self, y: f32) -> f32 {
        // 2.0 * y * (self.camera.real_rad / self.rect.size().x)
        lerp(
            self.imag_lo(),
            self.imag_hi(),
            1.0 - inv_lerp(self.rect.min.y, self.rect.max.y, y),
        )
    }

    fn pos_to_complex(&self, pos: Pos2) -> (f32, f32) {
        (self.x_to_real(pos.x), self.y_to_imag(pos.y))
    }

    fn real_to_x(&self, real: f32) -> f32 {
        lerp(
            self.rect.min.x,
            self.rect.max.x,
            inv_lerp(self.camera.real_lo(), self.camera.real_hi(), real),
        )
    }

    fn imag_to_y(&self, imag: f32) -> f32 {
        lerp(
            self.rect.min.y,
            self.rect.max.y,
            1.0 - inv_lerp(self.imag_lo(), self.imag_hi(), imag),
        )
    }

    // fn complex_to_pos(&self, real: f32, imag: f32) -> Pos2 {
    //     Pos2::new(self.real_to_x(real), self.imag_to_y(imag))
    // }

    fn complex_to_pos(&self, c: (f32, f32)) -> Pos2 {
        Pos2::new(self.real_to_x(c.0), self.imag_to_y(c.1))
    }

    fn window_to_rect(&self, window: impl Into<Window>) -> Rect {
        let window = window.into();
        Rect {
            min: self.complex_to_pos((window.real_lo, window.imag_hi)),
            max: self.complex_to_pos((window.real_hi, window.imag_lo)),
        }
    }

    fn pixels(&self, stride: usize) -> impl Iterator<Item = (Rect, Square)> {
        (0..self.rect.size().y as usize)
            .step_by(stride)
            .flat_map(move |row| {
                (0..self.rect.size().x as usize)
                    .step_by(stride)
                    .filter_map(move |col| {
                        Some((
                            Rect::from_min_size(
                                Pos2::new(col as f32, row as f32),
                                Vec2::new(stride as f32, stride as f32),
                            ),
                            Square::try_new(
                                self.x_to_real(col as f32),
                                self.x_to_real((col + stride) as f32),
                                self.y_to_imag((row + stride) as f32),
                                self.y_to_imag(row as f32),
                            )?,
                        ))
                    })
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(align(32))]
struct Window {
    real_lo: f32,
    real_hi: f32,
    imag_lo: f32,
    imag_hi: f32,
}
impl Window {
    fn new(real_lo: f32, real_hi: f32, imag_lo: f32, imag_hi: f32) -> Self {
        assert!(real_lo.is_finite());
        assert!(real_hi.is_finite());
        assert!(imag_lo.is_finite());
        assert!(imag_hi.is_finite());
        assert!(real_lo < real_hi);
        assert!(imag_lo < imag_hi);
        Self {
            real_lo,
            real_hi,
            imag_lo,
            imag_hi,
        }
    }

    fn real_mid(self) -> f32 {
        (self.real_hi + self.real_lo) / 2.0
    }

    fn imag_mid(self) -> f32 {
        (self.imag_hi + self.imag_lo) / 2.0
    }

    fn real_rad(self) -> f32 {
        (self.real_hi - self.real_lo) / 2.0
    }

    fn imag_rad(self) -> f32 {
        (self.imag_hi - self.imag_lo) / 2.0
    }

    fn area(self) -> f32 {
        (self.real_hi - self.real_lo) * (self.imag_hi - self.imag_lo)
    }

    fn intersect(self, other: impl Into<Self>) -> Option<Self> {
        todo!()
    }

    fn overlaps(self, other: impl Into<Self>) -> bool {
        self.intersect(other).is_some()
    }

    fn contains(self, other: impl Into<Self>) -> bool {
        let other = other.into();
        self.intersect(other) == Some(other)
    }
}
impl From<Square> for Window {
    fn from(value: Square) -> Self {
        Window {
            real_lo: value.real_lo(),
            real_hi: value.real_hi(),
            imag_lo: value.imag_lo(),
            imag_hi: value.imag_hi(),
        }
    }
}
impl PartialOrd for Window {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            return Some(Ordering::Equal);
        }
        if other.real_lo <= self.real_lo
            && self.real_hi <= other.real_hi
            && other.imag_lo <= self.imag_lo
            && self.imag_hi <= other.imag_hi
        {
            return Some(Ordering::Less);
        }
        if self.real_lo <= other.real_lo
            && other.real_hi <= self.real_hi
            && self.imag_lo <= other.imag_lo
            && other.imag_hi <= self.imag_hi
        {
            return Some(Ordering::Greater);
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(align(32))]
#[repr(C)]
struct Square {
    real_lo: f32,
    imag_lo: f32,
    real_hi: f32,
    imag_hi: f32,
    // real_mid: f32,
    // imag_mid: f32,
    // rad: f32,
}
impl Square {
    // fn new(real_lo: f32, real_hi: f32, imag_lo: f32, imag_hi: f32) -> Self {
    //     assert!(real_lo < real_hi);
    //     assert!(imag_lo < imag_hi);
    //     {
    //         let dx = real_hi as f64 - real_lo as f64;
    //         let dy = imag_hi as f64 - imag_lo as f64;
    //         let diff = dx - dy;
    //         let ratio = dx / dy;
    //         assert!(diff.abs() < 1e-4 || (1.0 - ratio).abs() < 1e-4);
    //     }
    //     Self {
    //         real_lo,
    //         real_hi,
    //         imag_lo,
    //         imag_hi,
    //     }
    // }
    const fn try_new(real_lo: f32, real_hi: f32, imag_lo: f32, imag_hi: f32) -> Option<Self> {
        if !(real_lo < real_hi && imag_lo < imag_hi && {
            let dx = real_hi as f64 - real_lo as f64;
            let dy = imag_hi as f64 - imag_lo as f64;
            let diff = dx - dy;
            let ratio = dx / dy;
            diff.abs() < 1e-4 || (1.0 - ratio).abs() < 1e-4
        }) {
            None
        } else {
            Some(Self {
                real_lo,
                real_hi,
                imag_lo,
                imag_hi,
            })
            // Some(Self {
            //     real_mid: (real_lo + real_hi) / 2.0,
            //     imag_mid: (imag_lo + imag_hi) / 2.0,
            //     rad: (real_hi - real_lo) / 2.0,
            // })
        }
    }

    fn real_mid(self) -> f32 {
        (self.real_hi + self.real_lo) / 2.0
        // self.real_mid
    }

    fn imag_mid(self) -> f32 {
        (self.imag_hi + self.imag_lo) / 2.0
        // self.imag_mid
    }

    fn rad(self) -> f32 {
        (self.real_hi - self.real_lo) / 2.0
        // self.rad
    }

    fn real_lo(self) -> f32 {
        self.real_lo
        // self.real_mid - self.rad
    }

    fn real_hi(self) -> f32 {
        self.real_hi
        // self.real_mid + self.rad
    }

    fn imag_lo(self) -> f32 {
        self.imag_lo
        // self.imag_mid - self.rad
    }

    fn imag_hi(self) -> f32 {
        self.imag_hi
        // self.imag_mid + self.rad
    }

    // fn area(self) -> f32 {
    //     (self.real_hi - self.real_lo) * (self.imag_hi - self.imag_lo)
    // }

    // fn contains(self, real: f32, imag: f32) -> bool {
    //     (self.real_lo..=self.real_hi).contains(&real)
    //         && (self.imag_lo..=self.imag_hi).contains(&imag)
    // }
    // #[inline(never)]
    fn contains(self, real: f32, imag: f32) -> bool {
        (self.real_lo..=self.real_hi).contains(&real)
            && (self.imag_lo..=self.imag_hi).contains(&imag)
        // (self.real_mid() - real).abs() <= self.rad() && (self.imag_mid() - imag).abs() <= self.rad()
        // f32::max(
        //     (self.real_mid() - real).abs(),
        //     (self.imag_mid() - imag).abs(),
        // ) <= self.rad()
    }

    // #[inline(never)]
    // fn overlaps(self, other: Self) -> bool {
    //     ((self.real_mid() - other.real_mid()).abs() <= (self.rad() + other.rad()))
    //         && ((self.imag_mid() - other.imag_mid()).abs() <= (self.rad() + other.rad()))
    //     // f32::max(
    //     //     (self.real_mid() - other.real_mid()).abs(),
    //     //     (self.imag_mid() - other.imag_mid()).abs(),
    //     // ) <= self.rad() + other.rad()
    // }

    fn overlaps(self, other: Self) -> bool {
        let real_lo = f32::max(self.real_lo(), other.real_lo());
        let real_hi = f32::min(self.real_hi(), other.real_hi());
        let imag_lo = f32::max(self.imag_lo(), other.imag_lo());
        let imag_hi = f32::min(self.imag_hi(), other.imag_hi());
        real_lo <= real_hi && imag_lo <= imag_hi
    }

    // #[inline(never)]
    // fn overlaps(self, other: Self) -> bool {
    //     let self_lo: Simd<f32, 2> = [self.real_lo(), self.imag_lo()].into();
    //     let self_hi: Simd<f32, 2> = [self.real_hi(), self.imag_hi()].into();
    //     let other_lo: Simd<f32, 2> = [other.real_lo(), other.imag_lo()].into();
    //     let other_hi: Simd<f32, 2> = [other.real_hi(), other.imag_hi()].into();
    //     let max = self_lo.simd_max(other_lo);
    //     let min = self_hi.simd_min(other_hi);
    //     max.simd_gt(min) == Mask::from_bitmask(0)
    // }

    // #[inline(never)]
    // fn overlaps(self, other: Self) -> bool {
    //     !(self.real_hi() < other.real_lo()
    //         || other.real_hi() < self.real_lo()
    //         || self.imag_hi() < other.imag_lo()
    //         || other.imag_hi() < self.imag_lo())
    // }
}
impl PartialOrd for Square {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        std::convert::Into::<Window>::into(*self).partial_cmp(&(*other).into())
    }
}

/// represents the average of `count` colors
#[derive(Debug, Default)]
#[repr(align(32))]
struct ColorBuilder {
    // count: NonZero<u32>,
    count: u32,
    r: u32,
    g: u32,
    b: u32,
}
impl ColorBuilder {
    fn build(self) -> Option<Color32> {
        if self.count == 0 {
            None
        } else {
            Some(Color32::from_rgb(
                (self.r / self.count) as u8,
                (self.g / self.count) as u8,
                (self.b / self.count) as u8,
            ))
        }
    }
}
impl From<Color32> for ColorBuilder {
    fn from(value: Color32) -> Self {
        Self {
            count: 1,
            r: value.r() as _,
            g: value.g() as _,
            b: value.b() as _,
        }
    }
}
impl AddAssign<ColorBuilder> for ColorBuilder {
    fn add_assign(&mut self, rhs: ColorBuilder) {
        self.count += rhs.count;
        self.r += rhs.r;
        self.g += rhs.g;
        self.b += rhs.b;
    }
}
impl Add<ColorBuilder> for ColorBuilder {
    type Output = ColorBuilder;

    fn add(self, rhs: ColorBuilder) -> ColorBuilder {
        let mut result = self;
        result += rhs;
        result
    }
}
impl Sum for ColorBuilder {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        let mut ret = Self::default();
        for c in iter {
            ret += c;
        }
        ret
    }
}

#[derive(Debug)]
struct Tree {
    dom: Square,
    color: Color32,
    /// 0 1
    ///
    /// 2 3
    children: Option<[Box<Tree>; 4]>,
}
impl Tree {
    fn new_leaf(window: Square) -> Self {
        Self {
            // color: mandelbrot_sample(0.0, 0.0, window.real_mid(), window.imag_mid()).color(),
            color: metabrot_sample(window.real_mid(), window.imag_mid()).color(),
            dom: window,
            children: None,
        }
    }

    fn is_leaf(&self) -> bool {
        self.children.is_none()
    }

    fn child_i_closest_to(&self, real: f32, imag: f32) -> Option<usize> {
        let Some(children) = &self.children else {
            return None;
        };
        Some(
            (0..children.len())
                .map(|i| {
                    let dx = children[i].dom.real_mid() - real;
                    let dy = children[i].dom.imag_mid() - imag;
                    (i, dx * dx + dy * dy)
                })
                .min_by(|(_, left), (_, right)| left.total_cmp(right))
                .unwrap()
                .0,
        )
    }

    fn split(&mut self) {
        if let Some(children) = {
            || {
                Some([
                    Box::new(Self::new_leaf(Square::try_new(
                        self.dom.real_lo(),
                        self.dom.real_mid(),
                        self.dom.imag_mid(),
                        self.dom.imag_hi(),
                    )?)),
                    Box::new(Self::new_leaf(Square::try_new(
                        self.dom.real_mid(),
                        self.dom.real_hi(),
                        self.dom.imag_mid(),
                        self.dom.imag_hi(),
                    )?)),
                    Box::new(Self::new_leaf(Square::try_new(
                        self.dom.real_lo(),
                        self.dom.real_mid(),
                        self.dom.imag_lo(),
                        self.dom.imag_mid(),
                    )?)),
                    Box::new(Self::new_leaf(Square::try_new(
                        self.dom.real_mid(),
                        self.dom.real_hi(),
                        self.dom.imag_lo(),
                        self.dom.imag_mid(),
                    )?)),
                ])
            }
        }() {
            self.children = Some(children);
        }
    }

    fn count_overlaps(&self, window: Square) -> u32 {
        todo!()
    }

    fn count_contained(&self, window: Square) -> u32 {
        todo!()
    }

    // actually i think this basically just returns 1
    // fn count_samples_weak(&self, pixel: Square) -> u32 {
    //     if !self.window.overlaps(pixel) {
    //         return 0;
    //     }
    //     (if pixel.contains(self.window.real_mid(), self.window.imag_mid()) {
    //         1
    //     } else {
    //         0
    //     } + if self.is_leaf() {
    //         0
    //     } else {
    //         self.children
    //             .as_ref()
    //             .unwrap()
    //             .iter()
    //             .map(|c| c.count_samples_weak(pixel))
    //             .sum()
    //     })
    // }

    fn count_samples_strong(&self, pixel: Square) -> u32 {
        if !self.dom.overlaps(pixel) {
            return 0;
        }
        (if pixel.contains(self.dom.real_mid(), self.dom.imag_mid()) {
            1
        } else {
            0
        } + if self.is_leaf() {
            0
        } else {
            let closest_child_i = self
                .child_i_closest_to(pixel.real_mid(), pixel.imag_mid())
                .unwrap();
            self.children.as_ref().unwrap()[closest_child_i].count_samples_strong(pixel)
        })
    }

    /// whether the pixel contains any samples
    #[inline(never)]
    fn contains_sample(&self, pixel: Square) -> bool {
        if !self.dom.overlaps(pixel) {
            return false;
        }
        if pixel.contains(self.dom.real_mid(), self.dom.imag_mid()) {
            return true;
        }
        if self.is_leaf() {
            return false;
        }
        self.children
            .as_ref()
            .unwrap()
            .iter()
            .any(|c| c.contains_sample(pixel))
    }

    // // TODO: rename
    // /// ensures that every pixel in the window contains at least subsamples leaves
    // // fn ensure_pixel_safe(&mut self, window: Window, pixel_width: f32, subsamples: u8) {
    // fn ensure_pixel_safe(&mut self, window: Window, pixel_width: f32) {
    //     if !window.overlaps(self.window) {
    //         return;
    //     }
    //     match self.children {
    //         Some(children) => children.iter().map(|c| c.overlaps(window)),
    //         None => todo!(),
    //     }
    //     todo!()
    // }

    // fn ensure_pixel_safe(&mut self, pixel: Square) {
    //     if !self.window.overlaps(pixel) {
    //         return;
    //     }
    //     match &self.children {
    //         Some(children) => if !children.iter().all(|c| c.window.overlaps(pixel) {todo!()}),
    //         None => {
    //             if self.window.contains(pixel) {
    //                 self.split();
    //                 for c in self.children.as_mut().unwrap() {
    //                     c.ensure_pixel_safe(pixel);
    //                 }
    //             }
    //         }
    //     };
    //     todo!()
    // }

    // fn ensure_pixel_safe(&mut self, pixel: Square) {
    //     if self.count_overlaps(pixel) >= 4 {
    //         return;
    //     }
    // }

    // TODO: subsampling / area average
    // every pixel must contain a node
    // fn ensure_pixel_safe(&mut self, pixel: Square) {
    //     // println!("ensure_pixel_safe: {self:?}");
    //     if !self.window.overlaps(pixel) {
    //         // println!("!self.window.overlaps(pixel)");
    //         return;
    //     }
    //     if self.window <= pixel {
    //         // println!("self.window <= pixel");
    //         return;
    //     }
    //     // match &mut self.children {
    //     //     Some(children) => {
    //     //         children
    //     //             .iter_mut()
    //     //             .map(|c| {
    //     //                 let dx = c.window.real_mid() - pixel.real_mid();
    //     //                 let dy = c.window.imag_mid() - pixel.imag_mid();
    //     //                 (c, dx * dx + dy * dy)
    //     //             })
    //     //             .min_by(|(_, left), (_, right)| left.total_cmp(right))
    //     //             .unwrap()
    //     //             .0
    //     //             .ensure_pixel_safe(pixel);
    //     //     }
    //     //     None => {
    //     //         self.split();
    //     //         for c in self.children.as_mut().unwrap().iter_mut() {
    //     //             c.ensure_pixel_safe(pixel)
    //     //         }
    //     //     }
    //     // };
    //     if self.is_leaf() {
    //         // println!("self.is_leaf()");
    //         self.split();
    //     }
    //     let closest_child_i = self
    //         .child_i_closest_to(pixel.real_mid(), pixel.imag_mid())
    //         .unwrap();
    //     self.children.as_mut().unwrap()[closest_child_i].ensure_pixel_safe(pixel);
    // }

    /// every pixel must contain a sample
    fn ensure_pixel_safe(&mut self, pixel: Square) {
        if !self.dom.overlaps(pixel) {
            return;
        }
        if pixel.contains(self.dom.real_mid(), self.dom.imag_mid()) {
            return;
        }
        if self.is_leaf() {
            self.split();
        }
        // TODO: this isn't really what i want
        if !self.is_leaf() {
            let closest_child_i = self
                .child_i_closest_to(pixel.real_mid(), pixel.imag_mid())
                .unwrap();
            self.children.as_mut().unwrap()[closest_child_i].ensure_pixel_safe(pixel);
        }
    }

    // fn is_strong_pixel_safe(&self, pixel: Square) -> bool {
    //     if !self.window.overlaps(pixel) {
    //         return false;
    //     }
    //     if self.window <= pixel {
    //         return true;
    //     }
    //     if self.is_leaf() {
    //         return false;
    //     }
    //     let closest_child_i = self
    //         .child_i_closest_to(pixel.real_mid(), pixel.imag_mid())
    //         .unwrap();
    //     self.children.as_ref().unwrap()[closest_child_i].is_strong_pixel_safe(pixel)
    // }

    // fn is_weak_pixel_safe(&self, pixel: Square) -> bool {
    //     if !self.window.overlaps(pixel) {
    //         return false;
    //     }
    //     if self.window <= pixel {
    //         return true;
    //     }
    //     if self.is_leaf() {
    //         return false;
    //     }
    //     self.children
    //         .as_ref()
    //         .unwrap()
    //         .iter()
    //         .any(|c| c.is_weak_pixel_safe(pixel))
    // }

    /// ensures that we have < n nodes
    /// or maybe that each pixel contains at most n leaves
    /// or maybe if you're in the window, you get at most subsamples leaves,
    /// if you're not in the window, you all collectively get m leaves
    fn prune(&mut self, window: Window, pixel_width: f32, n: u32, subsamples: u8) {
        todo!()
    }

    // /// the average color of leaves inside the pixel weighted by area that's overlapping the pixel
    // /// or maybe weighted by distance to the center of the pixel
    // /// the color of the highest node contained in pixel
    // fn color(&self, pixel: Square) -> Option<Color32> {
    //     if !self.window.overlaps(pixel) {
    //         return None;
    //     }
    //     if self.window <= pixel {
    //         return Some(self.color);
    //     }
    //     if self.is_leaf() {
    //         // we're too zoomed in
    //         return None;
    //     }
    //     // TODO: i think it's actually possible that it's not the child closest to the pixel center that has a child eventually inside pixel
    //     let closest_child_i = self
    //         .child_i_closest_to(pixel.real_mid(), pixel.imag_mid())
    //         .unwrap();
    //     self.children.as_ref().unwrap()[closest_child_i].color(pixel)
    // }

    // TODO: average of all samples inside the pixel
    /// the color of the first node who's sample is inside the pixel
    // fn color(&self, pixel: Square) -> ColorBuilder {
    //     if !self.dom.overlaps(pixel) {
    //         return ColorBuilder::default();
    //     }
    //     (if pixel.contains(self.dom.real_mid(), self.dom.imag_mid()) {
    //         self.color.into()
    //     } else {
    //         ColorBuilder::default()
    //     } + match &self.children {
    //         Some(children) => children.iter().map(|c| c.color(pixel)).sum(),
    //         None => ColorBuilder::default(),
    //     })
    // }
    #[inline(never)]
    fn color(&self, pixel: Square) -> ColorBuilder {
        let mut stack = Vec::with_capacity(64);
        stack.push(self);
        let mut ret = ColorBuilder::default();
        while let Some(node) = stack.pop() {
            if !node.dom.overlaps(pixel) {
                continue;
            }
            if pixel.contains(node.dom.real_mid(), node.dom.imag_mid()) {
                ret += node.color.into();
            }
            if let Some(children) = &node.children {
                stack.extend(children.iter().map(|c| c.as_ref()));
            }
        }
        // assert_eq!(stack.capacity(), 64);
        ret
    }

    // fn validate(&self) {
    //     assert!(self.window.real_lo < self.window.real_hi);
    //     assert!(self.window.imag_lo < self.window.imag_hi);
    //     if let Some(children) = &self.children {
    //         for c in children {
    //             assert!(self.window.real_lo <= c..real_lo);
    //             assert!(c.real_hi <= self.window.real_hi);
    //             assert!(self.window.imag_lo <= c.imag_lo);
    //             assert!(c.imag_hi <= self.window.imag_hi);
    //         }
    //     }
    // }
}

struct App {
    tree: Tree,
    stride: usize,
    camera: Camera,
    velocity: Vec2,
    dts: egui::util::History<f32>,
}
impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            tree: Tree::new_leaf(Square::try_new(-4.0, 4.0, -4.0, 4.0).unwrap()),
            stride: 2,
            camera: Camera::new(0.0, 0.0, 2.0),
            velocity: Vec2::ZERO,
            dts: egui::util::History::new(1..100, 0.1),
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show(ctx, |ui| {
                self.dts.add(
                    ctx.input(|input_state| input_state.time),
                    ctx.input(|input_state| input_state.stable_dt),
                );

                // panning stuff
                {
                    let rect = ui.available_rect_before_wrap();
                    let r = ui.allocate_rect(rect, egui::Sense::click_and_drag());

                    let pan_offset = |pan_vec: Vec2, real_rad: f32| -> (f32, f32) {
                        (
                            -2.0 * pan_vec.x / rect.size().x * real_rad,
                            2.0 * pan_vec.y * (real_rad / rect.size().x),
                        )
                    };

                    let dt = ctx.input(|input_state| input_state.stable_dt);
                    if r.is_pointer_button_down_on() && ctx.input(|i| i.pointer.primary_down()) {
                        self.camera += pan_offset(r.drag_delta(), self.camera.real_rad);
                        self.velocity = r.drag_delta() / dt;
                    } else {
                        const VELOCITY_DAMPING: f32 = 0.9999;
                        self.camera += pan_offset(self.velocity * dt, self.camera.real_rad);
                        self.velocity *= (1.0 - VELOCITY_DAMPING).powf(dt);
                    }
                    if self.velocity.length_sq() < 0.0001 {
                        self.velocity = Vec2::ZERO;
                    }
                    if r.contains_pointer()
                        && let Some(mouse_pos) = ctx.input(|i| i.pointer.latest_pos())
                    {
                        let mouse = mouse_pos - rect.center();
                        let zoom = ctx.input(|i| (i.smooth_scroll_delta.y / 300.0).exp());
                        self.camera += pan_offset(-mouse, self.camera.real_rad);
                        self.camera.real_rad /= zoom;
                        self.camera += pan_offset(mouse, self.camera.real_rad);
                    }
                }

                let camera_map = CameraMap::new(ui.max_rect(), self.camera);
                // ensure_pixel_safe for all pixels
                // if ctx.input(|i| i.key_down(Key::Space)) {
                //     for (_, pixel) in camera_map.pixels(self.stride) {
                //         self.tree.ensure_pixel_safe(pixel);
                //     }
                // }

                // // ensure_pixel_safe with time bound
                // if !ctx.input(|i| i.key_down(Key::Space)) {
                //     const MAX_TIME: Duration = Duration::from_millis(100);
                //     let start = Instant::now();
                //     let mut rng = rand::rng();
                //     let pixels = {
                //         let mut pixels = camera_map
                //             .pixels(self.stride)
                //             .map(|(_, pixel)| pixel)
                //             .filter(|pixel| !self.tree.contains_sample(*pixel))
                //             .collect::<Vec<_>>();
                //         pixels.shuffle(&mut rng);
                //         pixels
                //     };
                //     for pixel in pixels {
                //         if start.elapsed() > MAX_TIME {
                //             break;
                //         }
                //         if !self.tree.contains_sample(pixel) {
                //             self.tree.ensure_pixel_safe(pixel);
                //         }
                //     }
                // }

                // ensure_pixel_safe with time bound
                // but with a decreasing stride
                if !ctx.input(|i| i.key_down(Key::Space)) {
                    const MAX_TIME: Duration = Duration::from_millis(100);
                    let start = Instant::now();
                    let mut rng = rand::rng();

                    'outer: for stride_pow in
                        (self.stride.ilog2()..(ui.max_rect().width() as u32).ilog2()).rev()
                    {
                        let stride = 1 << stride_pow;

                        let pixels = {
                            let mut pixels = camera_map
                                .pixels(stride)
                                .map(|(_, pixel)| pixel)
                                .filter(|pixel| !self.tree.contains_sample(*pixel))
                                .collect::<Vec<_>>();
                            pixels.shuffle(&mut rng);
                            pixels
                        };
                        for pixel in pixels {
                            if start.elapsed() > MAX_TIME {
                                break 'outer;
                            }
                            if !self.tree.contains_sample(pixel) {
                                self.tree.ensure_pixel_safe(pixel);
                            }
                        }
                    }
                }

                // // draw the fractal
                // {
                //     let painter = ui.painter_at(ui.max_rect());

                //     painter.rect_filled(ui.max_rect(), 0.0, Color32::RED);

                //     // const STRIDE: u32 = 1;
                //     for (rect, pixel) in camera_map.pixels(self.stride) {
                //         let color = self.tree.color(pixel).build().unwrap_or(Color32::MAGENTA);
                //         // .expect("tree invariant not satisfied");

                //         painter.rect_filled(rect, 0.0, color);
                //     }
                // }

                // draw the fractal,
                // but instead of drawing error magenta,
                // draw pixels decreasing in stride
                {
                    let painter = ui.painter_at(ui.max_rect());

                    painter.rect_filled(ui.max_rect(), 0.0, Color32::RED);

                    // don't draw pixels that will be completely overdrawn in the future
                    let stride_pow_hi = {
                        || {
                            for stride_pow in
                                (self.stride.ilog2()..(ui.max_rect().width() as u32).ilog2()).rev()
                            {
                                let stride = 1 << stride_pow;
                                for (_, pixel) in camera_map.pixels(stride) {
                                    if !self.tree.contains_sample(pixel) {
                                        return stride_pow + 1;
                                    }
                                }
                            }
                            // idk why this need + 1
                            self.stride.ilog2() + 1
                        }
                    }();

                    for stride_pow in (self.stride.ilog2()..=stride_pow_hi).rev() {
                        let stride = 1 << stride_pow;
                        for (rect, pixel) in camera_map.pixels(stride) {
                            if let Some(color) = self.tree.color(pixel).build() {
                                painter.rect_filled(rect, 0.0, color);
                            }
                        }
                    }
                }

                // // draw sequence of nodes that contain the mouse
                // {
                //     if let Some(mouse_pos) = ctx.input(|i| i.pointer.latest_pos()) {
                //         fn draw_node(
                //             node: &Tree,
                //             depth: u32,
                //             painter: &egui::Painter,
                //             camera_map: &CameraMap,
                //             real: f32,
                //             imag: f32,
                //         ) {
                //             // println!("here");
                //             painter.rect_stroke(
                //                 camera_map.window_to_rect(node.window),
                //                 0.0,
                //                 egui::Stroke::new(
                //                     3.0,
                //                     // Color32::from_rgb(100, 100, 255u32.saturating_sub(5*depth) as u8),
                //                     // Color32::from_rgb(100, 100, {
                //                     //     let mut h = DefaultHasher::new();
                //                     //     depth.hash(&mut h);
                //                     //     h.finish() as u8
                //                     // }),
                //                     {
                //                         let mut h = DefaultHasher::new();
                //                         depth.hash(&mut h);
                //                         let hash = h.finish();
                //                         Color32::from_rgb(
                //                             (hash >> 24) as u8,
                //                             (hash >> 16) as u8,
                //                             (hash >> 8) as u8,
                //                         )
                //                     },
                //                 ),
                //                 egui::StrokeKind::Inside,
                //             );
                //             let Some(children) = &node.children else {
                //                 return;
                //             };
                //             for child in children {
                //                 if child.window.contains(real, imag) {
                //                     draw_node(child, depth + 1, painter, camera_map, real, imag);
                //                 }
                //             }
                //         }
                //         let (real, imag) = camera_map.pos_to_complex(mouse_pos);

                //         draw_node(
                //             &self.tree,
                //             0,
                //             &ui.painter_at(ui.max_rect()),
                //             &camera_map,
                //             real,
                //             imag,
                //         );
                //         // panic!();
                //     }
                // }

                // TODO: debug draw sequence of nodes that eventually have a child who's sample is inside the pixel the mouse is in

                // // debug coloring of how many samples are inside each pixel
                // {
                //     let painter = ui.painter_at(ui.max_rect());
                //     for (rect, pixel) in camera_map.pixels(self.stride) {
                //         // let color = self.tree.color(pixel).unwrap_or(Color32::MAGENTA);
                //         let count = self.tree.count_samples_strong(pixel);
                //         painter.rect_filled(
                //             rect,
                //             0.0,
                //             if count == 0 {
                //                 Color32::MAGENTA
                //             } else {
                //                 Color32::from_gray((count * 50).min(255) as u8)
                //             },
                //         );
                //     }
                // }

                // area is to allow the frame to be drawn on top of the fractal
                egui::Area::new(egui::Id::new("area"))
                    .constrain_to(ctx.screen_rect())
                    .anchor(egui::Align2::LEFT_TOP, egui::Vec2::ZERO)
                    .show(ui.ctx(), |ui| {
                        // frame rate
                        {
                            let average_dt = self
                                .dts
                                .average()
                                .expect("we added one this frame so dts must be non-empty");
                            // ui.label(format!(
                            //     "    dt: {:08.04}\n1/dt: {:08.04}",
                            //     average_dt,
                            //     1.0 / average_dt,
                            // ));
                            ui.label(
                                RichText::new(format!(
                                    "    dt: {:08.04}\n1/dt: {:08.04}",
                                    average_dt,
                                    1.0 / average_dt,
                                ))
                                .background_color(Color32::BLACK),
                            );
                        }

                        // // view stuff
                        // {
                        //     ui.label(format!(
                        //         "center: {:12.09} + {:12.09}i\nreal_radius: {:12.09}",
                        //         self.camera.real_mid,
                        //         self.camera.imag_mid,
                        //         self.camera.real_rad,
                        //     ));
                        // }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_map() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 30.0), Pos2::new(10.0, 50.0));
        let camera = Camera::new(1.0, 2.0, 3.0);
        let camera_map = CameraMap::new(rect, camera);

        assert_eq!(camera_map.camera.real_lo(), -2.0);
        assert_eq!(camera_map.camera.real_hi(), 4.0);
        assert_eq!(camera_map.imag_lo(), -4.0);
        assert_eq!(camera_map.imag_hi(), 8.0);

        assert!((0.0 - camera_map.real_to_x(-2.0)).abs() < 1e-4);
        assert!((10.0 - camera_map.real_to_x(4.0)).abs() < 1e-4);
        assert!((30.0 - camera_map.imag_to_y(8.0)).abs() < 1e-4);
        assert!((50.0 - camera_map.imag_to_y(-4.0)).abs() < 1e-4);

        for (p, c) in [
            (Pos2::new(0.0, 30.0), (-2.0, 8.0)),
            (Pos2::new(0.0, 50.0), (-2.0, -4.0)),
            (Pos2::new(10.0, 30.0), (4.0, 8.0)),
            (Pos2::new(10.0, 50.0), (4.0, -4.0)),
        ] {
            let c_actual = camera_map.pos_to_complex(p);
            assert!((c.0 - c_actual.0).abs() + (c.1 - c_actual.1).abs() < 1e-4);
            let p_actual = camera_map.complex_to_pos(c);
            assert!((p - p_actual).length() < 1e-4);
        }

        for pos in [
            Pos2::new(1.0, 30.0),
            Pos2::new(1.0, 50.0),
            Pos2::new(10.0, 30.0),
            Pos2::new(10.0, 50.0),
            Pos2::new(9.871, 38.635),
            Pos2::new(1.248, 45.656),
            Pos2::new(3.463, 48.559),
            Pos2::new(1.684, 32.323),
            Pos2::new(2.809, 31.250),
            Pos2::new(8.142, 36.146),
            Pos2::new(3.938, 48.579),
            Pos2::new(5.761, 42.575),
            Pos2::new(9.691, 42.933),
            Pos2::new(2.457, 30.097),
        ] {
            assert!(
                (pos - camera_map.complex_to_pos(camera_map.pos_to_complex(pos))).length() < 1e-4
            );
        }
        for c in [
            (-2.0, -1.0),
            (-2.0, 5.0),
            (4.0, -1.0),
            (4.0, 5.0),
            (-1.885, -0.978),
            (0.254, 0.793),
            (3.634, 3.274),
            (3.332, 1.716),
            (0.063, 3.933),
            (2.132, 1.927),
            (1.848, 4.781),
            (2.971, 4.047),
            (0.194, 2.966),
            (1.173, -0.435),
        ] {
            let actual = camera_map.pos_to_complex(camera_map.complex_to_pos(c));
            assert!((c.0 - actual.0).abs() + (c.1 - actual.1).abs() < 1e-4);
        }
    }
}
