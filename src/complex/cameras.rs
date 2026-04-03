use std::ops;

use eframe::egui::{self, Pos2, Rect, Vec2};

use super::{Square, Window, fixed::*};

// TODO: maybe `Square`?
pub(crate) type Pixel = Window;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Camera {
    real_mid: Real,
    imag_mid: Imag,
    real_rad: Real,
}
impl Camera {
    /// panics if `real_rad` is not positive
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
}
impl ops::AddAssign<(Real, Imag)> for Camera {
    fn add_assign(&mut self, (real, imag): (Real, Imag)) {
        self.real_mid += real;
        self.imag_mid += imag;
    }
}
impl ops::SubAssign<(Real, Imag)> for Camera {
    fn sub_assign(&mut self, (real, imag): (Real, Imag)) {
        self.real_mid -= real;
        self.imag_mid -= imag;
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
        Window::from_lo_hi(
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
            .mul_f64(self.rect.height() as f64 / self.rect.width() as f64)
    }

    pub(crate) fn x_to_real(&self, x: f32) -> Real {
        Fixed::lerp(
            self.camera.real_lo(),
            self.camera.real_hi(),
            super::inv_lerp(self.rect.min.x as f64, self.rect.max.x as f64, x as f64),
        )
    }
    pub(crate) fn y_to_imag(&self, y: f32) -> Imag {
        Fixed::lerp(
            self.imag_lo(),
            self.imag_hi(),
            1.0 - super::inv_lerp(self.rect.min.y as f64, self.rect.max.y as f64, y as f64),
        )
    }
    pub(crate) fn real_to_x(&self, real: Real) -> f32 {
        super::lerp(
            self.rect.min.x as f64,
            self.rect.max.x as f64,
            Fixed::inv_lerp(self.camera.real_lo(), self.camera.real_hi(), real),
        ) as f32
    }
    pub(crate) fn imag_to_y(&self, imag: Imag) -> f32 {
        super::lerp(
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

    /// equivalent to `self.real_to_x(fixed) - self.real_to_x(Fixed::ZERO)`
    /// and to `self.imag_to_y(Fixed::ZERO) - self.imag_to_y(fixed)`
    /// keywords: displacement, delta, difference, rad_to_vec1
    pub(crate) fn delta_real_to_vec1(&self, real: Real) -> f32 {
        super::lerp(
            0.0,
            self.rect.width() as f64,
            Fixed::inv_lerp(Fixed::ZERO, self.camera.real_rad.mul2(), real),
        ) as f32
    }
    pub(crate) fn delta_imag_to_vec1(&self, imag: Imag) -> f32 {
        -self.delta_real_to_vec1(imag)
    }
    pub(crate) fn delta_complex_to_vec2(&self, (real, imag): (Real, Imag)) -> Vec2 {
        Vec2::new(self.delta_real_to_vec1(real), self.delta_imag_to_vec1(imag))
    }

    pub(crate) fn vec1_to_delta_real(&self, vec1: f32) -> Real {
        Fixed::lerp(
            Fixed::ZERO,
            self.camera.real_rad.mul2(),
            super::inv_lerp(0.0, self.rect.width() as f64, vec1 as f64),
        )
    }
    pub(crate) fn vec1_to_delta_imag(&self, vec1: f32) -> Imag {
        -self.vec1_to_delta_real(vec1)
    }
    pub(crate) fn vec2_to_delta_complex(&self, vec2: Vec2) -> (Real, Imag) {
        (
            self.vec1_to_delta_real(vec2.x),
            self.vec1_to_delta_imag(vec2.y),
        )
    }

    pub(crate) fn rect_to_window(&self, rect: Rect) -> Window {
        Window::from_lo_hi(
            self.x_to_real(rect.min.x),
            self.x_to_real(rect.max.x),
            self.y_to_imag(rect.max.y),
            self.y_to_imag(rect.min.y),
        )
    }
    pub(crate) fn window_to_rect(&self, window: impl Into<Window>) -> Rect {
        let window = window.into();
        Rect {
            min: self.complex_to_pos((window.real_lo(), window.imag_hi())),
            max: self.complex_to_pos((window.real_hi(), window.imag_lo())),
        }
    }

    /// pixel is None if it couldn't be constructed,
    /// so it would be too small or outside the fixed point domain
    pub(crate) fn pixels(
        &self,
        stride: usize,
    ) -> impl Iterator<Item = ((usize, usize), Rect, Option<Pixel>)> {
        (0..self.rect.size().y as usize)
            .step_by(stride)
            .flat_map(move |row| {
                (0..self.rect.size().x as usize)
                    .step_by(stride)
                    .map(move |col| {
                        (
                            (row / stride, col / stride),
                            Rect::from_min_size(
                                Pos2::new(col as f32, row as f32),
                                Vec2::new(stride as f32, stride as f32),
                            ),
                            Pixel::try_from_lo_hi(
                                self.x_to_real(col as f32),
                                self.x_to_real((col + stride) as f32),
                                self.y_to_imag((row + stride) as f32),
                                self.y_to_imag(row as f32),
                            ),
                        )
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

    pub(crate) fn pan_zoom(
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        camera: &mut Camera,
        velocity: &mut Vec2,
    ) {
        let rect = ui.max_rect();
        let r = ui.allocate_rect(rect, egui::Sense::drag());
        let dt = ctx.input(|i| i.stable_dt);
        let camera_map = CameraMap::new(rect, *camera);

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
            let zoom = ctx.input(|i| (i.smooth_scroll_delta.y / 300.0).exp());
            *camera += camera_map.vec2_to_delta_complex(mouse);
            *camera.real_rad_mut() = camera_map.camera.real_rad().div_f64(zoom as f64);
            let camera_map = CameraMap::new(rect, *camera);
            *camera -= camera_map.vec2_to_delta_complex(mouse);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_rect_camera() -> (Rect, Camera) {
        let rect = Rect::from_min_max(Pos2::new(0.0, 30.0), Pos2::new(10.0, 50.0));
        let camera = Camera::new(1.0.into(), 2.0.into(), 1.0.into());
        (rect, camera)
    }

    #[test]
    fn test_bounds() {
        let (rect, camera) = get_rect_camera();
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
    }

    #[test]
    fn test_map_pos2() {
        let (rect, camera) = get_rect_camera();
        let camera_map = CameraMap::new(rect, camera);

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
    }

    #[test]
    fn test_window() {
        let (rect, camera) = get_rect_camera();
        let camera_map = CameraMap::new(rect, camera);

        let window = Window::from_lo_hi(
            camera_map.camera.real_lo(),
            camera_map.camera.real_hi(),
            camera_map.imag_lo(),
            camera_map.imag_hi(),
        );
        assert_eq!(camera_map.rect, camera_map.window_to_rect(window));
        assert_eq!(window, camera_map.rect_to_window(rect));
    }

    #[test]
    fn test_map_vec1() {
        let (rect, camera) = get_rect_camera();
        let camera_map = CameraMap::new(rect, camera);

        for fixed in [
            -2.0, -1.0, -2.0, 5.0, 4.0, -1.0, 4.0, 5.0, -1.885, -0.978, 0.254, 0.793, 3.634, 3.274,
            3.332, 1.716, 0.063, 3.933, 2.132, 1.927, 1.848, 4.781, 2.971, 4.047, 0.194, 2.966,
        ]
        .map(|fixed| fixed.into())
        {
            let vec1_fixed = camera_map.delta_real_to_vec1(fixed);
            let vec1_real = camera_map.real_to_x(fixed) - camera_map.real_to_x(Fixed::ZERO);
            let vec1_imag = camera_map.imag_to_y(Fixed::ZERO) - camera_map.imag_to_y(fixed);
            assert!((vec1_fixed - vec1_real).abs() < 1e-4);
            assert!((vec1_fixed - vec1_imag).abs() < 1e-4);
        }
    }
}
