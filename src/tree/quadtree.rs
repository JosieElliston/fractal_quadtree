use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering, fence};

use egui::Color32;

use crate::{
    complex::{Pixel, Window, fixed::*},
    log,
    pool::RenderMoment,
    sample::metabrot_sample,
};

use super::{
    Domain, Node, Rgb,
    alloc::{Alloc, AllocLocal, BlockHandle, NodeHandle},
};

/// the min size of a node to retire is `window.real_rad() / RETIRE_MAX_WIDTH`.
pub(crate) static RETIRE_MAX_WIDTH: AtomicUsize = AtomicUsize::new(1000);

/// draw the uncolored nodes a special color,
/// rather than just skipping them.
pub(crate) static DRAW_UNCOLORED_NODES: AtomicBool = AtomicBool::new(true);

/// whether we should split nodes that may immediately get reclaimed.
pub(crate) static SPLIT_RETIRABLE_NODES: AtomicBool = AtomicBool::new(false);

/// per-thread data for the tree.
/// this allows us to reuse allocations for stacks.
///
/// dropping this will not leak memory.
#[derive(Debug, Default)]
pub(crate) struct TreeLocal {
    /// for various stacks.
    /// should be cleared before use, but not when done.
    vec_handle: Vec<NodeHandle>,
    /// for various stacks.
    /// should be cleared before use, but not when done.
    vec_handle4: Vec<BlockHandle>,
    /// for various stacks.
    /// should be cleared before use, but not when done.
    vec_handle_u16: Vec<(NodeHandle, u16)>,
    // /// should be cleared before use, but not when done.
    // deque_handle_u16: VecDeque<(NodeHandle, u16)>,
}

// TODO: doc how we never give out handles except for reclamation.
// TODO: note what "reclaim" vs "retire" vs "free" means.
// reclamation is the general process, which is split into retiring and freeing nodes.
#[derive(Debug)]
pub(crate) struct Tree {
    dom: Domain,
    alloc: Alloc,
    root: NodeHandle,
}
impl Tree {
    pub(crate) fn new(_tree_local: &mut TreeLocal, alloc_local: &mut AllocLocal) -> Self {
        let dom = Domain::default();
        let color = metabrot_sample::<false>(&mut None, dom.mid())
            .color()
            .try_into()
            .unwrap();
        let alloc = Alloc::default();

        // we leave the root's siblings uninit.
        let root_handle = alloc.alloc(alloc_local).into();
        let root = alloc.get_uninit(root_handle);

        unsafe {
            root.write_dom(dom);
        }
        root.children_handle.store(None, Ordering::Relaxed);
        root.color.store(Some(color), Ordering::Relaxed);
        root.min_height.store(0, Ordering::Relaxed);
        root.max_height.store(0, Ordering::Relaxed);
        root.timestamp
            .store(RenderMoment::default(), Ordering::Relaxed);

        // TODO: do i need a fence?
        fence(Ordering::Release);

        Self {
            dom,
            alloc,
            root: root_handle,
        }
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn node_count(&self, tree_local: &mut TreeLocal) -> usize {
        let mut count = 0;
        let stack = &mut tree_local.vec_handle;
        stack.clear();

        stack.push(self.root);

        while let Some(node_handle) = stack.pop() {
            count += 1;
            if let Some(children_handle) = self
                .alloc
                .get(node_handle)
                .children_handle
                .load(Ordering::SeqCst)
            {
                stack.extend(children_handle.siblings());
                // debug heights
                // min_height
                #[cfg(false)]
                {
                    let min_height = self
                        .alloc
                        .get(node_handle)
                        .min_height
                        .load(Ordering::SeqCst);
                    let oracle_min_height = children_handle
                        .siblings()
                        .map(|child_handle| {
                            self.alloc
                                .get(children_handle)
                                .min_height
                                .load(Ordering::SeqCst)
                        })
                        .iter()
                        .min()
                        .unwrap()
                        + 1;
                    if min_height != oracle_min_height {
                        log!(format!(
                            "node {:?} has min_height {}, but oracle min_height is {}",
                            node_handle, min_height, oracle_min_height
                        ));
                    }
                }
                // max_height
                #[cfg(false)]
                {
                    let max_height = self
                        .alloc
                        .get(node_handle)
                        .max_height
                        .load(Ordering::SeqCst);
                    let oracle_max_height = children_handle
                        .siblings()
                        .map(|child_handle| {
                            self.alloc
                                .get(children_handle)
                                .max_height
                                .load(Ordering::SeqCst)
                        })
                        .iter()
                        .max()
                        .unwrap()
                        + 1;
                    if max_height != oracle_max_height {
                        log!(format!(
                            "node {:?} has max_height {}, but oracle max_height is {}",
                            node_handle, max_height, oracle_max_height
                        ));
                    }
                }
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

    /// updates the `min_height` and `max_height` of the node at `node_handle` and all its ancestors.
    #[cfg_attr(feature = "profiling", inline(never))]
    fn update_ancestor_heights(&self, stack: &mut Vec<NodeHandle>, node_handle: NodeHandle) {
        // note that we must go bottom up for correctness,
        // so collect the path down to the node.
        {
            stack.clear();
            let target_mid = unsafe { self.alloc.get(node_handle).dom().mid() };
            let mut node_handle = self.root;
            loop {
                stack.push(node_handle);
                let node = self.alloc.get(node_handle);
                let dom = unsafe { node.dom() };
                if dom.mid() == target_mid {
                    // we found the node.
                    break;
                }
                match node.children_handle.load(Ordering::SeqCst) {
                    Some(children_handle) => {
                        let child_offset = dom.child_offset_containing(target_mid);
                        let child_handle = children_handle.offset(child_offset);
                        node_handle = child_handle;
                    }
                    None => {
                        log!(
                            "failed to find node in update_ancestor_heights, this probably means it got reclaimed"
                        );
                        break;
                    }
                }
            }
        }

        #[expect(unused_variables)]
        let node_handle = ();

        for node_handle in stack.iter().rev() {
            let node = self.alloc.get(*node_handle);
            update_min_height(self, node);
            update_max_height(self, node);
        }

        // everything below this is helper function definitions.
        return;

        #[cfg_attr(feature = "profiling", inline(never))]
        fn update_min_height(tree: &Tree, node: &Node) {
            loop {
                let old_min_height = node.min_height.load(Ordering::SeqCst);

                // TODO: weaken orderings.
                // // ensure we see updates to node's heights before we look at whether we still have children.
                // fence(Ordering::SeqCst);

                let new_min_height = match node.children_handle.load(Ordering::SeqCst) {
                    Some(children_handle) => {
                        children_handle
                            .siblings()
                            .into_iter()
                            .map(|child_handle| {
                                tree.alloc
                                    .get(child_handle)
                                    .min_height
                                    .load(Ordering::SeqCst)
                            })
                            .min()
                            .unwrap()
                            + 1
                    }
                    None => {
                        // our children got reclaimed,
                        // but we still need to update the height.
                        0
                    }
                };

                match node.min_height.compare_exchange(
                    old_min_height,
                    new_min_height,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(prev) => {
                        assert_eq!(prev, old_min_height);
                        break;
                    }
                    Err(actual_cur) => {
                        assert_ne!(
                            actual_cur, old_min_height,
                            "hopefully guaranteed by `compare_exchange` not being weak"
                        );
                        // someone else updated the height,
                        // but they may have not seen our updates to the children,
                        // so we must retry.
                        continue;
                    }
                }
            }
        }

        #[cfg_attr(feature = "profiling", inline(never))]
        fn update_max_height(tree: &Tree, node: &Node) {
            loop {
                let old_max_height = node.max_height.load(Ordering::SeqCst);

                // TODO: weaken orderings.
                // // ensure we see updates to node's heights before we look at whether we still have children.
                // fence(Ordering::SeqCst);

                let new_max_height = match node.children_handle.load(Ordering::SeqCst) {
                    Some(children_handle) => {
                        children_handle
                            .siblings()
                            .into_iter()
                            .map(|child_handle| {
                                tree.alloc
                                    .get(child_handle)
                                    .max_height
                                    .load(Ordering::SeqCst)
                            })
                            .max()
                            .unwrap()
                            + 1
                    }
                    None => {
                        // our children got reclaimed,
                        // but we still need to update the height.
                        0
                    }
                };

                match node.max_height.compare_exchange(
                    old_max_height,
                    new_max_height,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(prev) => {
                        assert_eq!(prev, old_max_height);
                        break;
                    }
                    Err(actual_cur) => {
                        assert_ne!(
                            actual_cur, old_max_height,
                            "hopefully guaranteed by `compare_exchange` not being weak"
                        );
                        // someone else updated the height,
                        // but they may have not seen our updates to the children,
                        // so we must retry.
                        continue;
                    }
                }
            }
        }
    }

    /// updates the `timestamp` of the node at `node_handle` and all its ancestors.
    // note we also update timestamps in `insert`,
    // but there we're already going down the path to the node.
    #[cfg_attr(feature = "profiling", inline(never))]
    fn update_ancestor_timestamps(&self, node_handle: NodeHandle, now: RenderMoment) {
        // if we go bottom up, we can avoid trying to update our ancestors' timestamp
        // if we find out we are up to date.
        // but this isn't faster bc you need to need to fetch the ancestors anyway
        // when we construct the path down.
        // so just go top down.

        let target_mid = unsafe { self.alloc.get(node_handle).dom().mid() };
        #[expect(unused_variables)]
        let node_handle = ();

        let mut node_handle = self.root;
        loop {
            let node = self.alloc.get(node_handle);

            let _ = node.update_timestamp(now);

            let dom = unsafe { node.dom() };
            if dom.mid() == target_mid {
                return;
            }
            match node.children_handle.load(Ordering::SeqCst) {
                Some(children_handle) => {
                    let child_offset = dom.child_offset_containing(target_mid);
                    let child_handle = children_handle.offset(child_offset);
                    node_handle = child_handle;
                }
                None => return,
            }
        }
    }

    /// for a node to have rad <= retire_rad,
    /// it must have depth >= ret.
    ///
    /// returns `None` if there the depth needed for rad
    /// is deeper than `Domain` can represent
    #[cfg_attr(feature = "profiling", inline(never))]
    fn depth_needed_for_rad(&self, retire_rad: Real) -> Option<u16> {
        let mut depth = 0;
        let mut rad = self.dom.rad();
        while rad > retire_rad {
            depth += 1;
            rad = rad.div2_exact_checked()?;
        }
        Some(depth)
    }

    /// we're allowed to retire a node if it has depth >= ret.
    #[cfg_attr(feature = "profiling", inline(never))]
    fn depth_needed_for_window(&self, retire_window: Window) -> Result<u16, &'static str> {
        let Some(retire_scale) =
            Real::try_from_f64(1.0 / RETIRE_MAX_WIDTH.load(Ordering::SeqCst) as f64)
        else {
            return Err("failed to compute reclaim scale, RETIRE_MAX_WIDTH is too big");
        };

        let Some(retire_rad) = retire_scale.mul_checked(retire_window.real_rad()) else {
            return Err("retire_window is too big/small to reclaim anything");
        };

        match self.depth_needed_for_rad(retire_rad) {
            Some(depth) => Ok(depth),
            None => Err("depth needed for rad is deeper than `Domain` can represent"),
        }
    }

    /// selects and retires a block of siblings.
    /// returns `None` if we shouldn't/can't retire.
    ///
    /// the siblings should be eventually freed to not leak memory.
    ///
    /// we guarantee that the siblings and their descendants are inaccessible from the root.
    /// note that other threads can still have access to them via direct handles.
    /// we guarantee that the siblings are inaccessible from any non-direct handle.
    /// we guarantee that the siblings and their descendants will eventually be inaccessible except through the returned handle.
    /// we guarantee that eventually all direct handles to the sibling must have been derived from the returned handle.
    /// TODO: more docs / proof
    ///
    /// we guarantee that there will eventually no other thread can access the siblings.
    ///
    /// note that we don't guarantee that the siblings are leafs.
    ///
    /// `now` is used to update timestamps of the retired nodes' ancestors, so they get redrawn,
    /// and is *not* from the clock used to prove freeing is correct.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn retire(
        &self,
        tree_local: &mut TreeLocal,
        window: Window,
        now: RenderMoment,
    ) -> Option<BlockHandle> {
        /// returns the parent of the sibling block to retire.
        /// must have depth >= `retire_depth`.
        #[cfg_attr(feature = "profiling", inline(never))]
        fn select(
            tree: &Tree,
            stack: &mut Vec<(NodeHandle, u16)>,
            retire_depth: u16,
        ) -> impl Iterator<Item = NodeHandle> {
            stack.clear();
            stack.push((tree.root, 0));
            std::iter::from_fn(move || {
                while let Some((node_handle, depth)) = stack.pop() {
                    let node = tree.alloc.get(node_handle);

                    let Some(children_handle) = node.children_handle.load(Ordering::SeqCst) else {
                        // don't explore or select leafs
                        continue;
                    };

                    // TODO: don't do this for debugging,
                    // so we can test retiring nodes whose parents have been retired but not freed
                    if depth >= retire_depth {
                        // if we fail to retire this node,
                        // don't explore its children
                        // because those will get freed with the node by a different thread.
                        // (it would still be correct, but it's wasted work)
                        return Some(node_handle);
                    }

                    let max_height = node.max_height.load(Ordering::SeqCst);
                    let deepest_descendant_leaf_depth = max_height + depth;
                    // > bc we need internal nodes, not leafs.
                    if deepest_descendant_leaf_depth > retire_depth {
                        stack.extend(
                            children_handle
                                .siblings()
                                .map(|child_handle| (child_handle, depth + 1)),
                        );
                    }
                }
                None
            })
        }

        let retire_depth = match self.depth_needed_for_window(window) {
            Ok(depth) => depth,
            Err(err) => {
                log!(err);
                return None;
            }
        };

        let vec_handle_u16 = &mut tree_local.vec_handle_u16;
        let vec_handle = &mut tree_local.vec_handle;

        for node_handle in select(self, vec_handle_u16, retire_depth) {
            // erase the child pointer.
            // we can't deinit the children's fields at this time
            // because other threads can still be looking at the children,
            // and we need their child pointers later.
            let node = self.alloc.get(node_handle);
            let block_handle = match node.children_handle.swap(None, Ordering::SeqCst) {
                Some(block_handle) => {
                    // note that i call this `block_handle` and not `children_handle`
                    // bc they're no longer anyone's children.
                    block_handle
                }
                None => {
                    // log!("someone else reclaimed the node we selected");
                    continue;
                }
            };

            self.update_ancestor_heights(vec_handle, node_handle);
            self.update_ancestor_timestamps(node_handle, now);

            return Some(block_handle);
        }

        None
    }

    /// frees the siblings and all their (accessible) descendants.
    ///
    /// SAFETY: the caller must ensure that no other thread can have access to the siblings or any of their descendants.
    /// this is done by waiting at least two (or maybe three) ticks/epochs after retirement.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) unsafe fn free(
        &self,
        tree_local: &mut TreeLocal,
        alloc_local: &mut AllocLocal,
        block_handle: BlockHandle,
    ) {
        let stack = &mut tree_local.vec_handle4;
        stack.clear();
        stack.push(block_handle);
        while let Some(block_handle) = stack.pop() {
            for sibling_handle in block_handle.siblings() {
                let sibling = self.alloc.get(sibling_handle);
                if let Some(children_handle) = sibling.children_handle.swap(None, Ordering::SeqCst)
                {
                    stack.push(children_handle);
                }
            }
            unsafe {
                self.alloc.free(alloc_local, block_handle);
            }
        }
    }

    /// returns `None` if we shouldn't/can't refine.
    /// returns handles to nodes who we need to sample.
    ///
    /// to select the leaf to split, we require that it
    /// - overlaps the window
    /// - is among the shallowest such leafs
    ///
    /// we need `now` in order to initialize the timestamps of the new nodes,
    /// but we don't update the timestamps of the ancestors here, we do that in [`insert`].
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn refine(
        &self,
        tree_local: &mut TreeLocal,
        alloc_local: &mut AllocLocal,
        sample_window: Window,
        retire_window: Option<Window>,
        now: RenderMoment,
    ) -> Option<[(Real, Imag); 4]> {
        // used for filtering out leafs that would be immediately reclaimed.
        let retire_depth = if SPLIT_RETIRABLE_NODES.load(Ordering::Relaxed) {
            None
        } else {
            match retire_window {
                Some(retire_window) => match self.depth_needed_for_window(retire_window) {
                    Ok(depth) => Some(depth),
                    Err(err) => {
                        log!(err);
                        None
                    }
                },
                None => None,
            }
        };

        // // debug disabled to make more races happen
        // let retire_rad = None;
        let shallowest_depth = depth_of_shallowest_leaf_overlapping_window(
            self,
            tree_local,
            sample_window,
            retire_depth,
        )?;

        // #[cfg(false)]
        // {
        //     let shallowest_depth_oracle = depth_of_shallowest_leaf_oracle(self, window, data)?;
        //     log!(
        //         "oracle: {}, actual: {}",
        //         shallowest_depth_oracle, shallowest_depth
        //     );
        // }

        let block_handle = self.alloc.alloc(alloc_local);

        // initialize the speculative children except for dom, which we don't know yet.
        // actually, also init the dom with a non uninit sentinel for debugging,
        // bc i like assert that nodes are either init or uninit and never partially init.
        for sibling_handle in block_handle.siblings() {
            let sibling = self.alloc.get_uninit(sibling_handle);

            unsafe {
                sibling.write_dom(Domain::default());
            }
            sibling.children_handle.store(None, Ordering::Relaxed);
            sibling.color.store(None, Ordering::Relaxed);
            sibling.min_height.store(0, Ordering::Relaxed);
            sibling.max_height.store(0, Ordering::Relaxed);
            sibling.timestamp.store(now, Ordering::Relaxed);
        }

        let vec_handle_u16 = &mut tree_local.vec_handle_u16;
        let vec_handle = &mut tree_local.vec_handle;

        // let mut debug_attempts = 0;
        let draw_uncolored_nodes = DRAW_UNCOLORED_NODES.load(Ordering::Relaxed);
        for leaf_handle in select(self, vec_handle_u16, sample_window, shallowest_depth) {
            // debug_attempts += 1;
            if let Some(()) = try_split(self, leaf_handle, block_handle) {
                self.update_ancestor_heights(vec_handle, leaf_handle);
                // this should be done in insert,
                // but this allows us to debug draw uncolored nodes.
                // (uncolored nodes would still sometimes get drawn,
                // but that's just bc their timestamp got updated by someone else.)
                if draw_uncolored_nodes {
                    self.update_ancestor_timestamps(leaf_handle, now);
                }
                return Some(
                    block_handle
                        .siblings()
                        .map(|h| unsafe { self.alloc.get(h).dom().mid() }),
                );
            }
        }
        // log!(format!(
        //     "found {} leafs to try splitting but they all failed",
        //     debug_attempts
        // ));

        unsafe {
            self.alloc.free(alloc_local, block_handle);
        }

        // everything below this is helper function definitions.
        return None;

        /// returns the depth of the shallowest leaf that overlaps the window.
        /// returns `None` if there are no such leafs.
        /// oracle without using the cached height.
        #[cfg_attr(feature = "profiling", inline(never))]
        #[cfg(false)]
        fn depth_of_shallowest_leaf_oracle(
            tree: &Tree,
            tree_local: &mut TreeLocal,
            window: Window,
        ) -> Option<u16> {
            let stack = &mut tree_local.vec_handle_u16;
            stack.clear();
            stack.push((tree.root, 0));
            let mut shallowest_depth = u16::MAX;
            while let Some((node_handle, depth)) = stack.pop() {
                if depth >= shallowest_depth {
                    continue;
                }
                let node = tree.alloc.get(node_handle);
                let dom = unsafe { node.dom() };
                // TODO: instead of doing this check on pop, do it on push
                // this also lets us do less work in the case where the domain is contained inside the window
                if !window.overlaps(dom) {
                    continue;
                }
                if let Some(children_handle) = node.children_handle.load(Ordering::SeqCst) {
                    // let height = node.height.load(Ordering::SeqCst);
                    // let shallowest_descendant_leaf_depth = height + depth;
                    // if shallowest_descendant_leaf_depth >= shallowest_depth {
                    //     continue;
                    // }
                    // if window.contains(dom) {
                    //     if shallowest_descendant_leaf_depth < shallowest_depth {
                    //         shallowest_depth = shallowest_descendant_leaf_depth;
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
                    stack.extend(
                        children_handle
                            .siblings()
                            .map(|child_handle| (child_handle, depth + 1)),
                    );
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

        /// returns the depth of the shallowest leaf that overlaps the window.
        /// returns `None` if there are no such leafs.
        #[cfg_attr(feature = "profiling", inline(never))]
        fn depth_of_shallowest_leaf_overlapping_window(
            tree: &Tree,
            tree_local: &mut TreeLocal,
            window: Window,
            retire_depth: Option<u16>,
        ) -> Option<u16> {
            let stack = &mut tree_local.vec_handle_u16;
            stack.clear();
            stack.push((tree.root, 0));
            // this makes checks for retire_depth get subsumed by checks for shallowest_depth.
            let initial_bound = retire_depth.unwrap_or(u16::MAX);
            let mut shallowest_depth = initial_bound;
            while let Some((node_handle, depth)) = stack.pop() {
                // quick check bc the bound may have improved since we pushed this node.
                if depth >= shallowest_depth {
                    continue;
                }

                let node = tree.alloc.get(node_handle);
                let dom = unsafe { node.dom() };

                // if we don't overlap the window,
                // then none of our descendants can overlap the window.
                if !window.overlaps(dom) {
                    continue;
                }

                match node.children_handle.load(Ordering::Acquire) {
                    Some(children_handle) => {
                        let min_height = node.min_height.load(Ordering::Relaxed);
                        let shallowest_descendant_leaf_depth = min_height + depth;

                        // prune if no descendant leaf can improve the bound
                        // (also subsumes the retire_depth prune since shallowest_depth <= retire_depth).
                        if shallowest_descendant_leaf_depth >= shallowest_depth {
                            continue;
                        }

                        // if the domain is contained in the window,
                        // then all descendant leaves are also contained in (and therefore overlapping) the window,
                        // so we can just used the cached depth,
                        // and not explore the children (if any).
                        if window.contains(dom) {
                            if shallowest_descendant_leaf_depth < shallowest_depth {
                                shallowest_depth = shallowest_descendant_leaf_depth;
                            }
                            continue;
                        }

                        #[cfg(false)]
                        {
                            let oracle_height = children_handle
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
                            if min_height != oracle_height {
                                log!(format!(
                                    "cached height was {}, but actual height is {}",
                                    min_height, oracle_height
                                ));
                            }
                        }

                        // TODO: sort to do principal variation search,
                        // so we can find a shallow leaf faster, which lets us pune more.
                        // we should look at the child closest to the center of the window first.
                        // or maybe look at the child with the shallowest height.
                        // actually it doesn't seem faster

                        stack.extend(
                            children_handle
                                .siblings()
                                .map(|child_handle| (child_handle, depth + 1)),
                        );

                        // let mut children = child_handle.siblings();
                        // // bring min_height to front
                        // let offset = (0..children.len())
                        //     .min_by_key(|i| {
                        //         tree.alloc
                        //             .get(children[*i])
                        //             .min_height
                        //             .load(Ordering::SeqCst)
                        //     })
                        //     .unwrap();
                        // children.swap(0, offset);
                        // stack.extend(children.into_iter().map(|child_handle| (child_handle, depth + 1)));

                        // // bring child in quadrant containing window mid
                        // let offset = dom.quadrant_offset_containing(window.mid());
                        // let mut children = child_handle.siblings();
                        // children.swap(0, offset);
                        // stack.extend(children.into_iter().map(|child_handle| (child_handle, depth + 1)));
                    }
                    None => {
                        if depth < shallowest_depth {
                            shallowest_depth = depth;
                        }
                    }
                }
            }

            if shallowest_depth == initial_bound {
                None
            } else {
                Some(shallowest_depth)
            }
        }

        // TODO: this could be cached until the window changes.
        // possibly cached by the main thread? but then you need synced pops?
        // actually this would be handles persisting across method calls, which is invalid.
        // maybe we can relax it to persisting for a bounded number of ticks,
        // and then keep stuff in the nursing home for longer?

        // could we cache just the depth?
        // (and invalidate the cache if there were no leaves at that depth?
        // just running depth_of_shallowest_leaf_overlapping_window with a tighter initial bound probably isn't faster.)
        // maybe we can also invalidate the cache if any retire occurs (it would be eventually consistent)?
        // note the depth is monotonically increasing for a constant window in the absence of retires.
        // how to communicate that any reclaim occurred?
        // we can also just invalidate it every reclamation tick? which would give us ~100 cache hits?

        // TODO: ask ai for ideas

        /// returns an iterator over the leaves that overlap the window with the target depth.
        #[cfg_attr(feature = "profiling", inline(never))]
        fn select(
            tree: &Tree,
            stack: &mut Vec<(NodeHandle, u16)>,
            window: Window,
            shallowest_depth: u16,
        ) -> impl Iterator<Item = NodeHandle> {
            stack.clear();
            stack.push((tree.root, 0));
            std::iter::from_fn(move || {
                while let Some((node_handle, depth)) = stack.pop() {
                    debug_assert!(depth <= shallowest_depth);

                    let node = tree.alloc.get(node_handle);
                    let dom = unsafe { node.dom() };

                    // if we don't overlap the window,
                    // then none of our descendants can overlap the window.
                    if !window.overlaps(dom) {
                        continue;
                    }

                    match node.children_handle.load(Ordering::Acquire) {
                        Some(children_handle) => {
                            let min_height = node.min_height.load(Ordering::Relaxed);
                            // do this in case min_height is stale,
                            // to make debug_assert!(depth <= shallowest_depth); pass.
                            let min_height = min_height.max(1);
                            let shallowest_descendant_leaf_depth = min_height + depth;

                            // prune if no descendant leaf can have the target depth.
                            if shallowest_descendant_leaf_depth > shallowest_depth {
                                continue;
                            }

                            // TODO: sort to do principal variation search,
                            // so we can find a shallow leaf faster, which lets us pune more.
                            // we should look at the child closest to the center of the window first.
                            // or maybe look at the child with the shallowest height.
                            stack.extend(
                                children_handle
                                    .siblings()
                                    .map(|child_handle| (child_handle, depth + 1)),
                            );
                        }
                        None => {
                            if depth == shallowest_depth {
                                return Some(node_handle);
                            }
                        }
                    }
                }
                None
            })
        }

        #[cfg_attr(feature = "profiling", inline(never))]
        fn try_split(
            tree: &Tree,
            leaf_handle: NodeHandle,
            block_handle: BlockHandle,
        ) -> Option<()> {
            let leaf = tree.alloc.get(leaf_handle);

            let leaf_dom = unsafe { leaf.dom() };

            // initialize the sibling's dom
            {
                let Some(doms) = leaf_dom.split() else {
                    log!("leaf_dom.split() is None");
                    return None;
                };
                for (offset, dom) in doms.into_iter().enumerate() {
                    let sibling_handle = block_handle.offset(offset);
                    let sibling = tree.alloc.get(sibling_handle);

                    // SAFETY: we allocated the children and never gave them to anyone,
                    // so we have exclusive access.
                    unsafe {
                        sibling.write_dom(dom);
                    }
                }
            }

            // fence(Ordering::Release);

            // this is the linearization point
            match leaf.children_handle.compare_exchange_weak(
                None,
                Some(block_handle),
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(prev) => {
                    // log!("swap succeeded");
                    assert_eq!(
                        prev, None,
                        "this is guaranteed by compare_exchange_weak, despite it being documented incorrectly"
                    );
                    Some(())
                }
                Err(_actual_cur) => {
                    // log!("children_handle was not already `None`");
                    None
                }
            }
        }
    }

    /// inserts the previously reserved sample into the node.
    /// use a point rather than a node handle to make reclaiming work.
    /// also updates the render timestamps of the ancestors.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn insert(
        &self,
        _tree_local: &mut TreeLocal,
        (real, imag): (Real, Imag),
        color: Color32,
        now: RenderMoment,
    ) {
        let mut node_handle = self.root;
        // find the node, updating timestamps on the way down.
        loop {
            let node = self.alloc.get(node_handle);

            // we should only do this if the insert succeeds,
            // though it fails rarely enough that it's probably not worth
            // the cost of traversing the path twice.
            let _ = node.update_timestamp(now);

            // break if we found the node.
            let dom = unsafe { node.dom() };
            if dom.mid() == (real, imag) {
                node.color
                    .store(Some(color.try_into().unwrap()), Ordering::SeqCst);
                break;
            }

            // go to the correct child.
            let Some(children_handle) = node.children_handle.load(Ordering::SeqCst) else {
                // log!(
                //     "failed to follow child pointer during insert. this probably means it got reclaimed."
                // );
                return;
            };
            let child_offset = dom.child_offset_containing((real, imag));
            node_handle = children_handle.offset(child_offset);
        }
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn any_on_line_needs_redraw(
        &self,
        tree_local: &mut TreeLocal,
        real_lo: Real,
        real_hi: Real,
        imag: Imag,
        prev_frame_start: RenderMoment,
    ) -> bool {
        debug_assert!(real_lo <= real_hi);

        let stack = &mut tree_local.vec_handle;
        stack.clear();
        stack.push(self.root);

        // if we have a segment that's definitely good, we stop exploring.
        // if we have a segment that's definitely bad, we fail.
        // to know that a segment is definitely bad, it must be a leaf.
        while let Some(node_handle) = stack.pop() {
            let node = self.alloc.get(node_handle);
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

            if let Some(children_handle) = node.children_handle.load(Ordering::Acquire) {
                stack.extend(children_handle.siblings());
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
    /// returns white if not in the trees domain.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn color_of_pixel(
        &self,
        _tree_local: &mut TreeLocal,
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

        let pixel_mid = pixel.mid();
        // we never touch pixel again
        #[expect(unused_variables)]
        let pixel = ();

        if !self.dom.contains_point(pixel_mid) {
            const UNCONTAINED_COLOR: Color32 = Color32::WHITE;
            return Some(UNCONTAINED_COLOR);
        }

        let mut node_handle = self.root;
        let mut closest_sample_dist = distance(pixel_mid, self.dom.mid());
        // TODO: we don't need to maintain this in the common case.
        // the closest_sample_color is only ever not the color of the closest_node.dom.mid
        // when the closest_node doesn't have a color, which is rare
        let mut closest_sample_color = self
            .alloc
            .get(node_handle)
            .color
            .load(Ordering::Relaxed)
            .expect("root must have a color");

        let uncolored_node_color = if DRAW_UNCOLORED_NODES.load(Ordering::Relaxed) {
            Some(Rgb::new(255, 255, 0))
        } else {
            None
        };
        loop {
            let node = self.alloc.get(node_handle);
            let dom = unsafe { node.dom() };

            // check whether the node's timestamp proves that the color hasn't changed since the last frame
            {
                let timestamp = node.timestamp.load(Ordering::Relaxed);
                // TODO: <=?
                if timestamp < prev_frame_start {
                    // let elapsed = start.elapsed();
                    return None;
                }
            }

            // update color
            {
                let dist = distance(pixel_mid, dom.mid());
                let color = node.color.load(Ordering::Relaxed);

                if dist < closest_sample_dist
                    && let Some(color) = color.or(uncolored_node_color)
                {
                    closest_sample_dist = dist;
                    closest_sample_color = color;
                    // debug_closest_sample_depth = debug_explored_depth;
                }
            }

            // i++
            {
                let Some(children_handle) = node.children_handle.load(Ordering::Acquire) else {
                    break;
                };
                let child_offset = dom.child_offset_containing(pixel_mid);
                node_handle = children_handle.offset(child_offset);
            }
        }

        Some(closest_sample_color.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depth_needed_for_rad() {
        let tree = Tree::new(&mut TreeLocal::default(), &mut AllocLocal::default());
        assert_eq!(tree.dom.rad(), Real::from_f64(4.0));
        assert_eq!(tree.depth_needed_for_rad(Real::from_f64(6.0)), Some(0));
        assert_eq!(tree.depth_needed_for_rad(Real::from_f64(5.0)), Some(0));
        assert_eq!(tree.depth_needed_for_rad(Real::from_f64(4.0)), Some(0));
        assert_eq!(tree.depth_needed_for_rad(Real::from_f64(3.0)), Some(1));
        assert_eq!(tree.depth_needed_for_rad(Real::from_f64(2.0)), Some(1));
        assert_eq!(tree.depth_needed_for_rad(Real::from_f64(1.0)), Some(2));
    }
}
