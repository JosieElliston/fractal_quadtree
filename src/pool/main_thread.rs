use std::{
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

use atomic::Atomic;
use egui::{Color32, emath::GuiRounding};
use itertools::Itertools;

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

        let camera_map = shared_texture_data.camera_map_mut().take().unwrap();

        // assert!(
        //     shared_texture_data.camera_map().is_some(),
        //     "i should change this in the future so that a worker resets the camera, but right now that's the main thread's job"
        // );
        // *shared_texture_data.camera_map_mut() = None;

        // update local_texture from the diff
        for (texture_line, diff_line) in self
            .local_texture
            .iter_mut()
            .zip_eq(shared_texture_data.diff().iter())
        {
            for (texture_color, diff_color) in texture_line
                .iter_mut()
                .zip_eq(diff_line.try_lock().unwrap().iter())
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
            let texture: Vec<Box<[Color32]>> = if !shared_texture_data.needs_full_redraw
                && DRAW_COLOR_DIFF.load(Ordering::Relaxed)
            {
                // // map is annoying bc of the mutex,
                // // so don't bother with iterators.
                // let mut ret = Vec::with_capacity(width * height);
                // for (texture_line, diff_line) in self
                //     .local_texture
                //     .iter_mut()
                //     .zip_eq(shared_texture_data.diff().iter())
                // {
                //     for (texture_color, diff_color) in texture_line
                //         .iter_mut()
                //         .zip_eq(diff_line.try_lock().unwrap().iter())
                //     {
                //         ret.push(if diff_color.is_none() {
                //             *texture_color
                //         } else {
                //             texture_color.lerp_to_gamma(DRAW_COLOR_DIFF_COLOR, 0.3)
                //         });
                //     }
                // }
                // ret

                // if the diff is `Some`, debug color the pixel blended towards `DRAW_COLOR_DIFF_COLOR`.
                self.local_texture
                    .iter_mut()
                    .zip_eq(shared_texture_data.diff().iter())
                    .map(|(texture_line, diff_line)| {
                        texture_line
                            .iter_mut()
                            .zip_eq(diff_line.try_lock().unwrap().iter())
                            .map(|(texture_color, diff_color)| match diff_color {
                                None => *texture_color,
                                Some(_) => texture_color.lerp_to_gamma(DRAW_COLOR_DIFF_COLOR, 0.3),
                            })
                            .collect()
                    })
                    .collect()
            } else {
                // self.local_texture
                //     .iter()
                //     .flat_map(|line| line.clone().into_vec())
                //     .collect()
                self.local_texture.clone()
            };
            set_texture(handle, &camera_map, &texture);
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
        let colors: Vec<Box<[Color32]>> = camera_map
            .pixels()
            .collect_vec()
            .into_par_iter()
            .map(|line| {
                line.map(|(_rect, pixel)| {
                    if let Some(pixel) = pixel {
                        let c = pixel.mid();
                        sample::quadratic_map::<false>(&mut None, (z0_real, z0_imag), c).color()
                    } else {
                        Color32::MAGENTA
                    }
                })
                .collect()
            })
            .collect();
        set_texture(handle, camera_map, &colors);
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn render_color(handle: &mut egui::TextureHandle, camera_map: &CameraMap) {
        let colors: Vec<Box<[Color32]>> = camera_map
            .pixels()
            .collect_vec()
            .into_par_iter()
            .map(|line| {
                line.map(|(_rect, pixel)| {
                    if pixel.is_some() {
                        Color32::BLACK
                    } else {
                        Color32::MAGENTA
                    }
                })
                .collect()
            })
            .collect();
        set_texture(handle, camera_map, &colors);
    }
}

/// the egui texture gets set at full resolution, without stride.
/// the strided_texture is the internal representation with stride.
/// in the strided_texture, the pixels at the end of a row/col
/// may be only partially contained inside the egui texture.
//
// TODO: it's weird and bouncy when resizing
// probably bc this is using a camera_map.rect that's a frame old.
//
// TODO: when resizing, there's a discontinuity when crossing a pixel boundary,
// and it seems like all the pixels are getting their `Fixed` position changed,
// and this is actually a sr-latch: going back and forth across
// one boundary has different behavior from two.
// this is probably related to how `camera_map.rect` is smaller than
// the disjoint union of `camera_map.pixels.rect`,
// and i should use that union for some stuff instead.
#[cfg_attr(feature = "profiling", inline(never))]
fn set_texture(
    handle: &mut egui::TextureHandle,
    camera_map: &CameraMap,
    strided_texture: &[Box<[Color32]>],
) {
    debug_assert_eq!(camera_map.pixels_height(), strided_texture.len());
    debug_assert_eq!(camera_map.pixels_width(), strided_texture[0].len());

    // TODO: maybe camera_map should round the rect to physical pixels?
    // camera_map.rect.round_to_pixels();
    // TODO: debug print the rect to see if it's aligned

    // TODO: maybe the egui texture can be 2x or 4x for the weird good antialiasing that gives.
    let subpixels = 2;

    // let height = subpixels * camera_map.rect().height().round() as usize;
    // let width = subpixels * camera_map.rect().width().round() as usize;

    let (width, height) = {
        let min_x = (subpixels as f32 * camera_map.rect().min.x).round() as usize;
        let min_y = (subpixels as f32 * camera_map.rect().min.y).round() as usize;
        let max_x = (subpixels as f32 * camera_map.rect().max.x).round() as usize;
        let max_y = (subpixels as f32 * camera_map.rect().max.y).round() as usize;
        (max_x - min_x, max_y - min_y)
    };

    // TODO: this is probably upside-down

    let colors = {
        let mut colors = vec![vec![None; width].into_boxed_slice(); height].into_boxed_slice();

        for (row0, line) in camera_map.pixels().enumerate() {
            for (col0, (rect, _pixel)) in line.enumerate() {
                let color = strided_texture[row0][col0];
                let min_x = (subpixels as f32 * rect.min.x).round() as usize;
                let min_y = (subpixels as f32 * rect.min.y).round() as usize;
                let max_x = (subpixels as f32 * rect.max.x).round() as usize;
                let max_y = (subpixels as f32 * rect.max.y).round() as usize;
                for row1 in min_y..max_y {
                    if row1 >= height {
                        continue;
                    }
                    for col1 in min_x..max_x {
                        if col1 >= width {
                            continue;
                        }
                        // dbg!(colors);
                        // dbg!(row0, col0);
                        // dbg!(row1, col1);
                        debug_assert!(
                            colors[row1][col1].is_none(),
                            "should only write to each color once"
                        );
                        colors[row1][col1] = Some(color);
                    }
                }
            }
        }

        // if all the pixels are `Some`,
        // all the egui pixels should have been filled.
        // #[cfg(false)]
        #[cfg(debug_assertions)]
        if camera_map
            .pixels()
            .all(|line| line.into_iter().all(|(_rect, pixel)| pixel.is_some()))
        {
            for (row, line) in colors.iter().enumerate() {
                for (col, color) in line.iter().enumerate() {
                    // debug_assert!(color.is_some(), "row: {row}, col: {col}");
                    if color.is_none() {
                        dbg!(width, height);
                        dbg!(colors[0].len(), colors.len());
                        dbg!(row, col);
                        panic!();
                    }
                }
            }
        }

        colors
            .into_iter()
            .flat_map(|line| {
                line.into_iter()
                    .map(|color| color.unwrap_or(Color32::MAGENTA))
            })
            .collect()
    };

    handle.set(
        egui::ColorImage::new([width, height], colors),
        egui::TextureOptions::NEAREST,
    );
}
