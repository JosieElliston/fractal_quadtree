use std::{
    cell::UnsafeCell,
    num::NonZeroU32,
    ptr::NonNull,
    sync::atomic::{AtomicU16, AtomicU32, AtomicUsize, Ordering},
};

use atomic::Atomic;
use egui::Color32;
use rand::Rng;

use crate::{
    complex::{Domain, Pixel, Window, fixed::*},
    log,
    sample::metabrot_sample,
};

// pub(crate) static ELAPSED_NANOS: std::sync::atomic::AtomicU64 =
//     std::sync::atomic::AtomicU64::new(0);
// pub(crate) static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// pub(crate) static PRUNED_ELAPSED: AtomicU64 = AtomicU64::new(0);
// pub(crate) static PRUNED_COUNTER: AtomicU64 = AtomicU64::new(0);
// pub(crate) static UNPRUNED_ELAPSED: AtomicU64 = AtomicU64::new(0);
// pub(crate) static UNPRUNED_COUNTER: AtomicU64 = AtomicU64::new(0);

/// hack to make this easy to add to the UI.
/// the min size of a node to reclaim is `window.real_rad() / RECLAIM_MAX_WIDTH`.
pub(crate) static RECLAIM_MAX_WIDTH: AtomicUsize = AtomicUsize::new(100);

#[repr(C, align(64))]
#[derive(Debug)]
struct Node {
    /// `dom` doesn't need to be atomic because it's never modified after being shown to the other threads.
    /// also `Domain` too big to fit in a u64 (or u128),
    /// so `Atomic` falls back to a global lock array, which is really slow.
    dom: UnsafeCell<Domain>,
    /// `parent` doesn't need to be atomic because it's never modified after being shown to the other threads.
    /// `None` iff we're the root.
    // TODO: remove maybe
    parent: UnsafeCell<Option<NodeHandle>>,
    /// `None` iff we're a leaf.
    /// if `Some`, then we have 4 children, who are `left_child.siblings()`.
    left_child: Atomic<Option<NodeHandle4>>,
    // TODO: maybe replace `Atomic` -> `UnsafeCell`
    // actually i think this has to be atomic bc it's modified when other threads might be looking at it.
    // we could avoid that by putting a tag on left_child that
    // prevents other threads from following it, but whatever.
    // this would also prevent us from splitting uncolored nodes.
    color: Atomic<Option<Rgb>>,
    /// distance to the closest descendant leaf.
    /// 0 if we're a leaf, else 1 + min(c.height for c in children).
    /// this is used in `refine` to find the shallowest leafs.
    /// btw, this could be a u16 or maybe even a u8.
    min_height: AtomicU16,
    /// distance to the farthest descendant leaf.
    /// 0 if we're a leaf, else 1 + max(c.height for c in children).
    /// this is used in `reclaim` to find the deepest nodes.
    max_height: AtomicU16,
    /// timestamp of the last update to this node or any of its descendants.
    /// "update" in the sense that we need to redraw.
    /// monotonically increasing.
    /// parents should have a timestamp of at least their children.
    /// and possibly parents should not have a timestamp strictly greater than any child.
    timestamp: Atomic<RenderMoment>,
    _pad: [u8; 8],
}
const _: () = assert!(size_of::<Node>() == 64);
const _: () = assert!(align_of::<Node>() == 64);
const _: () = assert!(Atomic::<Option<Rgb>>::is_lock_free());
const _: () = assert!(Atomic::<Option<NodeHandle>>::is_lock_free());
impl Node {
    fn uninit() -> Self {
        Self {
            dom: UnsafeCell::new(Domain::uninit()),
            parent: UnsafeCell::new(None),
            left_child: Atomic::new(None),
            color: Atomic::new(None),
            min_height: AtomicU16::new(0),
            max_height: AtomicU16::new(0),
            timestamp: Atomic::new(RenderMoment::uninit()),
            _pad: Default::default(),
        }
    }

    /// SAFETY: the caller probably should ensure that no one is writing to the node.
    /// i could return a reference, but immediately reading the pointer is a bit safer.
    unsafe fn dom(&self) -> Domain {
        unsafe { self.dom.get().read() }
    }
    /// SAFETY: the caller probably should ensure that no one is writing to the node.
    /// i could return a reference, but immediately reading the pointer is a bit safer.
    unsafe fn parent(&self) -> Option<NodeHandle> {
        unsafe { self.parent.get().read() }
    }

    /// SAFETY: the caller probably should ensure that we have exclusive access,
    /// tho maybe it's fine even without (like maybe we can't get partial writes bc it's small enough).
    unsafe fn write_dom(&self, dom: Domain) {
        unsafe {
            self.dom.get().write(dom);
        }
    }
    /// SAFETY: the caller probably should ensure that we have exclusive access,
    /// tho maybe it's fine even without (like maybe we can't get partial writes bc it's small enough).
    unsafe fn write_parent(&self, parent: Option<NodeHandle>) {
        unsafe {
            self.parent.get().write(parent);
        }
    }
}

#[derive(Debug)]
pub(crate) struct Tree {
    dom: Domain,
    alloc: Alloc,
    root: NodeHandle,
}
impl Tree {
    pub(crate) fn new(data: &mut ThreadData) -> Self {
        let dom = Domain::default();
        let color = metabrot_sample::<false>(&mut None, dom.mid())
            .color()
            .try_into()
            .unwrap();
        let alloc = Alloc::default();

        // we leave 3 nodes uninit
        // i don't want to deal with a main thread `TreadData`, so just leak
        let root_handle = alloc.alloc4(data).into();
        let root = alloc.get(root_handle);

        // this is UB because `Node` isn't inside an `UnsafeCell`
        // so we can't do the nice ~RAII thing
        // unsafe {
        //     (&raw const root as *mut Node).write(Node {
        //         dom,
        //         parent: None,
        //         left_child: Atomic::new(None),
        //         color: Atomic::new(Some(color)),
        //         height: AtomicU32::new(0),
        //         timestamp: Atomic::new(Moment::default()),
        //         _pad: Default::default(),
        //     });
        // }

        unsafe {
            root.write_dom(dom);
            root.write_parent(None);
        }
        root.left_child.store(None, Ordering::Relaxed);
        root.color.store(Some(color), Ordering::Relaxed);
        root.min_height.store(0, Ordering::Relaxed);
        root.max_height.store(0, Ordering::Relaxed);
        root.timestamp
            .store(RenderMoment::default(), Ordering::Relaxed);

        Self {
            dom,
            alloc,
            root: root_handle,
        }
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn node_count(&self, data: &mut ThreadData) -> usize {
        let mut count = 0;
        let stack = &mut data.vec_handle;
        stack.clear();

        stack.push(self.root);

        while let Some(handle) = stack.pop() {
            count += 1;
            if let Some(child_handle) = self.alloc.get(handle).left_child.load(Ordering::SeqCst) {
                stack.extend(child_handle.siblings());
            }
        }
        count
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn min_height(&self) -> u16 {
        self.alloc.get(self.root).min_height.load(Ordering::SeqCst)
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn max_height(&self) -> u16 {
        self.alloc.get(self.root).max_height.load(Ordering::SeqCst)
    }

    // pub(crate) fn mid_of_node_handle(&self, handle: NodeHandle) -> (Real, Imag) {
    //     unsafe { self.alloc.get(handle).dom().mid() }
    // }

    // /// maxes `handle` and all its ancestors timestamps with `now`.
    // #[cfg_attr(feature = "profiling", inline(never))]
    // fn update_ancestors_timestamp(&self, mut handle: NodeHandle, now: RenderMoment) {
    //     loop {
    //         let node = self.alloc.get(handle);
    //         // TODO: refactor so i can use fetch_max
    //         // node.timestamp.fetch_max(timestamp, Ordering::SeqCst);
    //         let _old = node
    //             .timestamp
    //             .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
    //                 if old >= now { None } else { Some(now) }
    //             });
    //         if let Some(parent) = unsafe { node.parent() } {
    //             handle = parent;
    //         } else {
    //             break;
    //         }
    //     }
    // }

    // /// probably should have that `left_child` is a leaf.
    // /// you can kinda merge this with `update_ancestors_timestamp`, but it's annoying
    // #[cfg_attr(feature = "profiling", inline(never))]
    // fn update_ancestors_height(&self, mut left_child: NodeHandle) {
    //     while let Some(parent_handle) = unsafe { self.alloc.get(left_child).parent() } {
    //         debug_assert_ne!(left_child, self.root);
    //         let height = left_child
    //             .siblings()
    //             .map(|child_handle| self.alloc.get(child_handle).height.load(Ordering::SeqCst))
    //             .iter()
    //             .min()
    //             .unwrap()
    //             + 1;

    //         let parent = self.alloc.get(parent_handle);
    //         let old_height = parent.height.load(Ordering::SeqCst);

    //         // see comment below for why this is >= and not ==
    //         if old_height >= height {
    //             return;
    //         }

    //         // we don't need to max, and can just store
    //         // it's possible this lowers it (eg if we sleep between computing height and storing),
    //         // but we don't actually rely on height being monotonically increasing,
    //         // and it'll get fixed in the future.
    //         parent.height.store(height, Ordering::SeqCst);

    //         // if i ever move the root from being leftmost in its group,
    //         // this will be UB.
    //         left_child = parent_handle.left_sibling();
    //     }
    // }

    /// erase the child pointer and return the children.
    /// we can't deinit the children's fields at this time
    /// because other threads can still be looking at the children,
    /// and we need their child pointers later.
    #[cfg_attr(feature = "profiling", inline(never))]
    fn retire_children_of_node(&self, node_handle: NodeHandle) -> Option<NodeHandle4> {
        let node = self.alloc.get(node_handle);
        let left_sibling = node.left_child.swap(None, Ordering::SeqCst)?;
        Some(left_sibling)
    }

    /// deinits the fields for debugging.
    /// any read of any sibling after this is UB.
    #[cfg_attr(feature = "profiling", inline(never))]
    fn reclaim_node(&self, left_sibling: NodeHandle4, data: &mut ThreadData) {
        for node_handle in left_sibling.siblings() {
            let node = self.alloc.get(node_handle);
            unsafe {
                node.write_dom(Domain::uninit());
                node.write_parent(None);
            }
            node.left_child.store(None, Ordering::Relaxed);
            node.color.store(None, Ordering::Relaxed);
            node.min_height.store(0, Ordering::Relaxed);
            node.max_height.store(0, Ordering::Relaxed);
            debug_assert_ne!(
                node.timestamp.load(Ordering::Relaxed),
                RenderMoment::uninit()
            );
            node.timestamp
                .store(RenderMoment::uninit(), Ordering::Relaxed);
        }
        data.free_list.push(left_sibling);
    }

    // const RECLAIM_SCALE: Real = Real::try_from_f64(1.0 / 1000.0).unwrap();
    // const RECLAIM_SCALE: Real = Real::try_from_f64(1.0 / 100.0).unwrap();

    /// selects a node to retire, and retires its children.
    /// returns `None` if we shouldn't/can't retire.
    /// must not call `reclaim` on the returned children within two (or maybe three) frames/moments/epochs.
    /// note that we don't guarantee that the returned nodes are leafs.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn retire(
        &self,
        window: Window,
        now: RenderMoment,
        data: &mut ThreadData,
    ) -> Option<NodeHandle4> {
        fn depth_needed_for_rad(tree: &Tree, reclaim_rad: Real) -> u16 {
            let mut depth = 0;
            let mut rad = tree.dom.rad();
            while rad > reclaim_rad {
                depth += 1;
                rad = rad.div2_exact();
            }
            depth
        }

        // /// checks if max_height implies that we can't reclaim any node.
        // fn can_retire_any(tree: &Tree, reclaim_rad: Real) -> bool {
        //     // find the deepest leaf
        //     let mut handle = tree.root;
        //     while let Some(left_child) = tree.alloc.get(handle).left_child.load(Ordering::SeqCst) {
        //         let node = tree.alloc.get(handle);
        //         let dom = unsafe { node.dom() };
        //         if dom.rad() <= reclaim_rad {
        //             return false;
        //         }
        //         let best_offset = (0..4)
        //             .max_by_key(|&i| {
        //                 tree.alloc
        //                     .get(left_child.siblings_offset(i))
        //                     .max_height
        //                     .load(Ordering::SeqCst)
        //             })
        //             .unwrap();
        //         handle = left_child.siblings_offset(best_offset);
        //     }

        //     let node = tree.alloc.get(handle);
        //     let dom = unsafe { node.dom() };
        //     dom.rad() <= reclaim_rad
        // }

        // if !can_retire_any(self, reclaim_rad) {
        //     return None;
        // }

        // /// returns the depth of the deepest leaf
        // /// st
        // /// returns `None` if there are no such leafs.
        // #[cfg_attr(feature = "profiling", inline(never))]
        // fn depth_of_deepest_leaf(
        //     tree: &Tree,
        //     window: Window,
        //     reclaim_rad: Option<Real>,
        //     data: &mut ThreadData,
        // ) -> Option<u16> {
        // }

        /// selects the deepest internal node.
        /// maybe the children should be leafs.
        /// must have radius smaller than `RECLAIM_SCALE * window.real_rad()`.
        /// also can't be the root.
        /// for now, select the first node we find that's small enough.
        // for now just do the same thing as refine to detect bugs?
        // TODO: return an iter over all such nodes
        // TODO: max_height
        #[cfg_attr(feature = "profiling", inline(never))]
        fn select(tree: &Tree, needed_depth: u16, data: &mut ThreadData) -> Option<NodeHandle> {
            let stack = &mut data.vec_handle_u16;
            stack.clear();
            stack.push((tree.root, 0));
            while let Some((handle, depth)) = stack.pop() {
                let node = tree.alloc.get(handle);
                let max_height = node.max_height.load(Ordering::SeqCst);
                let deepest_descended_leaf_depth = max_height + depth;
                if deepest_descended_leaf_depth < needed_depth {
                    continue;
                }

                // if node.max_height.load(Ordering::SeqCst) < needed_depth {
                //     continue;
                // }
                if let Some(child_handle) = node.left_child.load(Ordering::SeqCst) {
                    // let dom = unsafe { node.dom() };
                    if depth >= needed_depth {
                        if handle != tree.root {
                            return Some(handle);
                        } else {
                            continue;
                        }
                    }
                    stack.extend(child_handle.siblings().map(|c| (c, depth + 1)));
                }
            }
            None
        }

        // // TODO: maybe also update render timestamps?
        // // but it just makes the image look worse.
        // // it's still good for debugging.
        #[cfg_attr(feature = "profiling", inline(never))]
        fn update_render_timestamp(tree: &Tree, mut node_handle: NodeHandle, now: RenderMoment) {
            loop {
                let node = tree.alloc.get(node_handle);
                let old = node.timestamp.load(Ordering::SeqCst);
                if old >= now {
                    break;
                }
                let _ = node
                    .timestamp
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                        if old >= now { None } else { Some(now) }
                    });
                let Some(parent_handle) = (unsafe { node.parent() }) else {
                    break;
                };
                node_handle = parent_handle;
            }
        }

        // at this point, the node is a leaf (unless someone split it in this small window)
        // update heights
        // TODO: don't use parent pointers
        #[cfg_attr(feature = "profiling", inline(never))]
        fn update_ancestors_height(tree: &Tree, mut node_handle: NodeHandle4) {
            // let mut node_handle = node_handle.left_sibling();

            loop {
                let node = tree.alloc.get(node_handle.into());
                let Some(parent) = (unsafe { node.parent() }) else {
                    break;
                };
                let parent_node = tree.alloc.get(parent);

                let min_height = node_handle
                    .siblings()
                    .map(|sibling_handle| {
                        tree.alloc
                            .get(sibling_handle)
                            .min_height
                            .load(Ordering::SeqCst)
                    })
                    .iter()
                    .min()
                    .unwrap()
                    + 1;
                parent_node.min_height.store(min_height, Ordering::SeqCst);

                let max_height = node_handle
                    .siblings()
                    .map(|sibling_handle| {
                        tree.alloc
                            .get(sibling_handle)
                            .max_height
                            .load(Ordering::SeqCst)
                    })
                    .iter()
                    .max()
                    .unwrap()
                    + 1;
                parent_node.max_height.store(max_height, Ordering::SeqCst);

                node_handle = parent.left_sibling();
            }
        }

        let Some(reclaim_scale) =
            Real::try_from_f64(1.0 / RECLAIM_MAX_WIDTH.load(Ordering::SeqCst) as f64)
        else {
            log!("failed to compute reclaim scale");
            return None;
        };
        let Some(reclaim_rad) = reclaim_scale.mul_checked(window.real_rad()) else {
            log!("window is too big/small to reclaim anything");
            return None;
        };

        // the depth needed for a node to be small enough.
        // note that we're allowed to go deeper.
        let required_depth = depth_needed_for_rad(self, reclaim_rad);
        // let reclaim_rad = ();

        // let node_handle = select(self, target_rad, data)?;
        let Some(node_handle) = select(self, required_depth, data) else {
            // log!("failed to find a node to retire");
            return None;
        };
        debug_assert!(
            unsafe { self.alloc.get(node_handle).dom() }.rad() <= reclaim_rad,
            "dom isn't small enough"
        );
        let Some(left_sibling) = self.retire_children_of_node(node_handle) else {
            // log!("someone else retired the node we selected");
            return None;
        };
        // log!("successfully retired a node");

        let node = self.alloc.get(node_handle);

        // update heights
        {
            node.min_height.store(0, Ordering::SeqCst);
            node.max_height.store(0, Ordering::SeqCst);
            update_ancestors_height(self, node_handle.left_sibling());
        }

        // update timestamps
        {
            // node.timestamp.store(now, Ordering::SeqCst);
            update_render_timestamp(self, node_handle, now);
        }

        Some(left_sibling)
    }

    /// must be called on what you got from `retire` after a delay.
    /// retires `left`'s siblings' children.
    /// these children should be reclaimed after the delay.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn retire_siblings_children(
        &self,
        left_sibling: NodeHandle4,
    ) -> impl Iterator<Item = NodeHandle4> {
        left_sibling
            .siblings()
            .into_iter()
            .filter_map(|node_handle| self.retire_children_of_node(node_handle))
    }

    /// should be called soon after `retire_children`.
    /// but must be called after you've consumed the iterator `retire_children` returned.
    /// free `left`'s siblings,
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn reclaim(&self, left_sibling: NodeHandle4, data: &mut ThreadData) {
        // // see if immediately reclaiming the children is bad
        {
            let mut stack = Vec::new();
            stack.push(left_sibling);
            while let Some(left_sibling) = stack.pop() {
                for sibling in left_sibling.siblings() {
                    if let Some(left_child) = self.retire_children_of_node(sibling) {
                        stack.push(left_child);
                    }
                }
                self.reclaim_node(left_sibling, data);
            }
        }
        // self.reclaim_node(left_sibling, data);
    }

    /// returns `None` if we shouldn't/can't refine.
    /// returns handles to nodes who we need to sample.
    ///
    /// to select the node, we require that it
    /// - intersects the window
    /// - is among the shallowest such leafs
    ///
    /// we need `now` in order to initialize the timestamps of the new nodes,
    /// but we don't update the timestamps of the parents here, we do that in [`insert`].
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn refine(
        &self,
        sample_window: Window,
        reclaim_window: Option<Window>,
        now: RenderMoment,
        data: &mut ThreadData,
    ) -> Option<[(Real, Imag); 4]> {
        /// returns the depth of the shallowest leaf that intersects the window.
        /// returns `None` if there are no such leafs.
        /// oracle without using the cached height.
        #[cfg_attr(feature = "profiling", inline(never))]
        fn depth_of_shallowest_leaf_oracle(
            tree: &Tree,
            window: Window,
            data: &mut ThreadData,
        ) -> Option<u16> {
            let stack = &mut data.vec_handle_u16;
            stack.clear();
            stack.push((tree.root, 0));
            let mut shallowest_depth = u16::MAX;
            while let Some((handle, depth)) = stack.pop() {
                if depth >= shallowest_depth {
                    continue;
                }
                let node = tree.alloc.get(handle);
                let dom = unsafe { node.dom() };
                // TODO: instead of doing this check on pop, do it on push
                // this also lets us do less work in the case where the domain is contained inside the window
                if !window.overlaps(dom) {
                    continue;
                }
                if let Some(child_handle) = node.left_child.load(Ordering::SeqCst) {
                    // let height = node.height.load(Ordering::SeqCst);
                    // let shallowest_descended_leaf_depth = height + depth;
                    // if shallowest_descended_leaf_depth >= shallowest_depth {
                    //     continue;
                    // }
                    // if window.contains(dom) {
                    //     if shallowest_descended_leaf_depth < shallowest_depth {
                    //         shallowest_depth = shallowest_descended_leaf_depth;
                    //     }
                    //     // we don't need to explore the children
                    //     continue;
                    // }
                    if depth >= shallowest_depth {
                        continue;
                    }

                    // TODO: sort to do principal variation search,
                    // so we can find a shallow leaf faster, which lets us pune more.
                    // we should look at the child closest to the center of the window first.
                    // or maybe look at the child with the shallowest height.
                    stack.extend(child_handle.siblings().map(|c| (c, depth + 1)));
                } else {
                    if depth < shallowest_depth {
                        shallowest_depth = depth;
                    }
                }
            }

            if shallowest_depth == u16::MAX {
                None
            } else {
                Some(shallowest_depth)
            }
        }

        /// returns the depth of the shallowest leaf that intersects the window.
        /// returns `None` if there are no such leafs.
        #[cfg_attr(feature = "profiling", inline(never))]
        fn depth_of_shallowest_leaf(
            tree: &Tree,
            window: Window,
            reclaim_rad: Option<Real>,
            data: &mut ThreadData,
        ) -> Option<u16> {
            let stack = &mut data.vec_handle_u16;
            stack.clear();
            stack.push((tree.root, 0));
            let mut shallowest_depth = u16::MAX;
            while let Some((handle, depth)) = stack.pop() {
                if depth >= shallowest_depth {
                    continue;
                }
                let node = tree.alloc.get(handle);
                let dom = unsafe { node.dom() };
                // TODO: instead of doing this check on pop, do it on push
                // this also lets us do less work in the case where the domain is contained inside the window
                if !window.overlaps(dom) {
                    continue;
                }
                if let Some(reclaim_rad) = reclaim_rad
                    && dom.rad() <= reclaim_rad
                {
                    continue;
                }
                if let Some(child_handle) = node.left_child.load(Ordering::SeqCst) {
                    let min_height = node.min_height.load(Ordering::SeqCst);
                    #[cfg(false)]
                    {
                        let better_height = child_handle
                            .siblings()
                            .map(|child_handle| {
                                tree.alloc
                                    .get(child_handle)
                                    .min_height
                                    .load(Ordering::SeqCst)
                            })
                            .iter()
                            .min()
                            .unwrap()
                            + 1;
                        if height != better_height {
                            log!(format!(
                                "cached height was {}, but actual height is {}",
                                height, better_height
                            ));
                        }
                    }
                    let shallowest_descended_leaf_depth = min_height + depth;
                    if shallowest_descended_leaf_depth >= shallowest_depth {
                        continue;
                    }
                    if window.contains(dom) {
                        if shallowest_descended_leaf_depth < shallowest_depth {
                            shallowest_depth = shallowest_descended_leaf_depth;
                        }
                        // we don't need to explore the children
                        continue;
                    }

                    // TODO: sort to do principal variation search,
                    // so we can find a shallow leaf faster, which lets us pune more.
                    // we should look at the child closest to the center of the window first.
                    // or maybe look at the child with the shallowest height.
                    stack.extend(child_handle.siblings().map(|c| (c, depth + 1)));
                } else {
                    if depth < shallowest_depth {
                        shallowest_depth = depth;
                    }
                }
            }

            if shallowest_depth == u16::MAX {
                None
            } else {
                Some(shallowest_depth)
            }
        }

        /// returns an iterator over the leaves that intersect the window with the target depth.
        #[cfg_attr(feature = "profiling", inline(never))]
        fn leafs_in_window_at_depth(
            tree: &Tree,
            window: Window,
            reclaim_rad: Option<Real>,
            shallowest_depth: u16,
            data: &mut ThreadData,
        ) -> impl Iterator<Item = NodeHandle> {
            let stack = &mut data.vec_handle_u16;
            stack.clear();
            stack.push((tree.root, 0));
            std::iter::from_fn(move || {
                while let Some((handle, depth)) = stack.pop() {
                    if depth > shallowest_depth {
                        continue;
                    }
                    let node = tree.alloc.get(handle);
                    let dom = unsafe { node.dom() };
                    if !window.overlaps(dom) {
                        continue;
                    }
                    if let Some(reclaim_rad) = reclaim_rad
                        && dom.rad() <= reclaim_rad
                    {
                        continue;
                    }
                    if let Some(child_handle) = node.left_child.load(Ordering::SeqCst) {
                        let height = node.min_height.load(Ordering::SeqCst);
                        if height + depth > shallowest_depth {
                            continue;
                        }
                        stack.extend(child_handle.siblings().map(|c| (c, depth + 1)));
                    } else {
                        if depth == shallowest_depth {
                            // log!("found a leaf at the target depth");
                            return Some(handle);
                        }
                    }
                }
                None
            })
        }

        fn try_split(tree: &Tree, leaf_handle: NodeHandle, left_child: NodeHandle4) -> Option<()> {
            let leaf = tree.alloc.get(leaf_handle);

            let leaf_dom = unsafe { leaf.dom() };
            // #[cfg(debug_assertions)]
            // {
            //     let leaf_height = leaf.height.load(Ordering::SeqCst);
            //     let leaf_color = leaf.color.load(Ordering::SeqCst);
            //     let leaf_left_child = leaf.left_child.load(Ordering::SeqCst);
            // }

            // // if we do this, we can skip initializing the children's dom,
            // // but this requires an additional atomic operation,
            // // so it's probably not worth it.
            // if leaf_left_child.is_some() {
            //     log!("leaf_left_child.is_some()");
            //     continue;
            // }

            // initialize the children's dom and parent
            {
                let Some(doms) = leaf_dom.split() else {
                    log!("leaf_dom.split() is None");
                    return None;
                };
                for (offset, dom) in doms.into_iter().enumerate() {
                    let child_handle = left_child.siblings_offset(offset);
                    let child = tree.alloc.get(child_handle);
                    // SAFETY: we allocated the children and never gave them to anyone,
                    // so we have exclusive access.
                    unsafe {
                        child.write_dom(dom);
                        child.write_parent(Some(leaf_handle));
                    }
                }
            }

            // this is the linearization point
            match leaf.left_child.compare_exchange_weak(
                None,
                Some(left_child),
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(old_left_child) => {
                    // log!("swap succeeded");
                    debug_assert_eq!(
                        old_left_child, None,
                        "this is guaranteed by compare_exchange_weak, despite it being documented incorrectly"
                    );
                }
                Err(_old_left_child) => {
                    // log!("left_child was already `None`");
                    return None;
                }
            }

            // // wait do we actually need to update the timestamp at this point?
            // // we haven't filled in a color.
            // // also update the hight when inserting instead,
            // // not now, which is a bit incorrect,
            // // but still fine for the heuristic we need,
            // // and then we can remove the parent pointer.
            // // TODO: remove
            // tree.update_ancestors_timestamp(leaf_handle, now);
            // tree.update_ancestors_height(left_child);

            // // at this point we know that we were the ones who succeeded,
            // // so we get to do some fun checks
            // #[cfg(debug_assertions)]
            // {
            //     debug_assert_eq!(leaf_height, 0);
            //     debug_assert!(leaf_color.is_some());
            //     debug_assert_eq!(leaf_left_child, None);
            // }

            // TODO: see if this helps?
            // update leaf_height
            {
                // // this can fail if another thread updated the cache first
                // debug_assert_eq!(leaf.height.load(Ordering::SeqCst), 0);
                leaf.min_height.store(1, Ordering::SeqCst);
                leaf.max_height.store(1, Ordering::SeqCst);
            }

            Some(())
        }

        // used for filtering out leafs that would be immediately reclaimed.
        let reclaim_rad = reclaim_window.and_then(|w| {
            let Some(reclaim_scale) =
                Real::try_from_f64(1.0 / RECLAIM_MAX_WIDTH.load(Ordering::SeqCst) as f64)
            else {
                log!("failed to compute reclaim scale");
                return None;
            };
            reclaim_scale.mul_checked(w.real_rad())
        });

        let shallowest_depth = depth_of_shallowest_leaf(self, sample_window, reclaim_rad, data)?;
        // TODO: sometimes we get into a state where refine never succeeds.

        // #[cfg(false)]
        // {
        //     let shallowest_depth_oracle = depth_of_shallowest_leaf_oracle(self, window, data)?;
        //     log!(
        //         "oracle: {}, actual: {}",
        //         shallowest_depth_oracle, shallowest_depth
        //     );
        // }

        // note that we have an exclusive reference to the new children
        let left_child = match data.free_list.pop() {
            Some(handle) => handle,
            None => self.alloc.alloc4(data),
        };

        // initialize the children except for dom and parent, which we don't know yet
        for child_handle in left_child.siblings() {
            let child = self.alloc.get(child_handle);
            child.left_child.store(None, Ordering::Relaxed);
            child.color.store(None, Ordering::Relaxed);
            child.min_height.store(0, Ordering::Relaxed);
            child.max_height.store(0, Ordering::Relaxed);
            child.timestamp.store(now, Ordering::Relaxed);
        }

        // CAS in the new left_child,
        // and if it fails (bc someone already got there),
        // reuse the memory we allocated for the children for the next iteration.
        // if we go through all the leafs at this depth, don't bother retrying, just return `None`.
        // let mut debug_attempts = 0;
        for leaf_handle in
            leafs_in_window_at_depth(self, sample_window, reclaim_rad, shallowest_depth, data)
        {
            // debug_attempts += 1;
            // // filter out leafs that would be immediately reclaimed.
            // // TODO: put this in a better spot
            // if let Some(reclaim_rad) = reclaim_rad {
            //     let leaf = self.alloc.get(leaf_handle);
            //     let leaf_dom = unsafe { leaf.dom() };
            //     if leaf_dom.rad() <= reclaim_rad {
            //         // log!("skipping a leaf that would be immediately reclaimed");
            //         continue;
            //     }
            // }
            if let Some(()) = try_split(self, leaf_handle, left_child) {
                // log!("successfully split");
                return Some(
                    left_child
                        .siblings()
                        .map(|h| unsafe { self.alloc.get(h).dom().mid() }),
                );
            }
        }

        // log!(format!(
        //     "shallowest_depth: {}, attempts: {}",
        //     shallowest_depth, debug_attempts
        // ));
        // {
        //     let shallowest_depth_oracle =
        //         depth_of_shallowest_leaf_oracle(self, sample_window, data)?;
        //     log!(format!(
        //         "shallowest depth oracle: {}, actual: {}, attempts: {}",
        //         shallowest_depth_oracle, shallowest_depth, debug_attempts
        //     ));
        // }

        // dbg!("failed to split");
        // if rand::rng().random_bool(0.0001) {
        //     dbg!("failed to split");
        //     let shallowest_depth_oracle = depth_of_shallowest_leaf_oracle(self, window, data)?;
        //     log!(
        //         "oracle: {}, actual: {}",
        //         shallowest_depth_oracle, shallowest_depth
        //     );
        // }

        data.free_list.push(left_child);

        None
    }

    /// inserts the previously reserved sample into the node.
    /// use a point rather than a node handle to make reclaiming work.
    /// we also update the ancestors' timestamps and heights here,
    /// where we're traversing down anyway,
    /// so we don't need parent pointers as we would if we did it in `refine`.
    /// updating the heights here and not earlier is slightly incorrect,
    /// but fine for the way we use them as a heuristic.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn insert(
        &self,
        (real, imag): (Real, Imag),
        color: Color32,
        now: RenderMoment,
        data: &mut ThreadData,
    ) {
        let stack = &mut data.vec_handle;
        stack.clear();

        let mut node_handle = self.root;
        stack.push(node_handle);
        loop {
            let node = self.alloc.get(node_handle);

            // update timestamp.
            // TODO: .fetch_max
            let _ = node
                .timestamp
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                    if old >= now { None } else { Some(now) }
                });

            // break if we found it.
            let dom = unsafe { node.dom() };
            if dom.mid() == (real, imag) {
                node.color
                    .store(Some(color.try_into().unwrap()), Ordering::SeqCst);
                break;
            }

            // go to the correct child.
            let child_offset = dom.child_offset_containing((real, imag));
            let Some(left_child) = node.left_child.load(Ordering::SeqCst) else {
                log!(
                    "failed to follow child pointer during insert. this probably means it got reclaimed."
                );
                return;
            };
            node_handle = left_child.siblings_offset(child_offset);
            stack.push(node_handle);
        }

        // // this first debug_assert will get removed once i remove parent pointers.
        // debug_assert_eq!(
        //     stack.last(),
        //     unsafe { self.alloc.get(node_handle).parent() }.as_ref(),
        //     "the top of stack should be the parent of the node we inserted into."
        // );
        // {
        //     let last = stack
        //         .last()
        //         .expect("this only would only happen for the root, which we never insert into.");
        //     let node = self.alloc.get(*last);
        //     let child_offset = unsafe { node.dom().child_offset_containing((real, imag)) };
        //     let child_handle = node
        //         .left_child
        //         .load(Ordering::SeqCst)
        //         .expect("this really should not be a leaf.")
        //         .siblings_offset(child_offset);
        //     let child = self.alloc.get(child_handle);
        //     debug_assert_eq!(
        //         unsafe { child.dom().mid() },
        //         (real, imag),
        //         "the top of stack should be the parent of the node we inserted into."
        //     );
        // }
        #[cfg(debug_assertions)]
        {
            let last = stack
                .last()
                .expect("this only would only happen for the root, which we never insert into.");
            debug_assert_eq!(
                unsafe { self.alloc.get(*last).dom().mid() },
                (real, imag),
                "the top of stack should be the node we inserted into."
            );
        }

        // the node we inserted into should have had its height updated (set to 0) by `split`.
        stack.pop();
        // try updating the height anyway
        // if let Some(node_handle) = stack.pop() {
        //     let node = self.alloc.get(node_handle);
        //     if let Some(left_child) = node.left_child.load(Ordering::SeqCst) {
        //         let height = left_child
        //             .siblings()
        //             .map(|child_handle| {
        //                 self.alloc
        //                     .get(child_handle)
        //                     .min_height
        //                     .load(Ordering::SeqCst)
        //             })
        //             .iter()
        //             .min()
        //             .unwrap()
        //             + 1;
        //         node.min_height.store(height, Ordering::SeqCst);
        //     }
        // }

        // update ancestor's height.
        // note that even if we didn't insert into a leaf, everything is fine.
        while let Some(node_handle) = stack.pop() {
            let node = self.alloc.get(node_handle);
            let left_child = node
                .left_child
                .load(Ordering::SeqCst)
                .expect("this really should not be a leaf.");
            let min_height = left_child
                .siblings()
                .map(|child_handle| {
                    self.alloc
                        .get(child_handle)
                        .min_height
                        .load(Ordering::SeqCst)
                })
                .iter()
                .min()
                .unwrap()
                + 1;
            node.min_height.store(min_height, Ordering::SeqCst);

            let max_height = left_child
                .siblings()
                .map(|child_handle| {
                    self.alloc
                        .get(child_handle)
                        .max_height
                        .load(Ordering::SeqCst)
                })
                .iter()
                .max()
                .unwrap()
                + 1;
            node.max_height.store(max_height, Ordering::SeqCst);
        }
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn any_on_line_needs_redraw(
        &self,
        real_lo: Real,
        real_hi: Real,
        imag: Imag,
        prev_frame_start: RenderMoment,
        data: &mut ThreadData,
    ) -> bool {
        debug_assert!(real_lo <= real_hi);

        let stack = &mut data.vec_handle;
        stack.clear();
        stack.push(self.root);

        // if we have a segment that's definitely good, we stop exploring.
        // if we have a segment that's definitely bad, we fail.
        // to know that a segment is definitely bad, it must be a leaf.
        while let Some(handle) = stack.pop() {
            let node = self.alloc.get(handle);
            let dom = unsafe { node.dom() };

            if dom.imag_lo() > imag || dom.imag_hi() < imag {
                continue;
            }
            if dom.real_hi() < real_lo || dom.real_lo() > real_hi {
                continue;
            }

            let timestamp = node.timestamp.load(Ordering::Relaxed);
            if timestamp < prev_frame_start {
                continue;
            }

            if let Some(child_handle) = node.left_child.load(Ordering::Relaxed) {
                stack.extend(child_handle.siblings());
            } else {
                if timestamp >= prev_frame_start {
                    return true;
                }
            }
        }

        false
    }

    // TODO: if the pixel doesn't contain any samples,
    // return the color of the sample closest to the center of the pixel.
    // TODO: if the pixel contains any samples, do some weighting of the samples
    // TODO: if the pixel doesn't contain any samples, do some weighting of some nearby samples
    // TODO: if the pixel contains any samples, returns the average color of the samples inside the pixel
    //
    /// follow the path down to the leaf containing the center of the pixel,
    /// return the color of the sample closest to the center of the pixel.
    /// returns `None` if we prove that the color hasn't changed from the last frame.
    /// returns white if not in the trees domain
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn color_of_pixel(
        &self,
        pixel: Pixel,
        prev_frame_start: RenderMoment,
    ) -> Option<Color32> {
        #[cfg_attr(feature = "profiling", inline(never))]
        fn distance((real_0, imag_0): (Real, Imag), (real_1, imag_1): (Real, Imag)) -> Fixed {
            let real_delta = real_0 - real_1;
            let imag_delta = imag_0 - imag_1;
            // real_delta.mul(real_delta) + imag_delta.mul(imag_delta)

            // i think they give the same result
            // except manhattan maybe gives weird lines
            // real_delta.abs() + imag_delta.abs()
            real_delta.abs().max(imag_delta.abs())
        }

        // let start = std::time::Instant::now();

        let center = pixel.mid();
        // we never touch pixel again
        #[expect(unused_variables)]
        let pixel = ();

        if !self.dom.contains_point(center) {
            const UNCONTAINED_COLOR: Color32 = Color32::WHITE;
            return Some(UNCONTAINED_COLOR);
        }

        let mut node_handle = self.root;
        let mut closest_sample_dist = distance(center, self.dom.mid());
        // TODO: we don't need to maintain this in the common case.
        // the closest_sample_color is only ever not the color of the closest_node.dom.mid
        // when the closest_node doesn't have a color, which is rare
        let mut closest_sample_color = self
            .alloc
            .get(node_handle)
            .color
            .load(Ordering::Relaxed)
            .expect("root must have a color");
        // let mut debug_closest_sample_depth = 0;
        // let mut debug_explored_depth = 0;

        // // this is incorrect btw
        // fn should_prune(tree: &Tree, center: (Real, Imag), prev_frame_start: Moment) -> bool {
        //     let mut node_handle = tree.root;

        //     loop {
        //         let node = tree.alloc.get(node_handle);
        //         let dom = unsafe { node.dom() };

        //         // check whether the node's timestamp proves that the color hasn't changed since the last frame
        //         {
        //             let timestamp = node.timestamp.load(Ordering::Relaxed);
        //             // TODO: <=?
        //             if timestamp < prev_frame_start {
        //                 return true;
        //             }
        //         }

        //         // i++
        //         {
        //             let Some(left_child) = node.left_child.load(Ordering::Relaxed) else {
        //                 break;
        //             };
        //             let child_offset = dom.child_offset_containing(center);
        //             node_handle = left_child.siblings_offset(child_offset);
        //             // debug_explored_depth += 1;
        //         }
        //     }

        //     false
        // }

        // if should_prune(self, center, prev_frame_start) {
        //     let elapsed = start.elapsed();
        //     PRUNED_ELAPSED.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
        //     PRUNED_COUNTER.fetch_add(1, Ordering::Relaxed);
        //     return None;
        // }

        loop {
            let node = self.alloc.get(node_handle);
            let dom = unsafe { node.dom() };

            // check whether the node's timestamp proves that the color hasn't changed since the last frame
            {
                let timestamp = node.timestamp.load(Ordering::Relaxed);
                // TODO: <=?
                if timestamp < prev_frame_start {
                    // let elapsed = start.elapsed();
                    // PRUNED_ELAPSED.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
                    // PRUNED_COUNTER.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
            }

            // update color
            {
                let dist = distance(center, dom.mid());
                let color = node.color.load(Ordering::Relaxed);
                // debug color the uncolored nodes
                const UNCOLORED_NODE_COLOR: Option<Rgb> = Some(Rgb::new(255, 255, 0));
                // const UNCOLORED_NODE_COLOR: Option<RGB> = None;
                if dist < closest_sample_dist
                    && let Some(color) = color.or(UNCOLORED_NODE_COLOR)
                {
                    closest_sample_dist = dist;
                    closest_sample_color = color;
                    // debug_closest_sample_depth = debug_explored_depth;
                }
            }

            // i++
            {
                let Some(left_child) = node.left_child.load(Ordering::Relaxed) else {
                    break;
                };
                let child_offset = dom.child_offset_containing(center);
                node_handle = left_child.siblings_offset(child_offset);
                // debug_explored_depth += 1;
            }
        }

        // let elapsed = start.elapsed();
        // UNPRUNED_ELAPSED.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
        // UNPRUNED_COUNTER.fetch_add(1, Ordering::Relaxed);

        Some(closest_sample_color.into())
        // if debug_closest_sample_depth == 32 {
        //     Some(Color32::from_rgb(255, 0, 255))
        // } else {
        //     Some(closest_sample_color.into())
        // }
    }
}

/// for data that each thread should keep track of for itself.
/// basically just for allocations.
/// dropping this may leak memory, but is safe.
#[derive(Debug, Default)]
pub(crate) struct ThreadData {
    /// nodes we have freed.
    /// you should look in here before going to the global allocator.
    free_list: Vec<NodeHandle4>,
    /// the block we allocated in `realloc`
    block: Option<NonNull<Block>>,
    /// for various stacks (and perhaps queues).
    /// should be cleared before use, but not when done.
    vec_handle: Vec<NodeHandle>,
    /// for various stacks (and perhaps queues).
    /// should be cleared before use, but not when done.
    vec_handle_u16: Vec<(NodeHandle, u16)>,
}

use rbg::*;
mod rbg {
    use std::fmt;

    use super::*;

    /// basically a [`egui::Color32`] with max alpha.
    /// layout is 0xFFbbggrr, ie little endian [r, g, b, 255].
    /// we could allow any nonzero alpha, but i don't use this.
    #[repr(transparent)]
    #[derive(Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
    pub(super) struct Rgb(NonZeroU32);
    unsafe impl bytemuck::ZeroableInOption for Rgb {}
    unsafe impl bytemuck::PodInOption for Rgb {}
    impl Rgb {
        pub(super) const fn new(r: u8, g: u8, b: u8) -> Self {
            let arr = [r, g, b, 255];
            let value = u32::from_le_bytes(arr);
            Rgb(NonZeroU32::new(value).unwrap())
        }
    }
    impl fmt::Debug for Rgb {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let [r, g, b, a] = self.0.get().to_le_bytes();
            debug_assert_eq!(a, 255);
            f.debug_struct("RGB")
                .field("r", &r)
                .field("g", &g)
                .field("b", &b)
                .finish()
        }
    }
    impl TryFrom<Color32> for Rgb {
        type Error = &'static str;

        fn try_from(value: Color32) -> Result<Self, Self::Error> {
            if value.a() != 255 {
                return Err("alpha is not 255");
            }
            Ok(Self::new(value.r(), value.g(), value.b()))
        }
    }
    impl From<Rgb> for Color32 {
        fn from(value: Rgb) -> Self {
            let [r, g, b, a] = value.0.get().to_le_bytes();
            debug_assert_eq!(a, 255);
            Color32::from_rgb(r, g, b)
        }
    }
}

pub(crate) use alloc::*;
mod alloc {
    use std::{
        fmt,
        num::NonZeroUsize,
        sync::atomic::{AtomicPtr, AtomicUsize},
    };

    use super::*;

    const _: () = assert!(size_of::<Node>() == 64);
    const _: () = assert!(align_of::<Node>() == 64);
    /// bits 0..6: unused (for epoch stuff maybe?)
    ///
    /// bits 6..12: index of node within the block
    ///
    /// bits 12..: block pointer (the lower bits are 0 because of alignment)
    ///
    /// bits 6..: `Node` pointer
    // TODO: we can store [Node; 4] and get two more bits in the pointer
    #[repr(transparent)]
    #[derive(Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
    pub(crate) struct NodeHandle(NonZeroUsize);
    unsafe impl bytemuck::ZeroableInOption for NodeHandle {}
    unsafe impl bytemuck::PodInOption for NodeHandle {}
    impl NodeHandle {
        /// index is the index of the node in the block
        fn new(block: NonNull<Block>, index: usize) -> Self {
            debug_assert!(index < Block::CAPACITY);
            let block = block.as_ptr() as usize;
            debug_assert_eq!(block % Block::SIZE, 0);
            let offset = index * size_of::<Node>();
            debug_assert_eq!(block & offset, 0);
            let ret = block + offset;
            debug_assert_eq!(
                ret % size_of::<Node>(),
                0,
                "ret: {:?}",
                NodeHandle(NonZeroUsize::new(ret).unwrap())
            );
            debug_assert_ne!(ret, 0);
            unsafe { NodeHandle(NonZeroUsize::new_unchecked(ret)) }
        }

        fn to_block(self) -> NonNull<Block> {
            let block = self.0.get() & !(Block::SIZE - 1);
            debug_assert_eq!(block % Block::SIZE, 0);
            NonNull::new(block as *mut Block).unwrap()
        }

        fn to_index(self) -> usize {
            let index = (self.0.get() % Block::SIZE) / size_of::<Node>();
            debug_assert!(index < Block::CAPACITY);
            index
        }

        fn to_ptr(self) -> *mut Node {
            let ptr = self.0.get() as *mut Node;
            debug_assert_eq!(ptr as usize % size_of::<Node>(), 0);
            ptr
        }

        // /// offsets the node index within the block by `offset`.
        // /// should have that self is the first node in a group of 4,
        // /// ie has greater alignment.
        // fn offset(self, offset: usize) -> Self {
        //     debug_assert!(offset < 4);
        //     debug_assert_eq!(self.to_index() % 4, 0);
        //     // could just add offset * size_of::<Node>(), but this is a bit safer
        //     Self::new(self.to_block(), self.to_index() + offset)
        // }

        /// because the root is leftmost in its group,
        /// this is actually fine to call on the root.
        #[cfg_attr(feature = "profiling", inline(never))]
        pub(super) fn left_sibling(self) -> NodeHandle4 {
            let block = self.to_block();
            let i = self.to_index();
            NodeHandle4::try_from(NodeHandle::new(block, i - (i % 4))).unwrap()
        }
    }
    impl fmt::Debug for NodeHandle {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_tuple("NodeHandle")
                .field(&format_args!("hex: {:x}", self.0.get()))
                .field(&format_args!(
                    "block: {:x}",
                    self.to_block().as_ptr() as usize
                ))
                .field(&format_args!("index: {}", self.to_index()))
                .finish()
        }
    }
    impl From<NodeHandle4> for NodeHandle {
        fn from(value: NodeHandle4) -> Self {
            value.0
        }
    }

    /// represents a group of four siblings.
    #[repr(transparent)]
    #[derive(Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
    pub(crate) struct NodeHandle4(NodeHandle);
    unsafe impl bytemuck::ZeroableInOption for NodeHandle4 {}
    unsafe impl bytemuck::PodInOption for NodeHandle4 {}
    impl NodeHandle4 {
        /// equivalent to `self.siblings()[offset]`
        #[cfg_attr(feature = "profiling", inline(never))]
        pub(super) fn siblings_offset(self, offset: usize) -> NodeHandle {
            debug_assert!(offset < 4);
            #[cfg(debug_assertions)]
            let oracle = {
                let block = self.0.to_block();
                let i = self.0.to_index();
                debug_assert_eq!(i % 4, 0, "unaligned handle in siblings");
                NodeHandle::new(block, i + offset)
            };
            let ret = unsafe {
                NodeHandle(NonZeroUsize::new_unchecked(
                    self.0.0.get() + size_of::<Node>() * offset,
                ))
            };
            #[cfg(debug_assertions)]
            debug_assert_eq!(oracle, ret);
            ret
        }

        /// must have that self is the first node in a group of 4, ie has greater alignment.
        /// also note that it's probably bad to call this on the root.
        #[cfg_attr(feature = "profiling", inline(never))]
        pub(super) fn siblings(self) -> [NodeHandle; 4] {
            let block = self.0.to_block();
            let i = self.0.to_index();
            debug_assert_eq!(i % 4, 0, "unaligned handle in siblings");
            let oracle = [
                NodeHandle::new(block, i),
                NodeHandle::new(block, i + 1),
                NodeHandle::new(block, i + 2),
                NodeHandle::new(block, i + 3),
            ];
            let ret = unsafe {
                [
                    self.0,
                    NodeHandle(NonZeroUsize::new_unchecked(
                        self.0.0.get() + size_of::<Node>(),
                    )),
                    NodeHandle(NonZeroUsize::new_unchecked(
                        self.0.0.get() + size_of::<Node>() * 2,
                    )),
                    NodeHandle(NonZeroUsize::new_unchecked(
                        self.0.0.get() + size_of::<Node>() * 3,
                    )),
                ]
            };
            debug_assert_eq!(oracle, ret);
            ret
        }
    }
    impl fmt::Debug for NodeHandle4 {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_tuple("NodeHandle4")
                .field(&format_args!("hex: {:x}", self.0.0.get()))
                .field(&format_args!(
                    "block: {:x}",
                    self.0.to_block().as_ptr() as usize
                ))
                .field(&format_args!("index: {}", self.0.to_index()))
                .finish()
        }
    }
    impl TryFrom<NodeHandle> for NodeHandle4 {
        type Error = &'static str;

        fn try_from(value: NodeHandle) -> Result<Self, Self::Error> {
            if !value.to_index().is_multiple_of(4) {
                return Err("index is not a multiple of 4");
            }
            Ok(Self(value))
        }
    }

    /// footer rather than header because then indexing the node array
    /// is an offset from the block pointer,
    /// rather than the block pointer + header size.
    #[derive(Debug)]
    struct BlockFooter {
        // TODO: consider having the head point to itself instead of `None` / null
        // prev: *mut Block,
        prev: Option<NonNull<Block>>,
        /// how many nodes are allocated in this block.
        /// note that this can be > capacity.
        len: AtomicUsize,
    }

    // align requires an integer literal
    #[repr(C, align(4096))]
    #[derive(Debug)]
    pub(super) struct Block {
        /// not wrapped in `UnsafeCell` because we don't actually write to the nodes, only their fields.
        mem: [Node; Self::CAPACITY],
        foot: BlockFooter,
    }
    const _: () = assert!(size_of::<Block>() == Block::SIZE);
    const _: () = assert!(align_of::<Block>() == Block::SIZE);
    impl Block {
        // small size to force more reallocation for testing
        // actually i also need to change the bitfield in `NodeHandle` for this
        // const SIZE: usize = 128;
        // const SIZE: usize = 256;
        const SIZE: usize = 4096;
        const CAPACITY: usize = (Self::SIZE - size_of::<BlockFooter>()) / size_of::<Node>();

        fn with_prev(prev: Option<NonNull<Block>>) -> Self {
            Self {
                mem: std::array::from_fn(|_| Node::uninit()),
                foot: BlockFooter {
                    prev,
                    len: AtomicUsize::new(0),
                },
            }
        }
    }

    #[derive(Debug)]
    pub(super) struct Alloc {
        head: AtomicPtr<Block>,
        // last_len: AtomicUsize,
    }
    impl Default for Alloc {
        fn default() -> Self {
            Self::new()
        }
    }
    impl Alloc {
        // fn new(thread_count: usize) -> Self {
        #[cfg_attr(feature = "profiling", inline(never))]
        fn new() -> Self {
            Self {
                head: AtomicPtr::new(Box::into_raw(Box::new(Block::with_prev(None)))),
                // last_len: AtomicUsize::new(0),
                // local_cache: vec![std::ptr::null_mut(); thread_count].into_boxed_slice(),
            }
        }

        #[cfg_attr(feature = "profiling", inline(never))]
        fn realloc(&self, old_last: NonNull<Block>, data: &mut ThreadData) {
            let new_block = match data.block.take() {
                Some(block) => block.as_ptr(),
                None => Box::into_raw(Box::new(Block::with_prev(Some(old_last)))),
            };
            match self.head.compare_exchange_weak(
                old_last.as_ptr(),
                new_block,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    // successfully swapped in the new block, so we can just use it
                    // self.local_cache[thread_i] = std::ptr::null_mut();
                }
                Err(_actual) => {
                    // another thread already swapped in a new block.
                    // reuse the block we allocated for next time.
                    data.block = Some(NonNull::new(new_block).unwrap());
                }
            }
        }

        // #[cfg_attr(feature = "profiling", inline(never))]
        // fn alloc<const N: usize>(&self, data: &mut ThreadData) -> NodeHandle {
        //     // loop bc it's technically possible that after reallocating
        //     // or during waiting for another thread to reallocate,
        //     // we go to sleep and the new block fills up.
        //     loop {
        //         let last = unsafe { self.last.load(Ordering::SeqCst).as_ref().unwrap() };
        //         let len = last.foot.len.fetch_add(N, Ordering::SeqCst);
        //         if len + N <= Block::CAPACITY {
        //             return NodeHandle::new(last.into(), len);
        //         }
        //         // we could say that whoever got len == Block::CAPACITY is responsible for reallocating,
        //         // but what if that thread went to sleep during reallocation?
        //         // so just have all the threads realloc
        //         self.realloc(last.into(), data);
        //     }
        // }

        #[cfg_attr(feature = "profiling", inline(never))]
        pub(super) fn alloc4(&self, data: &mut ThreadData) -> NodeHandle4 {
            // loop bc it's technically possible that after reallocating
            // or during waiting for another thread to reallocate,
            // we go to sleep and the new block fills up.
            loop {
                let last = unsafe { self.head.load(Ordering::SeqCst).as_ref().unwrap() };
                let i = last.foot.len.fetch_add(4, Ordering::SeqCst);
                if i + 4 <= Block::CAPACITY {
                    return NodeHandle::new(last.into(), i)
                        .try_into()
                        .expect("we just made sure it's aligned");
                }
                // we could say that whoever got len == Block::CAPACITY is responsible for reallocating,
                // but what if that thread went to sleep during reallocation?
                // so just have all the threads realloc
                self.realloc(last.into(), data);
            }
        }

        #[cfg_attr(feature = "profiling", inline(never))]
        pub(super) fn get(&self, handle: NodeHandle) -> &Node {
            let ret = unsafe { handle.to_ptr().as_ref().unwrap() };

            #[cfg(debug_assertions)]
            {
                let block = handle.to_block();
                let block = unsafe { block.as_ref() };

                debug_assert_eq!(
                    ret as *const Node,
                    &block.mem[handle.to_index()] as *const Node
                );
            }

            ret
        }
    }
}

pub(crate) use moment::*;
mod moment {
    use std::ops;

    #[repr(transparent)]
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, bytemuck::NoUninit)]
    /// uses signed integer internally but should be nonnegative.
    /// i32 mean it'll overflow after 19884 hours at 60 fps.
    pub(crate) struct MomentInner(i32);
    pub(crate) type RenderMoment = MomentInner;
    pub(crate) type ReclaimMoment = MomentInner;
    impl MomentInner {
        pub(crate) const MIN: Self = Self(0);

        fn new(value: i32) -> Self {
            debug_assert!(value >= 0);
            Self(value)
        }

        pub(super) fn uninit() -> Self {
            Self(-987123)
            // Self::default()
        }
    }
    impl ops::Add<i32> for MomentInner {
        type Output = Self;

        fn add(self, rhs: i32) -> Self {
            Self::new(self.0 + rhs)
        }
    }
    impl ops::AddAssign<i32> for MomentInner {
        fn add_assign(&mut self, rhs: i32) {
            *self = Self::new(self.0 + rhs);
        }
    }
    impl ops::Sub<i32> for MomentInner {
        type Output = Self;

        fn sub(self, rhs: i32) -> Self {
            Self::new(self.0 - rhs)
        }
    }
}

// // TODO: once we factor drawing into tree.rs, this should become private
// /// represents the average of `count` colors
// #[derive(Debug, Default, Clone)]
// #[repr(align(32))]
// pub(crate) struct ColorBuilder {
//     // count: NonZero<u32>,
//     count: u32,
//     r: u32,
//     g: u32,
//     b: u32,
// }
// impl ColorBuilder {
//     pub(crate) fn build(self) -> Option<Color32> {
//         if self.count == 0 {
//             None
//         } else {
//             Some(Color32::from_rgb(
//                 (self.r / self.count) as u8,
//                 (self.g / self.count) as u8,
//                 (self.b / self.count) as u8,
//             ))
//         }
//     }
// }
// impl From<Color32> for ColorBuilder {
//     fn from(value: Color32) -> Self {
//         Self {
//             count: 1,
//             r: value.r() as _,
//             g: value.g() as _,
//             b: value.b() as _,
//         }
//     }
// }
// impl AddAssign<ColorBuilder> for ColorBuilder {
//     fn add_assign(&mut self, rhs: ColorBuilder) {
//         self.count += rhs.count;
//         self.r += rhs.r;
//         self.g += rhs.g;
//         self.b += rhs.b;
//     }
// }
// impl Add<ColorBuilder> for ColorBuilder {
//     type Output = ColorBuilder;

//     fn add(self, rhs: ColorBuilder) -> ColorBuilder {
//         let mut result = self;
//         result += rhs;
//         result
//     }
// }
// impl Sum for ColorBuilder {
//     fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
//         let mut ret = Self::default();
//         for c in iter {
//             ret += c;
//         }
//         ret
//     }
// }

// #[derive(Debug, Default, Clone, Copy)]
// struct Trace {
//     depth: u8,
//     real_bits: u64,
//     imag_bits: u64,
// }
// impl Trace {
//     fn top_right(self) -> Self {
//         Self {
//             depth: self.depth + 1,
//             real_bits: self.real_bits << 1,
//             imag_bits: self.imag_bits << 1,
//         }
//     }

//     fn top_left(self) -> Self {
//         Self {
//             depth: self.depth + 1,
//             real_bits: self.real_bits << 1 | 1,
//             imag_bits: self.imag_bits << 1,
//         }
//     }

//     fn bot_right(self) -> Self {
//         Self {
//             depth: self.depth + 1,
//             real_bits: self.real_bits << 1,
//             imag_bits: self.imag_bits << 1 | 1,
//         }
//     }

//     fn bot_left(self) -> Self {
//         Self {
//             depth: self.depth + 1,
//             real_bits: self.real_bits << 1 | 1,
//             imag_bits: self.imag_bits << 1 | 1,
//         }
//     }
// }
