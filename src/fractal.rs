use std::{
    sync::{
        Arc, Mutex, RwLock, TryLockError,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SendError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use atomic::Atomic;
use eframe::egui::{self, Color32};

use crate::{
    complex::{CameraMap, Window, fixed::*},
    sample,
    tree::{Moment, NodeHandle, Tree},
};

/// this mostly exists so i don't duplicate doc comments
struct Shared {
    tree: Arc<Tree>,
    // /// the moment we started drawing the previous frame.
    /// we can skip drawing nodes that haven't been updated since the start of the previous frame.
    /// it might happen that we will have correctly drawn pixels that got sampled during frame drawing,
    /// and then redraw those pixels, but this is rare and not too bad.
    // /// in the sample functions, we pass `frame_start + 1`.
    /// monotonically increasing.
    /// only ever updated by the main thread.
    /// we have that `frame_start < sample_time`.
    now: Arc<Atomic<Moment>>,
    /// the `Window` in which we're sampling.
    /// `None` iff sampling is disabled.
    /// note that this is similar to shared_texture.camera_map.window,
    /// it's just that sampling and rendering are decoupled (eg either one may be disabled).
    /// TODO: this could be an atomic option instead of a `RwLock`.
    window: Arc<RwLock<Option<Window>>>,
    /// how many samples were taken since we last cleared this?
    sample_counter: Arc<AtomicU64>,
    shared_texture: SharedTexture,
    /// set by the main thread to ask worker threads to exit.
    kill: Arc<AtomicBool>,
}

pub(crate) use main_thread::*;
mod main_thread {
    use crate::tree::ThreadData;

    use super::*;

    /// owned by the pool/main thread
    struct WorkerHandle {
        handle: thread::JoinHandle<()>,
        /// workers don't actually update this on every iteration,
        /// they batch updates using a local timer.
        /// this isn't in `shared` bc the workers shouldn't know about each other's timers.
        // TODO: maybe replace `Mutex` with `Atomic`
        shared_timer: Arc<Mutex<MultiTimer>>,
    }

    pub(crate) struct Fractal {
        shared: Shared,
        workers: Vec<WorkerHandle>,
        pub(crate) thread_data: ThreadData,
    }
    impl Fractal {
        pub(crate) fn new() -> Self {
            let thread_count = (thread::available_parallelism()
                .map(|thread_count| thread_count.get())
                .unwrap_or(1)
                - 1)
            .max(1);
            let mut thread_data = ThreadData::default();
            let shared = Shared {
                tree: Arc::new(Tree::new(&mut thread_data)),
                now: Arc::new(Atomic::new(Moment::default())),
                window: Arc::new(RwLock::new(None)),
                sample_counter: Arc::new(AtomicU64::new(0)),
                shared_texture: SharedTexture::default(),
                kill: Arc::new(AtomicBool::new(false)),
            };
            let workers = (0..thread_count)
                .map(|thread_i| {
                    let shared = Shared {
                        tree: Arc::clone(&shared.tree),
                        now: Arc::clone(&shared.now),
                        window: Arc::clone(&shared.window),
                        sample_counter: Arc::clone(&shared.sample_counter),
                        shared_texture: Arc::clone(&shared.shared_texture),
                        kill: Arc::clone(&shared.kill),
                    };
                    let shared_timer = Arc::new(Mutex::new(MultiTimer::default()));
                    let worker_timer = Arc::clone(&shared_timer);
                    let handle = thread::Builder::new()
                        .name(format!("pool {}", thread_i))
                        .spawn(move || WorkerLocal::new(shared, thread_i, worker_timer).run())
                        .unwrap();
                    WorkerHandle {
                        handle,
                        shared_timer,
                    }
                })
                .collect();
            Self {
                shared,
                workers,
                thread_data,
            }
        }

        pub(crate) fn tree(&self) -> &Arc<Tree> {
            &self.shared.tree
        }

        pub(crate) fn thread_count(&self) -> usize {
            self.workers.len()
        }

        // /// `&mut self` bc we need to receive updates.
        // pub(crate) fn timers(&mut self) -> Vec<MultiTimer> {
        //     for worker in &mut self.workers {
        //         while let Ok(timer) = worker.timer_receiver.try_recv() {
        //             worker.timer += timer;
        //         }
        //     }
        //     self.workers
        //         .iter()
        //         .map(|worker| worker.timer.clone())
        //         .collect()
        // }
        // pub(crate) fn reset_timers(&mut self) {
        //     for worker in &mut self.workers {
        //         worker.timer.reset();
        //     }
        // }

        /// receive updates and reset the timers.
        pub(crate) fn timer(&mut self) -> MultiTimer {
            self.workers
                .iter()
                .map(|worker| {
                    let mut timer = worker.shared_timer.lock().expect("shared_timer poisoned");
                    let ret = *timer;
                    // this here is why i prefer `reset` over `= MultiTimer::default()`,
                    // because otherwise it's a bit syntactically ambiguous whether we're just overwriting a local variable.
                    timer.reset();
                    ret
                })
                .reduce(|lhs, rhs| lhs + rhs)
                .expect("we don't have any workers")
        }

        pub(crate) fn join(&mut self) {
            self.shared.kill.store(true, Ordering::Relaxed);
            for worker in self.workers.drain(..) {
                worker.handle.join().expect("worker thread panicked");
            }
        }

        /// updates the window we're sampling in.
        /// should be called every frame.
        /// returns how many samples were taken since the last time this was called.
        /// TODO: rename.
        #[cfg_attr(feature = "profiling", inline(never))]
        pub(crate) fn enable_sampling(&mut self, window: Window) -> u64 {
            // update the shared window
            {
                let mut shared_window = self.shared.window.write().expect("window poisoned");
                *shared_window = Some(window);
            }

            self.shared.sample_counter.swap(0, Ordering::SeqCst)
        }

        /// sets the shared window to `None`
        #[cfg_attr(feature = "profiling", inline(never))]
        pub(crate) fn disable_sampling(&mut self) {
            let mut shared_window = self.shared.window.write().expect("window poisoned");
            *shared_window = None;
        }

        #[cfg_attr(feature = "profiling", inline(never))]
        pub(crate) fn enable_reclaiming(&mut self) {
            dbg!("somehow notify the workers");
        }

        #[cfg_attr(feature = "profiling", inline(never))]
        pub(crate) fn disable_reclaiming(&mut self) {
            dbg!("somehow notify the workers");
        }

        /// it's optional to call this every frame
        #[cfg_attr(feature = "profiling", inline(never))]
        pub(crate) fn begin_rendering(&mut self, camera_map: &CameraMap, needs_full_redraw: bool) {
            // acquire exclusive access to shared_texture
            // do it now instead of at each access
            // TODO: we shouldn't need to block here once i've gotten rid of `RwLock`
            let mut shared_texture = self
                .shared
                .shared_texture
                .write()
                .expect("shared_texture poisoned");
            // let mut shared_texture = match self.shared_texture.try_write() {
            //     Ok(shared_texture) => shared_texture,
            //     Err(TryLockError::Poisoned(_)) => panic!("shared_texture poisoned"),
            //     Err(TryLockError::WouldBlock) => panic!("we should have exclusive access"),
            // };

            shared_texture.needs_full_redraw = needs_full_redraw;

            // update now and prev_frame_start
            {
                // // i can't just fetch_add(1) because of how the atomic crate works
                // let prev_frame_start = self.shared.now.load(Ordering::SeqCst);
                // self.shared
                //     .now
                //     .store(prev_frame_start + 1, Ordering::SeqCst);
                self.shared
                    .now
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |now| Some(now + 1))
                    .expect("we should never fail to update `now`");
            }

            // resize self.texture if needed
            {
                shared_texture.resize_if_needed(camera_map);
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
                self.shared
                    .shared_texture
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
                .shared
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
}

use worker_thread::*;
mod worker_thread {
    use crate::tree::ThreadData;

    use super::*;

    /// owned by the worker thread
    pub(super) struct WorkerLocal {
        shared: Shared,
        /// `usize` bc [`thread::available_parallelism`] returns a `usize`.
        thread_i: usize,
        thread_data: ThreadData,
        /// nodes we split and need to find the color of.
        /// note that the len should be <= 4.
        /// TODO: rename
        to_be_colored: Vec<NodeHandle>,
        /// accumulate updates here.
        local_timer: MultiTimer,
        /// use batched updates.
        shared_timer: Arc<Mutex<MultiTimer>>,
        last_sent: Instant,
    }
    impl WorkerLocal {
        const SHARED_TIMER_UPDATE_INTERVAL: Duration = Duration::from_millis(5);

        pub(super) fn new(
            shared: Shared,
            thread_i: usize,
            shared_timer: Arc<Mutex<MultiTimer>>,
        ) -> Self {
            Self {
                shared,
                thread_i,
                thread_data: ThreadData::default(),
                to_be_colored: Vec::with_capacity(4),
                local_timer: MultiTimer::default(),
                shared_timer,
                last_sent: Instant::now(),
            }
        }

        fn try_draw(&mut self) -> Option<()> {
            let shared_texture = match self.shared.shared_texture.try_read() {
                Ok(shared_texture) => shared_texture,
                Err(TryLockError::Poisoned(_)) => panic!("shared_texture poisoned"),
                Err(TryLockError::WouldBlock) => {
                    // the main thread is rendering
                    return None;
                }
            };
            // shared_texture.camera_map() is `None` if the main thread has started but not finished rendering
            let camera_map = shared_texture.camera_map().as_ref()?;

            // let prev_frame_start = self.shared.now.load(Ordering::SeqCst) - 1;
            // let prev_frame_start = shared_texture.prev_frame_start;

            // find a line for us to render
            // by just trying to lock each line's texture lock
            // TODO: do this better
            for (row, lock) in shared_texture.texture_lock_begin().iter().enumerate() {
                if lock.load(Ordering::Relaxed) {
                    continue;
                }
                if lock
                    .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                    .is_err()
                {
                    continue;
                }
                // TODO: we don't need this mutex, replace with `UnsafeCell`
                let mut l = shared_texture.texture()[row]
                    .try_lock()
                    .expect("we just locked it");
                {
                    let prev_frame_start = if shared_texture.needs_full_redraw {
                        Moment::MIN
                    } else {
                        self.shared.now.load(Ordering::SeqCst) - 1
                    };

                    // TODO: do more of this, perhaps bisection bc that's easier than real spacial stuff
                    let line_needs_redraw = shared_texture.needs_full_redraw
                        || 'line_needs_redraw: {
                            let Some(first_pixel) = camera_map.pixel_at(row, 0) else {
                                break 'line_needs_redraw true;
                            };
                            let Some(last_pixel) =
                                camera_map.pixel_at(row, camera_map.pixels_width() - 1)
                            else {
                                break 'line_needs_redraw true;
                            };
                            debug_assert_eq!(first_pixel.imag_mid(), last_pixel.imag_mid());
                            let imag = first_pixel.imag_mid();
                            let real_lo = first_pixel.real_mid();
                            let real_hi = last_pixel.real_mid();
                            debug_assert_ne!(
                                prev_frame_start,
                                Moment::MIN,
                                "we should short circuit earlier"
                            );
                            self.shared.tree.any_on_line_needs_redraw(
                                real_lo,
                                real_hi,
                                imag,
                                prev_frame_start,
                                &mut self.thread_data,
                            )
                        };

                    if !line_needs_redraw {
                        // debug draw unchanged lines pink
                        // l.iter_mut()
                        //     .for_each(|pixel| *pixel = Color32::from_rgb(255, 50, 255));
                    } else {
                        for ((_rect, pixel), target) in
                            camera_map.pixels().nth(row).unwrap().zip(l.iter_mut())
                        {
                            *target = if let Some(pixel) = pixel {
                                if let Some(color) =
                                    self.shared.tree.color_of_pixel(pixel, prev_frame_start)
                                {
                                    color
                                } else {
                                    // we proved that the color hasn't changed
                                    // debug draw unchanged pixels blue
                                    // Color32::from_rgb(50, 50, 255)
                                    continue;
                                }
                            } else {
                                Color32::MAGENTA
                            };
                        }
                    }
                }
                debug_assert!(!shared_texture.texture_lock_finish()[row].load(Ordering::SeqCst));
                shared_texture.texture_lock_finish()[row].store(true, Ordering::SeqCst);
                return Some(());
            }
            // all locks have been set
            None
        }

        fn try_sample(&mut self) -> Option<()> {
            let handle = self.to_be_colored.pop()?;

            // dbg!("sample");
            let (real, imag) = self.shared.tree.mid_of_node_handle(handle);
            let color = sample::metabrot_sample((real, imag)).color();
            self.shared
                .tree
                .insert(handle, color, self.shared.now.load(Ordering::SeqCst));
            self.shared.sample_counter.fetch_add(1, Ordering::Relaxed);

            Some(())
        }

        fn try_reclaim(&mut self) -> Option<()> {
            dbg!("somehow notify the workers to reclaim/free/deallocate nodes");
            return None;
            dbg!("reclaim");

            Some(())
        }

        fn try_split(&mut self) -> Option<()> {
            let window = match self.shared.window.try_read() {
                Ok(window) => *window.as_ref()?,
                Err(TryLockError::Poisoned(_)) => panic!("window poisoned"),
                Err(TryLockError::WouldBlock) => {
                    // the main thread is updating the window
                    return None;
                }
            };

            // dbg!("split");
            debug_assert!(self.to_be_colored.is_empty());
            if let Some(handles) = self.shared.tree.refine(
                window,
                self.shared.now.load(Ordering::SeqCst),
                &mut self.thread_data,
            ) {
                // dbg!("refined");
                self.to_be_colored.extend(handles);
            }
            Some(())
        }

        pub(super) fn run(mut self) {
            loop {
                if self.shared.kill.load(Ordering::Relaxed) {
                    break;
                }

                // rendering is highest priority
                // followed by sampling
                // followed by reclaiming
                // followed by splitting

                let start = Instant::now();

                if self.try_draw().is_some() {
                    self.local_timer.draw.insert(start.elapsed());
                } else if self.try_sample().is_some() {
                    self.local_timer.sample.insert(start.elapsed());
                // } else if self.try_reclaim().is_some() {
                //     self.local_timer.reclaim.insert(start.elapsed());
                } else if self.try_split().is_some() {
                    self.local_timer.split.insert(start.elapsed());
                } else {
                    // dbg!("idle");
                    // thread::yield_now();
                    // weird workaround, but it fixing freezing
                    // for when pausing sampling or the fractal is outside the window.
                    // except it doesn't work in release mode.
                    thread::sleep(Duration::from_millis(10));

                    self.local_timer.idle.insert(start.elapsed());
                }

                // update the shared timer rarely for performance
                if self.last_sent.elapsed() >= Self::SHARED_TIMER_UPDATE_INTERVAL {
                    {
                        let mut guard = self.shared_timer.lock().expect("shared_timer poisoned");
                        *guard += self.local_timer;
                    }
                    self.local_timer.reset();
                    self.last_sent = Instant::now();
                }
            }
        }
    }
}

pub(crate) use timer::*;
mod timer {
    use std::ops;

    use super::*;

    #[derive(Debug, Clone, Copy, Default)]
    pub(crate) struct Timer {
        elapsed: Duration,
        count: u64,
    }
    impl Timer {
        pub(super) fn insert(&mut self, elapsed: Duration) {
            self.elapsed += elapsed;
            self.count += 1;
        }

        pub(crate) fn elapsed(&self) -> Duration {
            self.elapsed
        }

        pub(crate) fn count(&self) -> u64 {
            self.count
        }

        pub(crate) fn div_elapsed(&self, elapsed: Duration) -> f64 {
            self.elapsed.div_duration_f64(elapsed)
        }

        pub(crate) fn div_count(&self, count: u64) -> Option<Duration> {
            (self.elapsed.as_nanos() as u64)
                .checked_div(count)
                .map(Duration::from_nanos)
        }

        // pub(crate) fn time_per_iter(&self) -> Option<Duration> {
        //     self.div(self.count)
        // }
    }
    impl ops::AddAssign for Timer {
        fn add_assign(&mut self, rhs: Self) {
            self.elapsed += rhs.elapsed;
            self.count += rhs.count;
        }
    }
    impl ops::Add for Timer {
        type Output = Self;

        fn add(self, rhs: Self) -> Self::Output {
            Self {
                elapsed: self.elapsed + rhs.elapsed,
                count: self.count + rhs.count,
            }
        }
    }

    /// this exists for debugging / UX,
    /// and is not needed for the main algorithm.
    /// it needs to be `Copy` for [`egui::util::History`].
    #[derive(Debug, Clone, Copy, Default)]
    pub(crate) struct MultiTimer {
        pub(crate) draw: Timer,
        pub(crate) sample: Timer,
        pub(crate) reclaim: Timer,
        pub(crate) split: Timer,
        pub(crate) idle: Timer,
    }
    impl MultiTimer {
        pub(crate) fn reset(&mut self) {
            *self = Self::default();
        }

        pub(crate) fn total(&self) -> Timer {
            self.draw + self.sample + self.reclaim + self.split + self.idle
        }
    }
    impl ops::AddAssign for MultiTimer {
        fn add_assign(&mut self, rhs: Self) {
            self.draw += rhs.draw;
            self.sample += rhs.sample;
            self.split += rhs.split;
            self.idle += rhs.idle;
        }
    }
    impl ops::Add for MultiTimer {
        type Output = Self;

        fn add(self, rhs: Self) -> Self::Output {
            Self {
                draw: self.draw + rhs.draw,
                sample: self.sample + rhs.sample,
                reclaim: self.reclaim + rhs.reclaim,
                split: self.split + rhs.split,
                idle: self.idle + rhs.idle,
            }
        }
    }
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
        pub(super) needs_full_redraw: bool,
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
        /// these are set when a line begins rendering.
        texture_lock_begin: Vec<AtomicBool>,
        /// these are set when a line finishes rendering.
        texture_lock_finish: Vec<AtomicBool>,
        /// should never call `lock`, only `try_lock`.
        /// TODO: with the texture locks, maybe this doesn't need a `Mutex`, just an `UnsafeCell`.
        /// TODO: inner `Vec` should be a `Box<[Color32]>`.
        texture: Vec<Mutex<Vec<Color32>>>,
    }
    impl Default for SharedTextureInner {
        fn default() -> Self {
            Self {
                needs_full_redraw: false,
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

        pub(super) fn texture_lock_begin(&self) -> &Vec<AtomicBool> {
            &self.texture_lock_begin
        }
        pub(super) fn texture_lock_finish(&self) -> &Vec<AtomicBool> {
            &self.texture_lock_finish
        }
        pub(super) fn texture(&self) -> &Vec<Mutex<Vec<Color32>>> {
            &self.texture
        }

        /// also sets `needs_full_redraw` to `true`.
        pub(super) fn resize_if_needed(&mut self, camera_map: &CameraMap) {
            let width = camera_map.pixels_width();
            let height = camera_map.pixels_height();

            if self.width() == width && self.height() == height {
                return;
            }
            self.needs_full_redraw = true;
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
        pub(super) fn reset_locks(&mut self, camera_map: &CameraMap) {
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

            self.camera_map = Some(camera_map.clone());
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

fn set_texture(handle: &mut egui::TextureHandle, size: [usize; 2], colors: Vec<Color32>) {
    handle.set(
        egui::ColorImage::new(size, colors),
        egui::TextureOptions::NEAREST,
    );
}
