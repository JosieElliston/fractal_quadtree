use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU16, Ordering},
};

use atomic::Atomic;

use crate::pool::RenderMoment;

use super::{
    Domain, Rgb,
    alloc::{BlockHandle, NodeHandle},
};

// TODO: define "eventually"
#[repr(C, align(64))]
#[derive(Debug)]
pub(super) struct Node {
    /// the domain of the node.
    /// our four children (if any) are an axis-aligned subdivision of this domain into four equal squares.
    /// all descendants of the node have a domain contained in this domain.
    /// `color` is sampled at the center of `dom`.
    /// `dom` doesn't need to be atomic because it's never modified after being shown to the other threads.
    /// also `Domain` too big to fit in a u64 (or u128),
    /// so `Atomic` falls back to a global lock array, which is really slow.
    // TODO: i think you can derive the radius from the center.
    // note that the domains of the leafs are all disjoint and exactly cover the initial domain.
    // TODO: debug draw leaf's domain's outlines
    pub(super) dom: UnsafeCell<Domain>,

    /// the handle to child block, if it exists.
    /// note that the tree is full: nodes have either zero or four children.
    ///
    /// `None` iff we're a leaf.
    ///
    /// if `Some`, then we have four children, who are `children_handle.siblings()`.
    ///
    /// the induced shape of the tree is strongly consistent.
    pub(super) children_handle: Atomic<Option<BlockHandle>>,

    // TODO: maybe replace `Atomic` -> `UnsafeCell`
    // actually i think this has to be atomic bc it's modified when other threads might be looking at it.
    // we could avoid that by putting a tag on children_handle that
    // prevents other threads from following it, but whatever.
    // this would also prevent us from splitting uncolored nodes.
    /// this never participates in the synchronizes-with relation,
    /// we only need atomicity for load/store,
    /// and which are therefore `Relaxed`.
    pub(super) color: Atomic<Option<Rgb>>,

    // TODO: store depth instead of height? tho that's annoying for splitting.
    /// the distance to the closest descendant leaf.
    /// `0` if we're a leaf, else (eventually) `1 + min(c.min_height for c in children)`.
    /// this is used in `refine` to find the shallowest leafs.
    /// this is updated in `refine` and `retire`.
    pub(super) min_height: AtomicU16,

    /// the distance to the farthest descendant leaf.
    /// `0` if we're a leaf, else (eventually) `1 + max(c.max_height for c in children)`.
    /// this is used in `retire` to find the deepest nodes.
    /// this is updated in `refine` and `retire`.
    pub(super) max_height: AtomicU16,

    /// the timestamp of the last update to color of this node or any descendant.
    /// this is monotonically increasing (over time, not as you traverse the tree).
    ///
    /// (eventually) `timestamp >= max(c.timestamp for c in children)`.
    ///
    /// note that this is not (eventually) `timestamp == max(c.timestamp for c in children)`,
    /// because we allow splitting uncolored nodes,
    /// which can cause a node to have a newer timestamp than its children.
    ///
    /// this is used in `color_of_pixel` and `any_on_line_needs_redraw`
    /// to prove that a node hasn't had its color changed since we last drew it.
    ///
    /// this is updated in `insert` and `retire`.
    pub(super) timestamp: Atomic<RenderMoment>,

    _pad: [u8; 20],
}
const _: () = assert!(size_of::<Node>() == 64);
const _: () = assert!(align_of::<Node>() == 64);
const _: () = assert!(Atomic::<Option<Rgb>>::is_lock_free());
const _: () = assert!(Atomic::<Option<NodeHandle>>::is_lock_free());
impl Node {
    pub(super) const UNINIT_HEIGHT: u16 = 0xABCD;

    pub(super) fn uninit() -> Self {
        Self {
            dom: UnsafeCell::new(Domain::uninit()),
            children_handle: Atomic::new(Some(BlockHandle::uninit())),
            color: Atomic::new(Some(Rgb::uninit())),
            min_height: AtomicU16::new(Self::UNINIT_HEIGHT),
            max_height: AtomicU16::new(Self::UNINIT_HEIGHT),
            timestamp: Atomic::new(RenderMoment::uninit()),
            _pad: Default::default(),
        }
    }

    /// deinitializes all the fields.
    /// a bit like `*self = Node::uninit()`.
    ///
    /// SAFETY: the caller must ensure that no other thread could be touching `self`.
    /// (bc of the call to [`Self::write_dom`]).
    #[cfg(feature = "deinit_nodes")]
    pub(super) unsafe fn deinit(&self) {
        unsafe {
            self.write_dom(Domain::uninit());
        }
        self.children_handle
            .store(Some(BlockHandle::uninit()), Ordering::Relaxed);
        self.color.store(Some(Rgb::uninit()), Ordering::Relaxed);
        self.min_height
            .store(Node::UNINIT_HEIGHT, Ordering::Relaxed);
        self.max_height
            .store(Node::UNINIT_HEIGHT, Ordering::Relaxed);
        self.timestamp
            .store(RenderMoment::uninit(), Ordering::Relaxed);
    }

    #[track_caller]
    #[cfg(feature = "deinit_nodes")]
    pub(super) fn assert_all_uninit(&self) {
        assert_eq!(
            unsafe { self.dom() },
            Domain::uninit(),
            "dom should be uninit"
        );
        assert_eq!(
            self.children_handle.load(Ordering::Relaxed),
            Some(BlockHandle::uninit()),
            "children_handle should be uninit"
        );
        assert_eq!(
            self.color.load(Ordering::Relaxed),
            Some(Rgb::uninit()),
            "color should be uninit"
        );
        assert_eq!(
            self.min_height.load(Ordering::Relaxed),
            Self::UNINIT_HEIGHT,
            "min_height should be uninit"
        );
        assert_eq!(
            self.max_height.load(Ordering::Relaxed),
            Self::UNINIT_HEIGHT,
            "max_height should be uninit"
        );
        assert_eq!(
            self.timestamp.load(Ordering::Relaxed),
            RenderMoment::uninit(),
            "timestamp should be uninit"
        );
    }

    #[track_caller]
    #[cfg(feature = "deinit_nodes")]
    pub(super) fn assert_not_any_uninit(&self) {
        assert_ne!(
            unsafe { self.dom() },
            Domain::uninit(),
            "dom should not be uninit"
        );
        assert_ne!(
            self.children_handle.load(Ordering::Relaxed),
            Some(BlockHandle::uninit()),
            "children_handle should not be uninit"
        );
        assert_ne!(
            self.color.load(Ordering::Relaxed),
            Some(Rgb::uninit()),
            "color should not be uninit"
        );
        assert_ne!(
            self.min_height.load(Ordering::Relaxed),
            Self::UNINIT_HEIGHT,
            "min_height should not be uninit"
        );
        assert_ne!(
            self.max_height.load(Ordering::Relaxed),
            Self::UNINIT_HEIGHT,
            "max_height should not be uninit"
        );
        assert_ne!(
            self.timestamp.load(Ordering::Relaxed),
            RenderMoment::uninit(),
            "timestamp should not be uninit"
        );
    }

    /// SAFETY: the caller must ensure that no thread could be writing to `dom`
    /// (tho that's mostly maintained by threads that want to write to `dom`).
    ///
    /// i could return a reference, but immediately reading the pointer is a bit safer.
    pub(super) unsafe fn dom(&self) -> Domain {
        unsafe { self.dom.get().read() }
    }

    /// SAFETY: the caller must ensure that no other thread could be touching `self`.
    ///
    /// tho maybe it's fine even if other threads can access `self`,
    /// like maybe we can't get partial writes bc `Domain`/`Node` is small enough?
    /// but that's still a data race,
    /// so `dom` would need to be a `Atomic<Domain>` where we only have `Relaxed` `load`/`store`s.
    pub(super) unsafe fn write_dom(&self, dom: Domain) {
        unsafe {
            self.dom.get().write(dom);
        }
    }

    /// `Ok(prev)` if we updated the timestamp.
    /// we guarantee that `prev < now`.
    ///
    /// `Err(cur)` if we didn't update the timestamp.
    /// we guarantee that `cur >= now`.
    /// this happens if the timestamp was already up to date,
    /// or another thread brought it up to date while we were trying to update it.
    ///
    /// in any case, the timestamp is guaranteed to be at least `now` after this function returns
    /// (this is basically a `fetch_max`).
    // TODO: weaken orderings.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(super) fn update_timestamp(&self, now: RenderMoment) -> Result<RenderMoment, RenderMoment> {
        let mut expected_cur = self.timestamp.load(Ordering::SeqCst);

        if expected_cur >= now {
            // this node doesn't need to be updated.
            // because timestamps are monotonically increasing as you go up the tree,
            // the ancestors also don't need to be updated.
            return Err(expected_cur);
        }

        // this is basically a fetch_max
        loop {
            match self.timestamp.compare_exchange_weak(
                expected_cur,
                now,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(prev) => {
                    assert_eq!(prev, expected_cur);
                    return Ok(prev);
                }
                Err(actual_cur) => {
                    assert!(
                        actual_cur >= expected_cur,
                        "timestamps are monotonically increasing"
                    );
                    // if actual_cur == expected_cur {
                    //     log!("spurious failure in update_timestamp");
                    // }
                    if actual_cur >= now {
                        // someone else updated the timestamp,
                        // and their timestamp is newer, so we should stop.
                        return Err(actual_cur);
                    } else {
                        // someone else updated the timestamp
                        // but their timestamp is older,
                        // or we experienced a spurious failure.
                        // in both cases, we should retry.
                        expected_cur = actual_cur;
                    }
                }
            }
        }
    }
}
