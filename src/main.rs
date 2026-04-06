mod app;
mod complex;
mod fractal;
mod pool;
mod sample;
mod tree;
mod typestate_sample;

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use eframe::egui::{Pos2, Rect, Vec2};
use mimalloc::MiMalloc;

use crate::{
    complex::{Camera, CameraMap, Domain},
    pool::Pool,
    tree::Tree,
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> eframe::Result {
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    // bench_refine();
    // panic!("bench done");

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "fractal",
        native_options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

fn bench_refine() {
    let start = Instant::now();
    let mut tree = Tree::new(Domain::default());
    // let stride = 1;
    // let stride = 8;
    let camera = Camera::default();
    let camera_map = CameraMap::new_without_stride(
        Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0)),
        camera,
    );
    // let mut pool = Pool::new(7);
    let mut pool = Pool::default();
    let mut frame_start = Instant::now();
    let frame_duration = Duration::from_secs_f32(1.0 / 60.0);
    let frame_count = 300;
    // sample inserted each frame
    let mut sample_counts = Vec::with_capacity(frame_count);
    // TODO: less code reuse
    for _ in 0..frame_count {
        // take samples out of the pool
        let mut sample_count = 0;
        while let Some(((real, imag), color)) = pool.receive_sample() {
            tree.insert((real, imag), color).unwrap();
            sample_count += 1;
        }
        sample_counts.push(sample_count);

        // request samples
        const MAX_IN_FLIGHT: usize = 512;
        while pool.samples_in_flight() < MAX_IN_FLIGHT {
            let Some(points) = tree.refine(camera_map.window().unwrap_or(Domain::default().into()))
            else {
                break;
            };
            for (real, imag) in points {
                pool.request_sample((real, imag));
            }
        }

        loop {
            let elapsed = frame_start.elapsed();
            if elapsed >= frame_duration {
                break;
            }
            std::thread::sleep((frame_duration - elapsed) / 2);
        }
        frame_start = Instant::now();
    }

    println!("nodes: {:?}", tree.node_count());
    println!("time: {:?}", start.elapsed());

    // empty the pool
    while pool.samples_in_flight() > 0 {
        while let Some(((real, imag), color)) = pool.receive_sample() {
            tree.insert((real, imag), color).unwrap();
        }
        std::thread::yield_now();
    }

    black_box(&tree);
    println!("nodes: {:?}", tree.node_count());
    println!("time: {:?}", start.elapsed());
    println!("sample_counts: {:?}", sample_counts);
}
