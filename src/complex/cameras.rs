use std::{num::NonZeroUsize, ops};

use eframe::egui::{self, Pos2, Rect, Vec2};

use super::{Window, fixed::*};

// TODO: maybe `Square`?
pub(crate) type Pixel = Window;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Camera {
    real_mid: f64,
    imag_mid: f64,
    real_rad: f64,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            real_mid: 0.0,
            imag_mid: 0.0,
            real_rad: 2.0,
        }
    }
}
impl Camera {
    /// panics if `real_rad` is not positive
    // pub(crate) fn new(real_mid: Real, imag_mid: Imag, real_rad: Real) -> Self {
    pub(crate) fn new(real_mid: f64, imag_mid: f64, real_rad: f64) -> Self {
        // assert!(real_rad > Fixed::ZERO);
        assert!(real_rad > 0.0);
        Self {
            real_mid,
            imag_mid,
            real_rad,
        }
    }

    pub(crate) fn real_lo(self) -> f64 {
        self.real_mid - self.real_rad
    }
    pub(crate) fn real_hi(self) -> f64 {
        self.real_mid + self.real_rad
    }
    pub(crate) fn imag_mid(self) -> f64 {
        self.imag_mid
    }
    pub(crate) fn real_mid(self) -> f64 {
        self.real_mid
    }
    pub(crate) fn real_rad(self) -> f64 {
        self.real_rad
    }
    pub(crate) fn real_rad_mut(&mut self) -> &mut f64 {
        &mut self.real_rad
    }
    pub(crate) fn mid(self) -> (f64, f64) {
        (self.real_mid, self.imag_mid)
    }
}
impl ops::AddAssign<(f64, f64)> for Camera {
    fn add_assign(&mut self, (real, imag): (f64, f64)) {
        self.real_mid += real;
        self.imag_mid += imag;
    }
}
impl ops::SubAssign<(f64, f64)> for Camera {
    fn sub_assign(&mut self, (real, imag): (f64, f64)) {
        self.real_mid -= real;
        self.imag_mid -= imag;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CameraMap {
    rect: Rect,
    camera: Camera,
    /// how many egui pixels do we draw a "pixel" as?
    /// not needed for the mapping itself,
    /// but it is needed when dealing with pixels.
    stride: Option<NonZeroUsize>,
}
impl CameraMap {
    pub(crate) fn new(rect: Rect, camera: Camera, stride: usize) -> Self {
        assert!(rect.min.x < rect.max.x);
        assert!(rect.min.y < rect.max.y);
        Self {
            rect,
            camera,
            stride: Some(NonZeroUsize::new(stride).unwrap()),
        }
    }

    pub(crate) fn new_without_stride(rect: Rect, camera: Camera) -> Self {
        assert!(rect.min.x < rect.max.x);
        assert!(rect.min.y < rect.max.y);
        Self {
            rect,
            camera,
            stride: None,
        }
    }

    pub(crate) fn rect(&self) -> Rect {
        self.rect
    }
    pub(crate) fn camera(&self) -> Camera {
        self.camera
    }
    /// equivalent to `self.rect_to_window(self.rect())`
    pub(crate) fn window(&self) -> Option<Window> {
        Window::from_lo_hi(
            self.camera.real_lo().try_into().ok()?,
            self.camera.real_hi().try_into().ok()?,
            self.imag_lo().try_into().ok()?,
            self.imag_hi().try_into().ok()?,
        )
    }

    pub(crate) fn imag_lo(&self) -> f64 {
        self.camera.imag_mid - self.imag_rad()
    }
    pub(crate) fn imag_hi(&self) -> f64 {
        self.camera.imag_mid + self.imag_rad()
    }
    pub(crate) fn imag_rad(&self) -> f64 {
        // self.camera
        //     .real_rad
        //     .mul_f64(self.rect.height() as f64 / self.rect.width() as f64)
        self.camera.real_rad * (self.rect.height() as f64 / self.rect.width() as f64)
    }

    /// returns `None` if it would be out of the fixed point domain
    pub(crate) fn x_to_real(&self, x: f32) -> Option<Real> {
        Real::try_from_f64(super::lerp(
            self.camera.real_lo(),
            self.camera.real_hi(),
            super::inv_lerp(self.rect.min.x as f64, self.rect.max.x as f64, x as f64),
        ))
    }
    /// returns `None` if it would be out of the fixed point domain
    pub(crate) fn y_to_imag(&self, y: f32) -> Option<Imag> {
        Imag::try_from_f64(super::lerp(
            self.imag_lo(),
            self.imag_hi(),
            1.0 - super::inv_lerp(self.rect.min.y as f64, self.rect.max.y as f64, y as f64),
        ))
    }
    pub(crate) fn real_to_x(&self, real: Real) -> f32 {
        super::lerp(
            self.rect.min.x as f64,
            self.rect.max.x as f64,
            super::inv_lerp(self.camera.real_lo(), self.camera.real_hi(), real.into()),
        ) as f32
    }
    pub(crate) fn imag_to_y(&self, imag: Imag) -> f32 {
        super::lerp(
            self.rect.min.y as f64,
            self.rect.max.y as f64,
            1.0 - super::inv_lerp(self.imag_lo(), self.imag_hi(), imag.into()),
        ) as f32
    }

    /// returns `None` if it would be out of the fixed point domain
    pub(crate) fn pos_to_complex(&self, pos: Pos2) -> Option<(Real, Imag)> {
        Some((self.x_to_real(pos.x)?, self.y_to_imag(pos.y)?))
    }
    pub(crate) fn complex_to_pos(&self, (real, imag): (Real, Imag)) -> Pos2 {
        Pos2::new(self.real_to_x(real), self.imag_to_y(imag))
    }

    // pub(crate) fn vec1_to_delta_real(&self, vec1: f32) -> Option<Real> {
    //     Real::try_from_f64(super::lerp(
    //         0.0,
    //         2.0 * self.camera.real_rad,
    //         super::inv_lerp(0.0, self.rect.width() as f64, vec1 as f64),
    //     ))
    // }
    // pub(crate) fn vec1_to_delta_imag(&self, vec1: f32) -> Option<Imag> {
    //     self.vec1_to_delta_real(-vec1)
    // }
    // pub(crate) fn vec2_to_delta_complex(&self, vec2: Vec2) -> Option<(Real, Imag)> {
    //     Some((
    //         self.vec1_to_delta_real(vec2.x)?,
    //         self.vec1_to_delta_imag(vec2.y)?,
    //     ))
    // }
    pub(crate) fn vec1_to_delta_real(&self, vec1: f32) -> f64 {
        super::lerp(
            0.0,
            2.0 * self.camera.real_rad,
            super::inv_lerp(0.0, self.rect.width() as f64, vec1 as f64),
        )
    }
    pub(crate) fn vec1_to_delta_imag(&self, vec1: f32) -> f64 {
        self.vec1_to_delta_real(-vec1)
    }
    pub(crate) fn vec2_to_delta_complex(&self, vec2: Vec2) -> (f64, f64) {
        (
            self.vec1_to_delta_real(vec2.x),
            self.vec1_to_delta_imag(vec2.y),
        )
    }
    /// equivalent to `self.real_to_x(fixed) - self.real_to_x(Fixed::ZERO)`
    /// and to `self.imag_to_y(Fixed::ZERO) - self.imag_to_y(fixed)`
    /// keywords: displacement, delta, difference, rad_to_vec1
    pub(crate) fn delta_real_to_vec1(&self, real: Real) -> f32 {
        super::lerp(
            0.0,
            self.rect.width() as f64,
            super::inv_lerp(0.0, 2.0 * self.camera.real_rad, real.into()),
        ) as f32
    }
    pub(crate) fn delta_imag_to_vec1(&self, imag: Imag) -> f32 {
        self.delta_real_to_vec1(-imag)
    }
    pub(crate) fn delta_complex_to_vec2(&self, (real, imag): (Real, Imag)) -> Vec2 {
        Vec2::new(self.delta_real_to_vec1(real), self.delta_imag_to_vec1(imag))
    }

    pub(crate) fn rect_to_window(&self, rect: Rect) -> Option<Window> {
        Window::from_lo_hi(
            self.x_to_real(rect.min.x)?,
            self.x_to_real(rect.max.x)?,
            self.y_to_imag(rect.max.y)?,
            self.y_to_imag(rect.min.y)?,
        )
    }
    pub(crate) fn window_to_rect(&self, window: impl Into<Window>) -> Rect {
        let window = window.into();
        Rect {
            min: self.complex_to_pos((window.real_lo(), window.imag_hi())),
            max: self.complex_to_pos((window.real_hi(), window.imag_lo())),
        }
    }

    pub(crate) fn pixels_width(&self) -> usize {
        let stride = self.stride.unwrap().get();
        let ret = (self.rect.width() as usize).div_ceil(stride);
        #[cfg(debug_assertions)]
        if let Some(line) = self.pixels().next() {
            debug_assert_eq!(ret, line.count());
        }
        ret
    }
    pub(crate) fn pixels_height(&self) -> usize {
        let stride = self.stride.unwrap().get();
        let ret = (self.rect.height() as usize).div_ceil(stride);
        debug_assert_eq!(ret, self.pixels().count());
        ret
    }

    pub(crate) fn rect_at(&self, row: usize, col: usize) -> Rect {
        let stride = self.stride.unwrap().get();
        Rect::from_min_size(
            Pos2::new(col as f32, row as f32) * stride as f32 + self.rect.min.to_vec2(),
            Vec2::new(stride as f32, stride as f32),
        )
    }
    pub(crate) fn pixel_at(&self, row: usize, col: usize) -> Option<Pixel> {
        let stride = self.stride.unwrap().get();
        Pixel::from_lo_hi(
            self.x_to_real(col as f32)?,
            self.x_to_real((col + stride) as f32)?,
            self.y_to_imag((row + stride) as f32)?,
            self.y_to_imag(row as f32)?,
        )
    }

    /// pixel is None if it couldn't be constructed,
    /// so it would be too small or outside the fixed point domain
    pub(crate) fn pixels(
        &self,
    ) -> impl Iterator<Item = impl Iterator<Item = (Rect, Option<Pixel>)>> {
        let stride = self.stride.unwrap().get();
        (0..self.rect.size().y as usize)
            .step_by(stride)
            .map(move |row| {
                (0..self.rect.size().x as usize)
                    .step_by(stride)
                    .map(move |col| (self.rect_at(row, col), self.pixel_at(row, col)))
            })
    }

    pub(crate) fn pan_zoom(
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        camera: &mut Camera,
        velocity: &mut Vec2,
    ) {
        let rect = ui.max_rect();
        let r = ui.allocate_rect(rect, egui::Sense::drag());
        let dt = ctx.input(|i| i.stable_dt);
        let camera_map = CameraMap::new_without_stride(rect, *camera);

        // pan
        if r.is_pointer_button_down_on() && ctx.input(|i| i.pointer.primary_down()) {
            *camera += camera_map.vec2_to_delta_complex(-r.drag_delta());
            *velocity = -r.drag_delta() / dt;
        } else {
            const VELOCITY_DAMPING: f32 = 0.9999;
            *camera += camera_map.vec2_to_delta_complex(*velocity * dt);
            *velocity *= (1.0 - VELOCITY_DAMPING).powf(dt);
        }
        if velocity.length_sq() < 0.0001 {
            *velocity = Vec2::ZERO;
        }

        // zoom
        if r.hovered()
            && let Some(mouse_pos) = r.hover_pos()
        {
            let mouse = mouse_pos - rect.center();
            let zoom = ctx.input(|i| (i.smooth_scroll_delta.y / 300.0).exp()) as f64;
            *camera += camera_map.vec2_to_delta_complex(mouse);
            // *camera.real_rad_mut() = camera_map
            //     .camera
            //     .real_rad()
            //     .mul_f64_saturating(zoom.recip());
            *camera.real_rad_mut() /= zoom;
            let camera_map = CameraMap::new_without_stride(rect, *camera);
            *camera -= camera_map.vec2_to_delta_complex(mouse);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_rect_camera() -> (Rect, Camera) {
        let rect = Rect::from_min_max(Pos2::new(0.0, 30.0), Pos2::new(10.0, 50.0));
        let camera = Camera::new(1.0, 2.0, 1.0);
        (rect, camera)
    }

    #[test]
    fn test_bounds() {
        let (rect, camera) = get_rect_camera();
        let camera_map = CameraMap::new_without_stride(rect, camera);

        assert_eq!(camera_map.camera.real_lo(), 0.0);
        assert_eq!(camera_map.camera.real_hi(), 2.0);
        assert_eq!(camera_map.imag_lo(), 0.0);
        assert_eq!(camera_map.imag_hi(), 4.0);

        assert!((0.0 - camera_map.real_to_x(0.0.try_into().unwrap())).abs() < 1e-4);
        assert!((10.0 - camera_map.real_to_x(2.0.try_into().unwrap())).abs() < 1e-4);
        assert!((30.0 - camera_map.imag_to_y(4.0.try_into().unwrap())).abs() < 1e-4);
        assert!((50.0 - camera_map.imag_to_y(0.0.try_into().unwrap())).abs() < 1e-4);

        assert_eq!(
            rect.min.x,
            camera_map.real_to_x(Fixed::from_f64(camera.real_lo()))
        );
        assert_eq!(
            rect.max.x,
            camera_map.real_to_x(Fixed::from_f64(camera.real_hi()))
        );
        assert_eq!(
            rect.max.y,
            camera_map.imag_to_y(Fixed::from_f64(camera_map.imag_lo()))
        );
        assert_eq!(
            rect.min.y,
            camera_map.imag_to_y(Fixed::from_f64(camera_map.imag_hi()))
        );

        for (p, c) in [
            (
                Pos2::new(0.0, 30.0),
                (0.0.try_into().unwrap(), 4.0.try_into().unwrap()),
            ),
            (
                Pos2::new(0.0, 50.0),
                (0.0.try_into().unwrap(), 0.0.try_into().unwrap()),
            ),
            (
                Pos2::new(10.0, 30.0),
                (2.0.try_into().unwrap(), 4.0.try_into().unwrap()),
            ),
            (
                Pos2::new(10.0, 50.0),
                (2.0.try_into().unwrap(), 0.0.try_into().unwrap()),
            ),
        ] {
            let c_actual = camera_map.pos_to_complex(p).unwrap();
            // assert!((c.0 - c_actual.0).abs() + (c.1 - c_actual.1).abs() < 1e-4);
            assert_eq!(c, c_actual);
            let p_actual = camera_map.complex_to_pos(c);
            assert!((p - p_actual).length() < 1e-4);
        }
    }

    #[test]
    fn test_map_pos2() {
        let (rect, camera) = get_rect_camera();
        let camera_map = CameraMap::new_without_stride(rect, camera);

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
                (pos - camera_map.complex_to_pos(camera_map.pos_to_complex(pos).unwrap())).length()
                    < 1e-4
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
        .map(|(real, imag)| (real.try_into().unwrap(), imag.try_into().unwrap()))
        {
            let actual = camera_map
                .pos_to_complex(camera_map.complex_to_pos(c))
                .unwrap();
            // assert!((c.0 - actual.0).abs() + (c.1 - actual.1).abs() < 1e-4);
            // it's not precise enough for this to pass
            // assert_eq!(c, actual);
            assert!((c.0 - actual.0).abs().into_f64() < 1e-4);
            assert!((c.1 - actual.1).abs().into_f64() < 1e-4);
        }
    }

    #[test]
    fn test_window() {
        let (rect, camera) = get_rect_camera();
        let camera_map = CameraMap::new_without_stride(rect, camera);

        let window = Window::from_lo_hi(
            camera_map.camera.real_lo().try_into().unwrap(),
            camera_map.camera.real_hi().try_into().unwrap(),
            camera_map.imag_lo().try_into().unwrap(),
            camera_map.imag_hi().try_into().unwrap(),
        )
        .unwrap();
        assert_eq!(camera_map.rect, camera_map.window_to_rect(window));
        assert_eq!(window, camera_map.rect_to_window(rect).unwrap());
    }

    #[test]
    fn test_map_vec1() {
        let (rect, camera) = get_rect_camera();
        let camera_map = CameraMap::new_without_stride(rect, camera);

        for fixed in [
            -2.0, -1.0, -2.0, 5.0, 4.0, -1.0, 4.0, 5.0, -1.885, -0.978, 0.254, 0.793, 3.634, 3.274,
            3.332, 1.716, 0.063, 3.933, 2.132, 1.927, 1.848, 4.781, 2.971, 4.047, 0.194, 2.966,
        ]
        .map(|fixed| fixed.try_into().unwrap())
        {
            let vec1_fixed = camera_map.delta_real_to_vec1(fixed);
            let vec1_real = camera_map.real_to_x(fixed) - camera_map.real_to_x(Fixed::ZERO);
            let vec1_imag = camera_map.imag_to_y(Fixed::ZERO) - camera_map.imag_to_y(fixed);
            assert!((vec1_fixed - vec1_real).abs() < 1e-4);
            assert!((vec1_fixed - vec1_imag).abs() < 1e-4);
        }
    }

    #[test]
    fn test_pixels() {
        let (rect, camera) = get_rect_camera();
        let camera_map = CameraMap::new(rect, camera, 2);

        assert_eq!(camera_map.pixels_width(), 5);
        assert_eq!(camera_map.pixels_height(), 10);

        assert_eq!(camera_map.pixels().count(), camera_map.pixels_height());
        assert_eq!(
            camera_map.pixels().next().unwrap().count(),
            camera_map.pixels_width()
        );
    }
}
