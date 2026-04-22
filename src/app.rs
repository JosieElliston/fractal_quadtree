use std::sync::atomic::Ordering;

use eframe::egui::{self, Color32, Key, Pos2, Rect, Vec2};

use crate::{
    complex::{Camera, CameraMap, Domain, Window, fixed::*},
    fractal::{self, Fractal},
    sample,
    tree::ThreadData,
};

/// fancy dynamic radius based on zoom
/// so that if you're zoomed out, points don't cover everything
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

pub(crate) struct App {
    metabrot: Fractal,

    // tree: Tree,
    stride: usize,
    primary_camera: Camera,
    primary_camera_velocity: Vec2,
    secondary_camera: Camera,
    secondary_camera_velocity: Vec2,
    dts: egui::util::History<f32>,
    /// how many samples we received on each frame
    sample_counts: egui::util::History<u64>,
    timers: egui::util::History<fractal::MultiTimer>,
    texture: egui::TextureHandle,
    sampling: bool,
    draw_crosshair: bool,
    // current_fractal: usize,
    current_fractal: CurrentFractal,
    control_other_camera: bool,
    draw_gradient_steps: bool,
    draw_sample_grid: bool,
    draw_sample_subgrid: bool,
    draw_sample_grid_gradient_steps: i32,
}
impl App {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            metabrot: Fractal::new_metabrot(),
            stride: 1,
            // stride: 8,
            primary_camera: Camera::default(),
            primary_camera_velocity: Vec2::ZERO,
            secondary_camera: Camera::default(),
            secondary_camera_velocity: Vec2::ZERO,
            dts: egui::util::History::new(10..1000, 1.0),
            sample_counts: egui::util::History::new(10..1000, 5.0),
            timers: egui::util::History::new(10..1000, 5.0),
            texture: cc
                .egui_ctx
                .load_texture("fractal", egui::ColorImage::example(), egui::TextureOptions::NEAREST),
            sampling: true,
            draw_crosshair: false,
            current_fractal: CurrentFractal::Metabrot,
            control_other_camera: false,
            draw_gradient_steps: true,
            draw_sample_grid: true,
            draw_sample_subgrid: false,
            draw_sample_grid_gradient_steps: -1,
        }
    }

    /// draw the fractal and debug stuff
    fn show_fractal(
        &mut self,
        ctx: &eframe::egui::Context,
        ui: &mut egui::Ui,
        primary_camera_map: &CameraMap,
        secondary_camera_map: &CameraMap,
        needs_full_redraw: bool,
    ) {
        let screen_center = ui.max_rect().center();
        let z0 = primary_camera_map.pos_to_complex(screen_center);
        let painter = ui.painter_at(ui.max_rect());

        // draw the fractal
        {
            match self.current_fractal {
                CurrentFractal::Metabrot => {
                    self.metabrot.begin_rendering(primary_camera_map.clone(), needs_full_redraw);
                    self.metabrot.finish_rendering(&mut self.texture);
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
            painter.image(
                self.texture.id(),
                primary_camera_map.rect(),
                Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        let draw_complex_circle_stroke = |(c_real, c_imag): (Real, Imag), rad: Fixed, stroke: egui::Stroke| {
            painter.circle_stroke(
                secondary_camera_map.complex_to_pos((c_real, c_imag)),
                secondary_camera_map.delta_real_to_vec1(rad),
                stroke,
            );
        };
        let draw_complex_segment = |(c1_real, c1_imag): (Real, Imag), (c2_real, c2_imag): (Real, Imag), stroke: egui::Stroke| {
            painter.line_segment(
                [
                    secondary_camera_map.complex_to_pos((c1_real, c1_imag)),
                    secondary_camera_map.complex_to_pos((c2_real, c2_imag)),
                ],
                stroke,
            );
        };

        // draw stuff that uses the window we're sampling the mandelbrot in
        'mandelbrot_window: {
            if self.current_fractal != CurrentFractal::Mandelbrot {
                break 'mandelbrot_window;
            }
            let Some((z0_real, z0_imag)) = z0 else {
                break 'mandelbrot_window;
            };
            // let window = Window::from_center_size(Fixed::ZERO, Fixed::ZERO, 4.0.into(), 4.0.into());
            let Some(window) = (|| {
                Window::from_mid_rad(
                    (f64::from(z0_imag) * f64::from(z0_imag) - f64::from(z0_real) * f64::from(z0_real))
                        .try_into()
                        .ok()?,
                    (-2.0 * f64::from(z0_real) * f64::from(z0_imag)).try_into().ok()?,
                    2.0.try_into().unwrap(),
                    2.0.try_into().unwrap(),
                )
            })() else {
                break 'mandelbrot_window;
            };

            // draw the outline of the window
            painter.rect_stroke(
                secondary_camera_map.window_to_rect(window),
                0.0,
                egui::Stroke::new(2.0, Color32::WHITE),
                egui::StrokeKind::Middle,
            );

            // debug for gradient descent steps
            // draw dots where we took samples, to debug aliasing
            // #[cfg(false)]
            'gradient_descent_steps: {
                if self.draw_sample_grid_gradient_steps < 0 {
                    break 'gradient_descent_steps;
                }
                let Some(z0) = z0 else {
                    break 'gradient_descent_steps;
                };
                for line in window.grid_centers(sample::WIDTH0, sample::WIDTH0) {
                    for (c_real, c_imag) in line {
                        // the image of the sample under a few gradient descent steps
                        if let Some(stepped_c) = (|| {
                            let (mut c_real, mut c_imag) = (c_real, c_imag);
                            for _ in 0..self.draw_sample_grid_gradient_steps {
                                (c_real, c_imag) = sample::gradient_step(z0, (c_real, c_imag))?;
                            }
                            Some((c_real, c_imag))
                        })() {
                            painter.circle_filled(
                                secondary_camera_map.complex_to_pos(stepped_c),
                                dynamic_draw_size(secondary_camera_map, 3.0),
                                Color32::WHITE,
                            );
                        }
                    }
                }
            }

            // debug hierarchical windows
            // draw points that got resampled in a small window around them
            // TODO: less code reuse
            if self.draw_sample_grid {
                let mut deepest: f32 = 0.0;
                let mut deepest_point = (Fixed::ZERO, Fixed::ZERO);
                // we want to look through all the points at a coarse grain before resampling
                let mut to_resample = Vec::with_capacity(sample::WIDTH0 * sample::WIDTH0);
                let cell_rad =
                    { (window.real_rad().div_f64(sample::WIDTH0 as f64)).max(window.imag_rad().div_f64(sample::WIDTH0 as f64)) };
                // draw initial points
                for (c_real, c_imag) in window.grid_centers(sample::WIDTH0, sample::WIDTH0).flatten() {
                    let (sample, distance) = sample::distance_estimator((z0_real, z0_imag), (c_real, c_imag));
                    if sample.depth > deepest {
                        deepest = sample.depth;
                        deepest_point = (c_real, c_imag);
                    }
                    if let Some(distance) = distance
                        && distance < cell_rad.mul2()
                    {
                        to_resample.push((c_real, c_imag));
                    }
                    painter.circle_filled(
                        secondary_camera_map.complex_to_pos((c_real, c_imag)),
                        dynamic_draw_size(secondary_camera_map, 5.0),
                        Color32::DARK_RED,
                    );
                }

                if self.draw_sample_subgrid {
                    // TODO: try sorting the vec by distance estimate
                    // draw points that got resampled
                    for (c0_real, c0_imag) in to_resample {
                        let resample_window = Window::from_mid_rad(c0_real, c0_imag, cell_rad, cell_rad).unwrap();
                        for (c_real, c_imag) in resample_window.grid_centers(sample::WIDTH1, sample::WIDTH1).flatten() {
                            if (c0_real, c0_imag) == (c_real, c_imag) {
                                continue;
                            }
                            let sample = sample::quadratic_map((z0_real, z0_imag), (c_real, c_imag));
                            if sample.depth > deepest {
                                deepest = sample.depth;
                                deepest_point = (c_real, c_imag);
                            }
                            painter.circle_filled(
                                secondary_camera_map.complex_to_pos((c_real, c_imag)),
                                dynamic_draw_size(secondary_camera_map, 3.0),
                                Color32::ORANGE,
                            );
                        }
                    }
                }

                // draw the deepest point
                painter.circle_filled(
                    secondary_camera_map.complex_to_pos(deepest_point),
                    // don't use dynamic size for this
                    5.0,
                    Color32::RED,
                );
            }

            // draw deepest_on_grid
            {
                let (deepest_c, _sample) = sample::deepest_on_grid(
                    (z0_real, z0_imag),
                    window,
                    sample::WIDTH,
                    sample::WIDTH,
                    sample::GRADIENT_STEPS,
                );
                painter.circle_filled(
                    secondary_camera_map.complex_to_pos(deepest_c),
                    // don't use dynamic size for this
                    5.0,
                    Color32::WHITE,
                );
            }
        }

        // draw distance estimator with gradient descent steps
        if self.draw_gradient_steps {
            (|| {
                if self.current_fractal != CurrentFractal::Mandelbrot {
                    return Some(());
                }
                let Some(z0) = z0 else {
                    return Some(());
                };
                let Some(mut c) = secondary_camera_map.pos_to_complex(ctx.input(|i| i.pointer.latest_pos().unwrap_or_default()))
                else {
                    return Some(());
                };

                const MAX_STEPS: usize = 8;
                for _ in 0..MAX_STEPS {
                    let (distance, (_grad_real, _grad_imag)) = sample::distance_estimator_gradient(z0, c)?;
                    draw_complex_circle_stroke(c, distance, egui::Stroke::new(1.0, Color32::WHITE));
                    // let scale = distance / ((grad_real * grad_real + grad_imag * grad_imag).into_f32()).sqrt();
                    // draw_complex_segment(
                    //     c,
                    //     (c_real - grad_real * scale, c_imag - grad_imag * scale),
                    //     egui::Stroke::new(1.0, Color32::RED),
                    // );
                    let next_c = sample::gradient_step(z0, c)?;
                    draw_complex_segment(c, next_c, egui::Stroke::new(1.0, Color32::WHITE));
                    painter.circle_filled(
                        secondary_camera_map.complex_to_pos(next_c),
                        // don't use dynamic size for this
                        3.0,
                        Color32::WHITE,
                    );
                    c = next_c;
                }

                Some(())
            })();
        }

        // draw a dot at the center of the screen or at z0
        'draw_z0: {
            if !self.draw_crosshair {
                break 'draw_z0;
            }
            painter.circle_filled(
                match self.current_fractal {
                    CurrentFractal::Metabrot => screen_center,
                    CurrentFractal::Mandelbrot => {
                        if let Some(z0) = z0 {
                            secondary_camera_map.complex_to_pos(z0)
                        } else {
                            break 'draw_z0;
                        }
                    }
                },
                3.0,
                Color32::BLUE,
            );
        }
    }

    /// factor this out,
    /// bc rustfmt dies on deeply nested code with string literals.
    /// it's fixed by increasing max_width, but that's really coarse.
    fn show_ui(&mut self, ctx: &eframe::egui::Context, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("info")
            .default_open(true)
            .show_unindented(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                egui::CollapsingHeader::new("stats").default_open(true).show(ui, |ui| {
                    // frame rate
                    {
                        let average_dt = self.dts.average().expect("we added one this frame so dts must be non-empty");
                        ui.label(format!("fps: {:.01}", 1.0 / average_dt));
                        ui.label(format!("spf: {:.05}", average_dt));
                    }

                    // sample count
                    {
                        ui.label(format!(
                            "samples/sec: {:.01}",
                            self.sample_counts.values().sum::<u64>() as f32 / self.sample_counts.len() as f32
                        ));
                    }

                    // node count
                    {
                        // note that this leaks memory
                        ui.label(format!(
                            "node count: {}",
                            self.metabrot.tree().node_count(&mut ThreadData::default())
                        ));
                    }

                    egui::CollapsingHeader::new("camera").show(ui, |ui| {
                        ui.label(format!("metabrot real mid: {:12.09}", self.primary_camera.real_mid()));
                        ui.label(format!("metabrot imag mid: {:12.09}", self.primary_camera.imag_mid()));
                        ui.label(format!("metabrot real rad: {:12.09}", self.primary_camera.real_rad()));
                        ui.label(format!("mandelbrot real mid: {:12.09}", self.secondary_camera.real_mid()));
                        ui.label(format!("mandelbrot imag mid: {:12.09}", self.secondary_camera.imag_mid()));
                        ui.label(format!("mandelbrot real rad: {:12.09}", self.secondary_camera.real_rad()));
                    });

                    egui::CollapsingHeader::new("timers").default_open(true).show(ui, |ui| {
                        // fn show_timer(ui: &mut egui::Ui,  timer: &fractal::MultiTimer) {
                        //     // TODO: be better
                        // // note that for a flamegraph the average should be with respect to the total count, not each count
                        //     ui.horizontal(|ui|{
                        //         ui.label(format!("time per draw: {}", timer.draw.average().unwrap_or_default().as_secs_f32()));
                        //     });
                        // }
                        // for timer in self.metabrot.timers(){
                        //     show_timer(ui, &timer);
                        // }

                        let sum_timer = self.metabrot.timers().into_iter().reduce(|a, b| a + b).unwrap_or_default();
                        self.timers.add(ctx.input(|i| i.time), sum_timer);
                        self.metabrot.reset_timers();
                        let sum_timer = self.timers.values().reduce(|a, b| a + b).unwrap_or_default();

                        ui.label(format!(
                            "us per draw: {:.03}",
                            sum_timer.draw.average().unwrap_or_default().as_nanos() as f64 / 1000.0
                        ));
                        ui.label(format!(
                            "us per sample: {:.03}",
                            sum_timer.sample.average().unwrap_or_default().as_nanos() as f64 / 1000.0
                        ));
                        ui.label(format!(
                            "us per split: {:.03}",
                            sum_timer.split.average().unwrap_or_default().as_nanos() as f64 / 1000.0
                        ));
                        ui.label(format!(
                            "us per idle: {:.03}",
                            sum_timer.idle.average().unwrap_or_default().as_nanos() as f64 / 1000.0
                        ));

                        let sum_elapsed = sum_timer.draw.elapsed.as_secs_f64()
                            + sum_timer.sample.elapsed.as_secs_f64()
                            + sum_timer.split.elapsed.as_secs_f64()
                            + sum_timer.idle.elapsed.as_secs_f64();
                        ui.label(format!(
                            "draw portion: {:.03}",
                            sum_timer.draw.elapsed.as_secs_f64() / sum_elapsed
                        ));
                        ui.label(format!(
                            "sample portion: {:.03}",
                            sum_timer.sample.elapsed.as_secs_f64() / sum_elapsed
                        ));
                        ui.label(format!(
                            "split portion: {:.03}",
                            sum_timer.split.elapsed.as_secs_f64() / sum_elapsed
                        ));
                        ui.label(format!(
                            "idle portion: {:.03}",
                            sum_timer.idle.elapsed.as_secs_f64() / sum_elapsed
                        ));
                    });
                });

                egui::CollapsingHeader::new("toggles").default_open(true).show(ui, |ui| {
                    // sampling
                    {
                        ui.checkbox(&mut self.sampling, "metabrot sampling").on_hover_text(
                            "keybinding: ".to_owned()
                                + &ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::X)),
                        );
                        self.sampling ^= ctx.input(|i| i.key_pressed(Key::X));
                    }

                    // crosshair
                    {
                        ui.checkbox(&mut self.draw_crosshair, "draw crosshair")
                            .on_hover_text("for metabrot, draw a dot at the screen center. for mandelbrot, draw a dot at z0.");
                    }

                    // current fractal
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new("metabrot").selected(self.current_fractal == CurrentFractal::Metabrot))
                            .on_hover_text(
                                "whether to render the metabrot (rather than the mandelbrot). keybinding: ".to_owned()
                                    + &ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, Key::N)),
                            )
                            .clicked()
                        {
                            self.current_fractal = CurrentFractal::Metabrot;
                        }
                        if ctx.input(|i| i.key_pressed(Key::N)) {
                            self.current_fractal = CurrentFractal::Metabrot;
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
                        if ctx.input(|i| i.key_pressed(Key::M)) {
                            self.current_fractal = CurrentFractal::Mandelbrot;
                        }
                    });

                    // control other camera
                    {
                        ui.checkbox(&mut self.control_other_camera, "control other fractal")
                            .on_hover_text(
                                "pan/zoom will affect the other fractal's camera. keybinding: ".to_owned()
                                    + &ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::NONE, Key::C)),
                            );
                        self.control_other_camera ^= ctx.input(|i| i.key_pressed(Key::C));
                    }

                    egui::CollapsingHeader::new("mandelbrot").show(ui, |ui| {
                        ui.checkbox(&mut self.draw_gradient_steps, "draw gradient steps")
                            .on_hover_text("draw the gradient steps starting from the mouse position");
                        ui.checkbox(&mut self.draw_sample_grid, "draw sample grid");
                        ui.checkbox(&mut self.draw_sample_subgrid, "draw sample subgrid")
                            .on_hover_text("requires draw sample grid to have an effect.");
                        ui.label("draw sample grid gradient steps:")
                            .on_hover_text("-1 to disable, 0 is the same as draw_sample_grid. keybinding: left and right arrows");
                        ui.add(
                            egui::Slider::new(&mut self.draw_sample_grid_gradient_steps, -1..=8)
                                .clamping(egui::SliderClamping::Never),
                        );
                        self.draw_sample_grid_gradient_steps +=
                            ctx.input(|i| i.key_pressed(Key::ArrowRight) as i32 - i.key_pressed(Key::ArrowLeft) as i32);
                        self.draw_sample_grid_gradient_steps = self.draw_sample_grid_gradient_steps.max(-1);
                    });
                });

                // give some margin on the bottom and right,
                // but only when uncollapsed,
                // so we can't use frame inner margin.
                ui.allocate_space(Vec2::new(ui.min_size().x + 3.0, 3.0));
            });
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx, |ui| {
            self.dts.add(ctx.input(|i| i.time), ctx.input(|i| i.stable_dt));

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
                    println!("pool average / tc: {} ns", nanos / self.pool.thread_count() as u64);
                }
                if let Some(max_i) = pool::WORKER_HIST
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(i, count)| if count.load(Ordering::Relaxed) > 0 { Some(i) } else { None })
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
            let needs_full_redraw = {
                let before = (self.primary_camera, self.secondary_camera);
                if self.control_other_camera != (self.current_fractal == CurrentFractal::Metabrot) {
                    CameraMap::pan_zoom(ctx, ui, &mut self.primary_camera, &mut self.primary_camera_velocity)
                } else {
                    CameraMap::pan_zoom(ctx, ui, &mut self.secondary_camera, &mut self.secondary_camera_velocity)
                };
                let after = (self.primary_camera, self.secondary_camera);
                before != after
            };

            let primary_camera_map = CameraMap::new(ui.max_rect(), self.primary_camera, self.stride);
            let secondary_camera_map = CameraMap::new(ui.max_rect(), self.secondary_camera, self.stride);

            // sampling
            if self.sampling {
                let samples_taken = self
                    .metabrot
                    .enable_sampling(primary_camera_map.window().unwrap_or(Domain::default().into()));
                self.sample_counts.add(ctx.input(|i| i.time), samples_taken);
            } else {
                self.metabrot.disable_sampling();
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

            self.show_fractal(ctx, ui, &primary_camera_map, &secondary_camera_map, needs_full_redraw);

            // area is to allow the frame to be drawn on top of the fractal
            egui::Area::new(egui::Id::new("area"))
                .constrain_to(ctx.screen_rect())
                .anchor(egui::Align2::LEFT_TOP, egui::Vec2::ZERO)
                .show(ui.ctx(), |ui| {
                    // frame is for background
                    egui::Frame::new()
                        .fill(Color32::from_gray(20))
                        .inner_margin(3)
                        .show(ui, |ui| {
                            self.show_ui(ctx, ui);
                        });
                });
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.metabrot.join();
    }
}
