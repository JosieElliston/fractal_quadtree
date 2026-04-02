use std::cmp::Ordering;

use eframe::egui::{Pos2, Rect, Vec2};

use crate::{fixed::*, inv_lerp, lerp};

// TODO: make this a mapping from egui rect to complex window
#[derive(Debug, Clone, Copy)]
pub(crate) struct Camera {
    real_mid: Real,
    imag_mid: Imag,
    real_rad: Real,
}
impl Camera {
    pub(crate) fn new(real_mid: Fixed, imag_mid: Fixed, real_rad: Fixed) -> Self {
        assert!(real_rad > Fixed::ZERO);
        Self {
            real_mid,
            imag_mid,
            real_rad,
        }
    }

    pub(crate) fn real_lo(self) -> Real {
        self.real_mid - self.real_rad
    }
    pub(crate) fn real_hi(self) -> Real {
        self.real_mid + self.real_rad
    }
    pub(crate) fn imag_mid(self) -> Imag {
        self.imag_mid
    }
    pub(crate) fn real_mid(self) -> Real {
        self.real_mid
    }
    pub(crate) fn real_rad(self) -> Real {
        self.real_rad
    }
    pub(crate) fn real_rad_mut(&mut self) -> &mut Real {
        &mut self.real_rad
    }
    pub(crate) fn mid(self) -> (Real, Imag) {
        (self.real_mid, self.imag_mid)
    }

    // these are undefined for a `Camera`,
    // they need an aspect ratio,
    // which is part of `CameraMap`
    // pub(crate) fn imag_lo(self) -> f32 {
    //     todo!()
    // }
    // pub(crate) fn imag_hi(self) -> f32 {
    //     todo!()
    // }
    // pub(crate) fn imag_rad(self) -> f32 {
    //     todo!()
    // }
}
// impl std::ops::AddAssign<(f32, f32)> for Camera {
//     fn add_assign(&mut self, rhs: (f32, f32)) {
//         self.real_mid += rhs.0;
//         self.imag_mid += rhs.1;
//     }
// }
impl std::ops::AddAssign<(Real, Imag)> for Camera {
    fn add_assign(&mut self, (real, imag): (Real, Imag)) {
        self.real_mid += real;
        self.imag_mid += imag;
    }
}

pub(crate) struct CameraMap {
    rect: Rect,
    camera: Camera,
}
impl CameraMap {
    pub(crate) fn new(rect: Rect, camera: Camera) -> Self {
        assert!(rect.min.x < rect.max.x);
        assert!(rect.min.y < rect.max.y);
        Self { rect, camera }
    }

    pub(crate) fn rect(&self) -> Rect {
        self.rect
    }
    /// equivalent to `self.rect_to_window(self.rect())`
    pub(crate) fn window(&self) -> Window {
        Window::new(
            self.camera.real_lo(),
            self.camera.real_hi(),
            self.imag_lo(),
            self.imag_hi(),
        )
    }

    pub(crate) fn imag_lo(&self) -> Imag {
        self.camera.imag_mid - self.imag_rad()
    }
    pub(crate) fn imag_hi(&self) -> Imag {
        self.camera.imag_mid + self.imag_rad()
    }
    pub(crate) fn imag_rad(&self) -> Imag {
        self.camera
            .real_rad
            .mul_f32(self.rect.height() / self.rect.width())
    }

    pub(crate) fn x_to_real(&self, x: f32) -> Real {
        // -2.0 * x / self.rect.size().x * self.camera.real_rad
        Fixed::lerp(
            self.camera.real_lo(),
            self.camera.real_hi(),
            inv_lerp(self.rect.min.x as f64, self.rect.max.x as f64, x as f64),
        )
    }
    pub(crate) fn y_to_imag(&self, y: f32) -> Imag {
        // 2.0 * y * (self.camera.real_rad / self.rect.size().x)
        Fixed::lerp(
            self.imag_lo(),
            self.imag_hi(),
            1.0 - inv_lerp(self.rect.min.y as f64, self.rect.max.y as f64, y as f64),
        )
    }
    pub(crate) fn real_to_x(&self, real: Real) -> f32 {
        lerp(
            self.rect.min.x as f64,
            self.rect.max.x as f64,
            Fixed::inv_lerp(self.camera.real_lo(), self.camera.real_hi(), real),
        ) as f32
    }
    pub(crate) fn imag_to_y(&self, imag: Imag) -> f32 {
        lerp(
            self.rect.min.y as f64,
            self.rect.max.y as f64,
            1.0 - Fixed::inv_lerp(self.imag_lo(), self.imag_hi(), imag),
        ) as f32
    }

    pub(crate) fn pos_to_complex(&self, pos: Pos2) -> (Real, Imag) {
        (self.x_to_real(pos.x), self.y_to_imag(pos.y))
    }
    pub(crate) fn complex_to_pos(&self, (real, imag): (Real, Imag)) -> Pos2 {
        Pos2::new(self.real_to_x(real), self.imag_to_y(imag))
    }

    pub(crate) fn rect_to_window(&self, rect: Rect) -> Window {
        Window::new(
            self.x_to_real(rect.min.x),
            self.x_to_real(rect.max.x),
            self.y_to_imag(rect.max.y),
            self.y_to_imag(rect.min.y),
        )
    }
    pub(crate) fn window_to_rect(&self, window: impl Into<Window>) -> Rect {
        let window = window.into();
        Rect {
            min: self.complex_to_pos((window.real_lo, window.imag_hi)),
            max: self.complex_to_pos((window.real_hi, window.imag_lo)),
        }
    }

    pub(crate) fn pixels(
        &self,
        stride: usize,
    ) -> impl Iterator<Item = ((usize, usize), Rect, Square)> {
        (0..self.rect.size().y as usize)
            .step_by(stride)
            .flat_map(move |row| {
                (0..self.rect.size().x as usize)
                    .step_by(stride)
                    .filter_map(move |col| {
                        Some((
                            (row / stride, col / stride),
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
                // .map(move |col| {
                //     (
                //         (row / stride, col / stride),
                //         Rect::from_min_size(
                //             Pos2::new(col as f32, row as f32),
                //             Vec2::new(stride as f32, stride as f32),
                //         ),
                //         Square::try_new(
                //             self.x_to_real(col as f32),
                //             self.x_to_real((col + stride) as f32),
                //             self.y_to_imag((row + stride) as f32),
                //             self.y_to_imag(row as f32),
                //         )
                //         .unwrap(),
                //     )
                // })
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(align(32))]
pub(crate) struct Window {
    real_lo: Real,
    real_hi: Real,
    imag_lo: Imag,
    imag_hi: Imag,
}
impl Window {
    /// fails if the window would be empty
    /// including if it would have zero width or height
    pub(crate) fn try_new(
        real_lo: Real,
        real_hi: Real,
        imag_lo: Imag,
        imag_hi: Imag,
    ) -> Option<Self> {
        if !(real_lo < real_hi && imag_lo < imag_hi) {
            return None;
        }
        Some(Self {
            real_lo,
            real_hi,
            imag_lo,
            imag_hi,
        })
    }
    pub(crate) fn new(real_lo: Real, real_hi: Real, imag_lo: Imag, imag_hi: Imag) -> Self {
        // assert!(real_lo.is_finite());
        // assert!(real_hi.is_finite());
        // assert!(imag_lo.is_finite());
        // assert!(imag_hi.is_finite());
        assert!(real_lo < real_hi);
        assert!(imag_lo < imag_hi);
        Self {
            real_lo,
            real_hi,
            imag_lo,
            imag_hi,
        }
    }
    pub(crate) fn from_center_size(
        real_mid: Real,
        imag_mid: Imag,
        real_diam: Real,
        imag_diam: Imag,
    ) -> Self {
        assert!(real_diam > Fixed::ZERO);
        assert!(imag_diam > Fixed::ZERO);
        let real_rad = real_diam.div2_floor();
        let imag_rad = imag_diam.div2_floor();
        let real_lo = real_mid - real_rad;
        let real_hi = real_mid + real_rad;
        let imag_lo = imag_mid - imag_rad;
        let imag_hi = imag_mid + imag_rad;
        Self::new(real_lo, real_hi, imag_lo, imag_hi)
    }

    pub(crate) fn real_lo(self) -> Real {
        self.real_lo
    }
    pub(crate) fn real_hi(self) -> Real {
        self.real_hi
    }
    pub(crate) fn real_mid(self) -> Real {
        (self.real_hi + self.real_lo).div2_exact().unwrap()
    }
    pub(crate) fn real_rad(self) -> Real {
        (self.real_hi - self.real_lo).div2_exact().unwrap()
    }

    pub(crate) fn imag_lo(self) -> Imag {
        self.imag_lo
    }
    pub(crate) fn imag_hi(self) -> Imag {
        self.imag_hi
    }
    pub(crate) fn imag_rad(self) -> Imag {
        (self.imag_hi - self.imag_lo).div2_exact().unwrap()
    }
    pub(crate) fn imag_mid(self) -> Imag {
        (self.imag_hi + self.imag_lo).div2_exact().unwrap()
    }

    // pub(crate) fn area(self) -> f32 {
    //     (self.real_hi - self.real_lo) * (self.imag_hi - self.imag_lo)
    // }

    pub(crate) fn intersect(self, other: impl Into<Self>) -> Option<Self> {
        let other = other.into();
        let real_lo = Fixed::max(self.real_lo, other.real_lo);
        let real_hi = Fixed::min(self.real_hi, other.real_hi);
        let imag_lo = Fixed::max(self.imag_lo, other.imag_lo);
        let imag_hi = Fixed::min(self.imag_hi, other.imag_hi);
        Self::try_new(real_lo, real_hi, imag_lo, imag_hi)
    }

    // fn overlaps(self, other: impl Into<Self>) -> bool {
    //     self.intersect(other).is_some()
    // }
    pub(crate) fn overlaps(self, other: impl Into<Self>) -> bool {
        let other = other.into();
        let real_lo = Fixed::max(self.real_lo, other.real_lo);
        let real_hi = Fixed::min(self.real_hi, other.real_hi);
        let imag_lo = Fixed::max(self.imag_lo, other.imag_lo);
        let imag_hi = Fixed::min(self.imag_hi, other.imag_hi);
        real_lo <= real_hi && imag_lo <= imag_hi
    }

    pub(crate) fn contains_point(self, (real, imag): (Real, Imag)) -> bool {
        (self.real_lo..=self.real_hi).contains(&real)
            && (self.imag_lo..=self.imag_hi).contains(&imag)
    }

    // fn contains(self, other: impl Into<Self>) -> bool {
    //     let other = other.into();
    //     self.intersect(other) == Some(other)
    // }

    /// returns an iterator over the centers of rectangles of a width by height grid
    /// so each point will be inside the window
    /// and the average of the points will be the center of the window
    pub(crate) fn grid(
        self,
        width: usize,
        height: usize,
    ) -> impl Iterator<Item = impl Iterator<Item = (Real, Imag)>> {
        (0..height).map(move |row| {
            let imag = Fixed::lerp(
                self.imag_lo(),
                self.imag_hi(),
                1.0 - (row as f64 + 0.5) / height as f64,
            );
            (0..width).map(move |col| {
                let real = Fixed::lerp(
                    self.real_lo(),
                    self.real_hi(),
                    (col as f64 + 0.5) / width as f64,
                );
                (real, imag)
            })
        })
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
pub(crate) struct Square {
    // real_lo: f32,
    // imag_lo: f32,
    // real_hi: f32,
    // imag_hi: f32,
    real_mid: Real,
    imag_mid: Imag,
    rad: Fixed,
}
impl Square {
    /// for pixels, where things are imprecise
    pub(crate) fn try_new(
        real_lo: Real,
        real_hi: Real,
        imag_lo: Imag,
        imag_hi: Imag,
    ) -> Option<Self> {
        if !(real_lo <= real_hi && imag_lo <= imag_hi) {
            return None;
        }
        if !(real_lo < real_hi && imag_lo < imag_hi) {
            return None;
        }
        // if !{
        //     let dx = real_hi - real_lo;
        //     let dy = imag_hi - imag_lo;
        //     let diff = dx - dy;
        //     let ratio = dx / dy;
        //     diff.abs() < 1e-4 || (1.0 - ratio).abs() < 1e-4
        // } {
        //     return None;
        // }

        // dbg!(real_lo, real_hi, -real_lo);
        // println!(
        //     "real_lo: {}, real_hi: {}, -real_lo: {}",
        //     real_lo, real_hi, -real_lo
        // );
        // std::hint::black_box(real_hi + (-real_lo));
        // std::hint::black_box(real_hi - real_lo);

        let real_diameter = real_hi - real_lo;
        let imag_diameter = imag_hi - imag_lo;
        if (real_diameter - imag_diameter).abs() > (1e-4).into() {
            return None;
        }

        // Some(Self {
        //     real_lo,
        //     real_hi,
        //     imag_lo,
        //     imag_hi,
        // })
        Some(Self {
            real_mid: (real_lo + real_hi).div2_floor(),
            imag_mid: (imag_lo + imag_hi).div2_floor(),
            // rad: (real_hi - real_lo).div2_floor(),
            rad: (real_diameter + imag_diameter).div2_floor().div2_floor(),
        })
    }

    /// for the tree, where domains are aligned to powers of 2
    pub(crate) fn new_exact(
        real_lo: Real,
        real_hi: Real,
        imag_lo: Imag,
        imag_hi: Imag,
    ) -> Option<Self> {
        if !(real_lo < real_hi && imag_lo < imag_hi) {
            return None;
        }
        if real_hi - real_lo != imag_hi - imag_lo {
            return None;
        }
        Some(Self {
            real_mid: (real_lo + real_hi).div2_exact()?,
            imag_mid: (imag_lo + imag_hi).div2_exact()?,
            rad: (real_hi - real_lo).div2_exact()?,
        })
    }

    pub(crate) fn real_lo(self) -> Real {
        // self.real_lo
        self.real_mid - self.rad
    }
    pub(crate) fn real_hi(self) -> Real {
        // self.real_hi
        self.real_mid + self.rad
    }
    pub(crate) fn real_mid(self) -> Real {
        // (self.real_hi + self.real_lo) / 2.0
        self.real_mid
    }

    pub(crate) fn imag_lo(self) -> Imag {
        // self.imag_lo
        self.imag_mid - self.rad
    }
    pub(crate) fn imag_hi(self) -> Imag {
        // self.imag_hi
        self.imag_mid + self.rad
    }
    pub(crate) fn imag_mid(self) -> Imag {
        // (self.imag_hi + self.imag_lo) / 2.0
        self.imag_mid
    }

    pub(crate) fn mid(self) -> (Real, Imag) {
        (self.real_mid(), self.imag_mid())
    }
    pub(crate) fn rad(self) -> Fixed {
        // (self.real_hi - self.real_lo) / 2.0
        self.rad
    }

    // fn area(self) -> f32 {
    //     (self.real_hi - self.real_lo) * (self.imag_hi - self.imag_lo)
    // }

    // fn contains(self, real: f32, imag: f32) -> bool {
    //     (self.real_lo..=self.real_hi).contains(&real)
    //         && (self.imag_lo..=self.imag_hi).contains(&imag)
    // }
    // #[inline(never)]
    pub(crate) fn contains_point(self, (real, imag): (Real, Imag)) -> bool {
        // (self.real_lo()..=self.real_hi()).contains(&real)
        //     && (self.imag_lo()..=self.imag_hi()).contains(&imag)
        (self.real_mid() - real).abs() <= self.rad() && (self.imag_mid() - imag).abs() <= self.rad()
        // f32::max(
        //     (self.real_mid() - real).abs(),
        //     (self.imag_mid() - imag).abs(),
        // ) <= self.rad()
    }

    // pub(crate) fn approx_contains_point(self, real: f32, imag: f32) -> bool {
    //     // (self.real_lo..=self.real_hi).contains(&real)
    //     //     && (self.imag_lo..=self.imag_hi).contains(&imag)
    //     (self.real_mid() - real).abs() <= self.rad() + 1e-4
    //         && (self.imag_mid() - imag).abs() <= self.rad() + 1e-4
    //     // f32::max(
    //     //     (self.real_mid() - real).abs(),
    //     //     (self.imag_mid() - imag).abs(),
    //     // ) <= self.rad()
    // }

    pub(crate) fn contains_square(self, other: Square) -> bool {
        Fixed::max(
            (self.real_mid() - other.real_mid()).abs(),
            (self.imag_mid() - other.imag_mid()).abs(),
        ) <= self.rad() - other.rad()
    }

    // #[inline(never)]
    pub(crate) fn overlaps(self, other: Self) -> bool {
        ((self.real_mid() - other.real_mid()).abs() <= (self.rad() + other.rad()))
            && ((self.imag_mid() - other.imag_mid()).abs() <= (self.rad() + other.rad()))
        // f32::max(
        //     (self.real_mid() - other.real_mid()).abs(),
        //     (self.imag_mid() - other.imag_mid()).abs(),
        // ) <= self.rad() + other.rad()
    }

    // fn overlaps(self, other: Self) -> bool {
    //     let real_lo = f32::max(self.real_lo(), other.real_lo());
    //     let real_hi = f32::min(self.real_hi(), other.real_hi());
    //     let imag_lo = f32::max(self.imag_lo(), other.imag_lo());
    //     let imag_hi = f32::min(self.imag_hi(), other.imag_hi());
    //     real_lo <= real_hi && imag_lo <= imag_hi
    // }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_map() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 30.0), Pos2::new(10.0, 50.0));
        let camera = Camera::new(1.0.into(), 2.0.into(), 1.0.into());
        let camera_map = CameraMap::new(rect, camera);

        assert_eq!(camera_map.camera.real_lo(), 0.0.into());
        assert_eq!(camera_map.camera.real_hi(), 2.0.into());
        assert_eq!(camera_map.imag_lo(), 0.0.into());
        assert_eq!(camera_map.imag_hi(), 4.0.into());

        assert!((0.0 - camera_map.real_to_x(0.0.into())).abs() < 1e-4);
        assert!((10.0 - camera_map.real_to_x(2.0.into())).abs() < 1e-4);
        assert!((30.0 - camera_map.imag_to_y(4.0.into())).abs() < 1e-4);
        assert!((50.0 - camera_map.imag_to_y(0.0.into())).abs() < 1e-4);

        assert_eq!(rect.min.x, camera_map.real_to_x(camera.real_lo()));
        assert_eq!(rect.max.x, camera_map.real_to_x(camera.real_hi()));
        assert_eq!(rect.max.y, camera_map.imag_to_y(camera_map.imag_lo()));
        assert_eq!(rect.min.y, camera_map.imag_to_y(camera_map.imag_hi()));

        for (p, c) in [
            (Pos2::new(0.0, 30.0), (0.0.into(), 4.0.into())),
            (Pos2::new(0.0, 50.0), (0.0.into(), 0.0.into())),
            (Pos2::new(10.0, 30.0), (2.0.into(), 4.0.into())),
            (Pos2::new(10.0, 50.0), (2.0.into(), 0.0.into())),
        ] {
            let c_actual = camera_map.pos_to_complex(p);
            // assert!((c.0 - c_actual.0).abs() + (c.1 - c_actual.1).abs() < 1e-4);
            assert_eq!(c, c_actual);
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
        ]
        .map(|(real, imag)| (real.into(), imag.into()))
        {
            let actual = camera_map.pos_to_complex(camera_map.complex_to_pos(c));
            // assert!((c.0 - actual.0).abs() + (c.1 - actual.1).abs() < 1e-4);
            // it's not precise enough for this to pass
            // assert_eq!(c, actual);
            assert!((c.0 - actual.0).abs() < (1e-4).into());
            assert!((c.1 - actual.1).abs() < (1e-4).into());
        }

        let window = Window::new(
            camera_map.camera.real_lo(),
            camera_map.camera.real_hi(),
            camera_map.imag_lo(),
            camera_map.imag_hi(),
        );
        assert_eq!(camera_map.rect, camera_map.window_to_rect(window));
        assert_eq!(window, camera_map.rect_to_window(rect));
    }
}
