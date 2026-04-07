use eframe::egui::{self, Color32, Key, Pos2, Rect, Vec2};
use rayon::prelude::*;

use crate::{
    complex::{Camera, CameraMap, Domain, Window, fixed::*},
    fractal::{self, Fractal},
    pool::{self, Pool},
    sample,
    tree::{self, Tree},
};

/// fancy dynamic radius based on zoom
/// so that if you're zoomed out, points don't cover everything
fn dynamic_draw_size(camera_map: &CameraMap, max_rad: f32) -> f32 {
    const BASE_RAD: Fixed = Fixed::try_from_f64(0.001).unwrap();
    let rad = BASE_RAD.mul_f64(max_rad as f64);
    (camera_map.delta_real_to_vec1(rad)).min(max_rad)
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
    sample_counts: egui::util::History<usize>,
    texture: egui::TextureHandle,
    sampling: bool,
    current_fractal: usize,
    // pool: Pool,
    // in_flight_target: usize,
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
            dts: egui::util::History::new(1..100, 1.0),
            sample_counts: egui::util::History::new(1..100, 1.0),
            texture: cc.egui_ctx.load_texture(
                "fractal",
                egui::ColorImage::example(),
                egui::TextureOptions::NEAREST,
            ),
            sampling: true,
            current_fractal: 0,
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
                    ctx.input(|i| i.time),
                    ctx.input(|i| i.stable_dt),
                );

                // toggle sampling with space
                self.sampling ^= ctx.input(|i| i.key_pressed(Key::Space));

                // switch the current fractal with number keys
                if let Some(key) = [
                    Key::Num1,
                    Key::Num2,
                    Key::Num3,
                    Key::Num4,
                    Key::Num5,
                    Key::Num6,
                    Key::Num7,
                    Key::Num8,
                    Key::Num9,
                    Key::Num0,
                ]
                .into_iter()
                .position(|k| ctx.input(|i| i.key_pressed(k)))
                {
                    self.current_fractal = key;
                }

                // debug static counters
                #[cfg(false)]
                {
                    // println!();
                    if let Some(nanos) = tree::ELAPSED_NANOS
                        .load(std::sync::atomic::Ordering::Relaxed)
                        .checked_div(tree::COUNTER.load(std::sync::atomic::Ordering::Relaxed))
                    {
                        println!("tree average time: {} ns", nanos);
                    }
                    if let Some(nanos) = pool::ELAPSED_NANOS
                        .load(std::sync::atomic::Ordering::Relaxed)
                        .checked_div(pool::COUNTER.load(std::sync::atomic::Ordering::Relaxed))
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
                                if count.load(std::sync::atomic::Ordering::Relaxed) > 0 {
                                    Some(i)
                                } else {
                                    None
                                }
                            })
                    {
                        println!("worker hist:");
                        for (i, worker) in pool::WORKER_HIST.iter().enumerate().take(max_i + 1) {
                            println!(
                                "worker {}: {} samples",
                                i,
                                worker.load(std::sync::atomic::Ordering::Relaxed)
                            );
                        }
                    }
                }

                // self.tree.validate_leaf_distance();

                // panning stuff
                // pan other camera when holding backtick
                if ctx.input(|i| i.key_down(Key::Backtick)) != (self.current_fractal == 0) {
                    CameraMap::pan_zoom(
                        ctx,
                        ui,
                        &mut self.primary_camera,
                        &mut self.primary_camera_velocity,
                    );
                } else {
                    CameraMap::pan_zoom(
                        ctx,
                        ui,
                        &mut self.secondary_camera,
                        &mut self.secondary_camera_velocity,
                    );
                }

                let primary_camera_map =
                    CameraMap::new(ui.max_rect(), self.primary_camera, self.stride);
                let secondary_camera_map =
                    CameraMap::new(ui.max_rect(), self.secondary_camera, self.stride);

                // if self.current_fractal == 0 {
                //     self.metabrot.begin_rendering(&primary_camera_map);
                // }

                // sampling
                if self.sampling {
                    let samples_taken = self.metabrot.enable_sampling(
                        primary_camera_map
                            .window()
                            .unwrap_or(Domain::default().into()),
                    );
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

                // draw the fractal and debug stuff
                // #[cfg(false)]
                {
                    let screen_center = ui.max_rect().center();
                    let z0 = primary_camera_map.pos_to_complex(screen_center);
                    let painter = ui.painter_at(ui.max_rect());

                    // draw the fractal
                    {
                        if self.current_fractal == 0 {
                            self.metabrot.begin_rendering(&primary_camera_map);
                            self.metabrot.finish_rendering(&mut self.texture);
                        } else if let Some(z0) = z0 {
                            fractal::render_mandelbrot(
                                &mut self.texture,
                                &secondary_camera_map,
                                z0,
                            );
                        } else {
                            fractal::render_color(&mut self.texture, &secondary_camera_map);
                        }
                        assert_eq!(primary_camera_map.rect(), secondary_camera_map.rect());
                        painter.image(
                            self.texture.id(),
                            primary_camera_map.rect(),
                            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }

                    let draw_complex_circle_stroke =
                        |(c_real, c_imag): (Real, Imag), rad: Fixed, stroke: egui::Stroke| {
                            painter.circle_stroke(
                                secondary_camera_map.complex_to_pos((c_real, c_imag)),
                                secondary_camera_map.delta_real_to_vec1(rad),
                                stroke,
                            );
                        };
                    let draw_complex_segment =
                        |(c1_real, c1_imag): (Real, Imag),
                         (c2_real, c2_imag): (Real, Imag),
                         stroke: egui::Stroke| {
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
                        if self.current_fractal == 0 {
                            break 'mandelbrot_window;
                        }
                        let Some((z0_real, z0_imag)) = z0 else {
                            break 'mandelbrot_window;
                        };
                        // let window = Window::from_center_size(Fixed::ZERO, Fixed::ZERO, 4.0.into(), 4.0.into());
                        let Some(window) = (|| {
                            Window::from_mid_rad(
                                (f64::from(z0_imag) * f64::from(z0_imag)
                                    - f64::from(z0_real) * f64::from(z0_real))
                                .try_into()
                                .ok()?,
                                (-2.0 * f64::from(z0_real) * f64::from(z0_imag))
                                    .try_into()
                                    .ok()?,
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
                            let Some(z0) = z0 else {
                                break 'gradient_descent_steps;
                            };
                            // how many times should we iterate?
                            // control with arrow keys
                            let gradient_steps = {
                                let delta: isize = ctx.input(|i| i.key_pressed(Key::ArrowRight))
                                    as isize
                                    - ctx.input(|i| i.key_pressed(Key::ArrowLeft)) as isize;
                                ctx.data_mut(|map| {
                                    let id = egui::Id::new("gradient_steps");
                                    let gradient_steps = map.get_temp_mut_or::<usize>(id, 0);
                                    *gradient_steps = gradient_steps.saturating_add_signed(delta);
                                    *gradient_steps
                                })
                            };
                            for line in window.grid_centers(sample::WIDTH0, sample::WIDTH0) {
                                for (c_real, c_imag) in line {
                                    // painter.circle_filled(
                                    //     secondary_camera_map.complex_to_pos((c_real, c_imag)),
                                    //     1.0,
                                    //     Color32::WHITE,
                                    // );
                                    // the image of the sample under a few gradient descent steps
                                    if let Some(stepped_c) = (|| {
                                        let (mut c_real, mut c_imag) = (c_real, c_imag);
                                        for _ in 0..gradient_steps {
                                            (c_real, c_imag) =
                                                sample::gradient_step(z0, (c_real, c_imag))?;
                                        }
                                        Some((c_real, c_imag))
                                    })(
                                    ) {
                                        painter.circle_filled(
                                            secondary_camera_map.complex_to_pos(stepped_c),
                                            dynamic_draw_size(&secondary_camera_map, 3.0),
                                            Color32::WHITE,
                                        );
                                    }
                                    // draw_distance_estimator((c_real, c_imag));
                                }
                            }
                        }

                        // debug hierarchical windows
                        // draw points that got resampled in a small window around them
                        // TODO: less code reuse
                        {
                            let mut deepest: f32 = 0.0;
                            let mut deepest_point = (Fixed::ZERO, Fixed::ZERO);
                            // we want to look through all the points at a coarse grain before resampling
                            let mut to_resample =
                                Vec::with_capacity(sample::WIDTH0 * sample::WIDTH0);
                            let cell_rad = {
                                (window.real_rad().div_f64(sample::WIDTH0 as f64))
                                    .max(window.imag_rad().div_f64(sample::WIDTH0 as f64))
                            };
                            // draw initial points
                            for (c_real, c_imag) in window
                                .grid_centers(sample::WIDTH0, sample::WIDTH0)
                                .flatten()
                            {
                                let (sample, distance) = sample::distance_estimator(
                                    (z0_real, z0_imag),
                                    (c_real, c_imag),
                                );
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
                                    dynamic_draw_size(&secondary_camera_map, 5.0),
                                    Color32::DARK_RED,
                                );
                            }
                            // TODO: try sorting the vec by distance estimate
                            // draw points that got resampled
                            for (c0_real, c0_imag) in to_resample {
                                let resample_window =
                                    Window::from_mid_rad(c0_real, c0_imag, cell_rad, cell_rad)
                                        .unwrap();
                                for (c_real, c_imag) in resample_window
                                    .grid_centers(sample::WIDTH1, sample::WIDTH1)
                                    .flatten()
                                {
                                    if (c0_real, c0_imag) == (c_real, c_imag) {
                                        continue;
                                    }
                                    let sample =
                                        sample::quadratic_map((z0_real, z0_imag), (c_real, c_imag));
                                    if sample.depth > deepest {
                                        deepest = sample.depth;
                                        deepest_point = (c_real, c_imag);
                                    }
                                    painter.circle_filled(
                                        secondary_camera_map.complex_to_pos((c_real, c_imag)),
                                        dynamic_draw_size(&secondary_camera_map, 3.0),
                                        Color32::ORANGE,
                                    );
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
                    (|| {
                        if self.current_fractal == 0 {
                            return Some(());
                        }
                        let Some(z0) = z0 else {
                            return Some(());
                        };
                        let Some(mut c) = secondary_camera_map.pos_to_complex(
                            ctx.input(|i| i.pointer.latest_pos().unwrap_or_default()),
                        ) else {
                            return Some(());
                        };

                        const MAX_STEPS: usize = 8;
                        for _ in 0..MAX_STEPS {
                            let (distance, (_grad_real, _grad_imag)) =
                                sample::distance_estimator_gradient(z0, c)?;
                            draw_complex_circle_stroke(
                                c,
                                distance,
                                egui::Stroke::new(1.0, Color32::WHITE),
                            );
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

                    // draw a dot at the center of the screen or at z0
                    'draw_z0: {
                        painter.circle_filled(
                            if self.current_fractal == 0 {
                                screen_center
                            } else if let Some(z0) = z0 {
                                secondary_camera_map.complex_to_pos(z0)
                            } else {
                                break 'draw_z0;
                            },
                            3.0,
                            Color32::BLUE,
                        );
                    }
                }

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
                            let t = format!(
                                "    dt: {:08.04}\n1/dt: {:08.04}\nsamples/s: {:08.04}\nnodes: {}",
                                average_dt,
                                1.0 / average_dt,
                                self.sample_counts.values().sum::<usize>() as f32
                                    / self.sample_counts.len() as f32,
                                self.metabrot.tree.node_count(),
                            );
                            ui.label(egui::RichText::new(t).background_color(Color32::BLACK));
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

    // fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
    //     self.metabrot.join();
    // }
}
