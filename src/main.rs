mod app;
mod camera;
mod fixed;
mod pool;
mod sample;
mod tree;
mod typestate_sample;

use mimalloc::MiMalloc;

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
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
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
