use std::{
    collections::VecDeque,
    mem::MaybeUninit,
    num::NonZeroU32,
    pin::Pin,
    process::Child,
    sync::atomic::{AtomicU32, Ordering::*},
};

use eframe::egui::Color32;

use crate::{
    complex::{Domain, Pixel, Window, fixed::*},
    sample::metabrot_sample,
};

pub(crate) static ELAPSED_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// #[derive(Debug)]
// struct Internal {
//     dom: Domain,
//     color: Color32,
//     /// the length of the shortest descending path to a leaf.
//     /// always > 0 bc this is an internal node.
//     leaf_distance_cache: u32,
//     /// 0 1
//     ///
//     /// 2 3
//     children: Box<[Node; 4]>,
// }

// #[derive(Debug)]
// struct LeafColor {
//     dom: Domain,
//     color: Color32,
// }

// #[derive(Debug)]
// struct LeafReserved {
//     dom: Domain,
// }

// #[derive(Debug)]
// enum Node {
//     Internal(Internal),
//     LeafColor(LeafColor),
//     LeafReserved(LeafReserved),
// }

#[derive(Debug)]
struct Node {
    dom: Domain,
    leaf_distance_cache: AtomicU32,
    color: Option<Color32>,
    /// leftmost child handle
    child_id: Option<NodeId>,
}
impl Node {
    fn new_leaf_uncolored(dom: Domain) -> Self {
        Self {
            dom,
            leaf_distance_cache: AtomicU32::new(0),
            color: None,
            child_id: None,
        }
    }

    fn new_leaf_colored(dom: Domain, color: Color32) -> Self {
        Self {
            dom,
            leaf_distance_cache: AtomicU32::new(0),
            color: Some(color),
            child_id: None,
        }
    }

    // fn new_internal_colored

    // invalid probably
    // fn new_internal_uncolored

    /// the point must be inside the domain.
    /// returns `None` if we're a leaf.
    fn child_i_containing(&self, alloc: &Alloc, (real, imag): (Real, Imag)) -> Option<usize> {
        let child_id = self.child_id?;
        let children = alloc.get4(child_id);
        debug_assert!(self.dom.contains_point((real, imag)));
        let ret = (if real < self.dom.real_mid() { 0 } else { 1 })
            + (if imag >= self.dom.imag_mid() { 0 } else { 2 });
        debug_assert!(children[ret].dom.contains_point((real, imag)));
        Some(ret)
    }

    // /// the point must be inside one of the children.
    // fn child_i_containing(dom: &Domain, children: &[Node; 4], (real, imag): (Real, Imag)) -> usize {
    //     debug_assert!(dom.contains_point((real, imag)));
    //     let ret = (if real < dom.real_mid() { 0 } else { 1 })
    //         + (if imag >= dom.imag_mid() { 0 } else { 2 });
    //     debug_assert!(children[ret].dom.contains_point((real, imag)));
    //     ret
    // }

    /// must have that self is a leaf.
    /// fails if the domain gets too small.
    fn try_split(leaf_id: NodeId, alloc: &mut Alloc) -> Option<()> {
        let child_id = {
            let slf = alloc.get1(leaf_id);
            assert!(slf.child_id.is_none());
            alloc.insert4(slf.dom.split()?.map(Self::new_leaf_uncolored))
        };
        let slf = alloc.get1_mut(leaf_id);
        *slf = Self {
            dom: slf.dom,
            leaf_distance_cache: AtomicU32::new(1),
            color: slf.color,
            child_id: Some(child_id),
        };
        Some(())
    }

    fn set_color(&mut self, color: Color32) {
        assert!(self.color.is_none());
        self.color = Some(color);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NodeId(NonZeroU32);
impl NodeId {
    // fn new(i: usize) -> Self {
    //     Self(NonZeroU32::new(i as u32).unwrap())
    // }

    // unsafe fn new_unchecked(i: usize) -> Self {
    //     unsafe { Self(NonZeroU32::new_unchecked(i as u32)) }
    // }

    fn to_index(self) -> usize {
        self.0.get() as usize
    }

    /// ret[0] = handle
    fn siblings(self) -> [NodeId; 4] {
        let i = self.to_index();
        debug_assert_eq!(i % 4, 0, "unaligned handle in siblings");
        unsafe {
            [
                NodeId(NonZeroU32::new_unchecked(self.0.get() + 0)),
                NodeId(NonZeroU32::new_unchecked(self.0.get() + 1)),
                NodeId(NonZeroU32::new_unchecked(self.0.get() + 2)),
                NodeId(NonZeroU32::new_unchecked(self.0.get() + 3)),
            ]
        }
    }
}

#[derive(Debug)]
struct Alloc {
    /// Option for maybe uninit runtime checking
    /// TODO: remove
    /// mem[0] is uninit/None so we can do `NonZeroU32` handles
    mem: Vec<MaybeUninit<Node>>,
    is_init: Vec<bool>,
}
impl Default for Alloc {
    fn default() -> Self {
        Self {
            mem: vec![
                MaybeUninit::uninit(),
                MaybeUninit::uninit(),
                MaybeUninit::uninit(),
            ],
            is_init: vec![false; 3],
        }
    }
}
impl Alloc {
    fn alloc1(&mut self) -> NodeId {
        let handle = NonZeroU32::new(self.mem.len() as u32).unwrap();
        debug_assert_eq!(handle.get(), 3);
        self.mem.push(MaybeUninit::uninit());
        self.is_init.push(false);
        NodeId(handle)
    }

    fn alloc4(&mut self) -> NodeId {
        let handle = NonZeroU32::new(self.mem.len() as u32).unwrap();
        debug_assert_eq!(handle.get() % 4, 0);
        self.mem.extend([
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
        ]);
        self.is_init.extend([false; 4]);
        NodeId(handle)
    }

    // fn set4(&mut self, handle: AllocHandle, nodes: [Node; 4]) {
    //     let i = handle.to_index();
    //     debug_assert_eq!(i % 4, 0);
    //     for (offset, node) in nodes.into_iter().enumerate() {
    //         self.mem[i + offset] = MaybeUninit::new(node);
    //         self.is_init[i + offset] = true;
    //     }
    // }

    fn insert1(&mut self, node: Node) -> NodeId {
        let handle = self.alloc1();
        let i = handle.to_index();
        debug_assert!(
            !self.is_init[i],
            "insert at already initialized memory at index {}",
            i
        );
        self.mem[i] = MaybeUninit::new(node);
        self.is_init[i] = true;
        handle
    }

    fn insert4(&mut self, nodes: [Node; 4]) -> NodeId {
        let handle = self.alloc4();
        let i = handle.to_index();
        for (offset, node) in nodes.into_iter().enumerate() {
            debug_assert!(
                !self.is_init[i + offset],
                "insert at already initialized memory at index {}",
                i + offset
            );
            self.mem[i + offset] = MaybeUninit::new(node);
            self.is_init[i + offset] = true;
        }
        handle
    }

    /// probably only used for getting the root
    // fn get1<'a>(&'a self, handle: AllocHandle) -> &'a Node {
    pub(crate) fn get1(&self, handle: NodeId) -> &Node {
        let i = handle.to_index();
        // debug_assert_eq!(
        //     i, 3,
        //     "we should probably only use get1 for getting the root"
        // );
        debug_assert!(self.is_init[i], "read uninitialized memory at index {}", i);
        unsafe { self.mem[i].assume_init_ref() }
    }

    fn get1_mut(&mut self, handle: NodeId) -> &mut Node {
        let i = handle.to_index();
        // debug_assert_eq!(
        //     i, 3,
        //     "we should probably only use get1_mut for getting the root"
        // );
        debug_assert!(self.is_init[i], "read uninitialized memory at index {}", i);
        unsafe { self.mem[i].assume_init_mut() }
    }

    /// used for getting the four children
    fn get4(&self, handle: NodeId) -> [&Node; 4] {
        let i = handle.to_index();
        debug_assert!(i >= 4, "probably bad");
        debug_assert_eq!(i % 4, 0, "unaligned read, probably bad");
        for offset in 0..4 {
            debug_assert!(
                self.is_init[i + offset],
                "read uninitialized memory at index {}",
                i + offset
            );
        }
        (self.mem[i..i + 4])
            .as_array::<4>()
            .unwrap()
            .each_ref()
            .map(|m| unsafe { m.assume_init_ref() })
    }

    fn get4_mut(&mut self, handle: NodeId) -> [&mut Node; 4] {
        let i = handle.to_index();
        debug_assert!(i >= 4, "probably bad");
        debug_assert_eq!(i % 4, 0, "unaligned read, probably bad");
        for offset in 0..4 {
            debug_assert!(
                self.is_init[i + offset],
                "read uninitialized memory at index {}",
                i + offset
            );
        }
        (self.mem[i..i + 4])
            .as_mut_array::<4>()
            .unwrap()
            .each_mut()
            .map(|m| unsafe { m.assume_init_mut() })
    }
}

#[derive(Debug)]
pub(crate) struct Tree {
    dom: Domain,
    alloc: Alloc,
    root: NodeId,
}

// impl Internal {
//     // fn child_i_closest_to(&self, real: f32, imag: f32) -> usize {
//     //     (0..self.children.len())
//     //         .map(|i| {
//     //             let dx = self.children[i].dom.real_mid() - real;
//     //             let dy = self.children[i].dom.imag_mid() - imag;
//     //             (i, dx * dx + dy * dy)
//     //         })
//     //         .min_by(|(_, left), (_, right)| left.total_cmp(right))
//     //         .unwrap()
//     //         .0
//     // }

//     // /// returns `None` if the point is outside the domain
//     /// the point must be inside the domain
//     fn child_i_containing(&self, (real, imag): (Real, Imag)) -> usize {
//         debug_assert!(self.dom.contains_point((real, imag)));
//         let ret = (if real < self.dom.real_mid() { 0 } else { 1 })
//             + (if imag >= self.dom.imag_mid() { 0 } else { 2 });
//         debug_assert!(self.children[ret].dom.contains_point((real, imag)));
//         ret
//     }

//     // /// returns None if the point is outside the domain
//     // fn child_containing(&self, (real, imag): (f32, f32)) -> Option<&Node> {
//     //     self.child_i_containing((real, imag))
//     //         .map(|i| self.children[i].as_ref())
//     // }

//     fn compute_leaf_distance(&self) -> u32 {
//         self.children
//             .iter()
//             .map(|c| c.leaf_distance_cache)
//             .min()
//             .unwrap()
//             + 1
//     }

//     /// returns whether the cache was updated
//     fn update_leaf_distance(&mut self) -> bool {
//         let new = self.compute_leaf_distance();
//         if new != self.leaf_distance_cache {
//             self.leaf_distance_cache = new;
//             true
//         } else {
//             false
//         }
//     }
// }

// impl LeafColor {
//     /// fails if the domain gets too small
//     fn try_split(&self) -> Option<Internal> {
//         let children = Box::new(
//             self.dom
//                 .split()?
//                 .map(LeafReserved::new)
//                 .map(Node::LeafReserved),
//         );
//         Some(Internal {
//             dom: self.dom,
//             color: self.color,
//             leaf_distance_cache: 1,
//             children,
//         })
//     }
// }

// impl LeafReserved {
//     fn new(dom: Domain) -> Self {
//         Self { dom }
//     }
// }

// impl Node {
//     #[cfg_attr(feature = "profiling", inline(never))]
//     fn dom(&self) -> Domain {
//         match self {
//             Node::Internal(internal) => internal.dom,
//             Node::LeafColor(leaf_color) => leaf_color.dom,
//             Node::LeafReserved(leaf_reserved) => leaf_reserved.dom,
//         }
//     }

//     #[cfg_attr(feature = "profiling", inline(never))]
//     fn color(&self) -> Option<Color32> {
//         match self {
//             Node::Internal(internal) => Some(internal.color),
//             Node::LeafColor(leaf_color) => Some(leaf_color.color),
//             Node::LeafReserved(_) => None,
//         }
//     }

//     #[cfg_attr(feature = "profiling", inline(never))]
//     fn leaf_distance_cache(&self) -> u32 {
//         match self {
//             Node::Internal(internal) => internal.leaf_distance_cache,
//             Node::LeafColor(_) | Node::LeafReserved(_) => 0,
//         }
//     }
// }

impl Tree {
    pub(crate) fn new(dom: Domain) -> Self {
        let mut alloc = Alloc::default();
        let root = alloc.insert1(Node::new_leaf_colored(
            dom,
            metabrot_sample(dom.mid()).color(),
        ));
        Self { dom, alloc, root }
    }

    pub(crate) fn node_count(&self) -> usize {
        let mut count = 0;
        let mut stack = Vec::with_capacity(64);
        stack.push(self.alloc.get1(self.root));
        while let Some(node) = stack.pop() {
            count += 1;
            if let Some(child_id) = node.child_id {
                let children = self.alloc.get4(child_id);
                stack.extend(children);
            }
        }
        count
    }

    pub(crate) fn mid_of_node_id(&self, node_id: NodeId) -> (Real, Imag) {
        let node = self.alloc.get1(node_id);
        node.dom.mid()
    }

    // fn ensure_pixel_safe(&mut self, pixel: Domain) {
    //     if !self.window.overlaps(pixel) {
    //         return;
    //     }
    //     match &self.children {
    //         Some(children) => if !children.iter().all(|c| c.window.overlaps(pixel) {todo!()}),
    //         None => {
    //             if self.window.contains(pixel) {
    //                 self.split();
    //                 for c in self.children.as_mut().unwrap() {
    //                     c.ensure_pixel_safe(pixel);
    //                 }
    //             }
    //         }
    //     };
    //     todo!()
    // }

    // fn ensure_pixel_safe(&mut self, pixel: Domain) {
    //     if self.count_overlaps(pixel) >= 4 {
    //         return;
    //     }
    // }

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

    // #[inline(never)]
    // fn color(&self) -> ColorBuilder {
    //     ColorBuilder::from(self.color)
    //         + match &self.children {
    //             Some(children) => children.iter().map(|c| c.color()).sum(),
    //             None => ColorBuilder::default(),
    //         }
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

    // #[inline(never)]
    // pub(crate) fn color_in_pixels(
    //     &self,
    //     window: Window,
    //     pixel_rad: f32,
    //     debug_camera_map: &CameraMap,
    //     debug_stride: usize,
    // ) -> Vec<Vec<ColorBuilder>> {
    //     fn update(
    //         node: &Tree,
    //         window: Window,
    //         pixel_rad: f32,
    //         ret: &mut [Vec<ColorBuilder>],
    //         debug_camera_map: &CameraMap,
    //         debug_stride: usize,
    //     ) {
    //         // TODO: maybe remove this check
    //         if !window.overlaps(node.dom) {
    //             return;
    //         }
    //         // let pixel_of_index = |row: usize, col: usize| {};

    //         // let row = ret.len() as f32
    //         //     * (1.0 - inv_lerp(window.imag_lo, window.imag_hi, node.dom.imag_mid()));
    //         // let col =
    //         //     ret[0].len() as f32 * inv_lerp(window.real_lo, window.real_hi, node.dom.real_mid());
    //         let row = (window.imag_rad() / pixel_rad)
    //             * (1.0 - inv_lerp(window.imag_lo(), window.imag_hi(), node.dom.imag_mid()));
    //         let col = (window.real_rad() / pixel_rad)
    //             * inv_lerp(window.real_lo(), window.real_hi(), node.dom.real_mid());

    //         // if (0.0..ret.len() as f32).contains(&row) && (0.0..ret[0].len() as f32).contains(&col) {
    //         //     let ((oracle_row, oracle_col), _, _oracle_pixel) = {
    //         //         debug_camera_map
    //         //             .pixels(debug_stride)
    //         //             .find(|((_row, _col), _, pixel)| {
    //         //                 pixel.approx_contains_point(node.dom.real_mid(), node.dom.imag_mid())
    //         //             })
    //         //             .unwrap()
    //         //     };
    //         //     assert_eq!(oracle_row, row as usize);
    //         //     assert_eq!(oracle_col, col as usize);
    //         // }
    //         for r in [
    //             Some(row.floor()),
    //             // if row.fract() <= row * 1e-4 {
    //             if row.fract() == 0.0 {
    //                 // Some(row.floor() + 1.0)
    //                 Some(row + 1.0)
    //             } else {
    //                 None
    //             },
    //         ]
    //         .iter()
    //         .flatten()
    //         {
    //             for c in [
    //                 Some(col.floor()),
    //                 // if col.fract().abs() <= col * 1e-4 {
    //                 if col.fract() == 0.0 {
    //                     // Some(col.floor() + 1.0)
    //                     Some(col + 1.0)
    //                 } else {
    //                     None
    //                 },
    //             ]
    //             .iter()
    //             .flatten()
    //             {
    //                 if let Some(e) = ret
    //                     .get_mut(*r as usize)
    //                     .and_then(|line| line.get_mut(*c as usize))
    //                 {
    //                     *e += node.color.into();
    //                 }
    //             }
    //         }
    //         // if let Some(e) = ret
    //         //     .get_mut(row.floor() as usize)
    //         //     .and_then(|line| line.get_mut(col.floor() as usize))
    //         // {
    //         //     *e += node.color.into();
    //         // }
    //         if let Some(children) = &node.children {
    //             for c in children {
    //                 update(c, window, pixel_rad, ret, debug_camera_map, debug_stride);
    //             }
    //         };
    //     }

    //     // ((row, col), rect, pixel) in camera_map.pixels(stride)
    //     let width = (window.real_rad() / pixel_rad).ceil();
    //     let height = (window.imag_rad() / pixel_rad).ceil();
    //     // let width = (window.real_rad() / pixel_rad).floor();
    //     // let height = (window.imag_rad() / pixel_rad).floor();
    //     let mut ret: Vec<Vec<ColorBuilder>> = (0..height as usize)
    //         .map(|_| {
    //             (0..width as usize)
    //                 .map(|_| ColorBuilder::default())
    //                 .collect()
    //         })
    //         .collect();
    //     update(
    //         self,
    //         window,
    //         pixel_rad,
    //         &mut ret,
    //         debug_camera_map,
    //         debug_stride,
    //     );
    //     ret
    // }

    // #[inline(never)]
    // fn color(&self, pixel: Domain) -> ColorBuilder {
    //     let mut stack = Vec::with_capacity(64);
    //     stack.push(self);
    //     let mut ret = ColorBuilder::default();
    //     while let Some(node) = stack.pop() {
    //         if !node.dom.overlaps(pixel) {
    //             continue;
    //         }
    //         if pixel.contains(node.dom.real_mid(), node.dom.imag_mid()) {
    //             ret += node.color.into();
    //         }
    //         if let Some(children) = &node.children {
    //             stack.extend(children.iter().map(|c| c.as_ref()));
    //         }
    //     }
    //     // assert_eq!(stack.capacity(), 64);
    //     ret
    // }

    // fn validate(&self) {
    //     assert!(self.window.real_lo < self.window.real_hi);
    //     assert!(self.window.imag_lo < self.window.imag_hi);
    //     if let Some(children) = &self.children {
    //         for c in children {
    //             assert!(self.window.real_lo <= c..real_lo);
    //             assert!(c.real_hi <= self.window.real_hi);
    //             assert!(self.window.imag_lo <= c.imag_lo);
    //             assert!(c.imag_hi <= self.window.imag_hi);
    //         }
    //     }
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
    pub(crate) fn refine(&mut self, window: Window) -> Option<[NodeId; 4]> {
        let start = std::time::Instant::now();

        // /// returns `None` if no leaf intersects the window
        // #[cfg_attr(feature = "profiling", inline(never))]
        // // fn get_shallowest_leaf(tree: &mut Tree, window: Window) -> Option<&mut Node> {
        // fn get_shallowest_leaf_oracle(
        //     tree: &mut Tree,
        //     window: Window,
        // ) -> Option<(u32, (Real, Imag))> {
        //     // queue instead of stack bc we want to visit shallower nodes first
        //     let mut queue = VecDeque::with_capacity(64);
        //     queue.push_back((&mut tree.root, 0));
        //     let mut shallowest_depth = u32::MAX;
        //     // TODO: change this to a Vec and store all the shallowest leafs
        //     let mut shallowest_leaf = None;
        //     while let Some((node, depth)) = queue.pop_front() {
        //         if !window.overlaps(node.dom) {
        //             continue;
        //         }
        //         if depth >= shallowest_depth {
        //             continue;
        //         }
        //         match (&node.color, node.children.as_deref_mut()) {
        //             (_, Some(children)) => {
        //                 queue.extend((children).iter_mut().map(|c| (c, depth + 1)));
        //             }
        //             (Some(color), None) => {
        //                 if depth < shallowest_depth {
        //                     shallowest_depth = depth;
        //                     shallowest_leaf = Some(node);
        //                 }
        //             }
        //             (None, None) => {}
        //         }
        //     }
        //     // shallowest_leaf
        //     Some((shallowest_depth, shallowest_leaf?.dom.mid()))
        // }

        #[cfg_attr(feature = "profiling", inline(never))]
        /// returns `None` if no leaf intersects the window
        fn get_shallowest_leaf(tree: &Tree, window: Window) -> Option<(u32, NodeId)> {
            // queue instead of stack bc we want to visit shallower nodes first
            // TODO: the queue is probably worse now
            let mut stack = Vec::with_capacity(64);
            // stack.push((&mut *tree.alloc.get1(tree.root), 0));
            stack.push((tree.root, 0));
            let mut shallowest_depth = u32::MAX;
            // TODO: change this to a Vec and store all the shallowest leafs
            // let mut shallowest_leaf = None;
            // let mut shallowest_leaf_mid = None;
            let mut shallowest_leaf_id = None;
            // while let Some((node, depth)) = queue.pop_front() {
            while let Some((node_id, depth)) = stack.pop() {
                let node = tree.alloc.get1(node_id);
                // TODO: instead of doing this check on pop, do it on push
                // this also lets us do less work in the case where the domain is contained inside the window
                if !window.overlaps(node.dom) {
                    continue;
                }
                if depth >= shallowest_depth {
                    continue;
                }
                match (node.color, node.child_id) {
                    (_, Some(child_id)) => {
                        let children = tree.alloc.get4(child_id);
                        // let leaf_distance = internal.compute_leaf_distance();
                        let leaf_distance = children
                            .iter()
                            .map(|c| c.leaf_distance_cache.load(Relaxed))
                            .min()
                            .unwrap()
                            + 1;
                        // if leaf_distance != node.leaf_distance_cache.load(Relaxed) {
                        //     node.leaf_distance_cache = leaf_distance;
                        // }
                        node.leaf_distance_cache.fetch_min(leaf_distance, Relaxed);
                        if leaf_distance + depth >= shallowest_depth {
                            continue;
                        }
                        // stack.extend(
                        //     internal
                        //         .children
                        //         .iter_mut()
                        //         .map(|c| (c.as_mut(), depth + 1)),
                        // );
                        // // TODO: does this sort the nodes in place???
                        // let mut children: [&mut Node; 4] = children.each_mut();
                        // children.sort_by_key(|c| c.leaf_distance_cache());
                        // stack.extend(children.into_iter().map(|c| (c, depth + 1)));
                        stack.extend(child_id.siblings().map(|c| (c, depth + 1)).into_iter());
                    }
                    (Some(color), None) => {
                        if depth < shallowest_depth {
                            shallowest_depth = depth;
                            shallowest_leaf_id = Some(node_id);
                        }
                    }
                    (None, None) => {}
                }
            }
            // shallowest_leaf
            // Some((shallowest_depth, shallowest_leaf?.dom.mid()))
            Some((shallowest_depth, shallowest_leaf_id?))
        }

        // how deep is the shallowest leaf that intersects the window?
        // let node = get_shallowest_leaf(self, window)?;

        // TODO: is this really correct?
        let (shallowest_depth, node_id) = get_shallowest_leaf(self, window)?;
        // let (shallowest_depth_oracle, _) = get_shallowest_leaf_oracle(self, window)?;
        // assert_eq!(shallowest_depth, shallowest_depth_oracle,);

        // #[inline(never)]
        // fn update_leaf_distance(tree: &mut Tree, leaf_mid: (Real, Imag)) {
        //     // let mut stack: Vec<&mut Internal> = Vec::with_capacity(64);
        //     // {
        //     //     let mut node = &mut tree.root;
        //     //     while let Node::Internal(internal) = node {
        //     //         let child_i = internal.child_i_containing(leaf_mid).unwrap();
        //     //         node = internal.children[child_i].as_mut();
        //     //         stack.push(internal);
        //     //     }
        //     // }
        //     let mut stack: Vec<&mut Internal> = Vec::with_capacity(64);
        //     {
        //         let mut node = &mut tree.root;
        //         while let Node::Internal(internal) = node {
        //             let child_i = internal.child_i_containing(leaf_mid).unwrap();
        //             unsafe {
        //                 node = (internal as *mut Internal).as_mut().unwrap().children[child_i]
        //                     .as_mut();
        //             }
        //             stack.push(internal);
        //         }
        //     }

        //     if let Some(internal) = stack.pop() {
        //         assert!(internal.leaf_distance == 1 || internal.leaf_distance == 2);
        //         // if internal.leaf_distance == 2 {
        //         //     return;
        //         // }
        //         internal.leaf_distance = 2;
        //     }
        //     while let Some(internal) = stack.pop() {
        //         // let internal = unsafe { &mut *internal };
        //         let leaf_distance = internal
        //             .children
        //             .iter()
        //             .map(|c| c.leaf_distance())
        //             .min()
        //             .unwrap()
        //             + 1;
        //         assert!(
        //             leaf_distance == internal.leaf_distance
        //                 || leaf_distance == internal.leaf_distance + 1
        //         );
        //         if leaf_distance == internal.leaf_distance {
        //             break;
        //         }
        //         internal.leaf_distance = leaf_distance;
        //     }
        // }
        // update_leaf_distance(self, node_mid);

        // #[cfg_attr(feature = "profiling", inline(never))]
        // fn get_leaf_from_mid(tree: &mut Tree, mid: (Real, Imag)) -> &mut Node {
        //     let mut node = &mut tree.root;
        //     // while let Some(children) = node.children {
        //     //     let child_i = node.child_i_containing(mid);
        //     while let Some(child_i) = node.child_i_containing(mid) {
        //         node = &mut node.children.as_mut().unwrap()[child_i];
        //     }
        //     node
        // }
        // let node = get_leaf_from_mid(self, node_mid);

        // let node = self.alloc.get1_mut(node_id);

        // assert!(node.color.is_some());
        // assert!(node.child_id.is_none());

        // let Node::LeafColor(leaf) = node else {
        //     // let Some(leaf) = get_leaf_from_mid(self, node_mid) else {
        //     unreachable!("the node must be a `LeafColor`");
        // };

        // TODO: if this is too small to split,
        // we should try to split another node,
        // rather than failing immediately

        Node::try_split(node_id, &mut self.alloc)?;
        let node = self.alloc.get1(node_id);
        // // we can't just `internal.children.map(|c| c.dom.mid())` bc of rust
        // let points = self
        //     .alloc
        //     .get4(node.child_id.expect("we just split the node"))
        //     .map(|c| c.dom.mid());

        #[cfg(false)]
        {
            let elapsed = start.elapsed();
            ELAPSED_NANOS.fetch_add(
                elapsed.as_nanos() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Some(node.child_id.expect("we just split it").siblings())
    }

    // /// returns `None` if we shouldn't/can't refine
    // /// returns the points we need to sample
    // /// we split a `LeafColor` into a `Internal([LeafReserved; 4])`)
    // ///
    // /// to select the node, we require that it
    // /// - intersects the window
    // /// - is among the shallowest leafs
    // /// - disagrees with its parent on color the most
    // // TODO: we don't actually need the leaf to be colored to split it
    // // TODO: better ordering
    // // TODO: max depth delta between deepest and shallowest leaf that's bigger than 1
    // #[inline(never)]
    // pub(crate) fn refine(&mut self, window: Window) -> Option<[(Real, Imag); 4]> {
    //     let start = std::time::Instant::now();
    //     // actually this would require ~cloning the entire tree
    //     // #[derive(Debug, PartialEq, Eq)]
    //     // struct Element {
    //     //     node: *mut Node,
    //     //     depth: usize,
    //     //     color_diff: i16,
    //     // }
    //     // impl PartialOrd for Element {
    //     //     fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    //     //         Some(self.cmp(other))
    //     //     }
    //     // }
    //     // impl Ord for Element {
    //     //     /// smaller depth is smaller
    //     //     /// then larger color diff is smaller
    //     //     fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    //     //         self.partial_cmp(other).unwrap()
    //     //     }
    //     // }

    //     #[inline(never)]
    //     fn get_shallowest_leaf_depth(tree: &Tree, window: Window) -> usize {
    //         // let mut node = self.root;
    //         // let mut depth = 0;
    //         // queue instead of stack bc we want to visit shallower nodes first
    //         let mut queue = VecDeque::with_capacity(64);
    //         queue.push_back((&tree.root, 0));
    //         let mut target_depth = usize::MAX;
    //         while let Some((node, depth)) = queue.pop_front() {
    //             if !window.overlaps(node.dom) {
    //                 continue;
    //             }
    //             if depth >= target_depth {
    //                 continue;
    //             }
    //             match node {
    //                 Node::Internal(internal) => {
    //                     queue.extend(internal.children.iter().map(|c| (c.as_ref(), depth + 1)));
    //                 }
    //                 Node::LeafColor(_) => {
    //                     target_depth = target_depth.min(depth);
    //                 }
    //                 Node::LeafReserved(_) => {}
    //             }
    //         }
    //         target_depth
    //     }

    //     // how deep is the shallowest leaf that intersects the window?
    //     let shallowest_leaf_depth = get_shallowest_leaf_depth(self, window);
    //     // probably no nodes overlap the window
    //     if shallowest_leaf_depth == usize::MAX {
    //         return None;
    //     }

    //     fn color_diff(lhs: Color32, rhs: Color32) -> i16 {
    //         if (lhs == Color32::BLACK) != (rhs == Color32::BLACK) {
    //             return i16::MAX;
    //         }
    //         (lhs.r() as i16 - rhs.r() as i16).abs()
    //             + (lhs.g() as i16 - rhs.g() as i16).abs()
    //             + (lhs.b() as i16 - rhs.b() as i16).abs()
    //     }

    //     /// how much deeper than the shallowest leaf are we allowed to expand?
    //     /// if 0, we only consider the shallowest leafs
    //     const DEPTH_RELAXATION: usize = 0;

    //     /// find a leaf that intersects the window
    //     /// and has depth == target_depth
    //     /// and disagrees with its parent on color the most
    //     #[inline(never)]
    //     fn get_largest_color_diff(
    //         tree: &mut Tree,
    //         window: Window,
    //         shallowest_leaf_depth: usize,
    //     ) -> i16 {
    //         let mut largest_color_diff = 0;

    //         let mut queue = VecDeque::with_capacity(64);
    //         queue.push_back((&mut tree.root, 0, Color32::BLACK));
    //         while let Some((node, depth, parent_color)) = queue.pop_front() {
    //             if !window.overlaps(node.dom) {
    //                 continue;
    //             }
    //             if depth > shallowest_leaf_depth + DEPTH_RELAXATION {
    //                 continue;
    //             }
    //             match node {
    //                 Node::Internal(internal) => {
    //                     queue.extend(
    //                         internal
    //                             .children
    //                             .iter_mut()
    //                             .map(|c| (c.as_mut(), depth + 1, internal.color)),
    //                     );
    //                 }
    //                 Node::LeafReserved(_) => {}
    //                 Node::LeafColor(leaf) => {
    //                     let color_diff = color_diff(leaf.color, parent_color);
    //                     if color_diff > largest_color_diff {
    //                         largest_color_diff = color_diff;
    //                     }
    //                 }
    //             }
    //         }
    //         largest_color_diff
    //         // unreachable!();
    //     }

    //     let largest_color_diff = get_largest_color_diff(self, window, shallowest_leaf_depth);

    //     // TODO: this is kinda awful,
    //     // we already found the node,
    //     // i just couldn't get the unsafe mut stuff to work
    //     #[inline(never)]
    //     fn select_node(
    //         tree: &mut Tree,
    //         window: Window,
    //         shallowest_leaf_depth: usize,
    //         largest_color_diff: i16,
    //     ) -> &mut Node {
    //         let mut queue = VecDeque::with_capacity(64);
    //         queue.push_back((&mut tree.root, 0, Color32::BLACK));
    //         while let Some((node, depth, parent_color)) = queue.pop_front() {
    //             if !window.overlaps(node.dom) {
    //                 continue;
    //             }
    //             if depth > shallowest_leaf_depth + DEPTH_RELAXATION {
    //                 continue;
    //             }
    //             match node {
    //                 Node::Internal(internal) => {
    //                     queue.extend(
    //                         internal
    //                             .children
    //                             .iter_mut()
    //                             .map(|c| (c.as_mut(), depth + 1, internal.color)),
    //                     );
    //                 }
    //                 Node::LeafReserved(_) => {}
    //                 Node::LeafColor(leaf) => {
    //                     let color_diff = color_diff(leaf.color, parent_color);
    //                     if color_diff == largest_color_diff {
    //                         return node;
    //                     }
    //                 }
    //             }
    //         }
    //         unreachable!("we already found the node we want, we just need to find it again");
    //     }

    //     let node = select_node(self, window, shallowest_leaf_depth, largest_color_diff);
    //     let Node::LeafColor(leaf) = node else {
    //         unreachable!("the node must be a `LeafColor`");
    //     };
    //     let internal = leaf.try_split()?;
    //     // we can't just `internal.children.map(|c| c.dom.mid())` bc of rust
    //     let points = internal
    //         .children
    //         .iter()
    //         .map(|c| c.dom.mid())
    //         .collect::<Vec<_>>()
    //         .try_into()
    //         .ok()?;
    //     *node = Node::Internal(internal);
    //     #[cfg(false)]
    //     {
    //         let elapsed = start.elapsed();
    //         ELAPSED_NANOS.fetch_add(
    //             elapsed.as_nanos() as u64,
    //             std::sync::atomic::Ordering::Relaxed,
    //         );
    //         COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    //     }
    //     Some(points)
    // }

    /// inserts the previously reserved sample into the the tree,
    /// promoting a `LeafReserved` to a `LeafColor`
    // TODO: should the point and color actually be a [_; 4]?
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn insert(
        &mut self,
        // (real, imag): (Real, Imag),
        node_id: NodeId,
        color: Color32,
    ) {
        // assert!(self.dom.contains_point((real, imag)));
        // let mut node = &mut self.root;
        // // while let Node::Internal(internal) = node {
        // //     let child_i = internal.child_i_containing((real, imag));
        // while let Some(child_i) = node.child_i_containing((real, imag)) {
        //     node = &mut node.children.as_mut().unwrap()[child_i];
        // }
        // match node.color {
        //     Some(_color) => Err("tried to insert into a leaf with color"),
        //     None => {
        //         assert!(node.dom.contains_point((real, imag)));
        //         node.set_color(color);
        //         Ok(())
        //     }
        // }
        let node = self.alloc.get1_mut(node_id);
        node.set_color(color);

        // if let Some(color) = node.color {
        //     assert!(node.dom.contains_point((real, imag)));
        //     *node = Node::LeafColor(LeafColor {
        //         dom: node.dom,
        //         color,
        //     });
        //     Ok(())
        // } else {
        //     Err("tried to insert into a leaf with color")
        // }
    }

    // TODO: if the pixel doesn't contain any samples,
    // return the color of the sample closest to the center of the pixel
    // TODO: if the pixel contains any samples, do some weighting of the samples
    // TODO: if the pixel doesn't contain any samples, do some weighting of some nearby samples
    // TODO: if the pixel contains any samples, returns the average color of the samples inside the pixel
    // /// if the pixel contains any samples,
    // /// else,
    // /// for now, return the color of the sample of the leaf that contains the center of the pixel
    /// follow the path down to the leaf containing the center of the pixel,
    /// return the color of the sample closest to the center of the pixel.
    ///
    /// returns white if not in the trees domain
    #[inline(never)]
    pub(crate) fn color_of_pixel(&self, pixel: Pixel) -> Color32 {
        // fn distance((real_0, imag_0): (Real, Imag), (real_1, imag_1): (Real, Imag)) -> Fixed {
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

        let mut node = self.alloc.get1(self.root);
        let mut closest_sample_dist = distance(center, self.dom.mid());
        let mut closest_sample_color = node.color.expect("root must have a color");

        while let Some(child_i) = node.child_i_containing(&self.alloc, center) {
            // TODO: this is bad
            node = self.alloc.get1(node.child_id.unwrap().siblings()[child_i]);
            let dist = distance(center, node.dom.mid());
            if dist < closest_sample_dist
                && let Some(color) = node.color
            {
                closest_sample_dist = dist;
                closest_sample_color = color;
            }
        }
        closest_sample_color
    }

    // pub(crate) fn rasterize(&self, map: &CameraMap) -> Vec<Vec<Color32>> {
    //     (0..height)
    //         .map(|row| {
    //             (0..width)
    //                 .map(|col| {
    //                     let pixel = Domain::try_new(
    //                         camera.real_lo() + col as f32 * camera.real_rad() * 2.0 / width as f32,
    //                         camera.real_lo()
    //                             + (col as f32 + 1.0) * camera.real_rad() * 2.0 / width as f32,
    //                         camera.imag_hi()
    //                             - (row as f32 + 1.0) * camera.imag_rad() * 2.0 / height as f32,
    //                         camera.imag_hi() - row as f32 * camera.imag_rad() * 2.0 / height as f32,
    //                     )
    //                     .unwrap();
    //                     self.color_of_pixel(pixel)
    //                 })
    //                 .collect()
    //         })
    //         .collect()
    // }
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
