mod camera;
mod sample;
mod tree;

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use eframe::egui::{self, Color32, Key, Pos2, Rect, RichText, Vec2};
use mimalloc::MiMalloc;
use rand::seq::SliceRandom;
use rayon::prelude::*;

use crate::{
    camera::{Camera, CameraMap, Square},
    sample::metabrot_sample,
    tree::Tree,
};

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
    camera: Camera,
    velocity: Vec2,
    dts: egui::util::History<f32>,
    texture: egui::TextureHandle,
    sampling: bool,
    // pool: Vec<std::thread::JoinHandle<()>>,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        const N_THREADS: usize = 4;
        Self {
            tree: Tree::new(Square::try_new(-4.0, 4.0, -4.0, 4.0).unwrap()),
            // stride: 1,
            stride: 8,
            camera: Camera::new(0.0, 0.0, 2.0),
            velocity: Vec2::ZERO,
            dts: egui::util::History::new(1..100, 0.1),
            texture: cc.egui_ctx.load_texture(
                "fractal",
                egui::ColorImage::example(),
                egui::TextureOptions::NEAREST,
            ),
            sampling: true,
            // pool: (0..N_THREADS)
            //     .map(|_| {
            //         std::thread::spawn(|| {
            //             loop {
            //                 std::thread::sleep(Duration::from_secs(1));
            //             }
            //         })
            //     })
            //     .collect(),
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
                        self.camera += pan_offset(r.drag_delta(), self.camera.real_rad());
                        self.velocity = r.drag_delta() / dt;
                    } else {
                        const VELOCITY_DAMPING: f32 = 0.9999;
                        self.camera += pan_offset(self.velocity * dt, self.camera.real_rad());
                        self.velocity *= (1.0 - VELOCITY_DAMPING).powf(dt);
                    }
                    if self.velocity.length_sq() < 0.0001 {
                        self.velocity = Vec2::ZERO;
                    }
                    if r.contains_pointer()
                        && let Some(mouse_pos) = ctx.input(|i| i.pointer.latest_pos())
                    {
                        // TODO: factor into camera
                        let mouse = mouse_pos - rect.center();
                        let zoom = ctx.input(|i| (i.smooth_scroll_delta.y / 300.0).exp());
                        self.camera += pan_offset(-mouse, self.camera.real_rad());
                        *self.camera.real_rad_mut() /= zoom;
                        self.camera += pan_offset(mouse, self.camera.real_rad());
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
                #[cfg(false)]
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
                                .map(|(_, _, pixel)| pixel)
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

                if self.sampling {
                    const MAX_TIME: Duration = Duration::from_millis(100);
                    let start = Instant::now();
                    while start.elapsed() < MAX_TIME {
                        let Some(points) = self
                            .tree
                            .refine(camera_map.rect_to_window(camera_map.rect()))
                        else {
                            break;
                        };

                        let colors = points
                            .into_par_iter()
                            .map(|(real, imag)| metabrot_sample(real, imag).color())
                            .collect::<Vec<_>>();

                        for ((real, imag), color) in points.into_iter().zip(colors.into_iter()) {
                            self.tree.insert((real, imag), color);
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
                #[cfg(false)]
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
                                for (_, _, pixel) in camera_map.pixels(stride) {
                                    if !self.tree.contains_sample(pixel) {
                                        // +2 instead of +1 to fix a weird bug
                                        return stride_pow + 2;
                                    }
                                }
                            }
                            // idk why this need + 1
                            self.stride.ilog2() + 1
                        }
                    }();

                    // for stride_pow in (self.stride.ilog2()..=stride_pow_hi).rev() {
                    //     let stride = 1 << stride_pow;
                    //     for (_, rect, pixel) in camera_map.pixels(stride) {
                    //         if let Some(color) = self.tree.color_in_pixel(pixel).build() {
                    //             painter.rect_filled(rect, 0.0, color);
                    //         }
                    //     }
                    // }

                    // for stride_pow in [1] {
                    for stride_pow in (self.stride.ilog2()..=stride_pow_hi).rev() {
                        let stride = 1 << stride_pow;
                        assert_eq!(camera_map.rect().min, Pos2::ZERO);
                        // let pow_2_camera_map = CameraMap::new(
                        //     Rect {
                        //         min: Pos2::ZERO,
                        //         max: Pos2 {
                        //             x: (1 << (camera_map.rect.max.x as i32).ilog2()) as f32,
                        //             y: (1 << (camera_map.rect.max.y as i32).ilog2()) as f32,
                        //         },
                        //     },
                        //     camera_map.camera,
                        // );
                        // let colors = self.tree.color_in_pixels(
                        //     pow_2_camera_map.rect_to_window(pow_2_camera_map.rect),
                        //     (pow_2_camera_map.x_to_real(stride as f32)
                        //         - pow_2_camera_map.x_to_real(0.0))
                        //         / 2.0,
                        // );

                        let colors = self.tree.color_in_pixels(
                            camera_map.rect_to_window(camera_map.rect()),
                            (camera_map.x_to_real(stride as f32) - camera_map.x_to_real(0.0)) / 2.0,
                            &camera_map,
                            stride,
                        );
                        for ((row, col), rect, pixel) in camera_map.pixels(stride) {
                            // for ((row, col), rect, pixel) in pow_2_camera_map.pixels(stride) {
                            // if let Some(color) = colors[row][col].clone().build() {
                            //     painter.rect_filled(rect, 0.0, color);
                            // }
                            // {
                            //     let rad = pixel.rad();
                            //     let actual = (camera_map.x_to_real(stride as f32)
                            //         - camera_map.x_to_real(0.0))
                            //         / 2.0;
                            //     assert!((rad - actual).abs() < 1e-6);
                            // }
                            if let Some(color) = colors
                                .get(row)
                                .and_then(|line| line.get(col))
                                .and_then(|c| c.clone().build())
                            {
                                // println!("here");
                                // let Some(oracle) = self.tree.color_in_pixel(pixel).build() else {
                                //     panic!()
                                // };
                                // assert_eq!(oracle, color);
                                painter.rect_filled(rect, 0.0, color);
                            } else if let Some(color) = self.tree.color_in_pixel(pixel).build() {
                                // painter.rect_filled(rect, 0.0, color);
                                painter.rect_filled(rect, 0.0, Color32::GREEN);
                                // painter.rect_stroke(
                                //     rect,
                                //     0.0,
                                //     egui::Stroke::new(0.5, Color32::MAGENTA),
                                //     egui::StrokeKind::Middle,
                                // );
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

                // .pixels small square debugging
                {
                    let pixels = camera_map.pixels(self.stride).collect::<Vec<_>>();
                    let expected_len = camera_map.rect().size().x as usize
                        * camera_map.rect().size().y as usize
                        / (self.stride * self.stride);
                    assert!(pixels.len() <= expected_len);
                    if pixels.len() < expected_len {
                        let painter = ui.painter_at(ui.max_rect());
                        for ((row, col), rect, pixel) in &pixels {
                            painter.rect_filled(
                                *rect,
                                0.0,
                                if (row + col) % 2 == 0 {
                                    Color32::MAGENTA
                                } else {
                                    Color32::LIGHT_GREEN
                                },
                            );
                        }
                        for ((row, col), rect, pixel) in &pixels {
                            if rect
                                .contains(ui.input(|i| i.pointer.latest_pos().unwrap_or_default()))
                            {
                                ui.label(format!(
                                    "real_mid: {}\nimag_mid: {}\nrad: {}\nreal_lo: {}\nreal_hi: {}\nimag_lo: {}\nimag_hi: {}",
                                    pixel.real_mid(), pixel.imag_mid(), pixel.rad(), pixel.real_lo(), pixel.real_hi(), pixel.imag_lo(), pixel.imag_hi())
                                );
                            }
                        }
                        return;
                    }
                }

                // new drawing
                {
                    #[cfg(false)]
                    {
                        let painter = ui.painter_at(ui.max_rect());
                        painter.rect_filled(ui.max_rect(), 0.0, Color32::RED);

                        camera_map
                            .pixels(self.stride)
                            .collect::<Vec<_>>()
                            .into_par_iter()
                            .map(|(_, rect, pixel)| {
                                let color = self.tree.color_of_pixel(pixel);
                                // let color = Color32::GREEN;
                                (rect, color)
                            })
                            .collect::<Vec<_>>()
                            .into_iter()
                            .for_each(|(rect, color)| {
                                painter.rect_filled(rect, 0.0, color);
                            });
                    }

                    #[cfg(false)]
                    {
                        let painter = ui.painter_at(ui.max_rect());
                        painter.rect_filled(ui.max_rect(), 0.0, Color32::RED);

                        // let num_threads = rayon::current_num_threads();
                        let num_threads = rayon::max_num_threads();
                        let pixels = camera_map.pixels(self.stride).collect::<Vec<_>>();
                        (0..num_threads).into_par_iter().for_each(|thread_i| {
                            (thread_i..pixels.len())
                                .step_by(num_threads)
                                .map(|i| pixels[i])
                                .for_each(|(_, rect, pixel)| {
                                    let color = self.tree.color_of_pixel(pixel);
                                    painter.rect_filled(rect, 0.0, color);
                                });
                        });
                    }

                    // #[cfg(false)]
                    {
                        let colors = camera_map
                            .pixels(self.stride)
                            .collect::<Vec<_>>()
                            .into_par_iter()
                            .map(|(_, _rect, pixel)| self.tree.color_of_pixel(pixel))
                            .collect::<Vec<_>>();
                        self.texture.set(
                            egui::ColorImage::new(
                                [
                                    camera_map.rect().size().x as usize / self.stride,
                                    camera_map.rect().size().y as usize / self.stride,
                                ],
                                colors,
                            ),
                            egui::TextureOptions::NEAREST,
                        );
                        ui.painter().image(
                            self.texture.id(),
                            camera_map.rect(),
                            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
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
