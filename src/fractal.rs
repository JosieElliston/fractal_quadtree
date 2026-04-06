use std::sync::{Arc, Mutex, RwLock};

use eframe::egui::{self, Color32};

use crate::{
    complex::{CameraMap, Domain, Window, fixed::*},
    pool::Pool,
    sample,
    tree::Tree,
};

type SampleFn = dyn Fn((Real, Imag)) -> Color32;

// trait RenderState {}
// struct NotRendering {}
// impl RenderState for NotRendering {}
// struct Rendering {}
// impl RenderState for Rendering {}

// TODO: probably fractal should live on a different thread?
// except switching is slow
// so we should try to just have it be in main

// TODO: maybe merge `Fractal` and `Pool`

struct Fractal {
    sample: Box<SampleFn>,
    tree: Arc<RwLock<Tree>>,
    pool: Pool,
    /// the `Window` in which we're sampling
    /// note: this is currently unused
    window: Option<Window>,
    /// the `CameraMap` where we're rendering
    /// `Some` iff we're between rendering begin and finish
    /// note: this is currently nearly unused
    camera_map: Option<CameraMap>,
    // texture: Arc<[Arc<Mutex<[Color32]>>]>,
    // texture: Arc<[Mutex<Box<[Color32]>>]>,
    texture: Box<[Box<[Color32]>]>,
}
impl Fractal {
    fn new(sample: Box<SampleFn>, pool: Pool) -> Self {
        Self {
            sample,
            tree: Arc::new(RwLock::new(Tree::new(Domain::default()))),
            pool,
            camera_map: None,
            window: None,
            texture: Box::new([]),
        }
    }
    fn new_metabrot(pool: Pool) -> Self {
        Self::new(
            Box::new(|point| sample::metabrot_sample(point).color()),
            pool,
        )
    }
    // fn new_mandelbrot(pool: Pool) -> Self {
    //     Self::new(Box::new(|point| sample::mandelbrot_sample(point).color()), pool)
    // }

    /// kinda hacky bc it'll change in the future.
    /// currently, you need to call this every frame.
    /// but in the future you'll only need to call this when the sampling state changes.
    /// i could make it match the future api by having Fractal spawn a thread and communicate with it, but whatever.
    /// returns how many samples were taken since the last time this was called.
    pub(crate) fn enable_sampling(&mut self, window: Window) -> usize {
        self.window = Some(window);

        // take samples out of the pool
        let mut sample_count = 0;
        while let Some(((real, imag), color)) = self.pool.receive_sample() {
            self.tree
                .write()
                .unwrap()
                .insert((real, imag), color)
                .unwrap();
            sample_count += 1;
        }

        // request samples
        // TODO: cancel samples if we pan away (this won't be needed in the future)
        const MAX_IN_FLIGHT: usize = 512;
        while self.pool.samples_in_flight() < MAX_IN_FLIGHT {
            let Some(points) = self.tree.write().unwrap().refine(window) else {
                break;
            };
            for (real, imag) in points {
                self.pool.request_sample((real, imag));
            }
        }
        sample_count
    }

    /// currently nearly a nop, but won't be in the future
    pub(crate) fn disable_sampling(&mut self) {
        self.window = None;
    }

    /// it's optional to call this every frame
    pub(crate) fn begin_rendering(&mut self, camera_map: &CameraMap) {
        assert!(
            self.camera_map.is_none(),
            "called begin_rendering while already rendering"
        );
        self.camera_map = Some(camera_map.clone());

        // resize self.texture if needed
        if self.texture.len() != camera_map.pixels_height()
            || self.texture.first().map_or(0, |row| row.len()) != camera_map.pixels_width()
        {
            self.texture = (0..camera_map.rect().height() as usize)
                .map(|_| {
                    (0..camera_map.rect().width() as usize)
                        .map(|_| Color32::MAGENTA)
                        .collect()
                })
                .collect();
        }

        // this will submit an ~equal number of requests to each worker,
        // but workers run at different speeds (in the short term) (due to performance vs efficiency cores)
        // TODO: this is bad
        for row in 0..camera_map.pixels_height() {
            self.pool.request_line(&self.tree, camera_map, row);
        }
    }

    /// writes to the texture handle
    pub(crate) fn finish_rendering(&mut self, handle: &mut egui::TextureHandle) {
        // receive lines from pool
        {
            let mut debug_recieved_line = vec![false; self.texture.len()].into_boxed_slice();
            while self.pool.render_in_flight() > 0 {
                let Some((row, line)) = self.pool.receive_line() else {
                    continue;
                };
                assert!(!debug_recieved_line[row], "recieved line {row} twice");
                debug_recieved_line[row] = true;
                self.texture[row] = line.into_boxed_slice();
            }
            assert!(
                debug_recieved_line.iter().all(|&b| b),
                "didn't receive lines: {:?}",
                debug_recieved_line
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &b)| if !b { Some(i) } else { None })
                    .collect::<Vec<_>>()
            );
        }

        // write to the texture handle
        {
            // TODO: don't allocate
            let colors = self
                .texture
                .iter()
                .flat_map(|line| line.iter().cloned())
                .collect();
            let Some(camera_map) = &self.camera_map else {
                panic!("called finish_rendering without begin_rendering")
            };
            handle.set(
                egui::ColorImage::new(
                    [camera_map.pixels_width(), camera_map.pixels_height()],
                    colors,
                ),
                egui::TextureOptions::NEAREST,
            );
            self.camera_map = None;
        }
    }
}
