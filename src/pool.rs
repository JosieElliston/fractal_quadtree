use std::{
    sync::{Arc, Mutex, RwLock, mpsc},
    thread,
};

use eframe::egui::Color32;

use crate::{
    complex::{CameraMap, fixed::*},
    sample,
    tree::{NodeId, Tree},
};

pub(crate) static ELAPSED_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) static WORKER_HIST: [std::sync::atomic::AtomicU64; 128] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 128];

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

pub(crate) struct Pool {
    workers: Vec<WorkerHandle>,
    // sample_response_receiver: mpsc::Receiver<(usize, (Real, Imag), Color32)>,
    sample_response_i: usize,
    render_response_i: usize,
}
impl Default for Pool {
    fn default() -> Self {
        // let thread_count = 3;

        // leave one thread for main and other processes,
        // but still take at least one thread
        let thread_count = (thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1)
            - 1)
        .max(1);
        Self::new(thread_count)
    }
}
impl Pool {
    pub(crate) fn new(thread_count: usize) -> Self {
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
            workers,
            sample_response_i: 0,
            render_response_i: 0,
        }
    }

    // pub(crate) fn join(&mut self) {
    //     for worker in self.workers.drain(..) {
    //         worker.handle.join().unwrap();
    //     }
    // }

    pub(crate) fn thread_count(&self) -> usize {
        self.workers.len()
    }

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

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn request_sample(&mut self, node_id: NodeId, (real, imag): (Real, Imag)) {
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
        let worker = self
            .workers
            .iter_mut()
            .min_by_key(|worker| worker.samples_in_flight)
            .unwrap();
        worker
            .sample_request_sender
            .send((node_id, (real, imag)))
            .unwrap();
        worker.samples_in_flight += 1;
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn receive_sample(&mut self) -> Option<SampleResponse> {
        let old_sample_response_i = self.sample_response_i;
        loop {
            let worker = &mut self.workers[self.sample_response_i];
            if let Ok((point, (real, imag), color)) = worker.sample_response_receiver.try_recv() {
                assert!(
                    worker.samples_in_flight > 0,
                    "this is an invariant of the type"
                );
                worker.samples_in_flight -= 1;
                return Some((point, (real, imag), color));
            }
            self.sample_response_i += 1;
            self.sample_response_i %= self.workers.len();
            if self.sample_response_i == old_sample_response_i {
                return None;
            }
        }
    }

    // /// line is an out parameter, ie the contents of line are never read
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn request_line(
        &mut self,
        // tree: &Arc<RwLock<Tree>>,
        tree: &Arc<Tree>,
        camera_map: &CameraMap,
        row: usize,
    ) {
        // TODO: or maybe just do round robin
        let worker = self
            .workers
            .iter_mut()
            .min_by_key(|worker| worker.render_in_flight)
            .unwrap();
        worker
            .render_request_sender
            .send((Arc::clone(tree), camera_map.clone(), row))
            .unwrap();
        worker.render_in_flight += 1;
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn receive_line(&mut self) -> Option<RenderResponse> {
        // println!("render_in_flight total: {}", self.render_in_flight());
        // println!(
        //     "render_in_flight each: {:?}",
        //     self.workers
        //         .iter()
        //         .map(|w| w.render_in_flight)
        //         .collect::<Vec<_>>()
        // );
        let old_render_response_i = self.render_response_i;
        loop {
            let worker = &mut self.workers[self.render_response_i];
            if let Ok((row, line)) = worker.render_response_receiver.try_recv() {
                assert!(
                    worker.render_in_flight > 0,
                    "this is an invariant of the type"
                );
                worker.render_in_flight -= 1;
                return Some((row, line));
            }
            self.render_response_i += 1;
            self.render_response_i %= self.workers.len();
            if self.render_response_i == old_render_response_i {
                return None;
            }
        }
    }
}
