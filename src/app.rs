#[cfg(debug_assertions)]
use std::hint;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use egui::{self, Color32, Key, Pos2, Rect, Vec2};

use crate::{
    complex::{Camera, CameraMap, Window, fixed::*},
    fractal::{self, Fractal},
    sample::{self, SampleDistanceGradient, SampleMaybeDistance},
};

/// fancy dynamic radius based on zoom,
/// so that if you're zoomed out, points don't cover everything.
fn dynamic_draw_size(camera_map: &CameraMap, max_rad: f32) -> f32 {
    const BASE_RAD: Fixed = Fixed::try_from_f64(0.001).unwrap();
    let rad = BASE_RAD.mul_f64(max_rad as f64);
    (camera_map.delta_real_to_vec1(rad)).min(max_rad)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CurrentFractal {
    Metabrot,
    Mandelbrot,
}

// TODO: separate this into smaller structs
// ui, fractal, metabrot, mandelbrot
pub(crate) struct App {
    metabrot: Fractal,
    fractal_target_spf: Duration,
    fractal_try_render_time: Instant,
    fractal_successful_render_time: Instant,
    fractal_accumulator_ns: i128,
    primary_camera: Camera,
    primary_camera_velocity: Vec2,
    primary_camera_stride: usize,
    secondary_camera: Camera,
    secondary_camera_velocity: Vec2,
    secondary_camera_stride: usize,
    /// the ui frame time, not the fractal frame time.
    last_frame_time: Instant,
    global_dts: egui::util::History<f32>,
    fractal_dts: egui::util::History<f32>,
    /// the last successful reclaim tick.
    last_reclaim_tick: Instant,
    reclaim_dts: egui::util::History<f32>,
    reclaim_counts: egui::util::History<u64>,
    /// how many samples we received on each frame
    sample_counts: egui::util::History<u64>,
    timers: egui::util::History<fractal::MultiTimer>,
    /// this allows us to prevent the main thread from needing to wait for the fractal to finish rendering.
    /// this is only false for the first frame.
    has_begun_rendering: bool,
    texture: egui::TextureHandle,
    needs_full_redraw: bool,
    sampling: bool,
    reclaiming: bool,
    draw_crosshair: bool,
    current_fractal: CurrentFractal,
    control_other_camera: bool,
    draw_z0: bool,
    draw_sample_window: bool,
    draw_sample_grid_cloud: bool,
    draw_sample_subgrid_cloud: bool,
    // /// -1 to disable, 0 is the same as draw_sample_grid_deepest
    // draw_sample_gradient_steps_deepest: i32,
    /// -1 to disable, 0 is the same as draw_sample_grid_cloud
    draw_sample_gradient_steps_cloud: i32,
    draw_sample_mouse_gradient_steps: bool,
}
impl App {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            metabrot: Fractal::new(),
            fractal_target_spf: Duration::from_secs_f64(1.0 / 30.0),
            fractal_try_render_time: Instant::now(),
            fractal_successful_render_time: Instant::now(),
            fractal_accumulator_ns: 0,
            primary_camera: Camera::default(),
            primary_camera_velocity: Vec2::ZERO,
            primary_camera_stride: 1,
            secondary_camera: Camera::default(),
            secondary_camera_velocity: Vec2::ZERO,
            secondary_camera_stride: 8,
            last_frame_time: Instant::now(),
            global_dts: egui::util::History::new(2..1000, 0.2),
            fractal_dts: egui::util::History::new(2..1000, 0.2),
            last_reclaim_tick: Instant::now(),
            reclaim_dts: egui::util::History::new(10..1000, 1.0),
            reclaim_counts: egui::util::History::new(10..1000, 1.0),
            sample_counts: egui::util::History::new(10..1000, 1.0),
            timers: egui::util::History::new(10..1000, 1.0),
            has_begun_rendering: false,
            texture: cc.egui_ctx.load_texture(
                "fractal",
                egui::ColorImage::example(),
                egui::TextureOptions::NEAREST,
            ),
            needs_full_redraw: true,
            sampling: true,
            reclaiming: true,
            draw_crosshair: false,
            current_fractal: CurrentFractal::Metabrot,
            control_other_camera: false,
            draw_z0: true,
            draw_sample_window: true,
            draw_sample_grid_cloud: true,
            draw_sample_subgrid_cloud: false,
            draw_sample_gradient_steps_cloud: 1,
            draw_sample_mouse_gradient_steps: true,
        }
    }

    /// this can't be in `show_ui` bc it needs to run even when the stats/settings panels are collapsed
    fn keybinds(&mut self, ctx: &egui::Context) {
        self.sampling ^= ctx.input(|i| i.key_pressed(Key::X));
        self.reclaiming ^= ctx.input(|i| i.key_pressed(Key::R));
        if ctx.input(|i| i.key_pressed(Key::N)) {
            self.current_fractal = CurrentFractal::Metabrot;
            self.needs_full_redraw = true;
        }
        if ctx.input(|i| i.key_pressed(Key::M)) {
            self.current_fractal = CurrentFractal::Mandelbrot;
        }
        self.control_other_camera ^= ctx.input(|i| i.key_pressed(Key::C));

        self.draw_sample_gradient_steps_cloud += ctx.input(|i| {
            i.key_pressed(Key::ArrowRight) as i32 - i.key_pressed(Key::ArrowLeft) as i32
        });
        self.draw_sample_gradient_steps_cloud = self.draw_sample_gradient_steps_cloud.max(-1);
    }

    fn show_fractal(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        primary_camera_map: &CameraMap,
        secondary_camera_map: &CameraMap,
    ) {
        let screen_center = ui.max_rect().center();
        let z0 = primary_camera_map.pos_to_complex(screen_center);
        let painter = ui.painter_at(ui.max_rect());

        // we need to do this on the first frame
        if !self.has_begun_rendering {
            self.metabrot
                .begin_rendering(primary_camera_map, self.needs_full_redraw);
            self.has_begun_rendering = true;
        }

        let now = Instant::now();
        let should_rerender = {
            let dt = (now - self.fractal_try_render_time).as_nanos();
            self.fractal_try_render_time = now;
            self.fractal_accumulator_ns += dt as i128;
            self.fractal_accumulator_ns > self.fractal_target_spf.as_nanos() as i128
        };
        // don't wait if we need a full redraw, bc then panning looks bad.
        if should_rerender || self.needs_full_redraw {
            self.fractal_dts.add(
                ctx.input(|i| i.time),
                (now - self.fractal_successful_render_time).as_secs_f32(),
            );
            self.fractal_successful_render_time = now;

            self.fractal_accumulator_ns -= self.fractal_target_spf.as_nanos() as i128;
            // because we sometimes are forced to redraw.
            // don't allow the accumulator to get too negative,
            // or else we get a big lag spike when we finally do redraw.
            self.fractal_accumulator_ns = self
                .fractal_accumulator_ns
                .max(-(self.fractal_target_spf.as_nanos() as i128));

            match self.current_fractal {
                CurrentFractal::Metabrot => {
                    // lmao we can just switch the order of these two calls
                    // note that this adds a frame of latency
                    self.metabrot.finish_rendering(&mut self.texture);
                    self.metabrot
                        .begin_rendering(primary_camera_map, self.needs_full_redraw);
                    self.needs_full_redraw = false;
                }
                CurrentFractal::Mandelbrot => {
                    if let Some(z0) = z0 {
                        fractal::render_mandelbrot(&mut self.texture, secondary_camera_map, z0);
                    } else {
                        fractal::render_color(&mut self.texture, secondary_camera_map);
                    }
                }
            }
            assert_eq!(primary_camera_map.rect(), secondary_camera_map.rect());
        }
        painter.image(
            self.texture.id(),
            primary_camera_map.rect(),
            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    fn draw_complex_circle_stroke(
        painter: &egui::Painter,
        camera_map: &CameraMap,
        (c_real, c_imag): (Real, Imag),
        rad: Fixed,
        stroke: egui::Stroke,
    ) {
        painter.circle_stroke(
            camera_map.complex_to_pos((c_real, c_imag)),
            camera_map.delta_real_to_vec1(rad),
            stroke,
        );
    }

    fn draw_complex_segment(
        painter: &egui::Painter,
        camera_map: &CameraMap,
        (c1_real, c1_imag): (Real, Imag),
        (c2_real, c2_imag): (Real, Imag),
        stroke: egui::Stroke,
    ) {
        painter.line_segment(
            [
                camera_map.complex_to_pos((c1_real, c1_imag)),
                camera_map.complex_to_pos((c2_real, c2_imag)),
            ],
            stroke,
        );
    }

    /// draw a dot at the center of the screen.
    fn show_crosshair(&self, painter: &egui::Painter, screen_center: Pos2) {
        painter.circle_filled(screen_center, 5.0, Color32::BLUE);
    }

    /// draw a dot at z0.
    fn show_z0(&self, painter: &egui::Painter, secondary_camera_map: &CameraMap, z0: (Real, Imag)) {
        painter.circle_filled(
            secondary_camera_map.complex_to_pos(z0),
            5.0,
            Color32::from_rgb(160, 0, 160),
        );
    }

    /// draw the outline of the window
    fn show_sample_window(
        &self,
        painter: &egui::Painter,
        secondary_camera_map: &CameraMap,
        window: Window,
    ) {
        painter.rect_stroke(
            secondary_camera_map.window_to_rect(window),
            0.0,
            egui::Stroke::new(2.0, Color32::WHITE),
            egui::StrokeKind::Middle,
        );
    }

    fn show_sample_grid_cloud(
        &self,
        painter: &egui::Painter,
        secondary_camera_map: &CameraMap,
        z0: (Real, Imag),
        window: Window,
    ) {
        // TODO: less code reuse
        let mut deepest: f32 = 0.0;
        let mut deepest_point = (Fixed::ZERO, Fixed::ZERO);

        // draw initial points
        for (c_real, c_imag) in window
            .grid_centers(sample::WIDTH0, sample::WIDTH0)
            .flatten()
        {
            let sample = sample::quadratic_map(z0, (c_real, c_imag));
            if sample.depth > deepest {
                deepest = sample.depth;
                deepest_point = (c_real, c_imag);
            }
            painter.circle_filled(
                secondary_camera_map.complex_to_pos((c_real, c_imag)),
                dynamic_draw_size(secondary_camera_map, 5.0),
                Color32::DARK_RED,
            );
        }

        // draw the deepest point
        painter.circle_filled(
            secondary_camera_map.complex_to_pos(deepest_point),
            // don't use dynamic size for this
            5.0,
            Color32::RED,
        );
    }

    /// debug hierarchical windows.
    /// draw points that got resampled in a small window around them.
    fn show_sample_subgrid_cloud(
        &self,
        painter: &egui::Painter,
        secondary_camera_map: &CameraMap,
        z0: (Real, Imag),
        window: Window,
    ) {
        // TODO: less code reuse

        let mut deepest: f32 = 0.0;
        let mut deepest_point = (Fixed::ZERO, Fixed::ZERO);
        // we want to look through all the points at a coarse grain before resampling
        let mut to_resample = Vec::with_capacity(sample::WIDTH0 * sample::WIDTH0);
        let cell_rad = {
            (window.real_rad().div_f64(sample::WIDTH0 as f64))
                .max(window.imag_rad().div_f64(sample::WIDTH0 as f64))
        };
        // draw initial points
        for (c_real, c_imag) in window
            .grid_centers(sample::WIDTH0, sample::WIDTH0)
            .flatten()
        {
            let SampleMaybeDistance { depth, distance } =
                sample::distance_estimator(z0, (c_real, c_imag));
            if depth > deepest {
                deepest = depth;
                deepest_point = (c_real, c_imag);
            }
            if let Some(distance) = distance
                && distance < cell_rad.mul2()
            {
                to_resample.push((c_real, c_imag));
            }
            // painter.circle_filled(
            //     secondary_camera_map.complex_to_pos((c_real, c_imag)),
            //     dynamic_draw_size(secondary_camera_map, 5.0),
            //     Color32::DARK_RED,
            // );
        }

        // TODO: try sorting the vec by distance estimate
        // draw points that got resampled
        for (c0_real, c0_imag) in to_resample {
            let resample_window =
                Window::from_mid_rad(c0_real, c0_imag, cell_rad, cell_rad).unwrap();
            for (c_real, c_imag) in resample_window
                .grid_centers(sample::WIDTH1, sample::WIDTH1)
                .flatten()
            {
                if (c0_real, c0_imag) == (c_real, c_imag) {
                    continue;
                }
                let sample = sample::quadratic_map(z0, (c_real, c_imag));
                if sample.depth > deepest {
                    deepest = sample.depth;
                    deepest_point = (c_real, c_imag);
                }
                painter.circle_filled(
                    secondary_camera_map.complex_to_pos((c_real, c_imag)),
                    dynamic_draw_size(secondary_camera_map, 3.0),
                    // dark orange
                    Color32::from_rgb(200, 120, 0),
                );
            }
        }

        // draw the deepest point
        painter.circle_filled(
            secondary_camera_map.complex_to_pos(deepest_point),
            // don't use dynamic size for this
            5.0,
            // bright orange or smt
            Color32::from_rgb(255, 200, 0),
        );
    }

    fn show_sample_gradient_steps_cloud(
        &self,
        painter: &egui::Painter,
        secondary_camera_map: &CameraMap,
        z0: (Real, Imag),
        window: Window,
    ) {
        // let mut deepest: f32 = 0.0;
        // let mut deepest_point = (Fixed::ZERO, Fixed::ZERO);

        for line in window.grid_centers(sample::WIDTH0, sample::WIDTH0) {
            for (c_real, c_imag) in line {
                // the image of the sample under a few gradient descent steps
                let Some(stepped_c) = (|| {
                    let (mut c_real, mut c_imag) = (c_real, c_imag);
                    for _ in 0..self.draw_sample_gradient_steps_cloud {
                        (c_real, c_imag) = SampleDistanceGradient::step(z0, (c_real, c_imag))?;
                    }
                    Some((c_real, c_imag))
                })() else {
                    continue;
                };
                painter.circle_filled(
                    secondary_camera_map.complex_to_pos(stepped_c),
                    dynamic_draw_size(secondary_camera_map, 3.0),
                    Color32::from_gray(200),
                );
            }
        }

        // // TODO: draw the deepest point
        // painter.circle_filled(
        //     secondary_camera_map.complex_to_pos(deepest_point),
        //     // don't use dynamic size for this
        //     5.0,
        //     Color32::WHITE,
        // );
    }

    /// draw distance estimator with gradient descent steps
    fn show_sample_mouse_gradient_steps(
        &self,
        painter: &egui::Painter,
        secondary_camera_map: &CameraMap,
        z0: (Real, Imag),
        ctx: &egui::Context,
    ) {
        let Some(mut c) = secondary_camera_map
            .pos_to_complex(ctx.input(|i| i.pointer.latest_pos().unwrap_or_default()))
        else {
            return;
        };

        const MAX_STEPS: usize = 8;
        for _ in 0..MAX_STEPS {
            let Some(sample) = SampleDistanceGradient::new(z0, c) else {
                return;
            };
            Self::draw_complex_circle_stroke(
                painter,
                secondary_camera_map,
                c,
                sample.distance,
                egui::Stroke::new(1.0, Color32::WHITE),
            );

            let Some(next_c) = sample.stepped(c) else {
                return;
            };
            Self::draw_complex_segment(
                painter,
                secondary_camera_map,
                c,
                next_c,
                egui::Stroke::new(1.0, Color32::WHITE),
            );
            painter.circle_filled(
                secondary_camera_map.complex_to_pos(next_c),
                // don't use dynamic size for this
                3.0,
                Color32::WHITE,
            );
            c = next_c;
        }
    }

    fn show_fractal_debug(
        &self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        primary_camera_map: &CameraMap,
        secondary_camera_map: &CameraMap,
    ) {
        let painter = ui.painter_at(ui.max_rect());
        let screen_center = ui.max_rect().center();
        let z0 = primary_camera_map.pos_to_complex(screen_center);
        let window = z0.and_then(sample::initial_window);

        if self.draw_crosshair {
            self.show_crosshair(&painter, screen_center);
        }

        if self.current_fractal == CurrentFractal::Mandelbrot
            && let Some(z0) = z0
        {
            if self.draw_z0 {
                self.show_z0(&painter, secondary_camera_map, z0);
            }

            if let Some(window) = window {
                if self.draw_sample_window {
                    self.show_sample_window(&painter, secondary_camera_map, window);
                }

                if self.draw_sample_grid_cloud {
                    self.show_sample_grid_cloud(&painter, secondary_camera_map, z0, window);
                }

                if self.draw_sample_subgrid_cloud {
                    self.show_sample_subgrid_cloud(&painter, secondary_camera_map, z0, window);
                }

                if self.draw_sample_gradient_steps_cloud >= 0 {
                    self.show_sample_gradient_steps_cloud(
                        &painter,
                        secondary_camera_map,
                        z0,
                        window,
                    );
                }
            }

            if self.draw_sample_mouse_gradient_steps {
                self.show_sample_mouse_gradient_steps(&painter, secondary_camera_map, z0, ctx);
            }
        }
    }

    /// factor this out,
    /// bc rustfmt dies on deeply nested code with string literals.
    /// it's fixed by increasing max_width, but that's really coarse.
    fn show_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

        egui::CollapsingHeader::new("stats")
            .default_open(true)
            .show(ui, |ui| {
                // global frame rate
                {
                    let average_dt = self
                        .global_dts
                        .average()
                        .expect("we added one this frame so dts must be non-empty");
                    ui.label(format!("global fps: {:.01}", 1.0 / average_dt))
                        .on_hover_text("the global / app / ui frames per second");
                    // ui.label(format!("global spf: {:.05}", average_dt))
                    //     .on_hover_text("the global / app / ui seconds per frame");
                }

                // fractal frame rate
                if let Some(average_dt) = self.fractal_dts.average() {
                    ui.label(format!("fractal fps: {:.01}", 1.0 / average_dt))
                        .on_hover_text("the fractal only frames per second");
                    // ui.label(format!("fractal spf: {:.05}", average_dt))
                    //     .on_hover_text("the fractal only seconds per frame");
                }

                // reclaim tick rate
                if let Some(average_dt) = self.reclaim_dts.average() {
                    ui.label(format!("reclaim tps: {:.01}", 1.0 / average_dt))
                        .on_hover_text("the reclaim ticks per second");
                }

                // reclaim count
                {
                    ui.label(format!(
                        "reclaims/sec: {:.01}",
                        self.reclaim_counts.values().sum::<u64>() as f32
                            / self.reclaim_counts.len() as f32
                    ));
                }

                // sample count
                {
                    ui.label(format!(
                        "samples/sec: {:.01}",
                        self.sample_counts.values().sum::<u64>() as f32
                            / self.sample_counts.len() as f32
                    ));
                }

                // node count
                // `CollapsingHeader` bc computing `node_count` is expensive
                egui::CollapsingHeader::new("node count").show(ui, |ui| {
                    // wacky stuff to get around the borrow checker
                    let tree = Arc::clone(self.metabrot.tree());
                    ui.label(format!(
                        "node count: {}",
                        tree.node_count(&mut self.metabrot.thread_data)
                    ));
                });

                egui::CollapsingHeader::new("camera").show(ui, |ui| {
                    let add_contents = |ui: &mut egui::Ui| {
                        ui.label(format!(
                            "metabrot real mid: {:12.09}",
                            self.primary_camera.real_mid()
                        ));
                        ui.label(format!(
                            "metabrot imag mid: {:12.09}",
                            self.primary_camera.imag_mid()
                        ));
                        ui.label(format!(
                            "metabrot real rad: {:12.09}",
                            self.primary_camera.real_rad()
                        ));

                        // do this so the separator's size is the size of the content,
                        // rather than the full width of the parent container.
                        // TODO: the separator only gets sized to the content above it.
                        ui.shrink_width_to_current();
                        ui.separator();

                        ui.label(format!(
                            "mandelbrot real mid: {:12.09}",
                            self.secondary_camera.real_mid()
                        ));
                        ui.label(format!(
                            "mandelbrot imag mid: {:12.09}",
                            self.secondary_camera.imag_mid()
                        ));
                        ui.label(format!(
                            "mandelbrot real rad: {:12.09}",
                            self.secondary_camera.real_rad()
                        ));
                    };
                    add_contents(ui);

                    // egui::Area::new("camera").
                    // egui::Frame::
                    // egui::UiBuilder::new()
                    // ui.set_sizing_pass();
                    // ui.allocate_exact_size(, egui::Sense::empty());
                    // ui.scope_builder(egui::UiBuilder::, add_contents)
                    // ui.scope(|ui| {});
                    // ui.shrink_width_to_current();

                    // ui.scope_builder(egui::UiBuilder::new(), |ui| {
                    //     add_contents(ui);
                    // });
                    // egui::Area::new(egui::Id::new("camera")).show(ctx, |ui| {
                    //     ui.scope_builder(egui::UiBuilder::new(), |ui| {
                    //         add_contents(ui);
                    //     });
                    // });
                    // let r = ui
                    //     .scope_builder(egui::UiBuilder::new().sizing_pass(), |ui| {
                    //         add_contents(ui);
                    //     })
                    //     .response;
                    // ui.scope_builder(egui::UiBuilder::new(), |ui| {
                    //     ui.set_max_size(r.rect.size());
                    //     add_contents(ui);
                    // });
                });
                // ui.shrink_width_to_current();

                egui::CollapsingHeader::new("timers")
                    .default_open(true)
                    .show(ui, |ui| {
                        let add_contents = |ui: &mut egui::Ui| {
                            let timer = self
                                .timers
                                .values()
                                .reduce(|lhs, rhs| lhs + rhs)
                                .unwrap_or_default();

                            ui.label(format!(
                                "us per draw: {:.03}",
                                timer
                                    .draw
                                    .div_count(timer.draw.count())
                                    .unwrap_or_default()
                                    .as_nanos() as f64
                                    / 1000.0
                            ));
                            ui.label(format!(
                                "us per reclaim: {:.03}",
                                timer
                                    .reclaim
                                    .div_count(timer.reclaim.count())
                                    .unwrap_or_default()
                                    .as_nanos() as f64
                                    / 1000.0
                            ));
                            ui.label(format!(
                                "us per retire: {:.03}",
                                timer
                                    .retire
                                    .div_count(timer.retire.count())
                                    .unwrap_or_default()
                                    .as_nanos() as f64
                                    / 1000.0
                            ));
                            ui.label(format!(
                                "us per sample: {:.03}",
                                timer
                                    .sample
                                    .div_count(timer.sample.count())
                                    .unwrap_or_default()
                                    .as_nanos() as f64
                                    / 1000.0
                            ));
                            ui.label(format!(
                                "us per split: {:.03}",
                                timer
                                    .split
                                    .div_count(timer.split.count())
                                    .unwrap_or_default()
                                    .as_nanos() as f64
                                    / 1000.0
                            ));
                            ui.label(format!(
                                "us per idle: {:.03}",
                                timer
                                    .idle
                                    .div_count(timer.idle.count())
                                    .unwrap_or_default()
                                    .as_nanos() as f64
                                    / 1000.0
                            ));

                            // do this so the separator's size is the size of the content,
                            // rather than the full width of the parent container.
                            // TODO: the separator only gets sized to the content above it.
                            ui.shrink_width_to_current();
                            ui.separator();
                            // TODO: flamegraph
                            let total_elapsed = timer.total().elapsed();

                            ui.label(format!(
                                "draw portion: {:.03}",
                                timer.draw.div_elapsed(total_elapsed)
                            ));
                            ui.label(format!(
                                "reclaim portion: {:.03}",
                                timer.reclaim.div_elapsed(total_elapsed)
                            ));
                            ui.label(format!(
                                "retire portion: {:.03}",
                                timer.retire.div_elapsed(total_elapsed)
                            ));
                            ui.label(format!(
                                "sample portion: {:.03}",
                                timer.sample.div_elapsed(total_elapsed)
                            ));
                            ui.label(format!(
                                "split portion: {:.03}",
                                timer.split.div_elapsed(total_elapsed)
                            ));
                            ui.label(format!(
                                "idle portion: {:.03}",
                                timer.idle.div_elapsed(total_elapsed)
                            ));
                        };
                        add_contents(ui);
                        // // we don't want to inherit the min rect from our parent
                        // // it should be even smaller
                        // ui.scope_builder(egui::UiBuilder::new(), |ui| {
                        //     add_contents(ui);
                        // });
                    });
                // ui.shrink_width_to_current();
            });

        egui::CollapsingHeader::new("settings").default_open(true).show(ui, |ui| {
            // max fps
            {
                let mut target_fps = 1.0 / self.fractal_target_spf.as_secs_f64();
                let r = ui
                    .add(MyDragValue::new(egui::Label::new("max fps:"), egui::DragValue::new(&mut target_fps)))
                    .on_hover_text(
                        "the max fps at which we render the fractal. the ui is always rendered at max speed. this is overwritten when eg panning.",
                    );
                target_fps = target_fps.max(1.0);
                if r.dragged() {
                    target_fps = target_fps.min(240.0);
                }
                if r.changed() {
                    self.fractal_target_spf = Duration::from_secs_f64(1.0 / target_fps);
                }
            }

            // metabrot stride
            {
                let mut stride = self.primary_camera_stride as f64;
                let r = ui
                    .add(MyDragValue::new(
                        egui::Label::new("metabrot pixel size:"),
                        egui::DragValue::new(&mut stride),
                    ))
                    .on_hover_text("how many pixels are in a pixel for the metabrot.");
                let mut stride = stride.round() as usize;
                stride = stride.max(1);
                if r.dragged() {
                    stride = stride.min(64);
                }
                if r.changed() {
                    self.primary_camera_stride = stride;
                    self.needs_full_redraw = true;
                }
            }

            // mandelbrot stride
            {
                let mut stride = self.secondary_camera_stride as f64;
                let r = ui
                    .add(MyDragValue::new(
                        egui::Label::new("mandelbrot pixel size:"),
                        egui::DragValue::new(&mut stride),
                    ))
                    .on_hover_text("how many pixels are in a pixel for the mandelbrot.");
                let mut stride = stride.round() as usize;
                stride = stride.max(1);
                if r.dragged() {
                    stride = stride.min(64);
                }
                if r.changed() {
                    self.secondary_camera_stride = stride;
                    self.needs_full_redraw = true;
                }
            }

            // sampling
            {
                ui.checkbox(&mut self.sampling, "sampling").on_hover_text(
                    "whether to get new samples of the metabrot. keybinding: ".to_owned()
                        + &ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::X)),
                );
            }

            // reclaiming
            {
                ui.checkbox(&mut self.reclaiming, "reclaiming").on_hover_text(
                    "whether to reclaim/free/deallocate nodes. keybinding: ".to_owned()
                        + &ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::R)),
                );
            }

            // crosshair
            {
                ui.checkbox(&mut self.draw_crosshair, "draw crosshair")
                    .on_hover_text("draw a dot at the screen center.");
            }

            // current fractal
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new("metabrot").selected(self.current_fractal == CurrentFractal::Metabrot))
                    .on_hover_text(
                        "whether to render the metabrot (rather than the mandelbrot). this also triggers a full redraw. keybinding: ".to_owned()
                            + &ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, Key::N)),
                    )
                    .clicked()
                {
                    self.current_fractal = CurrentFractal::Metabrot;
                    self.needs_full_redraw = true;
                }

                if ui
                    .add(egui::Button::new("mandelbrot").selected(self.current_fractal == CurrentFractal::Mandelbrot))
                    .on_hover_text(
                        "whether to render the mandelbrot (rather than the metabrot). keybinding: ".to_owned()
                            + &ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, Key::M)),
                    )
                    .clicked()
                {
                    self.current_fractal = CurrentFractal::Mandelbrot;
                };
            });

            // control other camera
            {
                ui.checkbox(&mut self.control_other_camera, "control other fractal").on_hover_text(
                    "pan/zoom will affect the other fractal's camera. keybinding: ".to_owned()
                        + &ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, Key::C)),
                );
            }

            egui::CollapsingHeader::new("mandelbrot").show(ui, |ui| {
                ui.checkbox(&mut self.draw_z0, "draw z0");                
                ui.checkbox(&mut self.draw_sample_window, "draw sample window");
                ui.checkbox(&mut self.draw_sample_grid_cloud, "draw sample grid cloud");
                ui.checkbox(&mut self.draw_sample_subgrid_cloud, "draw sample subgrid cloud");
                {
                    let r = ui
                        .add(MyDragValue::new(
                            egui::Label::new("draw sample grid gradient steps"),
                            egui::DragValue::new(&mut self.draw_sample_gradient_steps_cloud),
                        ))
                        .on_hover_text("-1 to disable, 0 is the same as draw_sample_grid. keybinding: left and right arrows");
                    self.draw_sample_gradient_steps_cloud = self.draw_sample_gradient_steps_cloud.max(-1);
                    if r.dragged() {
                        self.draw_sample_gradient_steps_cloud = self.draw_sample_gradient_steps_cloud.min(8);
                    }
                }
                ui.checkbox(&mut self.draw_sample_mouse_gradient_steps, "draw mouse gradient steps")
                    .on_hover_text("draw the gradient steps starting from the mouse position");
            });
        });

        // // give some margin on the bottom and right,
        // // but only when uncollapsed,
        // // so we can't use frame inner margin.
        // ui.allocate_space(Vec2::new(ui.min_size().x + 3.0, 3.0));
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show(ctx, |ui| {
                {
                    // i don't trust `stable_dt`
                    let now = Instant::now();
                    let elapsed = now - self.last_frame_time;
                    self.last_frame_time = now;
                    self.global_dts
                        .add(ctx.input(|i| i.time), elapsed.as_secs_f32());
                }

                if self.metabrot.try_reclaim_tick().is_some() {
                    let now = Instant::now();
                    let elapsed = now - self.last_reclaim_tick;
                    self.last_reclaim_tick = now;
                    self.reclaim_dts
                        .add(ctx.input(|i| i.time), elapsed.as_secs_f32());
                } else {
                    // dbg!("failed tick reclaim");
                }

                self.timers
                    .add(ctx.input(|i| i.time), self.metabrot.timer());

                self.keybinds(ctx);

                // debug static counters
                #[cfg(false)]
                {
                    // println!();
                    if let Some(nanos) = tree::ELAPSED_NANOS
                        .load(Ordering::Relaxed)
                        .checked_div(tree::COUNTER.load(Ordering::Relaxed))
                    {
                        println!("tree average time: {} ns", nanos);
                    }
                    if let Some(nanos) = pool::ELAPSED_NANOS
                        .load(Ordering::Relaxed)
                        .checked_div(pool::COUNTER.load(Ordering::Relaxed))
                    {
                        println!("pool average time: {} ns", nanos);
                        println!(
                            "pool average / tc: {} ns",
                            nanos / self.pool.thread_count() as u64
                        );
                    }
                    if let Some(max_i) =
                        pool::WORKER_HIST
                            .iter()
                            .enumerate()
                            .rev()
                            .find_map(|(i, count)| {
                                if count.load(Ordering::Relaxed) > 0 {
                                    Some(i)
                                } else {
                                    None
                                }
                            })
                    {
                        println!("worker hist:");
                        for (i, worker) in pool::WORKER_HIST.iter().enumerate().take(max_i + 1) {
                            println!("worker {}: {} samples", i, worker.load(Ordering::Relaxed));
                        }
                    }
                }

                // sample time counters
                #[cfg(false)]
                {
                    if let Some(nanos) = sample::SAMPLE_ELAPSED_NANOS
                        .load(Ordering::Relaxed)
                        .checked_div(sample::SAMPLE_COUNTER.load(Ordering::Relaxed))
                    {
                        println!("sample average time: {} us", nanos / 1000);
                    }
                }

                // coloring pruned vs unpruned
                #[cfg(false)]
                {
                    println!();
                    if let Some(nanos) = PRUNED_ELAPSED
                        .load(Ordering::Relaxed)
                        .checked_div(PRUNED_COUNTER.load(Ordering::Relaxed))
                    {
                        println!("pruned average time: {} ns", nanos);
                    }
                    if let Some(nanos) = UNPRUNED_ELAPSED
                        .load(Ordering::Relaxed)
                        .checked_div(UNPRUNED_COUNTER.load(Ordering::Relaxed))
                    {
                        println!("unpruned average time: {} ns", nanos);
                    }
                    {
                        let pruned_count = PRUNED_COUNTER.load(Ordering::Relaxed);
                        let unpruned_count = UNPRUNED_COUNTER.load(Ordering::Relaxed);
                        println!("pruned count: {}", pruned_count);
                        println!("unpruned count: {}", unpruned_count);
                        println!(
                            "pruned / (pruned + unpruned): {}",
                            pruned_count as f64 / (pruned_count as f64 + unpruned_count as f64)
                        );
                    }

                    PRUNED_ELAPSED.store(0, Ordering::Relaxed);
                    PRUNED_COUNTER.store(0, Ordering::Relaxed);
                    UNPRUNED_ELAPSED.store(0, Ordering::Relaxed);
                    UNPRUNED_COUNTER.store(0, Ordering::Relaxed);
                }

                // panning stuff
                // pan the other camera when holding backtick
                self.needs_full_redraw |= {
                    let before = (self.primary_camera, self.secondary_camera);
                    if self.control_other_camera
                        != (self.current_fractal == CurrentFractal::Metabrot)
                    {
                        CameraMap::pan_zoom(
                            ctx,
                            ui,
                            &mut self.primary_camera,
                            &mut self.primary_camera_velocity,
                        )
                    } else {
                        CameraMap::pan_zoom(
                            ctx,
                            ui,
                            &mut self.secondary_camera,
                            &mut self.secondary_camera_velocity,
                        )
                    };
                    let after = (self.primary_camera, self.secondary_camera);
                    before != after
                };

                let primary_camera_map = CameraMap::new(
                    ui.max_rect(),
                    self.primary_camera,
                    self.primary_camera_stride,
                );
                let secondary_camera_map = CameraMap::new(
                    ui.max_rect(),
                    self.secondary_camera,
                    self.secondary_camera_stride,
                );

                // reclaiming
                if self.reclaiming
                    && let Some(window) = primary_camera_map.window()
                {
                    let reclaim_count = self.metabrot.enable_reclaiming(window);
                    self.reclaim_counts
                        .add(ctx.input(|i| i.time), reclaim_count);
                } else {
                    self.metabrot.disable_reclaiming();
                }

                // sampling
                if self.sampling
                    && let Some(window) = primary_camera_map.window()
                {
                    let sample_count = self.metabrot.enable_sampling(window);
                    self.sample_counts.add(ctx.input(|i| i.time), sample_count);
                } else {
                    self.metabrot.disable_sampling();
                }

                // // reclaiming
                // if self.reclaiming {
                //     self.metabrot.enable_reclaiming();
                // } else {
                //     self.metabrot.disable_reclaiming();
                // }

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

                // // camera_map.pixels small square debugging
                // #[cfg(false)]
                // {
                //     let pixels = camera_map.pixels(self.stride).collect::<Vec<_>>();
                //     let expected_len = camera_map.rect().size().x as usize
                //         * camera_map.rect().size().y as usize
                //         / (self.stride * self.stride);
                //     assert!(pixels.len() <= expected_len);
                //     if pixels.len() < expected_len {
                //         let painter = ui.painter_at(ui.max_rect());
                //         for ((row, col), rect, pixel) in &pixels {
                //             painter.rect_filled(
                //                 *rect,
                //                 0.0,
                //                 if (row + col) % 2 == 0 {
                //                     Color32::MAGENTA
                //                 } else {
                //                     Color32::LIGHT_GREEN
                //                 },
                //             );
                //         }
                //         for ((row, col), rect, pixel) in &pixels {
                //             if rect
                //                 .contains(ui.input(|i| i.pointer.latest_pos().unwrap_or_default()))
                //             {
                //                 ui.label(format!(
                //                     "real_mid: {:?}\nimag_mid: {:?}\nrad: {:?}\nreal_lo: {:?}\nreal_hi: {:?}\nimag_lo: {:?}\nimag_hi: {:?}",
                //                     pixel.real_mid(), pixel.imag_mid(), pixel.rad(), pixel.real_lo(), pixel.real_hi(), pixel.imag_lo(), pixel.imag_hi())
                //                 );
                //             }
                //         }
                //         return;
                //     }
                // }

                self.show_fractal(ctx, ui, &primary_camera_map, &secondary_camera_map);
                self.show_fractal_debug(ctx, ui, &primary_camera_map, &secondary_camera_map);

                // area is to allow the frame to be drawn on top of the fractal
                egui::Area::new(egui::Id::new("area"))
                    .constrain_to(ctx.screen_rect())
                    .anchor(egui::Align2::LEFT_TOP, egui::Vec2::ZERO)
                    .show(ui.ctx(), |ui| {
                        // frame is for background
                        egui::Frame::new()
                            .fill(Color32::from_rgba_unmultiplied(40, 40, 40, 245))
                            .inner_margin(egui::Margin {
                                left: 3,
                                right: 6,
                                top: 3,
                                bottom: 6,
                            })
                            .corner_radius(egui::CornerRadius {
                                nw: 0,
                                ne: 0,
                                sw: 0,
                                se: 5,
                            })
                            .show(ui, |ui| {
                                egui::CollapsingHeader::new("info")
                                    .default_open(true)
                                    .show_unindented(ui, |ui| {
                                        self.show_ui(ctx, ui);
                                    });
                            });
                    });
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.metabrot.join();
    }
}

use my_drag_value::*;
mod my_drag_value {
    use egui::{DragValue, Label, Response, Ui, Widget};

    use super::*;

    /// puts the label to the left of the `DragValue`.
    pub(super) struct MyDragValue<'a> {
        label: Label,
        drag_value: DragValue<'a>,
    }
    impl<'a> MyDragValue<'a> {
        pub fn new(label: Label, drag_value: DragValue<'a>) -> MyDragValue<'a> {
            MyDragValue { label, drag_value }
        }
    }
    impl Widget for MyDragValue<'_> {
        fn ui(self, ui: &mut Ui) -> Response {
            let r = ui.horizontal(|ui| {
                let label_r = self.label.ui(ui);
                let slider_r = self.drag_value.ui(ui);
                label_r | slider_r
            });
            r.inner | r.response
        }
    }
}
