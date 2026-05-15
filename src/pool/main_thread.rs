use std::{
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

use atomic::Atomic;
use egui::Color32;

use crate::{
    complex::{CameraMap, Window},
    tree::{Tree, TreeLocal, alloc::AllocLocal},
};

use super::{
    ReclaimMoment, RenderMoment,
    shared::{Shared, SharedTextureData},
    timer::MultiTimer,
    worker_thread::Worker,
};

/// whether to draw pixel that weren't cached from the last frame for a frame.
/// but only for not a full redraw :nauseated_face:.
pub(crate) static DRAW_COLOR_DIFF: AtomicBool = AtomicBool::new(true);
// const DRAW_COLOR_DIFF_COLOR: Color32 = Color32::from_rgb(50, 50, 255);
const DRAW_COLOR_DIFF_COLOR: Color32 = Color32::WHITE;

/// owned by the main thread.
struct WorkerHandle {
    join_handle: thread::JoinHandle<()>,
    /// receive from the worker thread what tick they think it is.
    shared_reclaim_now: Arc<Atomic<ReclaimMoment>>,
    /// workers don't actually update this on every iteration,
    /// they batch updates using a local timer.
    /// this isn't in `shared` bc the workers shouldn't know about each other's timers.
    // TODO: maybe replace `Mutex` with `Atomic`
    shared_timer: Arc<Mutex<MultiTimer>>,
}

/// the main thread data.
pub(crate) struct Fractal {
    pub(crate) tree_local: TreeLocal,
    pub(crate) alloc_local: AllocLocal,
    pub(crate) shared: Arc<Shared>,
    workers: Vec<WorkerHandle>,
    /// we apply diffs the worker threads compute.
    local_texture: Vec<Box<[Color32]>>,
}
impl Fractal {
    pub(crate) fn new() -> Self {
        let thread_count = (thread::available_parallelism()
            .map(|thread_count| thread_count.get())
            .unwrap_or(1)
            - 1)
        .max(1);
        let mut tree_local = TreeLocal::default();
        let mut alloc_local = AllocLocal::default();
        let shared = Arc::new(Shared {
            tree: Tree::new(&mut tree_local, &mut alloc_local),
            render_now: Atomic::new(RenderMoment::default()),
            reclaim_now: Atomic::new(ReclaimMoment::default()),
            reclaim_window: RwLock::new(None),
            reclaim_counter: AtomicU64::new(0),
            sample_window: RwLock::new(None),
            sample_counter: AtomicU64::new(0),
            shared_texture_data: RwLock::new(SharedTextureData::default()),
            kill: AtomicBool::new(false),
        });
        let workers = (0..thread_count)
            .map(|thread_i| {
                let shared = Arc::clone(&shared);
                let shared_reclaim_now =
                    Arc::new(Atomic::new(shared.reclaim_now.load(Ordering::SeqCst)));
                let shared_timer = Arc::new(Mutex::new(MultiTimer::default()));
                let shared_reclaim_now_clone = Arc::clone(&shared_reclaim_now);
                let shared_timer_clone = Arc::clone(&shared_timer);
                let join_handle = thread::Builder::new()
                    .name(format!("pool {}", thread_i))
                    .spawn(move || {
                        Worker::new(
                            shared,
                            thread_i,
                            shared_reclaim_now_clone,
                            shared_timer_clone,
                        )
                        .run()
                    })
                    .unwrap();
                WorkerHandle {
                    join_handle,
                    shared_reclaim_now,
                    shared_timer,
                }
            })
            .collect();
        Self {
            tree_local,
            alloc_local,
            shared,
            workers,
            local_texture: Vec::new(),
        }
    }

    pub(crate) fn tree(&self) -> &Tree {
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
    #[cfg_attr(feature = "profiling", inline(never))]
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

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn join(&mut self) {
        self.shared.kill.store(true, Ordering::Relaxed);
        for worker in self.workers.drain(..) {
            worker.join_handle.join().expect("worker thread panicked");
        }
    }

    /// returns `None` if any worker is behind,
    /// otherwise increments the tick and returns `Some`.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn try_reclaim_tick(&mut self) -> Option<()> {
        let now = self.shared.reclaim_now.load(Ordering::SeqCst);
        for worker in &self.workers {
            let worker_now = worker.shared_reclaim_now.load(Ordering::SeqCst);
            debug_assert!(
                now <= worker_now + 1,
                "workers should never more than one tick behind the main thread"
            );
            debug_assert!(
                worker_now <= now,
                "workers should never be ahead of the main thread"
            );
            if worker_now != now {
                return None;
            }
        }
        self.shared.reclaim_now.store(now + 1, Ordering::SeqCst);
        Some(())
    }

    /// updates the window we're reclaiming in.
    /// returns how many nodes were reclaimed since the last time this was called.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn enable_reclaiming(&mut self, window: Window) -> u64 {
        // update the shared window
        {
            let mut reclaim_window = self.shared.reclaim_window.write().expect("window poisoned");
            *reclaim_window = Some(window);
        }

        self.shared.reclaim_counter.swap(0, Ordering::SeqCst)
    }

    /// sets the reclaiming window to `None`
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn disable_reclaiming(&mut self) {
        let mut reclaim_window = self.shared.reclaim_window.write().expect("window poisoned");
        *reclaim_window = None;
    }

    /// updates the window we're sampling in.
    /// returns how many samples were taken since the last time this was called.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn enable_sampling(&mut self, window: Window) -> u64 {
        // update the shared window
        {
            let mut sample_window = self.shared.sample_window.write().expect("window poisoned");
            *sample_window = Some(window);
        }

        self.shared.sample_counter.swap(0, Ordering::SeqCst)
    }

    /// sets the sampling window to `None`
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn disable_sampling(&mut self) {
        let mut sample_window = self.shared.sample_window.write().expect("window poisoned");
        *sample_window = None;
    }

    /// it's optional to call this every frame
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn begin_rendering(&mut self, camera_map: &CameraMap, needs_full_redraw: bool) {
        // acquire exclusive access to shared_texture
        // do it now instead of at each access
        // TODO: we shouldn't need to block here once i've gotten rid of `RwLock`
        let mut shared_texture_data = self
            .shared
            .shared_texture_data
            .write()
            .expect("shared_texture poisoned");
        // let mut shared_texture = match self.shared_texture.try_write() {
        //     Ok(shared_texture) => shared_texture,
        //     Err(TryLockError::Poisoned(_)) => panic!("shared_texture poisoned"),
        //     Err(TryLockError::WouldBlock) => panic!("we should have exclusive access"),
        // };

        shared_texture_data.needs_full_redraw = needs_full_redraw;

        // update render now
        {
            // // i can't just fetch_add(1) because of how the atomic crate works
            // let prev_frame_start = self.shared.now.load(Ordering::SeqCst);
            // self.shared
            //     .now
            //     .store(prev_frame_start + 1, Ordering::SeqCst);
            self.shared
                .render_now
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |now| Some(now + 1))
                .expect("we should never fail to update `now`");
        }

        // resize the textures if needed
        {
            let width = camera_map.pixels_width();
            let height = camera_map.pixels_height();

            if shared_texture_data.width() != width || shared_texture_data.height() != height {
                shared_texture_data.resize(width, height);
                self.local_texture.clear();
                self.local_texture
                    .resize(height, vec![Color32::MAGENTA; width].into_boxed_slice());
            }
        }

        // reset the diff
        {
            for line in shared_texture_data.diff().iter() {
                for c in line.try_lock().unwrap().iter_mut() {
                    *c = None;
                }
            }
        }

        // reset the texture locks
        {
            shared_texture_data.reset_locks(camera_map);
        }
    }

    /// writes to the texture handle
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn finish_rendering(&mut self, handle: &mut egui::TextureHandle) {
        // wait for all lines to finish
        {
            self.shared
                .shared_texture_data
                .try_read()
                .expect("no one should be writing")
                .block_until_finished();
        }

        // write to assert that we have exclusive access.
        // this can't be `try_write` bc workers read to check if they need to render.
        // TODO: we shouldn't need to block here once i've gotten rid of `RwLock`
        let mut shared_texture_data = self
            .shared
            .shared_texture_data
            .write()
            .expect("shared_texture poisoned");

        shared_texture_data.assert_finished_rendering();

        assert!(
            shared_texture_data.camera_map().is_some(),
            "i should change this in the future so that a worker resets the camera, but right now that's the main thread's job"
        );
        *shared_texture_data.camera_map_mut() = None;

        // update local_texture from the diff
        for (texture_line, diff_line) in self
            .local_texture
            .iter_mut()
            .zip(shared_texture_data.diff().iter())
        {
            for (texture_color, diff_color) in texture_line
                .iter_mut()
                .zip(diff_line.try_lock().unwrap().iter())
            {
                if let Some(diff_color) = diff_color {
                    *texture_color = *diff_color;
                }
            }
        }

        // write to the texture handle
        {
            let width = shared_texture_data.width();
            let height = shared_texture_data.height();
            let colors = if !shared_texture_data.needs_full_redraw
                && DRAW_COLOR_DIFF.load(Ordering::Relaxed)
            {
                // map is annoying bc of the mutex,
                // so don't bother with iterators.
                let mut ret = Vec::with_capacity(width * height);
                for (texture_line, diff_line) in self
                    .local_texture
                    .iter_mut()
                    .zip(shared_texture_data.diff().iter())
                {
                    for (texture_color, diff_color) in texture_line
                        .iter_mut()
                        .zip(diff_line.try_lock().unwrap().iter())
                    {
                        ret.push(if diff_color.is_none() {
                            *texture_color
                        } else {
                            texture_color.lerp_to_gamma(DRAW_COLOR_DIFF_COLOR, 0.3)
                        });
                    }
                }
                ret
            } else {
                self.local_texture
                    .iter()
                    .flat_map(|line| line.clone().into_vec())
                    .collect()
            };

            set_texture(handle, [width, height], colors);
        }
    }
}

pub(crate) use rayon_fractal::*;
mod rayon_fractal {
    use rayon::prelude::*;

    use crate::{complex::fixed::*, sample};

    use super::*;

    #[cfg_attr(feature = "profiling", inline(never))]
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
                    sample::quadratic_map::<false>(&mut None, (z0_real, z0_imag), c).color()
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

    #[cfg_attr(feature = "profiling", inline(never))]
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

#[cfg_attr(feature = "profiling", inline(never))]
fn set_texture(handle: &mut egui::TextureHandle, size: [usize; 2], colors: Vec<Color32>) {
    handle.set(
        egui::ColorImage::new(size, colors),
        egui::TextureOptions::NEAREST,
    );
}
