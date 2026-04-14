use core::panic;
use std::{
    sync::{
        Arc, Mutex, RwLock, TryLockError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

use eframe::egui::{self, Color32};

use crate::{
    complex::{CameraMap, Window, fixed::*},
    sample,
    tree::{NodeHandle, Tree},
};

// type SampleFn = dyn Fn((Real, Imag)) -> Color32;

pub(crate) static ELAPSED_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) static WORKER_HIST: [std::sync::atomic::AtomicU64; 128] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 128];

pub(crate) struct Fractal {
    // sample: Box<SampleFn>,
    pub(crate) tree: Arc<Tree>,
    // pub(crate) tree: RefCell<Tree>,
    // pub(crate) tree: Arc<Tree>,
    /// the `Window` in which we're sampling.
    /// `None` iff sampling is disabled.
    /// note that this is similar to shared_texture.camera_map.window,
    /// it's just that sampling and rendering are decoupled (eg either one may be disabled).
    /// TODO: this could be an atomic option instead of a `RwLock`.
    window: Arc<RwLock<Option<Window>>>,
    /// how many samples were taken since we last cleared this?
    sample_counter: Arc<AtomicU64>,
    shared_texture: SharedTexture,
    workers: Vec<WorkerHandle>,
}
impl Fractal {
    pub(crate) fn new_metabrot() -> Self {
        let thread_count = (thread::available_parallelism()
            .map(|thread_count| thread_count.get())
            .unwrap_or(1)
            - 1)
        .max(1);
        let tree = Arc::new(Tree::new());
        let window = Arc::new(RwLock::new(None));
        let sample_counter = Arc::new(AtomicU64::new(0));
        let shared_texture = SharedTexture::default();
        let workers = (0..thread_count)
            .map(|thread_i| {
                let tree = Arc::clone(&tree);
                let window = Arc::clone(&window);
                let sample_counter = Arc::clone(&sample_counter);
                let shared_texture = Arc::clone(&shared_texture);

                let handle = thread::Builder::new()
                    .name(format!("pool {}", thread_i))
                    .spawn(move || {
                        WorkerLocal {
                            thread_i,
                            tree,
                            window,
                            sample_counter,
                            to_be_colored: Vec::with_capacity(4),
                            shared_texture,
                        }
                        .run()
                    })
                    .unwrap();
                WorkerHandle { handle }
            })
            .collect();
        Self {
            tree,
            window,
            sample_counter,
            shared_texture,
            workers,
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

    // pub(crate) fn samples_in_flight(&self) -> usize {
    //     self.workers
    //         .iter()
    //         .map(|worker| worker.samples_in_flight)
    //         .sum()
    // }

    // pub(crate) fn render_in_flight(&self) -> usize {
    //     self.workers
    //         .iter()
    //         .map(|worker| worker.render_in_flight)
    //         .sum()
    // }

    pub(crate) fn thread_count(&self) -> usize {
        self.workers.len()
    }

    // pub(crate) fn join(&mut self) {
    //     self.pool.join();
    // }

    /// updates the window we're sampling in.
    /// should be called every frame.
    /// returns how many samples were taken since the last time this was called.
    /// TODO: rename.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn enable_sampling(&mut self, window: Window) -> u64 {
        // update the shared window
        {
            let mut shared_window = self.window.write().expect("window poisoned");
            *shared_window = Some(window);
        }

        self.sample_counter.swap(0, Ordering::SeqCst)
    }

    /// sets the shared window to `None`
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn disable_sampling(&mut self) {
        let mut shared_window = self.window.write().expect("window poisoned");
        *shared_window = None;
    }

    /// it's optional to call this every frame
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn begin_rendering(&mut self, camera_map: CameraMap) {
        // acquire exclusive access to shared_texture
        // do it now instead of at each access
        // TODO: we shouldn't need to block here once i've gotten rid of `RwLock`
        let mut shared_texture = self
            .shared_texture
            .write()
            .expect("shared_texture poisoned");
        // let mut shared_texture = match self.shared_texture.try_write() {
        //     Ok(shared_texture) => shared_texture,
        //     Err(TryLockError::Poisoned(_)) => panic!("shared_texture poisoned"),
        //     Err(TryLockError::WouldBlock) => panic!("we should have exclusive access"),
        // };

        // resize self.texture if needed
        {
            let (width, height) = (
                camera_map.rect().width() as usize,
                camera_map.rect().height() as usize,
            );
            if shared_texture.needs_resize(width, height) {
                shared_texture.resize(width, height);
            }
        }

        // reset the texture locks
        {
            shared_texture.reset_locks(camera_map);
        }
    }

    /// writes to the texture handle
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn finish_rendering(&mut self, handle: &mut egui::TextureHandle) {
        // wait for all lines to finish
        {
            self.shared_texture
                .try_read()
                .expect("no one should be writing")
                .block_until_finished();
        }
        // {
        //     // TODO: do this better
        //     while self
        //         .texture_lock_finish
        //         .iter()
        //         .find(|lock| !lock.load(Ordering::Relaxed))
        //         .is_some()
        //     {
        //         std::thread::yield_now();
        //     }
        // }

        // write to assert that we have exclusive access
        // TODO: we shouldn't need to block here once i've gotten rid of `RwLock`
        let mut shared_texture = self
            .shared_texture
            .write()
            .expect("shared_texture poisoned");
        // let mut shared_texture = match self.shared_texture.try_write() {
        //     Ok(shared_texture) => shared_texture,
        //     Err(TryLockError::Poisoned(_)) => panic!("shared_texture poisoned"),
        //     Err(TryLockError::WouldBlock) => panic!("we should have exclusive access"),
        // };

        assert!(
            shared_texture.camera_map().is_some(),
            "i should change this in the future so that a worker resets the camera, but right now that's the main thread's job"
        );
        *shared_texture.camera_map_mut() = None;

        // write to the texture handle
        shared_texture.set_texture(handle);
    }
}

fn set_texture(handle: &mut egui::TextureHandle, size: [usize; 2], colors: Vec<Color32>) {
    handle.set(
        egui::ColorImage::new(size, colors),
        egui::TextureOptions::NEAREST,
    );
}

pub(crate) use rayon_fractal::*;
mod rayon_fractal {
    use rayon::prelude::*;

    use super::*;

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
        set_texture(
            handle,
            [camera_map.pixels_width(), camera_map.pixels_height()],
            colors,
        );
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
        set_texture(
            handle,
            [camera_map.pixels_width(), camera_map.pixels_height()],
            colors,
        );
    }
}

/// owned by the pool/main thread
struct WorkerHandle {
    handle: thread::JoinHandle<()>,
}

use shared_texture::*;
mod shared_texture {
    use super::*;

    /// the main thread calls [`RwLock::write`] to resize the buffers.
    /// workers never call [`RwLock::write`].
    /// probably should never call read/write,
    /// instead only call `try_read` or `try_write`,
    /// bc those invariants are maintained manually.
    pub(super) type SharedTexture = Arc<RwLock<SharedTextureInner>>;

    pub(super) struct SharedTextureInner {
        /// the `CameraMap` where we're rendering.
        /// `Some` iff we're between rendering begin and finish.
        ///
        /// it's also kinda the global lock:
        /// the main thread sets it to `Some` when rendering begins,
        /// and a worker sets it to `None` once they're done.
        ///
        /// this isn't needed with `RwLock`, but i hope to get rid of `RwLock`.
        ///
        /// then when the main thread waits for workers to finish,
        /// *and* when workers check if they're finished,
        /// they only need to check one location and not all of the locks.
        camera_map: Option<CameraMap>,
        // pub(crate) camera_map: AtomicOption<CameraMap>,
        /// these are set when a line begins rendering
        texture_lock_begin: Vec<AtomicBool>,
        /// these are set when a line finishes rendering
        texture_lock_finish: Vec<AtomicBool>,
        /// should never call `lock`, only `try_lock`
        texture: Vec<Mutex<Vec<Color32>>>,
    }
    impl Default for SharedTextureInner {
        fn default() -> Self {
            Self {
                camera_map: None,
                texture_lock_begin: Vec::new(),
                texture_lock_finish: Vec::new(),
                texture: Vec::new(),
            }
        }
    }
    impl SharedTextureInner {
        fn width(&self) -> usize {
            let width = self
                .texture
                .first()
                .map(|line| line.try_lock().expect("no one should be writing").len())
                .unwrap_or(0);
            debug_assert!(
                self.texture.iter().all(|line| line
                    .try_lock()
                    .expect("no one should be writing")
                    .len()
                    == width)
            );
            width
        }
        fn height(&self) -> usize {
            let height = self.texture.len();
            debug_assert_eq!(self.texture_lock_begin.len(), height);
            debug_assert_eq!(self.texture_lock_finish.len(), height);
            height
        }

        pub(super) fn camera_map(&self) -> &Option<CameraMap> {
            &self.camera_map
        }
        pub(super) fn camera_map_mut(&mut self) -> &mut Option<CameraMap> {
            &mut self.camera_map
        }
        // pub(super) fn reset_camera_map(&mut self) { }

        pub(super) fn texture_lock_begin(&self) -> &Vec<AtomicBool> {
            &self.texture_lock_begin
        }
        pub(super) fn texture_lock_finish(&self) -> &Vec<AtomicBool> {
            &self.texture_lock_finish
        }
        pub(super) fn texture(&self) -> &Vec<Mutex<Vec<Color32>>> {
            &self.texture
        }

        pub(super) fn needs_resize(&self, width: usize, height: usize) -> bool {
            self.width() != width || self.height() != height
        }

        pub(super) fn resize(&mut self, width: usize, height: usize) {
            self.texture_lock_begin.clear();
            self.texture_lock_finish.clear();
            // TODO: is this correct with regard to the mutexes?
            self.texture.clear();
            self.texture_lock_begin
                .resize_with(height, || AtomicBool::new(true));
            self.texture_lock_finish
                .resize_with(height, || AtomicBool::new(true));
            self.texture
                .resize_with(height, || Mutex::new(vec![Color32::MAGENTA; width]));
        }

        /// for when we want to start rendering.
        /// checks that all the locks were set.
        /// should be called after resize.
        /// checks that self.camera_map is None
        // /// &mut self isn't really necessary, but it's semantically nice that only the main thread can reset the locks.
        // pub(super) fn reset_locks(&self) {
        pub(super) fn reset_locks(&mut self, camera_map: CameraMap) {
            assert!(self.camera_map.is_none(), "camera_map wasn't None");
            debug_assert!(
                self.texture_lock_begin
                    .iter()
                    .all(|lock| lock.load(Ordering::SeqCst)),
                "texture_lock_begin not all true"
            );
            debug_assert!(
                self.texture_lock_finish
                    .iter()
                    .all(|lock| lock.load(Ordering::SeqCst)),
                "texture_lock_finish not all true"
            );

            // it's important to reset finish before begin
            // at least if we aren't using `camera_map` as a lock
            for lock in self.texture_lock_finish.iter() {
                lock.store(false, Ordering::SeqCst);
            }
            for lock in self.texture_lock_begin.iter() {
                lock.store(false, Ordering::SeqCst);
            }

            self.camera_map = Some(camera_map);
        }

        pub(super) fn block_until_finished(&self) {
            // while self.camera_map.is_some() {
            //     std::thread::yield_now();
            // }
            // TODO: be better
            while self
                .texture_lock_finish
                .iter()
                .any(|lock| !lock.load(Ordering::SeqCst))
            {
                std::thread::yield_now();
            }
        }

        /// this allocates btw.
        pub(super) fn set_texture(&self, handle: &mut egui::TextureHandle) {
            assert!(self.camera_map.is_none(), "camera_map wan't reset");
            debug_assert!(
                self.texture_lock_begin
                    .iter()
                    .all(|lock| lock.load(Ordering::SeqCst)),
                "texture_lock_begin not all true"
            );
            debug_assert!(
                self.texture_lock_finish
                    .iter()
                    .all(|lock| lock.load(Ordering::SeqCst)),
                "texture_lock_finish not all true"
            );

            let size = [self.width(), self.height()];
            let colors = self
                .texture
                .iter()
                .flat_map(|line| line.try_lock().expect("rendering should be done").clone())
                .collect();
            set_texture(handle, size, colors);
        }
    }
}
/// owned by the worker thread
struct WorkerLocal {
    thread_i: usize,
    tree: Arc<Tree>,
    /// the `Window` in which we're sampling.
    /// `None` iff sampling is disabled.
    /// note that this is similar to shared_texture.camera_map.window,
    /// it's just that sampling and rendering are decoupled (eg either one may be disabled).
    /// TODO: this could be an atomic option instead of a `RwLock`.
    window: Arc<RwLock<Option<Window>>>,
    /// how many samples were taken since we last cleared this?
    sample_counter: Arc<AtomicU64>,
    /// nodes we split and need to find the color of.
    /// note that the len should be <= 4.
    /// TODO: rename
    to_be_colored: Vec<NodeHandle>,
    shared_texture: SharedTexture,
}
impl WorkerLocal {
    fn run(mut self) {
        loop {
            // rendering is highest priority
            // followed by sampling
            // followed by splitting

            if let Some(shared_texture) = match &self.shared_texture.try_read() {
                Ok(shared_texture) => Some(shared_texture),
                Err(TryLockError::Poisoned(_)) => panic!("shared_texture poisoned"),
                Err(TryLockError::WouldBlock) => {
                    // the main thread is rendering
                    None
                }
            } && let Some(camera_map) = shared_texture.camera_map()
            {
                // shared_texture.camera_map() is `None` if the main thread has started but not finished rendering

                // find a line for us to render
                // by just trying to lock each line's texture lock
                // TODO: do this better
                'outer: {
                    for (i, lock) in shared_texture.texture_lock_begin().iter().enumerate() {
                        // TODO: faster ordering
                        if lock
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            // let mut t = self.texture[i].clone();
                            // let mut l = &mut *self.texture[i];
                            // let l = Arc::get_mut(&mut t).expect("we just locked it");
                            // let t = self.texture[i].as_ref();
                            // let l = &Arc(t).expect("we just locked it");
                            // self.texture[i].get_mut()
                            // let l = Arc::get_mut(&mut self.texture[i]).expect("we just locked it");
                            let mut l = shared_texture.texture()[i]
                                .try_lock()
                                .expect("we just locked it");
                            for ((_rect, pixel), target) in
                                camera_map.pixels().nth(i).unwrap().zip(l.iter_mut())
                            {
                                *target = if let Some(pixel) = pixel {
                                    self.tree.color_of_pixel(pixel)
                                } else {
                                    Color32::MAGENTA
                                };
                            }
                            debug_assert!(
                                !shared_texture.texture_lock_finish()[i].load(Ordering::SeqCst)
                            );
                            shared_texture.texture_lock_finish()[i].store(true, Ordering::SeqCst);
                            break 'outer;
                        }
                    }
                    // no break
                    // all locks have been set
                    // shared_texture.reset_camera_map();
                    // self.camera_map = None;
                }
            } else if let Some(handle) = self.to_be_colored.pop() {
                // sample
                // dbg!("sample");
                let (real, imag) = self.tree.mid_of_node_id(handle);
                let color = sample::metabrot_sample((real, imag)).color();
                self.tree.insert(handle, color);
                self.sample_counter.fetch_add(1, Ordering::Relaxed);
            } else if let Some(window) = match self.window.try_read() {
                Ok(window) => *window,
                Err(TryLockError::Poisoned(_)) => panic!("window poisoned"),
                Err(TryLockError::WouldBlock) => {
                    // the main thread is updating the window
                    None
                }
            } {
                // split
                // dbg!("split");
                if let Some(handles) = self.tree.refine(window) {
                    // dbg!("refined");
                    self.to_be_colored.extend(handles);
                }
            }
        }
    }
}
