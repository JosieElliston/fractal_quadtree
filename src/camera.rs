use std::cmp::Ordering;

use eframe::egui::{Pos2, Rect, Vec2};

use crate::{inv_lerp, lerp};

// TODO: make this a mapping from egui rect to complex window
#[derive(Debug, Clone, Copy)]
pub(crate) struct Camera {
    real_mid: f32,
    imag_mid: f32,
    real_rad: f32,
}
impl Camera {
    pub(crate) fn new(real_mid: f32, imag_mid: f32, real_rad: f32) -> Self {
        assert!(real_rad > 0.0);
        Self {
            real_mid,
            imag_mid,
            real_rad,
        }
    }

    pub(crate) fn real_lo(self) -> f32 {
        self.real_mid - self.real_rad
    }
    pub(crate) fn real_hi(self) -> f32 {
        self.real_mid + self.real_rad
    }
    pub(crate) fn imag_mid(self) -> f32 {
        self.imag_mid
    }
    pub(crate) fn real_mid(self) -> f32 {
        self.real_mid
    }
    pub(crate) fn real_rad(self) -> f32 {
        self.real_rad
    }
    pub(crate) fn real_rad_mut(&mut self) -> &mut f32 {
        &mut self.real_rad
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
impl std::ops::AddAssign<(f32, f32)> for Camera {
    fn add_assign(&mut self, rhs: (f32, f32)) {
        self.real_mid += rhs.0;
        self.imag_mid += rhs.1;
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

    pub(crate) fn imag_lo(&self) -> f32 {
        self.camera.imag_mid - self.imag_rad()
    }
    pub(crate) fn imag_hi(&self) -> f32 {
        self.camera.imag_mid + self.imag_rad()
    }
    pub(crate) fn imag_rad(&self) -> f32 {
        self.camera.real_rad * self.rect.height() / self.rect.width()
    }

    pub(crate) fn x_to_real(&self, x: f32) -> f32 {
        // -2.0 * x / self.rect.size().x * self.camera.real_rad
        lerp(
            self.camera.real_lo(),
            self.camera.real_hi(),
            inv_lerp(self.rect.min.x, self.rect.max.x, x),
        )
    }

    pub(crate) fn y_to_imag(&self, y: f32) -> f32 {
        // 2.0 * y * (self.camera.real_rad / self.rect.size().x)
        lerp(
            self.imag_lo(),
            self.imag_hi(),
            1.0 - inv_lerp(self.rect.min.y, self.rect.max.y, y),
        )
    }

    pub(crate) fn pos_to_complex(&self, pos: Pos2) -> (f32, f32) {
        (self.x_to_real(pos.x), self.y_to_imag(pos.y))
    }

    pub(crate) fn real_to_x(&self, real: f32) -> f32 {
        lerp(
            self.rect.min.x,
            self.rect.max.x,
            inv_lerp(self.camera.real_lo(), self.camera.real_hi(), real),
        )
    }

    pub(crate) fn imag_to_y(&self, imag: f32) -> f32 {
        lerp(
            self.rect.min.y,
            self.rect.max.y,
            1.0 - inv_lerp(self.imag_lo(), self.imag_hi(), imag),
        )
    }

    // fn complex_to_pos(&self, real: f32, imag: f32) -> Pos2 {
    //     Pos2::new(self.real_to_x(real), self.imag_to_y(imag))
    // }

    pub(crate) fn complex_to_pos(&self, c: (f32, f32)) -> Pos2 {
        Pos2::new(self.real_to_x(c.0), self.imag_to_y(c.1))
    }

    pub(crate) fn window_to_rect(&self, window: impl Into<Window>) -> Rect {
        let window = window.into();
        Rect {
            min: self.complex_to_pos((window.real_lo, window.imag_hi)),
            max: self.complex_to_pos((window.real_hi, window.imag_lo)),
        }
    }

    pub(crate) fn rect_to_window(&self, rect: Rect) -> Window {
        Window::new(
            self.x_to_real(rect.min.x),
            self.x_to_real(rect.max.x),
            self.y_to_imag(rect.max.y),
            self.y_to_imag(rect.min.y),
        )
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
                    // .filter_map(move |col| {
                    //     Some((
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
                    //         )?,
                    //     ))
                    // })
                    .map(move |col| {
                        (
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
                            )
                            .unwrap(),
                        )
                    })
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(align(32))]
pub(crate) struct Window {
    real_lo: f32,
    real_hi: f32,
    imag_lo: f32,
    imag_hi: f32,
}
impl Window {
    pub(crate) fn new(real_lo: f32, real_hi: f32, imag_lo: f32, imag_hi: f32) -> Self {
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

    pub(crate) fn real_lo(self) -> f32 {
        self.real_lo
    }
    pub(crate) fn real_hi(self) -> f32 {
        self.real_hi
    }
    pub(crate) fn real_mid(self) -> f32 {
        (self.real_hi + self.real_lo) / 2.0
    }
    pub(crate) fn real_rad(self) -> f32 {
        (self.real_hi - self.real_lo) / 2.0
    }

    pub(crate) fn imag_lo(self) -> f32 {
        self.imag_lo
    }
    pub(crate) fn imag_hi(self) -> f32 {
        self.imag_hi
    }
    pub(crate) fn imag_rad(self) -> f32 {
        (self.imag_hi - self.imag_lo) / 2.0
    }
    pub(crate) fn imag_mid(self) -> f32 {
        (self.imag_hi + self.imag_lo) / 2.0
    }

    pub(crate) fn area(self) -> f32 {
        (self.real_hi - self.real_lo) * (self.imag_hi - self.imag_lo)
    }

    // fn intersect(self, other: impl Into<Self>) -> Option<Self> {
    //     todo!()
    // }

    // fn overlaps(self, other: impl Into<Self>) -> bool {
    //     self.intersect(other).is_some()
    // }
    pub(crate) fn overlaps(self, other: impl Into<Self>) -> bool {
        let other = other.into();
        let real_lo = f32::max(self.real_lo, other.real_lo);
        let real_hi = f32::min(self.real_hi, other.real_hi);
        let imag_lo = f32::max(self.imag_lo, other.imag_lo);
        let imag_hi = f32::min(self.imag_hi, other.imag_hi);
        real_lo <= real_hi && imag_lo <= imag_hi
    }

    // fn contains(self, other: impl Into<Self>) -> bool {
    //     let other = other.into();
    //     self.intersect(other) == Some(other)
    // }
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
    real_mid: f32,
    imag_mid: f32,
    rad: f32,
}
impl Square {
    pub(crate) const fn try_new(
        real_lo: f32,
        real_hi: f32,
        imag_lo: f32,
        imag_hi: f32,
    ) -> Option<Self> {
        if !(real_lo <= real_hi && imag_lo <= imag_hi) {
            return None;
        }
        if !(real_lo < real_hi && imag_lo < imag_hi) {
            return None;
        }
        if !{
            let dx = real_hi as f64 - real_lo as f64;
            let dy = imag_hi as f64 - imag_lo as f64;
            let diff = dx - dy;
            let ratio = dx / dy;
            diff.abs() < 1e-4 || (1.0 - ratio).abs() < 1e-4
        } {
            return None;
        }

        // Some(Self {
        //     real_lo,
        //     real_hi,
        //     imag_lo,
        //     imag_hi,
        // })
        Some(Self {
            real_mid: (real_lo + real_hi) / 2.0,
            imag_mid: (imag_lo + imag_hi) / 2.0,
            rad: (real_hi - real_lo) / 2.0,
        })
    }

    pub(crate) fn real_lo(self) -> f32 {
        // self.real_lo
        self.real_mid - self.rad
    }
    pub(crate) fn real_hi(self) -> f32 {
        // self.real_hi
        self.real_mid + self.rad
    }
    pub(crate) fn real_mid(self) -> f32 {
        // (self.real_hi + self.real_lo) / 2.0
        self.real_mid
    }

    pub(crate) fn imag_lo(self) -> f32 {
        // self.imag_lo
        self.imag_mid - self.rad
    }
    pub(crate) fn imag_hi(self) -> f32 {
        // self.imag_hi
        self.imag_mid + self.rad
    }
    pub(crate) fn imag_mid(self) -> f32 {
        // (self.imag_hi + self.imag_lo) / 2.0
        self.imag_mid
    }

    pub(crate) fn mid(self) -> (f32, f32) {
        (self.real_mid(), self.imag_mid())
    }
    pub(crate) fn rad(self) -> f32 {
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
    pub(crate) fn contains_point(self, (real, imag): (f32, f32)) -> bool {
        // (self.real_lo..=self.real_hi).contains(&real)
        //     && (self.imag_lo..=self.imag_hi).contains(&imag)
        (self.real_mid() - real).abs() <= self.rad() && (self.imag_mid() - imag).abs() <= self.rad()
        // f32::max(
        //     (self.real_mid() - real).abs(),
        //     (self.imag_mid() - imag).abs(),
        // ) <= self.rad()
    }

    pub(crate) fn approx_contains_point(self, real: f32, imag: f32) -> bool {
        // (self.real_lo..=self.real_hi).contains(&real)
        //     && (self.imag_lo..=self.imag_hi).contains(&imag)
        (self.real_mid() - real).abs() <= self.rad() + 1e-4
            && (self.imag_mid() - imag).abs() <= self.rad() + 1e-4
        // f32::max(
        //     (self.real_mid() - real).abs(),
        //     (self.imag_mid() - imag).abs(),
        // ) <= self.rad()
    }

    pub(crate) fn contains_square(self, other: Square) -> bool {
        f32::max(
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
