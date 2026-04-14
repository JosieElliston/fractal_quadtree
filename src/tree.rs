use std::sync::atomic::{AtomicU32, Ordering};

use atomic::Atomic;
use eframe::egui::Color32;

use crate::{
    complex::{Domain, Pixel, Window, fixed::*},
    sample::metabrot_sample,
};

pub(crate) static ELAPSED_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `None` iff self == 0
/// `Some` iff alpha == 255
#[repr(C, align(4))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
struct OptionColor(Color32);
// unsafe impl bytemuck::ZeroableInOption for NodeId {}
// unsafe impl bytemuck::PodInOption for NodeId {}
impl OptionColor {
    const NONE: Self = Self(Color32::TRANSPARENT);

    fn new_some(color: Color32) -> Self {
        debug_assert!(color.a() == 255);
        Self(color)
    }

    fn is_some(&self) -> bool {
        debug_assert!(self.0.a() == 0 || self.0.a() == 255);
        self.0.a() == 255
    }

    fn is_none(&self) -> bool {
        debug_assert!(self.0.a() == 0 || self.0.a() == 255);
        self.0.a() == 0
    }

    fn unwrap(&self) -> Color32 {
        assert!(self.is_some());
        self.0
    }

    fn expect(&self, msg: &str) -> Color32 {
        assert!(self.is_some(), "{}", msg);
        self.0
    }
}

#[repr(C, align(64))]
#[derive(Debug)]
struct Node {
    dom: Domain,
    leaf_distance_cache: AtomicU32,
    // TODO: remove Atomic wrapper around color maybe
    color: Atomic<OptionColor>,
    /// leftmost child id
    left_child: Atomic<Option<NodeHandle>>,
    _pad: [u8; 24],
}
const _: () = assert!(size_of::<Node>() == 64);
const _: () = assert!(align_of::<Node>() == 64);
const _: () = assert!(Atomic::<OptionColor>::is_lock_free());
const _: () = assert!(Atomic::<Option<NodeHandle>>::is_lock_free());
impl Node {
    fn uninit() -> Self {
        Self {
            dom: Domain::uninit(),
            leaf_distance_cache: AtomicU32::new(0),
            color: Atomic::new(OptionColor::NONE),
            left_child: Atomic::new(None),
            _pad: Default::default(),
        }
    }

    fn new_leaf_uncolored(dom: Domain) -> Self {
        Self {
            dom,
            leaf_distance_cache: AtomicU32::new(0),
            color: Atomic::new(OptionColor::NONE),
            left_child: Atomic::new(None),
            _pad: Default::default(),
        }
    }

    fn new_leaf_colored(dom: Domain, color: Color32) -> Self {
        Self {
            dom,
            leaf_distance_cache: AtomicU32::new(0),
            color: Atomic::new(OptionColor::new_some(color)),
            left_child: Atomic::new(None),
            _pad: Default::default(),
        }
    }

    /// the caller probably should ensure that we have exclusive access to the child,
    /// tho maybe it's fine even without.
    unsafe fn dom_write(&self, dom: Domain) {
        unsafe {
            (&raw const self.dom as *mut Domain).write(dom);
            // (child as *const Node as *mut Node).as_mut().unwrap().dom = dom;
        }
    }
}

pub(crate) use alloc3::*;
mod alloc3 {
    use std::{
        num::NonZeroUsize,
        ptr::NonNull,
        sync::atomic::{AtomicPtr, AtomicUsize},
    };

    use atomic::Atomic;

    use super::*;

    // #[repr(transparent)]
    // #[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
    // pub(crate) struct NodeHandle(NonZeroU32);
    // unsafe impl bytemuck::ZeroableInOption for NodeHandle {}
    // unsafe impl bytemuck::PodInOption for NodeHandle {}

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
            NodeHandle(NonZeroUsize::new(ret).unwrap())
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

        pub(super) fn siblings(self) -> [NodeHandle; 4] {
            let block = self.to_block();
            let i = self.to_index();
            debug_assert_eq!(i % 4, 0, "unaligned handle in siblings");
            [
                NodeHandle::new(block, i),
                NodeHandle::new(block, i + 1),
                NodeHandle::new(block, i + 2),
                NodeHandle::new(block, i + 3),
            ]
        }
    }
    impl std::fmt::Debug for NodeHandle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
    struct Block {
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
        last: AtomicPtr<Block>,
        // last_len: AtomicUsize,
        // TODO: use this caching thing
        // /// if you allocate a block but the CAS fails
        // /// (bc another thread already allocated a block),
        // /// instead of deallocating, store it for the next time we need to allocate a block.
        // /// null if there's no cached block.
        // local_cache: Box<[*mut Block]>,
    }
    impl Default for Alloc {
        fn default() -> Self {
            Self::new()
        }
    }
    impl Alloc {
        // fn new(thread_count: usize) -> Self {
        fn new() -> Self {
            Self {
                last: AtomicPtr::new(Box::into_raw(Box::new(Block::with_prev(None)))),
                // last_len: AtomicUsize::new(0),
                // local_cache: vec![std::ptr::null_mut(); thread_count].into_boxed_slice(),
            }
        }

        fn realloc(&self, old_last: NonNull<Block>) {
            let new_block = Box::into_raw(Box::new(Block::with_prev(Some(old_last))));
            match self.last.compare_exchange_weak(
                old_last.as_ptr(),
                new_block,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    // successfully swapped in the new block, so we can just use it
                    // self.local_cache[thread_i] = std::ptr::null_mut();
                }
                Err(actual) => {
                    // another thread already swapped in a new block, so we can just use that one
                    // self.local_cache[thread_i] = new_block;
                    unsafe { drop(Box::from_raw(new_block)) };
                    // debug_assert_ne!(actual, old_last.as_ptr(), "bc of _weak, can actually sometimes happen");
                }
            }
        }

        fn alloc<const N: usize>(&self) -> NodeHandle {
            // loop bc it's technically possible that after reallocating
            // or during waiting for another thread to reallocate,
            // we go to sleep and the new block fills up.
            loop {
                let last = unsafe { self.last.load(Ordering::SeqCst).as_ref().unwrap() };
                let len = last.foot.len.fetch_add(N, Ordering::SeqCst);
                if len + N <= Block::CAPACITY {
                    return NodeHandle::new(last.into(), len);
                }
                // we could say that whoever got len == Block::CAPACITY is responsible for reallocating,
                // but what if that thread went to sleep during reallocation?
                // so just have all the threads realloc
                self.realloc(last.into());
            }
        }

        pub(super) fn alloc4(&self) -> NodeHandle {
            self.alloc::<4>()
        }

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

#[derive(Debug)]
pub(crate) struct Tree {
    dom: Domain,
    alloc: Alloc,
    root: NodeHandle,
}

impl Tree {
    pub(crate) fn new() -> Self {
        let dom = Domain::default();
        let color = OptionColor::new_some(metabrot_sample(dom.mid()).color());
        let alloc = Alloc::default();

        // we leave 3 nodes uninit
        let root_handle = alloc.alloc4();
        let root = alloc.get(root_handle);
        unsafe { root.dom_write(dom) };
        root.leaf_distance_cache.store(0, Ordering::Relaxed);
        root.color.store(color, Ordering::Relaxed);
        root.left_child.store(None, Ordering::Relaxed);

        Self {
            dom,
            alloc,
            root: root_handle,
        }
    }

    pub(crate) fn node_count(&self) -> usize {
        let mut count = 0;
        let mut stack = Vec::with_capacity(64);

        stack.push(self.root);

        while let Some(handle) = stack.pop() {
            count += 1;
            if let Some(child_id) = self.alloc.get(handle).left_child.load(Ordering::SeqCst) {
                stack.extend(child_id.siblings());
            }
        }
        count
    }

    pub(crate) fn mid_of_node_id(&self, handle: NodeHandle) -> (Real, Imag) {
        self.alloc.get(handle).dom.mid()
    }

    // /// ensures that we have < n nodes
    // /// or maybe that each pixel contains at most n leaves
    // /// or maybe if you're in the window, you get at most subsamples leaves,
    // /// if you're not in the window, you all collectively get m leaves
    // fn prune(&mut self, window: Window, pixel_width: f32, n: u32, subsamples: u8) {
    //     todo!()
    // }

    // /// the average color of leaves inside the pixel weighted by area that's overlapping the pixel
    // /// or maybe weighted by distance to the center of the pixel
    // /// the color of the highest node contained in pixel
    // fn color(&self, pixel: Domain) -> Option<Color32> {
    //     if !self.window.overlaps(pixel) {
    //         return None;
    //     }
    //     if self.window <= pixel {
    //         return Some(self.color);
    //     }
    //     if self.is_leaf() {
    //         // we're too zoomed in
    //         return None;
    //     }
    //     // TODO: i think it's actually possible that it's not the child closest to the pixel center that has a child eventually inside pixel
    //     let closest_child_i = self
    //         .child_i_closest_to(pixel.real_mid(), pixel.imag_mid())
    //         .unwrap();
    //     self.children.as_ref().unwrap()[closest_child_i].color(pixel)
    // }

    // /// average color of samples inside the pixel
    // #[inline(never)]
    // pub(crate) fn color_in_pixel(&self, pixel: Domain) -> ColorBuilder {
    //     let d = f32::max(
    //         (self.dom.real_mid() - pixel.real_mid()).abs(),
    //         (self.dom.imag_mid() - pixel.imag_mid()).abs(),
    //     );
    //     // if !self.dom.overlaps(pixel) {
    //     if d > self.dom.rad() + pixel.rad() {
    //         return ColorBuilder::default();
    //     }
    //     // (if pixel.contains_point(self.dom.real_mid(), self.dom.imag_mid()) {
    //     (if d <= pixel.rad() {
    //         self.color.into()
    //     } else {
    //         ColorBuilder::default()
    //     } + match &self.children {
    //         Some(children) => {
    //             // if pixel.contains_square(self.dom) {
    //             if d <= pixel.rad() - self.dom.rad() {
    //                 children.iter().map(|c| c.color()).sum()
    //             } else {
    //                 children.iter().map(|c| c.color_in_pixel(pixel)).sum()
    //             }
    //         }
    //         None => ColorBuilder::default(),
    //     })
    // }

    /// returns `None` if we shouldn't/can't refine
    /// returns the points we need to sample
    /// we split a `LeafColor` into a `Internal([LeafReserved; 4])`)
    ///
    /// to select the node, we require that it
    /// - intersects the window
    /// - is among the shallowest leafs
    // TODO: we don't actually need the leaf to be colored to split it
    // TODO: better ordering
    #[cfg_attr(feature = "profiling", inline(never))]
    // pub(crate) fn refine(slf: Arc<Self>, window: Window) -> Option<[NodeId; 4]> {
    pub(crate) fn refine(&self, window: Window) -> Option<[NodeHandle; 4]> {
        /// returns the shallowest leaf with color that intersects the window, and its depth.
        /// returns `None` if no leaf intersects the window.
        #[cfg_attr(feature = "profiling", inline(never))]
        fn get_shallowest_leaf(tree: &Tree, window: Window) -> Option<(u32, NodeHandle)> {
            // TODO: avoid this allocation
            let mut stack = Vec::with_capacity(64);
            stack.push((tree.root, 0));
            let mut shallowest_depth = u32::MAX;
            let mut shallowest_leaf_id = None;
            while let Some((node_id, depth)) = stack.pop() {
                let node = tree.alloc.get(node_id);
                // TODO: instead of doing this check on pop, do it on push
                // this also lets us do less work in the case where the domain is contained inside the window
                if !window.overlaps(node.dom) {
                    continue;
                }
                if depth >= shallowest_depth {
                    continue;
                }
                if let Some(child_id) = node.left_child.load(Ordering::SeqCst) {
                    // TODO: restore leaf distance cache stuff

                    let leaf_distance = child_id
                        .siblings()
                        .map(|child_handle| {
                            tree.alloc
                                .get(child_handle)
                                .leaf_distance_cache
                                .load(Ordering::SeqCst)
                        })
                        .iter()
                        .min()
                        .unwrap()
                        + 1;
                    // if leaf_distance < tree.alloc.get_leaf_distance_cache(node_id) {
                    //     // let node_id = tree.alloc.promote(node_id);
                    //     tree.alloc.update_with(node_id, |node| {
                    //         let _ = node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
                    //             if leaf_distance < node.leaf_distance_cache {
                    //                 Some(Node {
                    //                     leaf_distance_cache: leaf_distance,
                    //                     ..node
                    //                 })
                    //             } else {
                    //                 None
                    //             }
                    //         });
                    //     });
                    //     // tree.alloc.demote(node_id);
                    //     // node.leaf_distance_cache = leaf_distance;
                    //     // todo!("this is incorrect now");
                    // }

                    // node.leaf_distance_cache.fetch_min(leaf_distance, Ordering::Relaxed);
                    // TODO: why was i mining and not maxing?
                    // TODO: just update the distance on insert and don't do stuff here
                    node.leaf_distance_cache
                        .store(leaf_distance, Ordering::SeqCst);
                    if leaf_distance + depth >= shallowest_depth {
                        continue;
                    }

                    // // TODO: does this sort the nodes in place???
                    // let mut children: [&mut Node; 4] = children.each_mut();
                    // children.sort_by_key(|c| c.leaf_distance_cache());
                    // stack.extend(children.into_iter().map(|c| (c, depth + 1)));
                    stack.extend(child_id.siblings().map(|c| (c, depth + 1)).into_iter());
                } else {
                    let color = node.color.load(Ordering::SeqCst);
                    // if color.is_none() {
                    //     println!("found uncolored leaf {:?} at depth {}", node_id, depth);
                    // }
                    if color.is_some() && depth < shallowest_depth {
                        shallowest_depth = depth;
                        shallowest_leaf_id = Some(node_id);
                    }
                }
            }
            Some((shallowest_depth, shallowest_leaf_id?))
        }

        fn colored_leafs_in_window_at_depth(
            tree: &Tree,
            window: Window,
            shallowest_depth: u32,
        ) -> impl Iterator<Item = NodeHandle> {
            // TODO: avoid this allocation
            let mut stack = Vec::with_capacity(64);
            stack.push((tree.root, 0));
            std::iter::from_fn(move || {
                while let Some((node_id, depth)) = stack.pop() {
                    if !window.overlaps(tree.alloc.get(node_id).dom) {
                        continue;
                    }
                    if depth > shallowest_depth {
                        continue;
                    }
                    if let Some(child_id) =
                        tree.alloc.get(node_id).left_child.load(Ordering::SeqCst)
                    {
                        // don't bother updating the caches
                        let leaf_distance = tree
                            .alloc
                            .get(node_id)
                            .leaf_distance_cache
                            .load(Ordering::SeqCst);
                        if leaf_distance + depth > shallowest_depth {
                            continue;
                        }
                        stack.extend(child_id.siblings().map(|c| (c, depth + 1)).into_iter());
                    } else {
                        let color = tree.alloc.get(node_id).color.load(Ordering::SeqCst);
                        if color.is_some() && depth == shallowest_depth {
                            return Some(node_id);
                        }
                    }
                }
                None
            })
        }

        // TODO: is this really correct?
        let (shallowest_depth, handle) = get_shallowest_leaf(self, window)?;

        // assert!(node.color.is_some());
        // assert!(node.child_id.is_none());

        // let Node::LeafColor(leaf) = node else {
        //     // let Some(leaf) = get_leaf_from_mid(self, node_mid) else {
        //     unreachable!("the node must be a `LeafColor`");
        // };

        // self.alloc.debug();
        // dbg!(shallowest_depth);

        fn try_split(tree: &Tree, left_child: NodeHandle, leaf_handle: NodeHandle) -> Option<()> {
            let leaf = tree.alloc.get(leaf_handle);

            let leaf_dom = leaf.dom;
            // #[cfg(debug_assertions)]
            // {
            //     let leaf_leaf_distance_cache = leaf.leaf_distance_cache.load(Ordering::SeqCst);
            //     let leaf_color = leaf.color.load(Ordering::SeqCst);
            //     let leaf_left_child = leaf.left_child.load(Ordering::SeqCst);
            // }

            // // if we do this, we can skip initializing the children's dom,
            // // but this requires an additional atomic operation,
            // // so it's probably not worth it.
            // if leaf_left_child.is_some() {
            //     dbg!("leaf_left_child.is_some()");
            //     continue;
            // }

            // initialize the children's dom
            {
                let Some(doms) = leaf_dom.split() else {
                    dbg!("leaf_dom.split() is None");
                    return None;
                };
                for (offset, dom) in doms.into_iter().enumerate() {
                    let child_handle = left_child.siblings()[offset];
                    let child = tree.alloc.get(child_handle);
                    // SAFETY: we allocated the children and never gave them to anyone,
                    // so we have exclusive access.
                    unsafe { child.dom_write(dom) };
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
                    debug_assert_eq!(
                        old_left_child, None,
                        "this is guaranteed by compare_exchange_weak, despite it being documented incorrectly"
                    );
                }
                Err(_old_left_child) => {
                    return None;
                }
            }

            // // at this point we know that we were the ones who succeeded,
            // // so we get to do some fun checks
            // #[cfg(debug_assertions)]
            // {
            //     debug_assert_eq!(leaf_leaf_distance_cache, 0);
            //     debug_assert!(leaf_color.is_some());
            //     debug_assert_eq!(leaf_left_child, None);
            // }

            // // update leaf_distance_cache
            // {
            //     // this can fail if another thread updated the cache first
            //     debug_assert_eq!(leaf.leaf_distance_cache.load(Ordering::SeqCst), 0);
            //     leaf.leaf_distance_cache.store(1, Ordering::SeqCst);
            // }

            Some(())
        }

        // note that we have an exclusive reference to the new children
        let left_child = self.alloc.alloc4();

        // initialize the children except for dom
        for child_handle in left_child.siblings() {
            let child = self.alloc.get(child_handle);
            child.leaf_distance_cache.store(0, Ordering::SeqCst);
            child.color.store(OptionColor::NONE, Ordering::SeqCst);
            child.left_child.store(None, Ordering::SeqCst);
        }

        // CAS in the new left_child,
        // and if it fails (bc someone already got there),
        // reuse the memory we allocated for the children for the next iteration.
        for leaf_handle in colored_leafs_in_window_at_depth(self, window, shallowest_depth) {
            if let Some(()) = try_split(self, left_child, leaf_handle) {
                return Some(left_child.siblings());
            }

            // dbg!("try_split failed for leaf_handle {:?}", leaf_handle);
        }
        dbg!("we're leaking memory and i want to know");
        None
    }

    /// inserts the previously reserved sample into the the tree,
    /// promoting a `LeafReserved` to a `LeafColor`
    // TODO: should the point and color actually be a [_; 4]?
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn insert(&self, handle: NodeHandle, color: Color32) {
        let node = self.alloc.get(handle);
        debug_assert_eq!(node.color.load(Ordering::SeqCst), OptionColor::NONE);
        node.color
            .store(OptionColor::new_some(color), Ordering::SeqCst);
    }

    // TODO: if the pixel doesn't contain any samples,
    // return the color of the sample closest to the center of the pixel.
    // TODO: if the pixel contains any samples, do some weighting of the samples
    // TODO: if the pixel doesn't contain any samples, do some weighting of some nearby samples
    // TODO: if the pixel contains any samples, returns the average color of the samples inside the pixel
    //
    /// follow the path down to the leaf containing the center of the pixel,
    /// return the color of the sample closest to the center of the pixel.
    ///
    /// returns white if not in the trees domain
    #[inline(never)]
    pub(crate) fn color_of_pixel(&self, pixel: Pixel) -> Color32 {
        fn distance((real_0, imag_0): (Real, Imag), (real_1, imag_1): (Real, Imag)) -> Fixed {
            let real_delta = real_0 - real_1;
            let imag_delta = imag_0 - imag_1;
            // real_delta * real_delta + imag_delta * imag_delta

            // i think they give the same result
            // except manhattan maybe gives weird lines
            // real_delta.abs() + imag_delta.abs()
            real_delta.abs().max(imag_delta.abs())
        }

        let center = (pixel.real_mid(), pixel.imag_mid());
        if !self.dom.contains_point(center) {
            const UNCONTAINED_COLOR: Color32 = Color32::WHITE;
            return UNCONTAINED_COLOR;
        }

        let mut node_id = self.root;
        let mut closest_sample_dist = distance(center, self.dom.mid());
        let mut closest_sample_color = self
            .alloc
            .get(node_id)
            .color
            .load(Ordering::SeqCst)
            .expect("root must have a color");

        loop {
            let node = self.alloc.get(node_id);
            let Some(left_child) = node.left_child.load(Ordering::SeqCst) else {
                break;
            };
            let child_offset = node.dom.child_offset_containing(center);
            node_id = left_child.siblings()[child_offset];
            let node = self.alloc.get(node_id);

            let dist = distance(center, node.dom.mid());
            let color = node.color.load(Ordering::SeqCst);
            // for debugging, draw uncolored leafs
            if dist < closest_sample_dist {
                closest_sample_dist = dist;
                closest_sample_color = if color.is_some() {
                    color.unwrap()
                } else {
                    Color32::from_rgb(255, 255, 0)
                };
            }
            // if dist < closest_sample_dist && color.is_some() {
            //     closest_sample_dist = dist;
            //     closest_sample_color = color.unwrap();
            // }
        }
        closest_sample_color
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
