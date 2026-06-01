use std::sync::{
    Mutex, RwLock,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

use atomic::Atomic;
use egui::Color32;

use crate::{
    complex::{CameraMap, Window},
    tree::Tree,
};

use super::{ReclaimMoment, RenderMoment};

/// stuff shared between the main and worker threads.
pub(crate) struct Shared {
    pub(crate) tree: Tree,

    /// the main thread's current moment.
    /// we can skip drawing nodes that haven't been updated since the start of the previous frame.
    /// it might happen that we will have correctly drawn pixels that got sampled during frame drawing,
    /// and then redraw those pixels, but this is rare and not too bad.
    // /// in the sample functions, we pass `frame_start + 1`.
    /// monotonically increasing.
    /// only ever updated by the main thread.
    /// we have that `frame_start < sample_time`.
    pub(super) render_now: Atomic<RenderMoment>,

    /// tell the worker threads the current reclaim moment.
    pub(super) reclaim_now: Atomic<ReclaimMoment>,

    /// the `Window` in which we're reclaiming.
    /// `None` iff reclaiming is disabled.
    /// note that this is similar to shared_texture.camera_map.window,
    /// it's just that reclaiming, sampling, and rendering are decoupled (eg any one may be disabled).
    /// TODO: this could be an atomic option instead of a `RwLock`.
    pub(super) reclaim_window: RwLock<Option<Window>>,

    /// how many nodes were reclaimed since we last cleared this?
    /// for debugging / UX, not needed for the main algorithm.
    pub(super) reclaim_counter: AtomicU64,

    /// the `Window` in which we're sampling.
    /// `None` iff sampling is disabled.
    /// note that this is similar to shared_texture.camera_map.window,
    /// it's just that reclaiming, sampling, and rendering are decoupled (eg any one may be disabled).
    /// TODO: this could be an atomic option instead of a `RwLock`.
    pub(super) sample_window: RwLock<Option<Window>>,

    /// how many samples were taken since we last cleared this?
    /// for debugging / UX, not needed for the main algorithm.
    pub(super) sample_counter: AtomicU64,

    // TODO: make this not `RwLock`.
    pub(super) shared_texture_data: RwLock<SharedTextureData>,

    /// set by the main thread to ask worker threads to exit.
    pub(super) kill: AtomicBool,
}

/// the main thread calls [`RwLock::write`] to resize the buffers.
/// workers never call [`RwLock::write`].
/// probably should never call read/write,
/// instead only call `try_read` or `try_write`,
/// bc those invariants are maintained manually.
pub(super) struct SharedTextureData {
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

    // /// these are set when a line begins rendering.
    // texture_lock_begin: Vec<AtomicBool>,
    // /// these are set when a line finishes rendering.
    // texture_lock_finish: Vec<AtomicBool>,
    /// these are incremented when a worker acquires a line to render.
    /// may be greater than the height.
    begin_count: AtomicUsize,

    /// these are incremented when a worker finishes rendering a line.
    /// must not be greater than the height.
    finish_count: AtomicUsize,

    /// the diff the main thread should apply to its local texture.
    /// a color is `None` if the color hasn't changed.
    /// should never call `lock`, only `try_lock`.
    // TODO: with the texture locks, maybe this doesn't need a `Mutex`, just an `UnsafeCell`.
    diff: Vec<Mutex<Box<[Option<Color32>]>>>,
}
impl Default for SharedTextureData {
    fn default() -> Self {
        Self {
            needs_full_redraw: false,
            camera_map: None,
            begin_count: AtomicUsize::new(0),
            finish_count: AtomicUsize::new(0),
            diff: Vec::new(),
        }
    }
}
impl SharedTextureData {
    pub(super) fn width(&self) -> usize {
        let width = self
            .diff
            .first()
            .map(|line| line.try_lock().expect("no one should be writing").len())
            .unwrap_or(0);
        debug_assert!(
            self.diff
                .iter()
                .all(|line| line.try_lock().expect("no one should be writing").len() == width)
        );
        width
    }
    pub(super) fn height(&self) -> usize {
        let height = self.diff.len();
        // debug_assert_eq!(self.texture_lock_begin.len(), height);
        // debug_assert_eq!(self.texture_lock_finish.len(), height);
        height
    }

    pub(super) fn camera_map(&self) -> &Option<CameraMap> {
        &self.camera_map
    }
    pub(super) fn camera_map_mut(&mut self) -> &mut Option<CameraMap> {
        &mut self.camera_map
    }

    // TODO: do i really need getters for these?
    pub(super) fn begin_count(&self) -> &AtomicUsize {
        &self.begin_count
    }
    pub(super) fn finish_count(&self) -> &AtomicUsize {
        &self.finish_count
    }
    pub(super) fn diff(&self) -> &Vec<Mutex<Box<[Option<Color32>]>>> {
        &self.diff
    }

    pub(super) fn assert_finished_rendering(&self) {
        assert!(
            self.begin_count.load(Ordering::SeqCst) >= self.diff.len(),
            "texture_lock_begin not all started"
        );
        assert_eq!(
            self.finish_count.load(Ordering::SeqCst),
            self.diff.len(),
            "texture_lock_finish not all ended"
        );
    }

    /// also sets `needs_full_redraw` to `true`.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(super) fn resize(&mut self, width: usize, height: usize) {
        self.needs_full_redraw = true;

        self.begin_count.store(height, Ordering::SeqCst);
        self.finish_count.store(height, Ordering::SeqCst);

        self.diff.clear();
        self.diff
            .resize_with(height, || Mutex::new(vec![None; width].into_boxed_slice()));
    }

    /// for when we want to start rendering.
    /// checks that all the locks were set.
    /// should be called after resize.
    /// checks that self.camera_map is None
    // /// &mut self isn't really necessary, but it's semantically nice that only the main thread can reset the locks.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(super) fn reset_locks(&mut self, camera_map: &CameraMap) {
        assert!(self.camera_map.is_none(), "camera_map wasn't None");
        // debug_assert!(
        //     self.texture_lock_begin
        //         .iter()
        //         .all(|lock| lock.load(Ordering::SeqCst)),
        //     "texture_lock_begin not all true"
        // );
        // debug_assert!(
        //     self.texture_lock_finish
        //         .iter()
        //         .all(|lock| lock.load(Ordering::SeqCst)),
        //     "texture_lock_finish not all true"
        // );
        #[cfg(debug_assertions)]
        self.assert_finished_rendering();

        // // it's important to reset finish before begin
        // // at least if we aren't using `camera_map` as a lock
        // for lock in self.texture_lock_finish.iter() {
        //     lock.store(false, Ordering::SeqCst);
        // }
        // for lock in self.texture_lock_begin.iter() {
        //     lock.store(false, Ordering::SeqCst);
        // }
        self.finish_count.store(0, Ordering::SeqCst);
        self.begin_count.store(0, Ordering::SeqCst);

        self.camera_map = Some(camera_map.clone());
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(super) fn block_until_finished(&self) {
        // while self.camera_map.is_some() {
        //     std::thread::yield_now();
        // }

        // TODO: std::hint::spin_loop()
        while self.finish_count.load(Ordering::SeqCst) < self.diff.len() {
            std::hint::spin_loop();
        }

        self.assert_finished_rendering();
    }

    // /// this allocates btw.
    // #[cfg_attr(feature = "profiling", inline(never))]
    // pub(super) fn set_texture(&self, handle: &mut egui::TextureHandle) {
    //     assert!(self.camera_map.is_none(), "camera_map wan't reset");
    //     // debug_assert!(
    //     //     self.texture_lock_begin
    //     //         .iter()
    //     //         .all(|lock| lock.load(Ordering::SeqCst)),
    //     //     "texture_lock_begin not all true"
    //     // );
    //     // debug_assert!(
    //     //     self.texture_lock_finish
    //     //         .iter()
    //     //         .all(|lock| lock.load(Ordering::SeqCst)),
    //     //     "texture_lock_finish not all true"
    //     // );
    //     debug_assert!(
    //         self.begin_count.load(Ordering::SeqCst) >= self.diff.len(),
    //         "texture_lock_begin not all started"
    //     );
    //     debug_assert_eq!(
    //         self.finish_count.load(Ordering::SeqCst),
    //         self.diff.len(),
    //         "texture_lock_finish not all ended"
    //     );

    //     let size = [self.width(), self.height()];
    //     let colors = self
    //         .diff
    //         .iter()
    //         .flat_map(|line| line.try_lock().expect("rendering should be done").clone())
    //         .collect();
    //     set_texture(handle, size, colors);
    // }
}
