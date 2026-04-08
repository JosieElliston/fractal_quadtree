use std::{
    cell::RefCell,
    sync::{Arc, Mutex, RwLock, mpsc},
    thread,
};

use eframe::egui::{self, Color32};
use rayon::prelude::*;

use crate::{
    complex::{CameraMap, Domain, Window, fixed::*},
    sample,
    tree::{NodeId, Tree},
};

// type SampleFn = dyn Fn((Real, Imag)) -> Color32;

// trait RenderState {}
// struct NotRendering {}
// impl RenderState for NotRendering {}
// struct Rendering {}
// impl RenderState for Rendering {}

// TODO: probably fractal should live on a different thread?
// except switching is slow
// so we should try to just have it be in main

pub(crate) static ELAPSED_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) static WORKER_HIST: [std::sync::atomic::AtomicU64; 128] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 128];

pub(crate) struct Fractal {
    // sample: Box<SampleFn>,
    // pub(crate) tree: Arc<RwLock<Tree>>,
    // pub(crate) tree: RefCell<Tree>,
    pub(crate) tree: Arc<Tree>,
    workers: Vec<WorkerHandle>,
    sample_response_i: usize,
    render_response_i: usize,
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
    // texture: Arc<[Arc<[Color32]>]>,
}
impl Fractal {
    pub(crate) fn new_metabrot() -> Self {
        let thread_count = (thread::available_parallelism()
            .map(|thread_count| thread_count.get())
            .unwrap_or(1)
            - 1)
        .max(1);
        let workers = (0..thread_count)
            .map(|thread_i| {
                let (sample_response_sender, sample_response_receiver) = mpsc::channel();
                let (sample_request_sender, sample_request_receiver) = mpsc::channel();
                let (render_response_sender, render_response_receiver) = mpsc::channel();
                let (render_request_sender, render_request_receiver) = mpsc::channel();

                let handle = thread::Builder::new()
                    .name(format!("pool {}", thread_i))
                    .spawn(move || {
                        WorkerLocal {
                            thread_i,
                            sample_request_receiver,
                            sample_response_sender,
                            render_request_receiver,
                            render_response_sender,
                        }
                        .run()
                    })
                    .unwrap();
                WorkerHandle {
                    handle,
                    sample_request_sender,
                    sample_response_receiver,
                    samples_in_flight: 0,
                    render_request_sender,
                    render_response_receiver,
                    render_in_flight: 0,
                }
            })
            .collect();
        Self {
            tree: Arc::new(Tree::new(Domain::default())),
            workers,
            sample_response_i: 0,
            render_response_i: 0,
            window: None,
            camera_map: None,
            texture: Box::new([]),
        }
    }
    // fn new_metabrot(pool: Pool) -> Self {
    //     Self::new(
    //         Box::new(|point| sample::metabrot_sample(point).color()),
    //         pool,
    //     )
    // }
    // fn new_mandelbrot(pool: Pool) -> Self {
    //     Self::new(Box::new(|point| sample::mandelbrot_sample(point).color()), pool)
    // }

    pub(crate) fn samples_in_flight(&self) -> usize {
        self.workers
            .iter()
            .map(|worker| worker.samples_in_flight)
            .sum()
    }

    pub(crate) fn render_in_flight(&self) -> usize {
        self.workers
            .iter()
            .map(|worker| worker.render_in_flight)
            .sum()
    }

    pub(crate) fn thread_count(&self) -> usize {
        self.workers.len()
    }

    // pub(crate) fn join(&mut self) {
    //     self.pool.join();
    // }

    /// kinda hacky bc it'll change in the future.
    /// currently, you need to call this every frame.
    /// but in the future you'll only need to call this when the sampling state changes.
    /// i could make it match the future api by having Fractal spawn a thread and communicate with it, but whatever.
    /// returns how many samples were taken since the last time this was called.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn enable_sampling(&mut self, window: Window) -> usize {
        self.window = Some(window);

        #[cfg_attr(feature = "profiling", inline(never))]
        fn receive_sample(
            workers: &mut [WorkerHandle],
            sample_response_i: &mut usize,
        ) -> Option<SampleResponse> {
            let old_sample_response_i = *sample_response_i;
            loop {
                let worker = &mut workers[*sample_response_i];
                if let Ok((point, (real, imag), color)) = worker.sample_response_receiver.try_recv()
                {
                    assert!(
                        worker.samples_in_flight > 0,
                        "this is an invariant of the type"
                    );
                    worker.samples_in_flight -= 1;
                    return Some((point, (real, imag), color));
                }
                *sample_response_i += 1;
                *sample_response_i %= workers.len();
                if *sample_response_i == old_sample_response_i {
                    return None;
                }
            }
        }

        // take samples out of the pool
        let mut sample_count = 0;
        while let Some((node_id, (real, imag), color)) =
            receive_sample(&mut self.workers, &mut self.sample_response_i)
        {
            Arc::get_mut(&mut self.tree).unwrap().insert(node_id, color);
            sample_count += 1;
        }

        // if self.pool.samples_in_flight() == 0 {
        //     println!("threads were starved, no samples in flight");
        // }

        #[cfg_attr(feature = "profiling", inline(never))]
        fn request_sample(
            workers: &mut [WorkerHandle],
            node_id: NodeId,
            (real, imag): (Real, Imag),
        ) {
            // TODO: or maybe just do round robin
            // actually with how efficiently cores work that would be bad
            // actually threads get put on different cores, so maybe it's not that bad
            // but it's still bad over a single frame
            #[cfg(false)]
            {
                WORKER_HIST[self
                    .workers
                    .iter()
                    .enumerate()
                    .min_by_key(|(i, worker)| worker.sample_in_flight)
                    .unwrap()
                    .0]
                    .fetch_add(1, std::sync::atomic::Ordering::Release);
            }
            let worker = workers
                .iter_mut()
                .min_by_key(|worker| worker.samples_in_flight)
                .unwrap();
            worker
                .sample_request_sender
                .send((node_id, (real, imag)))
                .unwrap();
            worker.samples_in_flight += 1;
        }

        // request samples
        // TODO: cancel samples if we pan away (this won't be needed in the future)
        const MAX_IN_FLIGHT: usize = 512;
        while self.samples_in_flight() < MAX_IN_FLIGHT {
            let Some(node_ids) = Arc::get_mut(&mut self.tree).unwrap().refine(window) else {
                break;
            };
            for node_id in node_ids {
                request_sample(
                    &mut self.workers,
                    node_id,
                    self.tree.mid_of_node_id(node_id),
                );
            }
        }
        sample_count
    }

    /// currently nearly a nop, but won't be in the future
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn disable_sampling(&mut self) {
        self.window = None;
    }

    /// it's optional to call this every frame
    #[cfg_attr(feature = "profiling", inline(never))]
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

        // /// line is an out parameter, ie the contents of line are never read
        #[cfg_attr(feature = "profiling", inline(never))]
        fn request_line(
            workers: &mut [WorkerHandle],
            tree: &Arc<Tree>,
            camera_map: &CameraMap,
            row: usize,
        ) {
            // TODO: or maybe just do round robin
            let worker = workers
                .iter_mut()
                .min_by_key(|worker| worker.render_in_flight)
                .unwrap();
            worker
                .render_request_sender
                .send((Arc::clone(tree), camera_map.clone(), row))
                .unwrap();
            worker.render_in_flight += 1;
        }

        // this will submit an ~equal number of requests to each worker,
        // but workers run at different speeds (in the short term) (due to performance vs efficiency cores)
        // TODO: this is bad
        for row in 0..camera_map.pixels_height() {
            request_line(&mut self.workers, &self.tree, camera_map, row);
        }
    }

    /// writes to the texture handle
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn finish_rendering(&mut self, handle: &mut egui::TextureHandle) {
        // receive lines from pool
        {
            #[cfg_attr(feature = "profiling", inline(never))]
            fn receive_line(
                workers: &mut [WorkerHandle],
                render_response_i: &mut usize,
            ) -> Option<RenderResponse> {
                // println!("render_in_flight total: {}", self.render_in_flight());
                // println!(
                //     "render_in_flight each: {:?}",
                //     self.workers
                //         .iter()
                //         .map(|w| w.render_in_flight)
                //         .collect::<Vec<_>>()
                // );
                let old_render_response_i = *render_response_i;
                loop {
                    let worker = &mut workers[*render_response_i];
                    if let Ok((row, line)) = worker.render_response_receiver.try_recv() {
                        assert!(
                            worker.render_in_flight > 0,
                            "this is an invariant of the type"
                        );
                        worker.render_in_flight -= 1;
                        return Some((row, line));
                    }
                    *render_response_i += 1;
                    *render_response_i %= workers.len();
                    if *render_response_i == old_render_response_i {
                        return None;
                    }
                }
            }

            let mut debug_received_line = vec![false; self.texture.len()].into_boxed_slice();
            while self.render_in_flight() > 0 {
                // println!(
                //     "seen {} / {} lines",
                //     debug_received_line.iter().filter(|&&b| b).count(),
                //     debug_received_line.len()
                // );
                let Some((row, line)) =
                    receive_line(&mut self.workers, &mut self.render_response_i)
                else {
                    // dbg!("did not receive line");
                    continue;
                };
                assert!(!debug_received_line[row], "received line {row} twice");
                debug_received_line[row] = true;
                self.texture[row] = line.into_boxed_slice();
            }
            assert!(
                debug_received_line.iter().all(|&b| b),
                "didn't receive lines: {:?}",
                debug_received_line
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &b)| if !b { Some(i) } else { None })
                    .collect::<Vec<_>>()
            );
        }

        // write to the texture handle
        {
            let Some(camera_map) = &self.camera_map else {
                panic!("called finish_rendering without begin_rendering")
            };
            // `egui::ColorImage::new` consumes the colors, so allocating is required :(
            let colors = self
                .texture
                .iter()
                .flat_map(|line| line.iter().cloned())
                .collect();
            set_texture(handle, camera_map, colors);
            self.camera_map = None;
        }
    }
}

pub(crate) fn render_mandelbrot(
    handle: &mut egui::TextureHandle,
    camera_map: &CameraMap,
    (z0_real, z0_imag): (Real, Imag),
) {
    let colors = camera_map
        .pixels()
        .flatten()
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_rect, pixel)| {
            if let Some(pixel) = pixel {
                let c = pixel.mid();
                sample::quadratic_map((z0_real, z0_imag), c).color()
            } else {
                Color32::MAGENTA
            }
        })
        .collect::<Vec<_>>();
    set_texture(handle, camera_map, colors);
}

pub(crate) fn render_color(handle: &mut egui::TextureHandle, camera_map: &CameraMap) {
    let colors = camera_map
        .pixels()
        .flatten()
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_rect, pixel)| {
            if pixel.is_some() {
                Color32::BLACK
            } else {
                Color32::MAGENTA
            }
        })
        .collect::<Vec<_>>();
    set_texture(handle, camera_map, colors);
}

fn set_texture(handle: &mut egui::TextureHandle, camera_map: &CameraMap, colors: Vec<Color32>) {
    handle.set(
        egui::ColorImage::new(
            [camera_map.pixels_width(), camera_map.pixels_height()],
            colors,
        ),
        egui::TextureOptions::NEAREST,
    );
}

// will need this later
// type CameraUpdate = CameraMap;
type SampleRequest = (NodeId, (Real, Imag));
type SampleResponse = (NodeId, (Real, Imag), Color32);
/// usize is the row
type RenderRequest = (Arc<Tree>, CameraMap, usize);
// type RenderRequest = (Arc<RwLock<Tree>>, CameraMap, usize);
/// usize is the row
type RenderResponse = (usize, Vec<Color32>);

/// owned by the pool/main thread
struct WorkerHandle {
    handle: thread::JoinHandle<()>,
    sample_request_sender: mpsc::Sender<SampleRequest>,
    /// instead of having one response channel per worker,
    /// we could just have one owned by pool,
    /// but this is more symmetric,
    /// and will be removed anyway once i do better parallelism
    sample_response_receiver: mpsc::Receiver<SampleResponse>,
    /// how many samples have been requested but not yet received a response for?
    samples_in_flight: usize,
    render_request_sender: mpsc::Sender<RenderRequest>,
    render_response_receiver: mpsc::Receiver<RenderResponse>,
    render_in_flight: usize,
}

/// owned by the worker thread
struct WorkerLocal {
    thread_i: usize,
    // tree: Arc<Tree>,
    sample_request_receiver: mpsc::Receiver<SampleRequest>,
    sample_response_sender: mpsc::Sender<SampleResponse>,
    render_request_receiver: mpsc::Receiver<RenderRequest>,
    render_response_sender: mpsc::Sender<RenderResponse>,
}
impl WorkerLocal {
    fn run(self) {
        loop {
            // render requests are higher priority than sample requests
            if let Some((row, line)) = self.try_render() {
                self.render_response_sender.send((row, line)).unwrap();
                continue;
            }

            if let Some(response) = self.try_sample() {
                self.sample_response_sender.send(response).unwrap();
                continue;
            }

            // we don't tokio::select!, so just block on sample_request
            // it deadlocks if we block on sample_request, so just yield the thread
            // TODO: maybe yield the thread
            // actually just busy wait
            // TODO: block on both
            // thread::yield_now();
        }
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    fn try_sample(&self) -> Option<SampleResponse> {
        let Ok((node_id, (real, imag))) = self.sample_request_receiver.try_recv() else {
            return None;
        };
        // metabrot
        let color = sample::metabrot_sample((real, imag)).color();
        // // mandelbrot
        // let color = sample::quadratic_map((Real::ZERO, Imag::ZERO), point).color();
        Some((node_id, (real, imag), color))
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    fn try_render(&self) -> Option<RenderResponse> {
        let Ok((tree, camera_map, row)) = self.render_request_receiver.try_recv() else {
            return None;
        };
        let line = camera_map
            .pixels()
            .nth(row)
            .unwrap()
            .map(|(_rect, pixel)| {
                if let Some(pixel) = pixel {
                    tree.color_of_pixel(pixel)
                } else {
                    Color32::MAGENTA
                }
            })
            .collect();
        Some((row, line))
    }
}
