use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, TryLockError, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

use atomic::Atomic;
use egui::Color32;

use crate::{
    complex::{Pixel, fixed::*},
    sample,
    tree::{
        TreeLocal,
        alloc::{AllocLocal, BlockHandle},
    },
};

use super::{ReclaimMoment, RenderMoment, shared::Shared, timer::MultiTimer};

/// owned by the worker thread
pub(super) struct Worker {
    tree_local: TreeLocal,
    alloc_local: AllocLocal,
    shared: Arc<Shared>,
    /// `usize` bc [`thread::available_parallelism`] returns a `usize`.
    thread_i: usize,
    /// our belief of the current moment.
    /// we could instead have only `shared_now`,
    /// but then you need an extra atomic load to check if our belief is up to date.
    local_reclaim_now: ReclaimMoment,
    /// tell the main thread about our belief of the current moment.
    shared_reclaim_now: Arc<Atomic<ReclaimMoment>>,
    /// points we need to find the color of.
    /// note that the len should be <= 4.
    // TODO: rename
    to_be_sampled: Vec<(Real, Imag)>,
    to_be_inserted: Option<((Real, Imag), Color32)>,
    /// moment is when they were retired,
    /// not when they should be reclaimed.
    /// alias: `to_be_reclaimed`,
    /// but this is sufficiently funnier that the unclarity is worth is.
    nursing_home: VecDeque<(ReclaimMoment, BlockHandle)>,
    timer: TimerData,
}
impl Worker {
    pub(super) fn new(
        shared: Arc<Shared>,
        thread_i: usize,
        shared_reclaim_now: Arc<Atomic<ReclaimMoment>>,
        shared_timer: Arc<Mutex<MultiTimer>>,
    ) -> Self {
        Self {
            tree_local: TreeLocal::default(),
            alloc_local: AllocLocal::default(),
            shared,
            thread_i,
            local_reclaim_now: shared_reclaim_now.load(Ordering::SeqCst),
            shared_reclaim_now,
            to_be_sampled: Vec::with_capacity(4),
            to_be_inserted: None,
            nursing_home: VecDeque::new(),
            timer: TimerData::new(shared_timer),
        }
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    fn update_reclaim_now(&mut self) {
        let now = self.shared.reclaim_now.load(Ordering::SeqCst);
        if now == self.local_reclaim_now {
            return;
        }
        debug_assert_eq!(
            now,
            self.local_reclaim_now + 1,
            "we should only ever be one behind"
        );
        self.local_reclaim_now = now;
        self.shared_reclaim_now.store(now, Ordering::SeqCst);
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    fn try_render(&mut self) -> Result<(), &'static str> {
        // pub(crate) struct DrawTimer {
        //     pub(crate) would_block: Timer,
        //     pub(crate) no_camera_map: Timer,
        // }
        // let start = Instant::now();
        let shared_texture = match self.shared.shared_texture_data.try_read() {
            Ok(shared_texture) => shared_texture,
            Err(TryLockError::Poisoned(_)) => panic!("shared_texture poisoned"),
            Err(TryLockError::WouldBlock) => {
                // the main thread is rendering.
                // self.timer.local.draw_would_block.insert(start.elapsed());
                return Err("shared_texture would block");
            }
        };
        // shared_texture.camera_map() is `None` if the main thread has started but not finished rendering
        let Some(camera_map) = shared_texture.camera_map().as_ref() else {
            // self.timer.local.draw_no_camera_map.insert(start.elapsed());
            return Err("shared_texture.camera_map is None");
        };

        // let prev_frame_start = self.shared.now.load(Ordering::SeqCst) - 1;
        // let prev_frame_start = shared_texture.prev_frame_start;

        let texture_lock_begin = shared_texture.begin_count();
        if texture_lock_begin.load(Ordering::Relaxed) >= camera_map.pixels_height() {
            return Err("texture_lock_begin >= camera_map.pixels_height()");
        }
        let row = texture_lock_begin.fetch_add(1, Ordering::Acquire);
        if row >= camera_map.pixels_height() {
            return Err("row >= camera_map.pixels_height()");
        }

        // TODO: we don't need this mutex, replace with `UnsafeCell`
        let mut l = shared_texture.diff()[row]
            .try_lock()
            .expect("we just locked it");
        {
            let prev_frame_start = if shared_texture.needs_full_redraw {
                RenderMoment::MIN
            } else {
                self.shared.render_now.load(Ordering::SeqCst) - 1
            };

            // // TODO: do more of this, perhaps bisection bc that's easier than real spacial stuff
            // let line_needs_redraw = shared_texture.needs_full_redraw
            //     || 'line_needs_redraw: {
            //         let Some(first_pixel) = camera_map.pixel_at(row, 0) else {
            //             break 'line_needs_redraw true;
            //         };
            //         let Some(last_pixel) = camera_map.pixel_at(row, camera_map.pixels_width() - 1)
            //         else {
            //             break 'line_needs_redraw true;
            //         };
            //         debug_assert_eq!(first_pixel.imag_mid(), last_pixel.imag_mid());
            //         let imag = first_pixel.imag_mid();
            //         let real_lo = first_pixel.real_mid();
            //         let real_hi = last_pixel.real_mid();
            //         debug_assert_ne!(
            //             prev_frame_start,
            //             RenderMoment::MIN,
            //             "we should short circuit earlier"
            //         );
            //         self.shared.tree.any_on_line_needs_redraw(
            //             &mut self.tree_local,
            //             real_lo,
            //             real_hi,
            //             imag,
            //             prev_frame_start,
            //         )
            //     };

            // if line_needs_redraw {
            //     for ((_rect, pixel), target) in
            //         camera_map.pixels().nth(row).unwrap().zip(l.iter_mut())
            //     {
            //         *target = match pixel {
            //             Some(pixel) => self.shared.tree.color_of_pixel(
            //                 &mut self.tree_local,
            //                 pixel,
            //                 prev_frame_start,
            //             ),
            //             None => {
            //                 // probably we're zoomed in too far
            //                 Some(Color32::MAGENTA)
            //             }
            //         };
            //     }
            // }

            #[cfg(false)]
            {
                let root_timestamp = self.shared.tree.root_timestamp();
                dbg!(prev_frame_start, root_timestamp);
            }

            let pixel_colors: Box<[(Pixel, &mut Option<Color32>)]> = camera_map
                .pixels()
                .nth(row)
                .unwrap()
                .zip(l.iter_mut())
                .filter_map(|((_rect, pixel), color)| match pixel {
                    Some(pixel) => Some((pixel, color)),
                    None => {
                        // probably we're zoomed in too far.
                        *color = Some(Color32::MAGENTA);
                        None
                    }
                })
                .collect();
            let (pixels, colors) = pixel_colors
                .into_iter()
                .unzip::<Pixel, &mut Option<Color32>, Vec<Pixel>, Vec<&mut Option<Color32>>>();
            let pixels = pixels.into_boxed_slice();
            let mut colors = colors.into_boxed_slice();

            let imag_mid = pixels[0].imag_mid();
            assert!(
                pixels.iter().all(|pixel| pixel.imag_mid() == imag_mid),
                "all pixels in a row should have the same imag"
            );

            let pixel_real_lo_his: Box<[(Real, Real)]> = pixels
                .iter()
                .map(|pixel| (pixel.real_mid(), pixel.real_mid()))
                .collect();

            self.shared.tree.color_of_line(
                &mut self.tree_local,
                prev_frame_start,
                &pixel_real_lo_his,
                imag_mid,
                &mut colors,
            );
        }
        debug_assert!(
            shared_texture.finish_count().load(Ordering::SeqCst) < camera_map.pixels_height()
        );
        shared_texture
            .finish_count()
            .fetch_add(1, Ordering::Release);
        Ok(())
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    fn try_insert(&mut self) -> Result<(), &'static str> {
        let Some(((real, imag), color)) = self.to_be_inserted.take() else {
            return Err("nothing to insert");
        };
        self.shared.tree.insert(
            &mut self.tree_local,
            (real, imag),
            color,
            self.shared.render_now.load(Ordering::SeqCst),
        );
        Ok(())
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    fn try_sample(&mut self) -> Result<(), &'static str> {
        assert!(self.to_be_inserted.is_none());

        let Some((real, imag)) = self.to_be_sampled.pop() else {
            return Err("nothing in sample queue");
        };

        let color = sample::metabrot_sample::<false>(&mut None, (real, imag)).color();
        self.shared.sample_counter.fetch_add(1, Ordering::Relaxed);
        self.to_be_inserted = Some(((real, imag), color));
        Ok(())
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    fn try_free(&mut self) -> Result<(), &'static str> {
        // if !self.nursing_home.is_empty() {
        //     dbg!(&self.nursing_home.len());
        // }

        debug_assert!(
            self.nursing_home
                .iter()
                .zip(self.nursing_home.iter().skip(1))
                .all(|((moment_a, _), (moment_b, _))| moment_a <= moment_b),
            "nursing_home should have increasing timestamps"
        );

        #[cfg(debug_assertions)]
        if let Some((moment, _)) = self.nursing_home.front() {
            debug_assert!(
                *moment <= self.local_reclaim_now,
                "local_reclaim_now should be increasing"
            );
        }

        // + 3 instead of + 2 because the reclaim_moment is from the start of retire, rather than the end
        let Some((_, block_handle)) = self
            .nursing_home
            .pop_front_if(|(reclaim_moment, _)| *reclaim_moment + 3 <= self.local_reclaim_now)
        else {
            return Err("nobody old enough");
        };
        unsafe {
            self.shared
                .tree
                .free(&mut self.tree_local, &mut self.alloc_local, block_handle);
        }
        Ok(())
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    fn try_retire(&mut self) -> Result<(), &'static str> {
        let window = match self.shared.reclaim_window.try_read() {
            Ok(window) => match window.as_ref() {
                Some(window) => *window,
                None => {
                    return Err("reclaim_window is None");
                }
            },
            Err(TryLockError::Poisoned(_)) => panic!("window poisoned"),
            Err(TryLockError::WouldBlock) => {
                // the main thread is updating the window
                return Err("reclaim_window would block");
            }
        };
        let Some(block_handle) = self.shared.tree.retire(
            &mut self.tree_local,
            window,
            self.shared.render_now.load(Ordering::SeqCst),
        ) else {
            return Err("nothing to retire");
        };
        self.shared.reclaim_counter.fetch_add(1, Ordering::Relaxed);
        self.nursing_home
            .push_back((self.local_reclaim_now, block_handle));
        Ok(())
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    fn try_refine(&mut self) -> Result<(), &'static str> {
        let sample_window = match self.shared.sample_window.try_read() {
            Ok(window) => match window.as_ref() {
                Some(window) => *window,
                None => {
                    return Err("sample_window is None");
                }
            },
            Err(TryLockError::Poisoned(_)) => panic!("sample_window poisoned"),
            Err(TryLockError::WouldBlock) => {
                // the main thread is updating the window.
                return Err("sample_window would block");
            }
        };

        let reclaim_window = match self.shared.reclaim_window.try_read() {
            Ok(window) => *window,
            Err(TryLockError::Poisoned(_)) => panic!("reclaim_window poisoned"),
            Err(TryLockError::WouldBlock) => {
                // the main thread is updating the window.
                None
            }
        };

        debug_assert!(self.to_be_sampled.is_empty());
        if let Some(handles) = self.shared.tree.refine(
            &mut self.tree_local,
            &mut self.alloc_local,
            sample_window,
            reclaim_window,
            self.shared.render_now.load(Ordering::SeqCst),
        ) {
            self.to_be_sampled.extend(handles);
            Ok(())
        } else {
            Err("nothing to refine")
        }
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(super) fn run(mut self) {
        loop {
            if self.shared.kill.load(Ordering::Relaxed) {
                break;
            }

            // must call this ever time,
            // not just when we want to retire,
            // bc draw uses the cached value too.
            self.update_reclaim_now();

            // update the shared timer rarely for performance
            if self.timer.last_sent.elapsed() >= TimerData::UPDATE_INTERVAL {
                self.timer.send();
            }

            // we need get an ack from each thread that they recognize the current moment for the correctness of reclaim.
            // workers read main thread's now,
            // if it's incremented compared to the local cache,
            // they increment the main thread's knowledge of them.

            // TODO: for debugging, do these in a random order.
            // TODO: put functions and ui in a consistent order.

            // rendering is highest priority
            // followed by inserting
            // followed by sampling
            // followed by freeing
            // followed by retiring
            // followed by refining

            {
                let start = Instant::now();
                match self.try_render() {
                    Ok(_) => {
                        self.timer.local.render_ok.insert(start.elapsed());
                        continue;
                    }
                    Err(_) => {
                        self.timer.local.render_err.insert(start.elapsed());
                    }
                }
            }

            {
                let start = Instant::now();
                match self.try_insert() {
                    Ok(_) => {
                        self.timer.local.insert_ok.insert(start.elapsed());
                        continue;
                    }
                    Err(_) => {
                        self.timer.local.insert_err.insert(start.elapsed());
                    }
                }
            }

            {
                let start = Instant::now();
                match self.try_sample() {
                    Ok(_) => {
                        self.timer.local.sample_ok.insert(start.elapsed());
                        continue;
                    }
                    Err(_) => {
                        self.timer.local.sample_err.insert(start.elapsed());
                    }
                }
            }

            {
                let start = Instant::now();
                match self.try_free() {
                    Ok(_) => {
                        self.timer.local.free_ok.insert(start.elapsed());
                        continue;
                    }
                    Err(_) => {
                        self.timer.local.free_err.insert(start.elapsed());
                    }
                }
            }

            {
                let start = Instant::now();
                match self.try_retire() {
                    Ok(_) => {
                        self.timer.local.retire_ok.insert(start.elapsed());
                        continue;
                    }
                    Err(_) => {
                        self.timer.local.retire_err.insert(start.elapsed());
                    }
                }
            }

            {
                let start = Instant::now();
                match self.try_refine() {
                    Ok(_) => {
                        self.timer.local.refine_ok.insert(start.elapsed());
                        continue;
                    }
                    Err(_) => {
                        self.timer.local.refine_err.insert(start.elapsed());
                    }
                }
            }

            {
                let start = Instant::now();
                // thread::yield_now();
                // weird workaround, but it fixing freezing
                // for when pausing sampling or the fractal is outside the window.
                // except it doesn't work in release mode.
                // TODO: std::hint::spin_loop()
                thread::sleep(Duration::from_millis(10));

                self.timer.local.idle.insert(start.elapsed());
            }
        }
    }
}

struct TimerData {
    /// accumulate updates here.
    local: MultiTimer,
    /// use batched updates.
    shared: Arc<Mutex<MultiTimer>>,
    /// when have we last sent an update to the main thread?
    last_sent: Instant,
}
impl TimerData {
    const UPDATE_INTERVAL: Duration = Duration::from_millis(5);

    fn new(shared: Arc<Mutex<MultiTimer>>) -> Self {
        Self {
            local: MultiTimer::default(),
            shared,
            last_sent: Instant::now(),
        }
    }

    fn send(&mut self) {
        let mut guard = self.shared.lock().expect("shared_timer poisoned");
        *guard += self.local;
        self.local.reset();
        self.last_sent = Instant::now();
    }
}
