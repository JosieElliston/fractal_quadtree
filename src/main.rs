mod camera;
mod fixed;
mod pool;
mod sample;
mod tree;

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Key, Pos2, Rect, RichText, Vec2};
use mimalloc::MiMalloc;
use rayon::prelude::*;

use crate::{
    camera::{Camera, CameraMap, Square, Window},
    fixed::*,
    pool::Pool,
    sample::*,
    tree::Tree,
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> eframe::Result {
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "1");
    }
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

fn lerp(lo: f64, hi: f64, t: f64) -> f64 {
    assert!(lo < hi);
    // assert!((0.0..=1.0).contains(&t));
    // lo * (1.0 - t) + hi * t
    lo + (hi - lo) * t
}

fn inv_lerp(lo: f64, hi: f64, x: f64) -> f64 {
    assert!(lo < hi);
    // assert!((lo..=hi).contains(&x));
    (x - lo) / (hi - lo)
}

// fn bench() {
//     let start = Instant::now();
//     let mut tree = Tree::new(Square::try_new(-4.0, 4.0, -4.0, 4.0).unwrap());
//     let stride = 8;
//     let camera = Camera::new(0.0, 0.0, 2.0);
//     let camera_map = CameraMap::new(
//         Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0)),
//         camera,
//     );
//     for (_, _, pixel) in camera_map.pixels(stride) {
//         tree.ensure_pixel_safe(pixel);
//     }
//     for _ in 0..600 {
//         for (_, _, pixel) in camera_map.pixels(stride) {
//             black_box(tree.color_in_pixel(pixel));
//         }
//     }
//     black_box(tree);
//     println!("time: {:?}", start.elapsed());
// }

struct App {
    tree: Tree,
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
    pool: Pool,
    // in_flight_target: usize,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // const N_THREADS: usize = 8;
        const N_THREADS: usize = 32;
        Self {
            tree: Tree::new(
                Square::new_exact((-4.0).into(), 4.0.into(), (-4.0).into(), 4.0.into()).unwrap(),
            ),
            stride: 1,
            // stride: 8,
            primary_camera: Camera::new(0.0.into(), 0.0.into(), 2.0.into()),
            primary_camera_velocity: Vec2::ZERO,
            secondary_camera: Camera::new(0.0.into(), 0.0.into(), 2.0.into()),
            secondary_camera_velocity: Vec2::ZERO,
            dts: egui::util::History::new(1..100, 0.1),
            sample_counts: egui::util::History::new(1..100, 1.0),
            texture: cc.egui_ctx.load_texture(
                "fractal",
                egui::ColorImage::example(),
                egui::TextureOptions::NEAREST,
            ),
            sampling: true,
            current_fractal: 0,
            pool: Pool::new(N_THREADS),
            // in_flight_target: 2 * N_THREADS,
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

                self.sampling ^= ctx.input(|i| i.key_pressed(Key::Space));

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

                // panning stuff
                fn pan(
                    ui: &mut egui::Ui,
                    ctx: &egui::Context,
                    camera: &mut Camera,
                    velocity: &mut Vec2,
                ) {
                    let rect = ui.available_rect_before_wrap();
                    let r = ui.allocate_rect(rect, egui::Sense::click_and_drag());

                    let pan_offset = |pan_vec: Vec2, real_rad: Real| -> (Real, Imag) {
                        (
                            (-2.0 * pan_vec.x / rect.size().x * f32::from(real_rad)).into(),
                            (2.0 * pan_vec.y * (f32::from(real_rad) / rect.size().x)).into(),
                        )
                    };

                    let dt = ctx.input(|input_state| input_state.stable_dt);
                    if r.is_pointer_button_down_on() && ctx.input(|i| i.pointer.primary_down()) {
                        *camera += pan_offset(r.drag_delta(), camera.real_rad());
                        *velocity = r.drag_delta() / dt;
                    } else {
                        const VELOCITY_DAMPING: f32 = 0.9999;
                        *camera += pan_offset(*velocity * dt, camera.real_rad());
                        *velocity *= (1.0 - VELOCITY_DAMPING).powf(dt);
                    }
                    if velocity.length_sq() < 0.0001 {
                        *velocity = Vec2::ZERO;
                    }
                    if r.contains_pointer()
                        && let Some(mouse_pos) = ctx.input(|i| i.pointer.latest_pos())
                    {
                        // TODO: factor into camera
                        let mouse = mouse_pos - rect.center();
                        let zoom = ctx.input(|i| (i.smooth_scroll_delta.y / 300.0).exp());
                        *camera += pan_offset(-mouse, camera.real_rad());
                        *camera.real_rad_mut() = camera.real_rad().div_f32(zoom);
                        *camera += pan_offset(mouse, camera.real_rad());
                    }
                }
                // move other camera when holding backtick
                if ctx.input(|i| i.key_down(Key::Backtick)) != (self.current_fractal == 0) {
                    pan(
                        ui,
                        ctx,
                        &mut self.primary_camera,
                        &mut self.primary_camera_velocity,
                    );
                } else {
                    pan(
                        ui,
                        ctx,
                        &mut self.secondary_camera,
                        &mut self.secondary_camera_velocity,
                    );
                }

                // let camera_map = if self.current_fractal == 0 {
                //     CameraMap::new(ui.max_rect(), self.primary_camera)
                // } else {
                //     CameraMap::new(ui.max_rect(), self.secondary_camera)
                // };
                let primary_camera_map = CameraMap::new(ui.max_rect(), self.primary_camera);
                let secondary_camera_map = CameraMap::new(ui.max_rect(), self.secondary_camera);

                // sampling with pool
                if self.sampling {
                    // if self.pool.in_flight() == 0 {
                    //     println!("pool has nothing in flight");
                    // }

                    // take samples out of the pool
                    let mut sample_count = 0;
                    while let Some(((real, imag), color)) = self.pool.recv() {
                        self.tree.insert((real, imag), color);
                        sample_count += 1;
                    }
                    self.sample_counts.add(ctx.input(|i| i.time), sample_count);

                    // they're always unsaturated, ~bc the main thread is also a thread
                    // println!(
                    //     "in_flight: {}, in_flight_target: {}, thread_count: {}",
                    //     self.pool.in_flight(),
                    //     self.in_flight_target,
                    //     self.pool.thread_count()
                    // );
                    // if self.pool.in_flight() < self.pool.thread_count() {
                    //     println!(
                    //         "threads are unsaturated: in_flight: {}, thread_count: {}",
                    //         self.pool.in_flight(),
                    //         self.pool.thread_count()
                    //     );
                    //     // self.in_flight_target *= 2;
                    // }
                    // if self.pool.in_flight() > 2 * self.pool.thread_count() {
                    //     println!(
                    //         "threads are oversaturated: in_flight: {}, thread_count: {}",
                    //         self.pool.in_flight(),
                    //         self.pool.thread_count()
                    //     );
                    //     self.in_flight_target /= 2;
                    // }

                    // request samples
                    // TODO: various canceling stuff
                    // while self.pool.in_flight() < self.in_flight_target {
                    while self.pool.in_flight() < 256 {
                        let Some(points) = self.tree.refine(primary_camera_map.window()) else {
                            break;
                        };
                        for (real, imag) in points {
                            self.pool.send((real, imag));
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

                // draw the fractal
                // #[cfg(false)]
                {
                    let screen_center = ui.max_rect().center();
                    let z0 = primary_camera_map.pos_to_complex(screen_center);
                    let colors = if self.current_fractal == 0 {
                        primary_camera_map
                            .pixels(self.stride)
                            .collect::<Vec<_>>()
                            .into_par_iter()
                            .map(|(_, _rect, pixel)| self.tree.color_of_pixel(pixel))
                            .collect::<Vec<_>>()
                    } else {
                        secondary_camera_map
                            .pixels(self.stride)
                            .collect::<Vec<_>>()
                            .into_par_iter()
                            .map(|(_, _rect, pixel)| {
                                let c = pixel.mid();
                                quadratic_map(z0, c).color()
                            })
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(primary_camera_map.rect(), secondary_camera_map.rect());
                    self.texture.set(
                        egui::ColorImage::new(
                            [
                                primary_camera_map.rect().size().x as usize / self.stride,
                                primary_camera_map.rect().size().y as usize / self.stride,
                            ],
                            colors,
                        ),
                        egui::TextureOptions::NEAREST,
                    );
                    let painter = ui.painter_at(ui.max_rect());
                    painter.image(
                        self.texture.id(),
                        primary_camera_map.rect(),
                        Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );

                    let draw_complex_circle =
                        |(c_real, c_imag): (Real, Imag), rad: Fixed, stroke: egui::Stroke| {
                            painter.circle_stroke(
                                secondary_camera_map.complex_to_pos((c_real, c_imag)),
                                secondary_camera_map.real_to_x(rad)
                                    - secondary_camera_map.real_to_x(0.0.into()),
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
                    // let draw_distance_estimator = |(c_real, c_imag)| {
                    //     // TODO: get derivative and draw line towards the estimated fractal location

                    //     let Some((distance, (grad_real, grad_imag))) =
                    //         distance_estimator_gradient(z0, (c_real, c_imag))
                    //     else {
                    //         return;
                    //     };
                    //     painter.circle_stroke(
                    //         secondary_camera_map.complex_to_pos((c_real, c_imag)),
                    //         secondary_camera_map.real_to_x(distance)
                    //             - secondary_camera_map.real_to_x(0.0.into()),
                    //         egui::Stroke::new(2.0, Color32::RED),
                    //     );
                    //     painter.line_segment(
                    //         [
                    //             secondary_camera_map.complex_to_pos((c_real, c_imag)),
                    //             secondary_camera_map
                    //                 .complex_to_pos((c_real - grad_real, c_imag - grad_imag)),
                    //         ],
                    //         egui::Stroke::new(2.0, Color32::RED),
                    //     );
                    // };

                    // draw stuff that uses the window we're sampling the mandelbrot in
                    if self.current_fractal != 0 {
                        // let window = Window::from_center_size(0.0.into(), 0.0.into(), 4.0.into(), 4.0.into());
                        let (z0_real, z0_imag) = z0;
                        let window = Window::from_center_size(
                            (f64::from(z0_imag) * f64::from(z0_imag)
                                - f64::from(z0_real) * f64::from(z0_real))
                            .into(),
                            (-2.0 * f64::from(z0_real) * f64::from(z0_imag)).into(),
                            4.0.into(),
                            4.0.into(),
                        );

                        // draw the outline of the window
                        painter.rect_stroke(
                            secondary_camera_map.window_to_rect(window),
                            0.0,
                            egui::Stroke::new(2.0, Color32::WHITE),
                            egui::StrokeKind::Middle,
                        );

                        // draw dots where we took samples, to debug aliasing
                        #[cfg(false)]
                        {
                            // how many times should we iterate?
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
                            for line in window.grid(sample::WIDTH0, sample::WIDTH0) {
                                for (c_real, c_imag) in line {
                                    // painter.circle_filled(
                                    //     secondary_camera_map.complex_to_pos((c_real, c_imag)),
                                    //     1.0,
                                    //     Color32::WHITE,
                                    // );
                                    // the image of the sample under a few gradient descent steps
                                    if let Some(stepped_c) = (|| {
                                        let (mut c_real, mut c_imag) = (c_real, c_imag);
                                        for step in 0..gradient_steps {
                                            (c_real, c_imag) = gradient_step(z0, (c_real, c_imag))?;
                                        }
                                        Some((c_real, c_imag))
                                    })(
                                    ) {
                                        painter.circle_filled(
                                            secondary_camera_map.complex_to_pos(stepped_c),
                                            1.0,
                                            Color32::WHITE,
                                        );
                                    }
                                    // draw_distance_estimator((c_real, c_imag));
                                }
                            }
                        }

                        // // draw deepest_on_grid
                        // let (deepest_c, _sample) = deepest_on_grid((z0_real, z0_imag), window);
                        // painter.circle_filled(
                        //     secondary_camera_map.complex_to_pos(deepest_c),
                        //     5.0,
                        //     Color32::RED,
                        // );

                        // draw points that got resampled in a small window around them
                        // TODO: less code reuse
                        {
                            let mut deepest: f32 = 0.0;
                            let mut deepest_point = (0.0.into(), 0.0.into());
                            // we want to look through all the points at a coarse grain before resampling
                            let mut to_resample = Vec::with_capacity(WIDTH0 * WIDTH0);
                            let cell_diameter = {
                                (window.real_rad().mul2().div_f64(WIDTH0 as f64))
                                    .max(window.imag_rad().mul2().div_f64(WIDTH0 as f64))
                            };
                            // draw initial points
                            for (c_real, c_imag) in window.grid(WIDTH0, WIDTH0).flatten() {
                                let (sample, distance) =
                                    distance_estimator((z0_real, z0_imag), (c_real, c_imag));
                                if sample.depth > deepest {
                                    deepest = sample.depth;
                                    deepest_point = (c_real, c_imag);
                                }
                                if let Some(distance) = distance
                                    && distance < cell_diameter
                                {
                                    to_resample.push((c_real, c_imag));
                                }
                                painter.circle_filled(
                                    secondary_camera_map.complex_to_pos((c_real, c_imag)),
                                    2.0,
                                    Color32::YELLOW,
                                );
                            }
                            // TODO: try sorting the vec by distance estimate
                            // draw points that got resampled
                            for (c0_real, c0_imag) in to_resample {
                                let resample_window = Window::from_center_size(
                                    c0_real,
                                    c0_imag,
                                    cell_diameter,
                                    cell_diameter,
                                );
                                for (c_real, c_imag) in
                                    resample_window.grid(WIDTH1, WIDTH1).flatten()
                                {
                                    if (c0_real, c0_imag) == (c_real, c_imag) {
                                        continue;
                                    }
                                    let sample =
                                        quadratic_map((z0_real, z0_imag), (c_real, c_imag));
                                    if sample.depth > deepest {
                                        deepest = sample.depth;
                                        deepest_point = (c_real, c_imag);
                                    }
                                    painter.circle_filled(
                                        secondary_camera_map.complex_to_pos((c_real, c_imag)),
                                        2.0,
                                        Color32::ORANGE,
                                    );
                                }
                            }
                            // draw the deepest point
                            painter.circle_filled(
                                secondary_camera_map.complex_to_pos(deepest_point),
                                5.0,
                                Color32::RED,
                            );
                        }
                    }

                    // // draw distance estimator
                    // if self.current_fractal != 0 {
                    //     draw_distance_estimator(secondary_camera_map.pos_to_complex(
                    //         ctx.input(|i| i.pointer.latest_pos().unwrap_or_default()),
                    //     ));
                    // }
                    // draw distance estimator with gradient descent steps
                    (|| -> Option<()> {
                        if self.current_fractal != 0 {
                            let mut c = secondary_camera_map.pos_to_complex(
                                ctx.input(|i| i.pointer.latest_pos().unwrap_or_default()),
                            );
                            for _ in 0..4 {
                                let (distance, (grad_real, grad_imag)) =
                                    distance_estimator_gradient(z0, c)?;
                                draw_complex_circle(
                                    c,
                                    distance,
                                    egui::Stroke::new(1.0, Color32::RED),
                                );
                                // let scale = distance / ((grad_real * grad_real + grad_imag * grad_imag).into_f32()).sqrt();
                                // draw_complex_segment(
                                //     c,
                                //     (c_real - grad_real * scale, c_imag - grad_imag * scale),
                                //     egui::Stroke::new(1.0, Color32::RED),
                                // );
                                let next_c = gradient_step(z0, c)?;
                                draw_complex_segment(
                                    c,
                                    next_c,
                                    egui::Stroke::new(1.0, Color32::RED),
                                );
                                painter.circle_filled(
                                    secondary_camera_map.complex_to_pos(next_c),
                                    3.0,
                                    Color32::RED,
                                );
                                c = next_c;
                            }
                        }
                        Some(())
                    })();

                    // draw a dot at the center of the screen or at z0
                    painter.circle_filled(
                        if self.current_fractal == 0 {
                            screen_center
                        } else {
                            secondary_camera_map.complex_to_pos((0.0.into(), 0.0.into()))
                        },
                        3.0,
                        Color32::WHITE,
                    );
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
                                self.tree.node_count(),
                            );
                            ui.label(RichText::new(t).background_color(Color32::BLACK));
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
