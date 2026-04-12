use std::{
    num::NonZeroU32,
    sync::{RwLock, atomic::Ordering},
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

/// `None` iff self == 0
/// `Some` iff alpha == 255
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::NoUninit)]
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

// TODO: Node where each field is atomic? NodeWrapper(Atomic<Node>)?
// we need atomic load/store
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::NoUninit)]
struct Node {
    // write_lock: bool,
    dom: Domain,
    // lock: bool,
    _pad: [u8; 4],
    // color: Option<Color32>,
    leaf_distance_cache: u32,
    color: OptionColor,
    /// leftmost child id
    left_child: Option<NodeHandle>,
}
impl Node {
    fn uninit() -> Self {
        Self {
            dom: Domain::default(),
            _pad: [0; 4],
            leaf_distance_cache: 0,
            color: OptionColor::NONE,
            left_child: None,
        }
    }

    fn new_leaf_uncolored(dom: Domain) -> Self {
        Self {
            dom,
            _pad: [0; 4],
            leaf_distance_cache: 0,
            color: OptionColor::NONE,
            left_child: None,
        }
    }

    fn new_leaf_colored(dom: Domain, color: Color32) -> Self {
        Self {
            dom,
            _pad: [0; 4],
            leaf_distance_cache: 0,
            color: OptionColor::new_some(color),
            left_child: None,
        }
    }

    // fn new_internal_colored

    // invalid probably
    // fn new_internal_uncolored

    // /// the point must be inside the domain.
    // /// returns `None` if we're a leaf.
    // // fn child_i_containing(&self, alloc: &Alloc, (real, imag): (Real, Imag)) -> Option<usize> {
    // fn child_i_containing(self, alloc: &Alloc, (real, imag): (Real, Imag)) -> Option<usize> {
    //     let child_id = self.child_id?;
    //     let children = alloc.get4_cloned(child_id);
    //     debug_assert!(self.dom.contains_point((real, imag)));
    //     let ret = (if real < self.dom.real_mid() { 0 } else { 1 })
    //         + (if imag >= self.dom.imag_mid() { 0 } else { 2 });
    //     debug_assert!(children[ret].dom.contains_point((real, imag)));
    //     Some(ret)
    // }

    /// the point must be inside the domain.
    /// returns `None` if we're a leaf.
    // fn child_i_containing(&self, alloc: &Alloc, (real, imag): (Real, Imag)) -> Option<usize> {
    fn child_i_containing(&self, (real, imag): (Real, Imag)) -> Option<usize> {
        debug_assert!(self.dom.contains_point((real, imag)));
        if self.left_child.is_none() {
            return None;
        }
        let ret = (if real < self.dom.real_mid() { 0 } else { 1 })
            + (if imag >= self.dom.imag_mid() { 0 } else { 2 });
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

    // /// must have that leaf_id is a leaf.
    // /// fails if the domain gets too small.
    // fn try_split(leaf_id: NodeId, alloc: &mut Alloc) -> Option<()> {
    //     let child_id = {
    //         let node = alloc.get1(leaf_id);
    //         assert!(node.child_id.is_none());
    //         alloc.insert4(node.dom.split()?.map(Self::new_leaf_uncolored))
    //     };
    //     let node = alloc.get1_mut(leaf_id);
    //     *node = Self {
    //         dom: node.dom,
    //         leaf_distance_cache: AtomicU32::new(1),
    //         color: node.color,
    //         child_id: Some(child_id),
    //     };
    //     Some(())
    // }

    // fn set_color(&mut self, color: Color32) {
    //     assert!(self.color.is_none());
    //     self.color = Some(color);
    // }
}

// pub(crate) use alloc::*;
// mod alloc {
//     use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

//     use atomic::Atomic;

//     use super::*;

//     #[repr(transparent)]
//     #[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
//     pub(crate) struct NodeId(NonZeroU32);
//     unsafe impl bytemuck::ZeroableInOption for NodeId {}
//     unsafe impl bytemuck::PodInOption for NodeId {}
//     impl NodeId {
//         // fn new(i: usize) -> Self {
//         //     Self(NonZeroU32::new(i as u32).unwrap())
//         // }

//         // unsafe fn new_unchecked(i: usize) -> Self {
//         //     unsafe { Self(NonZeroU32::new_unchecked(i as u32)) }
//         // }

//         fn to_index(self) -> usize {
//             self.0.get() as usize
//         }

//         /// ret[0] == self
//         pub(super) fn siblings(self) -> [NodeId; 4] {
//             let i = self.to_index();
//             debug_assert_eq!(i % 4, 0, "unaligned handle in siblings");
//             unsafe {
//                 [
//                     NodeId(NonZeroU32::new_unchecked(self.0.get())),
//                     NodeId(NonZeroU32::new_unchecked(self.0.get() + 1)),
//                     NodeId(NonZeroU32::new_unchecked(self.0.get() + 2)),
//                     NodeId(NonZeroU32::new_unchecked(self.0.get() + 3)),
//                 ]
//             }
//         }
//     }

//     #[derive(Debug)]
//     pub(super) struct Alloc {
//         // alloc_lock: Mutex<()>,
//         // we can have our epochs be the frame, and that just works i think
//         // alloc_lock: AtomicBool,
//         // touch_lock: AtomicUsize,
//         inner: AtomicPtr<AllocInner>,
//     }
//     impl Default for Alloc {
//         fn default() -> Self {
//             // const SIZE: usize = 16;
//             const SIZE: usize = 1048576;
//             Self {
//                 // alloc_lock: AtomicBool::new(false),
//                 // touch_lock: AtomicUsize::new(0),
//                 inner: AtomicPtr::new(Box::into_raw(Box::new(AllocInner::new(SIZE)))),
//             }
//         }
//     }
//     impl Alloc {
//         fn alloc<const N: usize>(&self) -> NodeId {
//             // // TODO: only acquire the lock if we reallocate
//             // while self
//             //     .alloc_lock
//             //     .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
//             //     .is_err()
//             // {
//             //     std::thread::yield_now();
//             // }
//             // // TODO: this is probably incorrect, maybe we need to go back to the start if this fails
//             // while self.touch_lock.load(Ordering::Acquire) != 0 {
//             //     std::thread::yield_now();
//             // }

//             // stronger ordering is implied by the locks
//             let inner = self.inner.load(Ordering::Relaxed);
//             let handle = match unsafe { &mut *inner }.try_alloc::<N>() {
//                 Some(id) => id,
//                 // if we only give out clones, it might be fine to just free the memory and not deal with epochs
//                 None => panic!("allocation failed"),
//             };

//             // self.alloc_lock.store(false, Ordering::Release);
//             handle
//         }
//         // pub(super) fn alloc1(&mut self) -> NodeId {
//         //     self.alloc::<1>()
//         // }
//         // pub(super) fn alloc4(&mut self) -> NodeId {
//         //     self.alloc::<4>()
//         // }

//         // fn set4(&mut self, handle: AllocHandle, nodes: [Node; 4]) {
//         //     let i = handle.to_index();
//         //     debug_assert_eq!(i % 4, 0);
//         //     for (offset, node) in nodes.into_iter().enumerate() {
//         //         self.mem[i + offset] = MaybeUninit::new(node);
//         //         self.is_init[i + offset] = true;
//         //     }
//         // }

//         fn init<const N: usize>(&self, handle: NodeId, nodes: [Node; N]) {
//             // while self.alloc_lock.load(Ordering::Acquire) {
//             //     std::thread::yield_now();
//             // }
//             // self.touch_lock.fetch_add(1, Ordering::Acquire);

//             // std::sync::RwLock::read(&self)

//             let inner = self.inner.load(Ordering::Relaxed);
//             // TODO: this is super unsafe
//             // the inner stuff needs to be atomic
//             // todo!("AAAAAAAAAA");
//             unsafe { &*inner }.init::<N>(handle, nodes);

//             // self.touch_lock.fetch_sub(1, Ordering::Release);
//         }

//         fn set<const N: usize>(&self, handle: NodeId, nodes: [Node; N]) {
//             // while self.alloc_lock.load(Ordering::Acquire) {
//             //     std::thread::yield_now();
//             // }
//             // self.touch_lock.fetch_add(1, Ordering::Acquire);

//             let inner = self.inner.load(Ordering::Relaxed);
//             unsafe { &*inner }.set::<N>(handle, nodes);

//             // self.touch_lock.fetch_sub(1, Ordering::Release);
//         }
//         pub(super) fn set1(&mut self, handle: NodeId, node: Node) {
//             self.set::<1>(handle, [node]);
//         }
//         pub(super) fn set4(&mut self, handle: NodeId, nodes: [Node; 4]) {
//             self.set::<4>(handle, nodes);
//         }

//         fn get_cloned<const N: usize>(&self, handle: NodeId) -> [Node; N] {
//             // while self.alloc_lock.load(Ordering::Acquire) {
//             //     std::thread::yield_now();
//             // }
//             // self.touch_lock.fetch_add(1, Ordering::Acquire);

//             let inner = self.inner.load(Ordering::Relaxed);
//             let nodes = unsafe { &mut *inner }.get_cloned::<N>(handle);

//             // self.touch_lock.fetch_sub(1, Ordering::Release);
//             nodes
//         }
//         pub(crate) fn get1_cloned(&self, handle: NodeId) -> Node {
//             let [node] = self.get_cloned::<1>(handle);
//             node
//         }
//         pub(crate) fn get4_cloned(&self, handle: NodeId) -> [Node; 4] {
//             self.get_cloned::<4>(handle)
//         }

//         fn insert<const N: usize>(&self, nodes: [Node; N]) -> NodeId {
//             let handle = self.alloc::<N>();
//             self.init::<N>(handle, nodes);
//             handle
//         }
//         pub(super) fn insert1(&self, node: Node) -> NodeId {
//             self.insert::<1>([node])
//         }
//         pub(super) fn insert4(&self, nodes: [Node; 4]) -> NodeId {
//             self.insert::<4>(nodes)
//         }

//         pub(super) fn update(&self, handle: NodeId, f: impl FnOnce(&Atomic<Node>)) {
//             // while self.alloc_lock.load(Ordering::Acquire) {
//             //     std::thread::yield_now();
//             // }
//             // self.touch_lock.fetch_add(1, Ordering::Acquire);

//             let inner = self.inner.load(Ordering::Relaxed);
//             unsafe { &mut *inner }.update(handle, f);

//             // self.touch_lock.fetch_sub(1, Ordering::Release);
//         }

//         // pub(super) fn get1_mut(&mut self, handle: NodeId) -> &mut Node {
//         //     let i = handle.to_index();
//         //     // debug_assert_eq!(
//         //     //     i, 3,
//         //     //     "we should probably only use get1_mut for getting the root"
//         //     // );
//         //     debug_assert!(self.is_init[i], "read uninitialized memory at index {}", i);
//         //     unsafe { self.mem[i].assume_init_mut() }
//         // }

//         // pub(super) fn get1_uninit_and<F>(&self, handle: NodeId, f: F) -> (&Node, usize)
//         // where
//         //     F: FnMut(&mut Node),
//         // {
//         // }
//         // pub(super) fn get1_and<F>(&self, handle: NodeId, f: F) -> (&Node, usize)
//         // where
//         //     F: FnMut(&mut Node),
//         // { }

//         // /// used for getting the four children
//         // pub(super) fn get4(&self, handle: NodeId) -> [&Node; 4] {
//         //     let i = handle.to_index();
//         //     debug_assert!(i >= 4, "probably bad");
//         //     debug_assert_eq!(i % 4, 0, "unaligned read, probably bad");
//         //     for offset in 0..4 {
//         //         debug_assert!(
//         //             self.is_init[i + offset],
//         //             "read uninitialized memory at index {}",
//         //             i + offset
//         //         );
//         //     }
//         //     (self.mem[i..i + 4])
//         //         .as_array::<4>()
//         //         .unwrap()
//         //         .each_ref()
//         //         .map(|m| unsafe { m.assume_init_ref() })
//         // }

//         // pub(super) fn get4_mut(&mut self, handle: NodeId) -> [&mut Node; 4] {
//         //     let i = handle.to_index();
//         //     debug_assert!(i >= 4, "probably bad");
//         //     debug_assert_eq!(i % 4, 0, "unaligned read, probably bad");
//         //     for offset in 0..4 {
//         //         debug_assert!(
//         //             self.is_init[i + offset],
//         //             "read uninitialized memory at index {}",
//         //             i + offset
//         //         );
//         //     }
//         //     (self.mem[i..i + 4])
//         //         .as_mut_array::<4>()
//         //         .unwrap()
//         //         .each_mut()
//         //         .map(|m| unsafe { m.assume_init_mut() })
//         // }
//     }

//     // struct AtomicFixed(AtomicU32);
//     // type AtomicReal = AtomicFixed;
//     // type AtomicImag = AtomicFixed;

//     // struct AtomicDomain {
//     //     real_mid: AtomicReal,
//     //     imag_mid: AtomicImag,
//     //     rad: AtomicFixed,
//     // }

//     // struct AtomicOptionColor {
//     //     r: AtomicU8,
//     //     g: AtomicU8,
//     //     b: AtomicU8,
//     //     a: AtomicU8,
//     // }

//     // struct AtomicOptionNodeId(AtomicU32);

//     // struct AtomicNode {
//     //     dom: AtomicDomain,
//     //     leaf_distance_cache: AtomicU32,
//     //     color: AtomicOptionColor,
//     //     /// leftmost child id
//     //     child_id: AtomicOptionNodeId,
//     // }

//     // #[derive(Debug)]
//     struct AllocInner {
//         len: AtomicUsize,
//         /// Option for maybe uninit runtime checking
//         /// TODO: remove
//         /// mem[0] is uninit/None so we can do `NonZeroU32` handles
//         // mem: Box<[MaybeUninit<Node>]>,
//         // mem: Box<[MaybeUninit<Atomic<Node>>]>,
//         mem: Box<[Atomic<Node>]>,
//         debug_is_init: Box<[AtomicBool]>,
//     }
//     // impl Default for AllocInner {
//     //     fn default() -> Self {
//     //         Self {
//     //             len: AtomicUsize::new(3),
//     //             mem: vec![
//     //                 MaybeUninit::uninit(),
//     //                 MaybeUninit::uninit(),
//     //                 MaybeUninit::uninit(),
//     //             ]
//     //             .into_boxed_slice(),
//     //             debug_is_init: vec![false; 3].into_boxed_slice(),
//     //         }
//     //     }
//     // }
//     impl AllocInner {
//         fn new(size: usize) -> Self {
//             assert!(size >= 4);
//             // let mut mem = Vec::with_capacity(size);
//             // mem.resize_with(size, MaybeUninit::uninit);
//             let mut mem = Vec::with_capacity(size);
//             mem.resize_with(size, || Atomic::new(Node::uninit()));
//             let mut debug_is_init = Vec::with_capacity(size);
//             debug_is_init.resize_with(size, || AtomicBool::new(false));
//             Self {
//                 len: AtomicUsize::new(3),
//                 mem: mem.into_boxed_slice(),
//                 debug_is_init: debug_is_init.into_boxed_slice(),
//             }
//         }

//         fn try_alloc<const N: usize>(&self) -> Option<NodeId> {
//             assert!(N == 1 || N == 4);
//             let i = self.len.fetch_add(N, Ordering::SeqCst);
//             if i + N > self.mem.len() {
//                 return None;
//             }
//             let handle = NodeId(NonZeroU32::new(i as u32).unwrap());
//             if N == 1 {
//                 debug_assert_eq!(i, 3);
//             }
//             if N == 4 {
//                 debug_assert_eq!(i % 4, 0);
//             }
//             for offset in 0..N {
//                 debug_assert!(
//                     !self.debug_is_init[i + offset].load(Ordering::SeqCst),
//                     "allocate at already initialized memory at index {}",
//                     i + offset
//                 );
//             }
//             Some(handle)
//         }

//         // fn try_alloc4(&mut self) -> Option<NodeId> {
//         //     let handle = NonZeroU32::new(self.mem.len() as u32).unwrap();
//         //     debug_assert_eq!(handle.get() % 4, 0);
//         //     self.mem.extend([
//         //         MaybeUninit::uninit(),
//         //         MaybeUninit::uninit(),
//         //         MaybeUninit::uninit(),
//         //         MaybeUninit::uninit(),
//         //     ]);
//         //     self.debug_is_init.extend([false; 4]);
//         //     Some(NodeId(handle))
//         // }

//         // fn set4(&mut self, handle: AllocHandle, nodes: [Node; 4]) {
//         //     let i = handle.to_index();
//         //     debug_assert_eq!(i % 4, 0);
//         //     for (offset, node) in nodes.into_iter().enumerate() {
//         //         self.mem[i + offset] = MaybeUninit::new(node);
//         //         self.is_init[i + offset] = true;
//         //     }
//         // }

//         /// the memory must be uninitialized.
//         /// /// see also [`AllocInner::set`].
//         fn init<const N: usize>(&self, handle: NodeId, nodes: [Node; N]) {
//             assert!(N == 1 || N == 4);
//             let i = handle.to_index();
//             if N == 1 {
//                 debug_assert_eq!(i, 3);
//             }
//             if N == 4 {
//                 debug_assert_eq!(i % 4, 0);
//             }
//             for (offset, node) in nodes.into_iter().enumerate() {
//                 debug_assert!(
//                     !self.debug_is_init[i + offset].load(Ordering::SeqCst),
//                     "insert at already initialized memory at index {}",
//                     i + offset
//                 );
//                 self.mem[i + offset].store(node, Ordering::SeqCst);
//                 self.debug_is_init[i + offset].store(true, Ordering::SeqCst);
//             }
//         }

//         /// the memory must already be initialized.
//         /// see also [`AllocInner::init`].
//         fn set<const N: usize>(&self, handle: NodeId, nodes: [Node; N]) {
//             assert!(N == 1 || N == 4);
//             let i = handle.to_index();
//             if N == 1 {
//                 debug_assert_eq!(i, 3);
//             }
//             if N == 4 {
//                 debug_assert_eq!(i % 4, 0);
//             }
//             for (offset, node) in nodes.into_iter().enumerate() {
//                 debug_assert!(
//                     self.debug_is_init[i + offset].load(Ordering::SeqCst),
//                     "insert at already initialized memory at index {}",
//                     i + offset
//                 );
//                 self.mem[i + offset].store(node, Ordering::SeqCst);
//             }
//         }

//         // fn try_insert1(&mut self, node: Node) -> Option<NodeId> {
//         //     let handle = self.try_alloc1()?;
//         //     let i = handle.to_index();
//         //     debug_assert!(
//         //         !self.debug_is_init[i],
//         //         "insert at already initialized memory at index {}",
//         //         i
//         //     );
//         //     self.mem[i] = MaybeUninit::new(node);
//         //     self.debug_is_init[i] = true;
//         //     Some(handle)
//         // }

//         // fn try_insert4(&mut self, nodes: [Node; 4]) -> Option<NodeId> {
//         //     let handle = self.try_alloc4()?;
//         //     let i = handle.to_index();
//         //     for (offset, node) in nodes.into_iter().enumerate() {
//         //         debug_assert!(
//         //             !self.debug_is_init[i + offset],
//         //             "insert at already initialized memory at index {}",
//         //             i + offset
//         //         );
//         //         self.mem[i + offset] = MaybeUninit::new(node);
//         //         self.debug_is_init[i + offset] = true;
//         //     }
//         //     Some(handle)
//         // }

//         fn get_cloned<const N: usize>(&self, handle: NodeId) -> [Node; N] {
//             assert!(N == 1 || N == 4);
//             let i = handle.to_index();
//             if N == 4 {
//                 debug_assert_eq!(i % 4, 0);
//             }
//             for offset in 0..N {
//                 debug_assert!(
//                     self.debug_is_init[i + offset].load(Ordering::SeqCst),
//                     "read uninitialized memory at index {}",
//                     i + offset
//                 );
//             }
//             (self.mem[i..i + N])
//                 .as_array::<N>()
//                 .unwrap()
//                 .each_ref()
//                 .map(|m| m.load(Ordering::SeqCst))
//             // (self.mem[i..i + N])
//             //     .as_array::<N>()
//             //     .unwrap()
//             //     .each_ref()
//             //     .map(|m| unsafe { m.assume_init_ref() }.clone())
//         }

//         // pub(crate) fn get1(&self, handle: NodeId) -> &Node {}

//         // fn get1_mut(&mut self, handle: NodeId) -> &mut Node {
//         //     let i = handle.to_index();
//         //     // debug_assert_eq!(
//         //     //     i, 3,
//         //     //     "we should probably only use get1_mut for getting the root"
//         //     // );
//         //     debug_assert!(
//         //         self.debug_is_init[i],
//         //         "read uninitialized memory at index {}",
//         //         i
//         //     );
//         //     unsafe { self.mem[i].assume_init_mut() }
//         // }

//         // /// used for getting the four children
//         // fn get4(&self, handle: NodeId) -> [&Node; 4] {
//         //     let i = handle.to_index();
//         //     debug_assert!(i >= 4, "probably bad");
//         //     debug_assert_eq!(i % 4, 0, "unaligned read, probably bad");
//         //     for offset in 0..4 {
//         //         debug_assert!(
//         //             self.debug_is_init[i + offset],
//         //             "read uninitialized memory at index {}",
//         //             i + offset
//         //         );
//         //     }
//         //     (self.mem[i..i + 4])
//         //         .as_array::<4>()
//         //         .unwrap()
//         //         .each_ref()
//         //         .map(|m| unsafe { m.assume_init_ref() })
//         // }

//         // fn get4_mut(&mut self, handle: NodeId) -> [&mut Node; 4] {
//         //     let i = handle.to_index();
//         //     debug_assert!(i >= 4, "probably bad");
//         //     debug_assert_eq!(i % 4, 0, "unaligned read, probably bad");
//         //     for offset in 0..4 {
//         //         debug_assert!(
//         //             self.debug_is_init[i + offset],
//         //             "read uninitialized memory at index {}",
//         //             i + offset
//         //         );
//         //     }
//         //     (self.mem[i..i + 4])
//         //         .as_mut_array::<4>()
//         //         .unwrap()
//         //         .each_mut()
//         //         .map(|m| unsafe { m.assume_init_mut() })
//         // }

//         fn update(
//             &self,
//             handle: NodeId,
//             // set_order: Ordering,
//             // fetch_order: Ordering,
//             // f: impl FnMut(Node) -> Option<Node>,
//             f: impl FnOnce(&Atomic<Node>),
//         ) {
//             let i = handle.to_index();
//             debug_assert!(self.debug_is_init[i].load(Ordering::SeqCst));
//             // self.mem[i].fetch_update(set_order, fetch_order, f);
//             f(&self.mem[i]);
//         }
//     }
// }

pub(crate) use new_alloc::*;
mod new_alloc {
    use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize};

    use atomic::Atomic;

    use super::*;

    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
    pub(crate) struct NodeHandle(NonZeroU32);
    unsafe impl bytemuck::ZeroableInOption for NodeHandle {}
    unsafe impl bytemuck::PodInOption for NodeHandle {}

    /// the differences from &mut is that we can have one `NodeHandleMut` and multiple `NodeHandle` existing at the same time
    /// TODO: maybe we can have more than one?
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
    pub(super) struct NodeHandleMut(NonZeroU32);

    // invariant: cur.has_mut_handle implies cur.up_to_date
    // invariant: if someone is reallocating, cur.up_to_date implies old.has_mut_handle
    #[derive(Debug)]
    pub(super) struct Alloc {
        cur: AtomicPtr<AllocInner>,
        old: AtomicPtr<AllocInner>,
        /// true iff you're allowed to allocate
        /// !can_alloc implies realloc_lock, !realloc_lock implies can_alloc
        can_alloc: AtomicBool,
        /// false iff you're allowed to reallocate
        realloc_lock: AtomicBool,
    }

    #[derive(Debug)]
    struct AllocInner {
        len: AtomicUsize,
        mem: Box<[Atomic<Node>]>,
        // TODO: pack these bools into a bitflag
        has_mut_handle: Box<[AtomicBool]>,
        /// whether it's ok to read from this memory.
        /// in the new alloc, it's whether we've moved the element.
        /// in the old alloc, once we've moved something away, set up_to_date to false.
        /// the canonical one is cur.up_to_date, the one in old is just for debugging.
        /// TODO: rename.
        up_to_date: Box<[AtomicBool]>,
        /// whether it's defined to read from this memory.
        /// we might not need this now that we have up to date.
        /// except that we use SeqCst for debug_is_init, so it might still catch bugs.
        /// this is also about whether up_to_date is uninit maybe.
        debug_is_init: Box<[AtomicBool]>,
    }

    impl NodeHandle {
        fn to_index(self) -> usize {
            self.0.get() as usize
        }

        fn offset(self, offset: usize) -> Self {
            debug_assert!(offset < 4);
            unsafe { NodeHandle(NonZeroU32::new_unchecked(self.0.get() + offset as u32)) }
        }

        /// ret[0] == self
        pub(super) fn siblings(self) -> [NodeHandle; 4] {
            let i = self.to_index();
            debug_assert_eq!(i % 4, 0, "unaligned handle in siblings");
            unsafe {
                [
                    NodeHandle(NonZeroU32::new_unchecked(self.0.get())),
                    NodeHandle(NonZeroU32::new_unchecked(self.0.get() + 1)),
                    NodeHandle(NonZeroU32::new_unchecked(self.0.get() + 2)),
                    NodeHandle(NonZeroU32::new_unchecked(self.0.get() + 3)),
                ]
            }
        }
    }

    impl NodeHandleMut {
        fn to_index(self) -> usize {
            self.0.get() as usize
        }
    }

    impl Default for Alloc {
        fn default() -> Self {
            // const SIZE: usize = 16;
            const SIZE: usize = 1048576;
            Self {
                cur: AtomicPtr::new(Box::into_raw(Box::new(AllocInner::with_capacity(SIZE)))),
                old: AtomicPtr::new(std::ptr::null_mut()),
                can_alloc: AtomicBool::new(true),
                realloc_lock: AtomicBool::new(false),
            }
        }
    }
    impl Alloc {
        /// you must demote the returned mut handle to avoid ~leaking memory.
        /// fails if someone already has a mut handle.
        /// fails if the thread reallocating hasn't moved this element over yet.
        fn try_promote(&self, handle: NodeHandle) -> Option<NodeHandleMut> {
            let i = handle.to_index();
            // TODO: what if the pointers swap between here
            if !unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.up_to_date[i]
                .load(Ordering::SeqCst)
            {
                return None;
            }
            // TODO: i'm pretty sure this can be `Relaxed`
            // if self.cur.has_mut_handle[i]
            //     .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            //     .is_err()
            if unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.has_mut_handle[i]
                .swap(true, Ordering::Relaxed)
            {
                return None;
            }
            Some(NodeHandleMut(handle.0))
        }
        /// this can block.
        /// you must demote the returned mut handle to avoid ~leaking memory.
        pub(super) fn promote(&self, handle: NodeHandle) -> NodeHandleMut {
            loop {
                if let Some(handle_mut) = self.try_promote(handle) {
                    return handle_mut;
                }
                std::thread::yield_now();
            }
        }
        /// you must call this to release the mutable handle.
        /// returns the immutable handle for convenience.
        pub(super) fn demote(&self, handle: NodeHandleMut) -> NodeHandle {
            let i = handle.to_index();
            // debug_assert!(self.cur.has_mut_handle[i].load(Ordering::SeqCst));
            // self.cur.has_mut_handle[i].store(false, Ordering::Relaxed);
            let cur = unsafe { self.cur.load(Ordering::Relaxed).as_ref().unwrap() };
            assert!(cur.has_mut_handle[i].swap(false, Ordering::Relaxed));
            NodeHandle(handle.0)
        }

        /// note that this will use the thread for a while.
        fn realloc(&self) {
            // move over the elements that you can acquire mutable handles to
            // so we can start getting from the new alloc
            // then slowly acquire mutable handles to the rest of the elements and move them over
            // once you have all the mutable handles, free old

            fn try_move_node(cur: &AllocInner, old: &AllocInner, i: usize) -> Option<()> {
                debug_assert!(!cur.has_mut_handle[i].load(Ordering::SeqCst));
                debug_assert!(!cur.up_to_date[i].load(Ordering::SeqCst));
                debug_assert!(!cur.debug_is_init[i].load(Ordering::SeqCst));
                debug_assert!(old.up_to_date[i].load(Ordering::SeqCst));
                debug_assert!(old.debug_is_init[i].load(Ordering::SeqCst));
                // TODO: relax these
                // try to acquire the mutable handle in old
                if !old.has_mut_handle[i].swap(true, Ordering::SeqCst) {
                    return None;
                }
                let node = old.mem[i].load(Ordering::SeqCst);
                cur.mem[i].store(node, Ordering::SeqCst);
                #[cfg(debug_assertions)]
                cur.debug_is_init[i].store(true, Ordering::SeqCst);
                // it's important to set cur.up_to_date before clearing old.up_to_date
                cur.up_to_date[i].store(true, Ordering::SeqCst);
                old.up_to_date[i].store(false, Ordering::SeqCst);
                Some(())
            }

            debug_assert!(!self.realloc_lock.load(Ordering::SeqCst));
            debug_assert!(self.can_alloc.load(Ordering::SeqCst));
            self.can_alloc.store(false, Ordering::SeqCst);

            let (old_capacity, old_len) = {
                let cur = unsafe { &*self.cur.load(Ordering::SeqCst) };
                (cur.mem.len(), cur.len.load(Ordering::SeqCst))
            };

            let new = AllocInner::with_capacity(2 * old_capacity);

            // allocate space in new
            {
                // TODO: we can do this all at once
                // doing it this way is just a bit safer
                debug_assert!(old_len % 4 == 0);
                let handle0 = new.try_alloc::<1>().unwrap();
                debug_assert!(handle0.to_index() == 3);
                for _ in 0..old_len / 4 {
                    new.try_alloc::<4>().unwrap();
                }
            }

            // swing the pointers.
            // immediately after this, all the gets and updates will fail and go to old,
            // but this lets us reenable allocations sooner.
            {
                debug_assert!(self.old.load(Ordering::SeqCst).is_null());
                let old = self.cur.load(Ordering::SeqCst);
                self.old.store(old, Ordering::SeqCst);
                let new = Box::into_raw(Box::new(new));
                self.cur.store(new, Ordering::SeqCst);
                debug_assert!(!self.can_alloc.load(Ordering::SeqCst));
                self.can_alloc.store(true, Ordering::SeqCst);
            }

            // the pointers won't move anymore, so just get references to the things
            #[expect(unused_variables)]
            let new = ();
            let cur = unsafe { &*self.cur.load(Ordering::SeqCst) };
            let old = unsafe { &*self.old.load(Ordering::SeqCst) };

            // move over the elements that we can immediately acquire mutable handles to
            let mut unmoved_handles = Vec::with_capacity(old_capacity);
            {
                // let mut debug_mut_handles = Vec::with_capacity(old_capacity);
                for i in 3..old_len {
                    match try_move_node(cur, old, i) {
                        Some(()) => {
                            // debug_mut_handles.push(i);
                        }
                        None => {
                            unmoved_handles.push(i);
                        }
                    }
                }
            }

            // move over the rest of the elements
            {
                let mut index = 0;
                while !unmoved_handles.is_empty() {
                    let i = unmoved_handles[index];
                    if try_move_node(cur, old, i).is_some() {
                        unmoved_handles.swap_remove(index);
                    } else {
                        index += 1;
                        index %= unmoved_handles.len();
                    }
                }
            }

            // free old
            {
                let old = old as *const AllocInner as *mut AllocInner;
                debug_assert_eq!(self.old.load(Ordering::SeqCst), old);
                debug_assert!(!old.is_null());
                unsafe { Box::from_raw(old) };
            }
        }

        fn alloc<const N: usize>(&self) -> NodeHandle {
            loop {
                if self.can_alloc.load(Ordering::SeqCst) {
                    if let Some(handle) =
                        unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }
                            .try_alloc::<N>()
                    {
                        return handle;
                    } else {
                        // maybe these can be `Relaxed`???
                        if !self.realloc_lock.swap(true, Ordering::SeqCst) {
                            self.realloc();
                            self.realloc_lock.store(false, Ordering::SeqCst);
                        }
                    }
                }
                std::thread::yield_now();
            }
        }

        // fn insert<const N: usize>(&self, nodes: [Node; N]) -> NodeId {
        //     let handle = self.alloc::<N>();
        //     self.init::<N>(handle, nodes);
        //     handle
        // }
        // insert_leaf_uncolored instead of insert so it's easier to transition to SOA
        fn insert_leaf_uncolored<const N: usize>(&self, doms: [Domain; N]) -> NodeHandle {
            let handle0 = self.alloc::<N>();
            for (offset, dom) in doms.into_iter().enumerate() {
                let handle = handle0.offset(offset);
                let i = handle.to_index();

                debug_assert!(
                    !unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.debug_is_init[i]
                        .load(Ordering::SeqCst),
                    "insert at already initialized memory at index {}",
                    i
                );
                #[cfg(debug_assertions)]
                unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.debug_is_init[i]
                    .store(true, Ordering::SeqCst);

                debug_assert!(
                    !unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.up_to_date[i]
                        .load(Ordering::SeqCst)
                );
                // TODO: relax this
                unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.up_to_date[i]
                    .store(true, Ordering::SeqCst);

                // we know statically that promoting the handle will succeed
                // so maybe we should do this better
                // we also know that the element is in cur not old
                let handle = self
                    .try_promote(handle)
                    .expect("no one else could have promoted this handle since we haven't given it to anyone, and it's up_to_date because TODO: why?");
                // TODO: this should panic bc it's uninit
                // self.update(handle, |node| node.store(val, order));
                unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.update_with(
                    handle,
                    |node| {
                        node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
                            // debug_assert!(node.dom.is_uninit());
                            Some(Node { dom, ..node })
                        })
                    },
                );
                self.demote(handle);
            }
            handle0
        }
        pub(super) fn insert_leaf_uncolored1(&self, dom: Domain) -> NodeHandle {
            self.insert_leaf_uncolored::<1>([dom])
        }
        pub(super) fn insert_leaf_uncolored4(&self, doms: [Domain; 4]) -> NodeHandle {
            self.insert_leaf_uncolored::<4>(doms)
        }

        fn get_with<Ret, F: Fn(Node) -> Ret>(&self, handle: NodeHandle, f: F) -> Ret {
            // look in `cur`
            // if not found, look in `old` (maybe here we need to do an expensive SeqCst thing)
            if let Some(ret) =
                unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.get_with(handle, &f)
            {
                return ret;
            }
            if let Some(ret) =
                unsafe { self.old.load(Ordering::SeqCst).as_ref().unwrap() }.get_with(handle, &f)
            {
                return ret;
            }
            unreachable!("probably not actually unreachable");
            if let Some(ret) =
                unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.get_with(handle, &f)
            {
                return ret;
            }
            unreachable!();
        }
        pub(super) fn get_dom(&self, handle: NodeHandle) -> Domain {
            self.get_with(handle, |node| node.dom)
        }
        pub(super) fn get_leaf_distance_cache(&self, handle: NodeHandle) -> u32 {
            self.get_with(handle, |node| node.leaf_distance_cache)
        }
        pub(super) fn get_color(&self, handle: NodeHandle) -> OptionColor {
            self.get_with(handle, |node| node.color)
        }
        pub(super) fn get_left_child(&self, handle: NodeHandle) -> Option<NodeHandle> {
            self.get_with(handle, |node| node.left_child)
        }

        /// returns the old value, or maybe just whatever the user wants to.
        pub(super) fn update_with<Ret, F: Fn(&Atomic<Node>) -> Ret>(
            &self,
            handle: NodeHandleMut,
            f: F,
        ) -> Ret {
            // look in `cur`
            // if not found, it means another thread is waiting for us to be done
            // we can't help out, bc what if they gave up after we exited `update` but before we released our mut handle
            // so just look in `old`, and our caller should release this handle soon

            if let Some(ret) =
                unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.update_with(handle, &f)
            {
                return ret;
            }
            if let Some(ret) =
                unsafe { self.old.load(Ordering::SeqCst).as_ref().unwrap() }.update_with(handle, &f)
            {
                return ret;
            }
            unreachable!();
        }
    }

    impl AllocInner {
        fn with_capacity(capacity: usize) -> Self {
            assert!(capacity >= 4);

            let mut mem = Vec::with_capacity(capacity);
            mem.resize_with(capacity, || Atomic::new(Node::uninit()));
            let mut has_mut_handle = Vec::with_capacity(capacity);
            has_mut_handle.resize_with(capacity, || AtomicBool::new(false));
            let mut up_to_date = Vec::with_capacity(capacity);
            up_to_date.resize_with(capacity, || AtomicBool::new(false));
            let mut debug_is_init = Vec::with_capacity(capacity);
            debug_is_init.resize_with(capacity, || AtomicBool::new(false));

            Self {
                len: AtomicUsize::new(3),
                mem: mem.into_boxed_slice(),
                has_mut_handle: has_mut_handle.into_boxed_slice(),
                up_to_date: up_to_date.into_boxed_slice(),
                debug_is_init: debug_is_init.into_boxed_slice(),
            }
        }

        fn capacity(&self) -> usize {
            let ret = self.mem.len();
            debug_assert_eq!(ret, self.has_mut_handle.len());
            debug_assert_eq!(ret, self.up_to_date.len());
            debug_assert_eq!(ret, self.debug_is_init.len());
            ret
        }

        /// fails if we're out of memory.
        fn try_alloc<const N: usize>(&self) -> Option<NodeHandle> {
            assert!(N == 1 || N == 4);
            let i = self.len.fetch_add(N, Ordering::SeqCst);
            if i + N > self.mem.len() {
                return None;
            }
            let handle = NodeHandle(NonZeroU32::new(i as u32).unwrap());
            if N == 1 {
                debug_assert_eq!(i, 3);
            }
            if N == 4 {
                debug_assert_eq!(i % 4, 0);
            }
            for offset in 0..N {
                debug_assert!(
                    !self.debug_is_init[i + offset].load(Ordering::SeqCst),
                    "allocate at already initialized memory at index {}",
                    i + offset
                );
            }
            Some(handle)
        }

        /// returns `None` if the element isn't up_to_date.
        /// this can fail spuriously (in the future).
        fn get_with<Ret, F: FnOnce(Node) -> Ret>(&self, handle: NodeHandle, f: F) -> Option<Ret> {
            let i = handle.to_index();
            // `Relaxed` implies this can fail spuriously
            // TODO: relax these orderings
            if self.up_to_date[i].load(Ordering::SeqCst) {
                debug_assert!(self.debug_is_init[i].load(Ordering::SeqCst));
                Some(f(self.mem[i].load(Ordering::SeqCst)))
            } else {
                None
            }
        }

        /// returns `None` if the element isn't up_to_date.
        fn update_with<Ret, F: FnOnce(&Atomic<Node>) -> Ret>(
            &self,
            handle: NodeHandleMut,
            f: F,
        ) -> Option<Ret> {
            let i = handle.to_index();
            // TODO: relax these orderings
            if self.up_to_date[i].load(Ordering::SeqCst) {
                debug_assert!(self.debug_is_init[i].load(Ordering::SeqCst));
                debug_assert!(self.has_mut_handle[i].load(Ordering::SeqCst));
                Some(f(&self.mem[i]))
            } else {
                None
            }
        }
    }
}

// mod new_alloc_soa {
//     use super::*;

//     struct NodeId(u32);
//     struct DomHandle(u32);
//     struct DomHandleMut(u32);
//     struct LeafDistanceCacheHandle(u32);
//     struct LeafDistanceCacheHandleMut(u32);
//     struct ColorHandle(u32);
//     struct ColorHandleMut(u32);
//     struct ChildIdHandle(u32);
//     struct ChildIdHandleMut(u32);

//     struct Alloc {
//         cur: AllocInner,
//         old: Option<AllocInner>,
//         // AllocInner should be inlined into Alloc
//         cur.up_to_date: AllocInnerT<bool>,
//     }

//     struct AllocInner {
//         doms: AllocInnerT<Domain>,
//         leaf_distance_caches: AllocInnerT<u32>,
//         colors: AllocInnerT<OptionColor>,
//         child_ids: AllocInnerT<Option<NodeId>>,
//         debug_is_init: AllocInnerT<bool>,
//     }

//     struct AllocInnerT<T> {
//         mem: Box<[T]>,
//         debug_is_init: Box<[T]>,
//     }

//     impl Alloc {
//         fn realloc(&self) {
//             let new_cur = AllocInner::with_capacity(self.cur.capacity() * 2);
//             // clone over the elements that you can acquire mutable handles to
//             // so we can start getting from the new alloc
//             // then slowly acquire mutable handles to the rest of the elements and clone them over
//             new_cur.partial_clone_from(self.cur);
//             self.cur = new_cur;
//         }

//         fn get(&self) {
//             // look in the current alloc
//             // if not found, look in the old alloc
//             todo!();
//         }
//     }

//     impl AllocInner {}

//     impl AllocInnerT<T> {}
// }

#[derive(Debug)]
pub(crate) struct Tree {
    dom: Domain,
    // TODO: concurrent `Alloc`
    alloc: RwLock<Alloc>,
    root: NodeHandle,
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
        let alloc = Alloc::default();
        // let root = alloc.insert1(Node::new_leaf_colored(
        //     dom,
        //     metabrot_sample(dom.mid()).color(),
        // ));
        let root = alloc.insert_leaf_uncolored1(dom);
        let root = alloc.promote(root);
        alloc.update_with(root, |node| {
            node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
                Some(Node {
                    color: OptionColor::new_some(metabrot_sample(node.dom.mid()).color()),
                    ..node
                })
            })
            .unwrap();
        });
        let root = alloc.demote(root);
        Self {
            dom,
            alloc: RwLock::new(alloc),
            root,
        }
    }

    pub(crate) fn node_count(&self) -> usize {
        let mut count = 0;
        let mut stack = Vec::with_capacity(64);

        let alloc = self.alloc.read().unwrap();
        stack.push(self.root);

        while let Some(handle) = stack.pop() {
            count += 1;
            if let Some(child_id) = alloc.get_left_child(handle) {
                stack.extend(child_id.siblings());
            }
        }
        count
    }

    pub(crate) fn mid_of_node_id(&self, handle: NodeHandle) -> (Real, Imag) {
        let alloc = self.alloc.read().unwrap();
        alloc.get_dom(handle).mid()
    }

    /// must have that leaf_id is a leaf.
    /// fails if the domain gets too small.
    fn try_split(&mut self, leaf_handle: NodeHandle) -> Option<()> {
        // let mut alloc = self.alloc.write().unwrap();
        let alloc = self.alloc.get_mut().expect("alloc poisoned");
        // let new_leafs = {
        //     let node = alloc.get1_cloned(leaf_id);
        //     assert!(node.child_id.is_none());
        //     node.dom.split()?.map(Node::new_leaf_uncolored)
        // };
        // let child_id = alloc.insert4(new_leafs);
        // let node = alloc.get1_cloned(leaf_id);
        // alloc.update(leaf_id, |node| {
        //     node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
        //         debug_assert!(node.child_id.is_none());
        //         Some(Node {
        //             left_child: Some(child_id),
        //             ..node
        //         })
        //     })
        //     .unwrap();
        // });
        let doms = {
            let dom = alloc.get_dom(leaf_handle);
            debug_assert!(alloc.get_left_child(leaf_handle).is_none());
            dom.split()?
        };
        let child_handle = alloc.insert_leaf_uncolored4(doms);
        let leaf_handle = alloc.promote(leaf_handle);
        alloc.update_with(leaf_handle, |node| {
            node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
                debug_assert!(node.left_child.is_none());
                debug_assert_eq!(node.leaf_distance_cache, 0);
                Some(Node {
                    left_child: Some(child_handle),
                    leaf_distance_cache: 1,
                    ..node
                })
            })
        });
        alloc.demote(leaf_handle);
        Some(())
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
    // pub(crate) fn refine(slf: Arc<Self>, window: Window) -> Option<[NodeId; 4]> {
    pub(crate) fn refine(&mut self, window: Window) -> Option<[NodeHandle; 4]> {
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
        fn get_shallowest_leaf(tree: &Tree, window: Window) -> Option<(u32, NodeHandle)> {
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
                let alloc = tree.alloc.read().unwrap();
                // let node = alloc.get1_cloned(node_id);
                // TODO: instead of doing this check on pop, do it on push
                // this also lets us do less work in the case where the domain is contained inside the window
                if !window.overlaps(alloc.get_dom(node_id)) {
                    continue;
                }
                if depth >= shallowest_depth {
                    continue;
                }
                match (alloc.get_color(node_id), alloc.get_left_child(node_id)) {
                    (_, Some(child_id)) => {
                        // let children = alloc.get4_cloned(child_id);
                        // let leaf_distance = internal.compute_leaf_distance();
                        // let leaf_distance = children
                        //     .iter()
                        //     .map(|c| c.leaf_distance_cache)
                        //     .min()
                        //     .unwrap()
                        //     + 1;
                        let leaf_distance = child_id
                            .siblings()
                            .map(|child_handle| alloc.get_leaf_distance_cache(child_handle))
                            .iter()
                            .min()
                            .unwrap()
                            + 1;
                        if leaf_distance < alloc.get_leaf_distance_cache(node_id) {
                            let node_id = alloc.promote(node_id);
                            alloc.update_with(node_id, |node| {
                                let _ =
                                    node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
                                        if leaf_distance < node.leaf_distance_cache {
                                            Some(Node {
                                                leaf_distance_cache: leaf_distance,
                                                ..node
                                            })
                                        } else {
                                            None
                                        }
                                    });
                            });
                            alloc.demote(node_id);
                            // node.leaf_distance_cache = leaf_distance;
                            // todo!("this is incorrect now");
                        }
                        // node.leaf_distance_cache.fetch_min(leaf_distance, Ordering::Relaxed);
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
                    (color, None) => {
                        if color.is_some() && depth < shallowest_depth {
                            shallowest_depth = depth;
                            shallowest_leaf_id = Some(node_id);
                        }
                    }
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

        // Node::try_split(node_id, &mut self.alloc)?;
        // Self::try_split(slf.clone(), node_id);
        self.try_split(node_id);
        // let alloc = self.alloc.read().unwrap();
        let alloc = self.alloc.get_mut().expect("alloc poisoned");
        // let node = alloc.get1_cloned(node_id);
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
        Some(
            alloc
                .get_left_child(node_id)
                .expect("we just split it")
                .siblings(),
        )
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
        node_id: NodeHandle,
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
        // let mut alloc = self.alloc.write().unwrap();
        let alloc = self.alloc.read().unwrap();
        let node_id = alloc.promote(node_id);
        alloc.update_with(node_id, |node| {
            node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
                assert!(node.color.is_none());
                Some(Node {
                    color: OptionColor::new_some(color),
                    ..node
                })
            })
            .unwrap();
        });
        alloc.demote(node_id);
        // let node = alloc.get1_mut(node_id);
        // node.set_color(color);

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

        let alloc = self.alloc.read().unwrap();
        let mut node_id = self.root;
        let mut closest_sample_dist = distance(center, self.dom.mid());
        let mut closest_sample_color = alloc.get_color(node_id).expect("root must have a color");

        loop {
            // let node = alloc.get1_cloned(node_id);
            let Some(left_child) = alloc.get_left_child(node_id) else {
                break;
            };
            let child_offset = alloc.get_dom(node_id).child_offset_containing(center);
            node_id = left_child.siblings()[child_offset];

            let dist = distance(center, alloc.get_dom(node_id).mid());
            let color = alloc.get_color(node_id);
            if dist < closest_sample_dist && color.is_some() {
                closest_sample_dist = dist;
                closest_sample_color = color.unwrap();
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
