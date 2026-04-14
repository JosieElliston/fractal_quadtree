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

// TODO: Node where each field is atomic? NodeWrapper(Atomic<Node>)?
// we need atomic load/store
// #[repr(C, align(64))]
// #[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
// struct Node {
//     // write_lock: bool,
//     dom: Domain,
//     // lock: bool,
//     // color: Option<Color32>,
//     leaf_distance_cache: u32,
//     color: OptionColor,
//     /// leftmost child id
//     left_child: Option<NodeHandle>,
//     _pad: [u8; 24],
// }
#[repr(C, align(64))]
#[derive(Debug)]
struct Node {
    // write_lock: bool,
    dom: Domain,
    // lock: bool,
    // color: Option<Color32>,
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

// pub(crate) use alloc2::*;
// mod alloc2 {
//     use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize};

//     use atomic::Atomic;

//     use super::*;

//     #[repr(transparent)]
//     #[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
//     pub(crate) struct NodeHandle(NonZeroU32);
//     unsafe impl bytemuck::ZeroableInOption for NodeHandle {}
//     unsafe impl bytemuck::PodInOption for NodeHandle {}

//     /// the differences from &mut is that we can have one `NodeHandleMut` and multiple `NodeHandle` existing at the same time
//     /// TODO: maybe we can have more than one?
//     #[repr(transparent)]
//     #[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
//     pub(super) struct NodeHandleMut(NonZeroU32);

//     #[repr(transparent)]
//     #[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
//     struct BitFlag(u8);

//     // invariant: cur.has_mut_handle implies cur.up_to_date
//     // invariant: if someone is reallocating, cur.up_to_date implies old.has_mut_handle
//     #[derive(Debug)]
//     pub(super) struct Alloc {
//         cur: AtomicPtr<AllocInner>,
//         old: AtomicPtr<AllocInner>,
//         /// true iff you're allowed to allocate
//         /// !can_alloc implies realloc_lock, !realloc_lock implies can_alloc
//         can_alloc: AtomicBool,
//         /// true iff someone is reallocating
//         realloc_lock: AtomicBool,
//     }

//     #[derive(Debug)]
//     struct AllocInner {
//         /// the actual allocated length, len_lo <= capacity
//         len_real: AtomicUsize,
//         /// the length we speculatively update when we allocate, len_lo <= len_hi, can be > capacity
//         len_speculative: AtomicUsize,
//         /// xxxx xxxx xxxx root node node node node
//         mem: Box<[Atomic<Node>]>,
//         // TODO: pack these bools into a bitflag
//         flags: Box<[Atomic<BitFlag>]>,
//         // has_mut_handle: Box<[AtomicBool]>,
//         // up_to_date: Box<[AtomicBool]>,
//         /// whether it's defined to read from this memory.
//         /// we might not need this now that we have up to date.
//         /// except that we use SeqCst for debug_is_init, so it might still catch bugs.
//         /// this is also about whether up_to_date is uninit maybe.
//         debug_is_init: Box<[AtomicBool]>,
//     }

//     impl NodeHandle {
//         fn to_index(self) -> usize {
//             self.0.get() as usize
//         }

//         fn offset(self, offset: usize) -> Self {
//             debug_assert!(offset < 4);
//             unsafe { NodeHandle(NonZeroU32::new_unchecked(self.0.get() + offset as u32)) }
//         }

//         /// ret[0] == self
//         pub(super) fn siblings(self) -> [NodeHandle; 4] {
//             let i = self.to_index();
//             debug_assert_eq!(i % 4, 0, "unaligned handle in siblings");
//             unsafe {
//                 [
//                     NodeHandle(NonZeroU32::new_unchecked(self.0.get())),
//                     NodeHandle(NonZeroU32::new_unchecked(self.0.get() + 1)),
//                     NodeHandle(NonZeroU32::new_unchecked(self.0.get() + 2)),
//                     NodeHandle(NonZeroU32::new_unchecked(self.0.get() + 3)),
//                 ]
//             }
//         }
//     }

//     impl NodeHandleMut {
//         fn to_index(self) -> usize {
//             self.0.get() as usize
//         }

//         /// note that this doesn't release the mutable handle, use `Alloc::demote` for that.
//         /// this just exists for convenience for some APIs that want a const handle.
//         pub(super) fn to_const(self) -> NodeHandle {
//             NodeHandle(self.0)
//         }
//     }

//     impl BitFlag {
//         // they're in this order bc we might allow multiple mutable handles in the future

//         const NONE: Self = BitFlag(0);
//         /// whether it's ok to read from this memory.
//         /// in the new alloc, it's whether we've moved the element.
//         /// in the old alloc, once we've moved something away, set up_to_date to false.
//         /// the canonical one is cur.up_to_date, the one in old is just for debugging.
//         /// TODO: rename.
//         const UP_TO_DATE: Self = BitFlag(1);
//         const HAS_MUT_HANDLE: Self = BitFlag(2);
//         const BOTH: Self = BitFlag(Self::UP_TO_DATE.0 | Self::HAS_MUT_HANDLE.0);

//         const fn new(up_to_date: bool, has_mut_handle: bool) -> Self {
//             let mut flags = 0;
//             if up_to_date {
//                 flags |= Self::UP_TO_DATE.0;
//             }
//             if has_mut_handle {
//                 flags |= Self::HAS_MUT_HANDLE.0;
//             }
//             Self(flags)
//         }

//         const fn up_to_date(self) -> bool {
//             self.0 & Self::UP_TO_DATE.0 != 0
//         }

//         const fn has_mut_handle(self) -> bool {
//             self.0 & Self::HAS_MUT_HANDLE.0 != 0
//         }
//     }
//     impl std::ops::BitOr for BitFlag {
//         type Output = Self;

//         fn bitor(self, rhs: Self) -> Self {
//             Self(self.0 | rhs.0)
//         }
//     }

//     impl Default for Alloc {
//         fn default() -> Self {
//             // const SIZE: usize = 16;
//             const SIZE: usize = 1048576;
//             Self {
//                 cur: AtomicPtr::new(Box::into_raw(Box::new(AllocInner::with_capacity(SIZE)))),
//                 old: AtomicPtr::new(std::ptr::null_mut()),
//                 can_alloc: AtomicBool::new(true),
//                 realloc_lock: AtomicBool::new(false),
//             }
//         }
//     }
//     impl Alloc {
//         /// you must demote the returned mut handle to avoid ~leaking memory.
//         /// fails if someone already has a mut handle.
//         /// fails if the thread reallocating hasn't moved this element over yet.
//         pub(super) fn try_promote(&self, handle: NodeHandle) -> Option<NodeHandleMut> {
//             let i = handle.to_index();

//             // // TODO: what if the pointers swap between here
//             // if !unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.up_to_date[i]
//             //     .load(Ordering::SeqCst)
//             // {
//             //     return None;
//             // }
//             // // TODO: i'm pretty sure this can be `Relaxed`
//             // // if self.cur.has_mut_handle[i]
//             // //     .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
//             // //     .is_err()
//             // if unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.has_mut_handle[i]
//             //     .swap(true, Ordering::Relaxed)
//             // {
//             //     return None;
//             // }
//             if unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.flags[i]
//                 .compare_exchange(
//                     BitFlag::new(true, false),
//                     BitFlag::new(true, true),
//                     Ordering::SeqCst,
//                     Ordering::SeqCst,
//                 )
//                 .is_err()
//             {
//                 return None;
//             }
//             debug_assert!(
//                 unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.debug_is_init[i]
//                     .load(Ordering::SeqCst)
//             );
//             println!("promoted handle {}", i);
//             Some(NodeHandleMut(handle.0))
//         }
//         /// this can block.
//         /// you must demote the returned mut handle to avoid ~leaking memory.
//         pub(super) fn promote(&self, handle: NodeHandle) -> NodeHandleMut {
//             println!("promote start handle {}", handle.to_index());
//             loop {
//                 if let Some(handle_mut) = self.try_promote(handle) {
//                     println!("promote end handle {}", handle.to_index());
//                     return handle_mut;
//                 }
//                 std::thread::yield_now();
//             }
//         }
//         /// you must call this to release the mutable handle.
//         /// returns the immutable handle for convenience.
//         pub(super) fn demote(&self, handle: NodeHandleMut) -> NodeHandle {
//             let i = handle.to_index();
//             // debug_assert!(self.cur.has_mut_handle[i].load(Ordering::SeqCst));
//             // self.cur.has_mut_handle[i].store(false, Ordering::Relaxed);
//             let cur = unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() };
//             // assert!(cur.has_mut_handle[i].swap(false, Ordering::Relaxed));
//             assert_eq!(
//                 cur.flags[i].swap(BitFlag::new(true, false), Ordering::SeqCst),
//                 BitFlag::new(true, true)
//             );
//             println!("demoted handle {}", i);
//             NodeHandle(handle.0)
//         }

//         /// note that this will use the thread for a while.
//         fn realloc(&self) {
//             // move over the elements that you can acquire mutable handles to
//             // so we can start getting from the new alloc
//             // then slowly acquire mutable handles to the rest of the elements and move them over
//             // once you have all the mutable handles, free old

//             dbg!("reallocating");

//             fn try_move_node(cur: &AllocInner, old: &AllocInner, i: usize) -> Option<()> {
//                 // debug_assert!(!cur.has_mut_handle[i].load(Ordering::SeqCst));
//                 // debug_assert!(!cur.up_to_date[i].load(Ordering::SeqCst));
//                 debug_assert_eq!(
//                     cur.flags[i].load(Ordering::SeqCst),
//                     BitFlag::new(false, false)
//                 );
//                 debug_assert!(!cur.debug_is_init[i].load(Ordering::SeqCst));
//                 // debug_assert!(old.up_to_date[i].load(Ordering::SeqCst));
//                 debug_assert!(old.flags[i].load(Ordering::SeqCst).up_to_date());
//                 debug_assert!(old.debug_is_init[i].load(Ordering::SeqCst));
//                 // TODO: relax these
//                 // try to acquire the mutable handle in old
//                 // if !old.has_mut_handle[i].swap(true, Ordering::SeqCst) {
//                 //     return None;
//                 // }
//                 // TODO: was i doing the opposite?
//                 if old.flags[i]
//                     .swap(BitFlag::new(true, true), Ordering::SeqCst)
//                     .has_mut_handle()
//                 {
//                     return None;
//                 }
//                 let node = old.mem[i].load(Ordering::SeqCst);
//                 cur.mem[i].store(node, Ordering::SeqCst);
//                 #[cfg(debug_assertions)]
//                 cur.debug_is_init[i].store(true, Ordering::SeqCst);
//                 // it's important to set cur.up_to_date before clearing old.up_to_date
//                 // cur.up_to_date[i].store(true, Ordering::SeqCst);
//                 // old.up_to_date[i].store(false, Ordering::SeqCst);
//                 assert_eq!(
//                     cur.flags[i].swap(BitFlag::new(true, false), Ordering::SeqCst),
//                     BitFlag::new(false, false)
//                 );
//                 // the value of old.has_mut_handle[i] is a bit unconstrained
//                 assert_eq!(
//                     old.flags[i].swap(BitFlag::new(false, true), Ordering::SeqCst),
//                     BitFlag::new(true, true)
//                 );
//                 Some(())
//             }

//             debug_assert!(self.realloc_lock.load(Ordering::SeqCst));
//             debug_assert!(self.can_alloc.load(Ordering::SeqCst));
//             self.can_alloc.store(false, Ordering::SeqCst);

//             let (old_capacity, old_len) = {
//                 let cur = unsafe { &*self.cur.load(Ordering::SeqCst) };
//                 (cur.capacity(), cur.len_real.load(Ordering::SeqCst))
//             };

//             let new = AllocInner::with_capacity(2 * old_capacity);

//             // allocate space in new
//             {
//                 // TODO: we can do this all at once
//                 // doing it this way is just a bit safer
//                 debug_assert!(old_len % 4 == 0);
//                 let handle0 = new.try_alloc::<1>().unwrap();
//                 debug_assert!(handle0.to_index() == 3);
//                 for _ in ((handle0.to_index() + 1)..old_len).step_by(4) {
//                     new.try_alloc::<4>().unwrap();
//                 }
//                 debug_assert_eq!(new.capacity(), 2 * old_capacity);
//                 debug_assert_eq!(old_len, new.len_real.load(Ordering::SeqCst));
//                 debug_assert_eq!(
//                     old_len,
//                     unsafe { &*self.cur.load(Ordering::SeqCst) }
//                         .len_real
//                         .load(Ordering::SeqCst)
//                 );
//             }

//             // swing the pointers.
//             // immediately after this, all the gets and updates will fail and go to old,
//             // but this lets us reenable allocations sooner.
//             {
//                 debug_assert!(self.old.load(Ordering::SeqCst).is_null());
//                 let old = self.cur.load(Ordering::SeqCst);
//                 self.old.store(old, Ordering::SeqCst);
//                 let new = Box::into_raw(Box::new(new));
//                 self.cur.store(new, Ordering::SeqCst);
//                 debug_assert!(!self.can_alloc.load(Ordering::SeqCst));
//                 self.can_alloc.store(true, Ordering::SeqCst);
//             }

//             // the pointers won't move anymore, so just get references to the things
//             #[expect(unused_variables)]
//             let new = ();
//             let cur = unsafe { &*self.cur.load(Ordering::SeqCst) };
//             let old = unsafe { &*self.old.load(Ordering::SeqCst) };

//             // move over the elements that we can immediately acquire mutable handles to
//             let mut unmoved_handles = Vec::with_capacity(old_capacity);
//             {
//                 // let mut debug_mut_handles = Vec::with_capacity(old_capacity);
//                 for i in 3..old_len {
//                     match try_move_node(cur, old, i) {
//                         Some(()) => {
//                             // debug_mut_handles.push(i);
//                         }
//                         None => {
//                             unmoved_handles.push(i);
//                         }
//                     }
//                 }
//             }

//             dbg!(&cur.flags);
//             dbg!(&cur.debug_is_init);
//             dbg!(&old.flags);
//             dbg!(&old.debug_is_init);
//             todo!(
//                 "it currently looks like someone (not one that uses try_promote) isn't freeing their handle, making it deadlock"
//             );

//             // move over the rest of the elements
//             {
//                 let mut index = 0;
//                 while !unmoved_handles.is_empty() {
//                     let i = unmoved_handles[index];
//                     if try_move_node(cur, old, i).is_some() {
//                         unmoved_handles.swap_remove(index);
//                     } else {
//                         index += 1;
//                         index %= unmoved_handles.len();
//                     }
//                 }
//             }

//             // free old
//             {
//                 let old = old as *const AllocInner as *mut AllocInner;
//                 debug_assert_eq!(self.old.load(Ordering::SeqCst), old);
//                 debug_assert!(!old.is_null());
//                 unsafe { drop(Box::from_raw(old)) };
//                 self.old.store(std::ptr::null_mut(), Ordering::SeqCst);
//             }

//             dbg!("done reallocating");
//         }

//         fn alloc<const N: usize>(&self) -> NodeHandle {
//             loop {
//                 if self.can_alloc.load(Ordering::SeqCst) {
//                     if let Some(handle) =
//                         unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }
//                             .try_alloc::<N>()
//                     {
//                         return handle;
//                     } else {
//                         // maybe these can be `Relaxed`???
//                         if !self.realloc_lock.swap(true, Ordering::SeqCst) {
//                             self.realloc();
//                             self.realloc_lock.store(false, Ordering::SeqCst);
//                         }
//                     }
//                 }
//                 std::thread::yield_now();
//             }
//         }

//         // fn insert<const N: usize>(&self, nodes: [Node; N]) -> NodeId {
//         //     let handle = self.alloc::<N>();
//         //     self.init::<N>(handle, nodes);
//         //     handle
//         // }
//         // insert_leaf_uncolored instead of insert so it's easier to transition to SOA
//         fn insert_leaf_uncolored<const N: usize>(&self, doms: [Domain; N]) -> NodeHandle {
//             let handle0 = self.alloc::<N>();
//             for (offset, dom) in doms.into_iter().enumerate() {
//                 let handle = handle0.offset(offset);
//                 let i = handle.to_index();

//                 debug_assert!(
//                     !unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.debug_is_init[i]
//                         .load(Ordering::SeqCst),
//                     "insert at already initialized memory at index {}",
//                     i
//                 );
//                 #[cfg(debug_assertions)]
//                 unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.debug_is_init[i]
//                     .store(true, Ordering::SeqCst);

//                 // debug_assert!(
//                 //     !unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.up_to_date[i]
//                 //         .load(Ordering::SeqCst)
//                 // );
//                 // // TODO: relax this
//                 // unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.up_to_date[i]
//                 //     .store(true, Ordering::SeqCst);
//                 assert_eq!(
//                     unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.flags[i]
//                         .swap(BitFlag::new(true, false), Ordering::SeqCst),
//                     BitFlag::new(false, false)
//                 );

//                 // we know statically that promoting the handle will succeed
//                 // so maybe we should do this better
//                 // we also know that the element is in cur not old
//                 let handle = self
//                     .try_promote(handle)
//                     .expect("no one else could have promoted this handle since we haven't given it to anyone, and it's up_to_date because TODO: why?");
//                 // TODO: this should panic bc it's uninit
//                 // self.update(handle, |node| node.store(val, order));
//                 unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.update_with(
//                     handle,
//                     |node| {
//                         node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
//                             // debug_assert!(node.dom.is_uninit());
//                             Some(Node { dom, ..node })
//                         })
//                     },
//                 );
//                 self.demote(handle);
//             }
//             handle0
//         }
//         pub(super) fn insert_leaf_uncolored1(&self, dom: Domain) -> NodeHandle {
//             self.insert_leaf_uncolored::<1>([dom])
//         }
//         pub(super) fn insert_leaf_uncolored4(&self, doms: [Domain; 4]) -> NodeHandle {
//             self.insert_leaf_uncolored::<4>(doms)
//         }

//         fn get_with<Ret, F: Fn(Node) -> Ret>(&self, handle: NodeHandle, f: F) -> Ret {
//             // look in `cur`
//             // if not found, look in `old` (maybe here we need to do an expensive SeqCst thing)
//             if let Some(ret) =
//                 unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.get_with(handle, &f)
//             {
//                 return ret;
//             }
//             if let Some(ret) =
//                 unsafe { self.old.load(Ordering::SeqCst).as_ref().unwrap() }.get_with(handle, &f)
//             {
//                 return ret;
//             }
//             unreachable!("probably not actually unreachable");
//             if let Some(ret) =
//                 unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.get_with(handle, &f)
//             {
//                 return ret;
//             }
//             unreachable!();
//         }
//         pub(super) fn get_dom(&self, handle: NodeHandle) -> Domain {
//             self.get_with(handle, |node| node.dom)
//         }
//         pub(super) fn get_leaf_distance_cache(&self, handle: NodeHandle) -> u32 {
//             self.get_with(handle, |node| node.leaf_distance_cache)
//         }
//         pub(super) fn get_color(&self, handle: NodeHandle) -> OptionColor {
//             self.get_with(handle, |node| node.color)
//         }
//         pub(super) fn get_left_child(&self, handle: NodeHandle) -> Option<NodeHandle> {
//             self.get_with(handle, |node| node.left_child)
//         }

//         /// returns the old value, or maybe just whatever the user wants to.
//         pub(super) fn update_with<Ret, F: Fn(&Atomic<Node>) -> Ret>(
//             &self,
//             handle: NodeHandleMut,
//             f: F,
//         ) -> Ret {
//             // look in `cur`
//             // if not found, it means another thread is waiting for us to be done
//             // we can't help out, bc what if they gave up after we exited `update` but before we released our mut handle
//             // so just look in `old`, and our caller should release this handle soon

//             if let Some(ret) =
//                 unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() }.update_with(handle, &f)
//             {
//                 return ret;
//             }
//             if let Some(ret) =
//                 unsafe { self.old.load(Ordering::SeqCst).as_ref().unwrap() }.update_with(handle, &f)
//             {
//                 return ret;
//             }
//             unreachable!();
//         }

//         pub(super) fn debug(&self) {
//             // let cur = unsafe { self.cur.load(Ordering::SeqCst).as_ref().unwrap() };
//             // dbg!(cur);
//             dbg!(self.can_alloc.load(Ordering::SeqCst));
//             dbg!(self.realloc_lock.load(Ordering::SeqCst));
//         }
//     }

//     impl AllocInner {
//         fn with_capacity(capacity: usize) -> Self {
//             assert!(capacity >= 4);

//             let mut mem = Vec::with_capacity(capacity);
//             mem.resize_with(capacity, || Atomic::new(Node::uninit()));
//             let mut flags = Vec::with_capacity(capacity);
//             flags.resize_with(capacity, || Atomic::new(BitFlag::NONE));
//             let mut debug_is_init = Vec::with_capacity(capacity);
//             debug_is_init.resize_with(capacity, || AtomicBool::new(false));

//             Self {
//                 len_real: AtomicUsize::new(3),
//                 len_speculative: AtomicUsize::new(3),
//                 mem: mem.into_boxed_slice(),
//                 flags: flags.into_boxed_slice(),
//                 debug_is_init: debug_is_init.into_boxed_slice(),
//             }
//         }

//         fn capacity(&self) -> usize {
//             let ret = self.mem.len();
//             debug_assert_eq!(ret, self.flags.len());
//             debug_assert_eq!(ret, self.debug_is_init.len());
//             ret
//         }

//         /// fails if we're out of memory.
//         fn try_alloc<const N: usize>(&self) -> Option<NodeHandle> {
//             assert!(N == 1 || N == 4);
//             let i = self.len_speculative.fetch_add(N, Ordering::SeqCst);
//             if i + N > self.mem.len() {
//                 return None;
//             }
//             let i = self.len_real.fetch_add(N, Ordering::SeqCst);
//             let handle = NodeHandle(NonZeroU32::new(i as u32).unwrap());
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

//         /// returns `None` if the element isn't up_to_date.
//         /// this can fail spuriously (in the future).
//         fn get_with<Ret, F: FnOnce(Node) -> Ret>(&self, handle: NodeHandle, f: F) -> Option<Ret> {
//             let i = handle.to_index();
//             // `Relaxed` implies this can fail spuriously
//             // TODO: relax these orderings
//             if self.flags[i].load(Ordering::SeqCst).up_to_date() {
//                 debug_assert!(self.debug_is_init[i].load(Ordering::SeqCst));
//                 Some(f(self.mem[i].load(Ordering::SeqCst)))
//             } else {
//                 None
//             }
//         }

//         /// returns `None` if the element isn't up_to_date.
//         fn update_with<Ret, F: FnOnce(&Atomic<Node>) -> Ret>(
//             &self,
//             handle: NodeHandleMut,
//             f: F,
//         ) -> Option<Ret> {
//             let i = handle.to_index();
//             // TODO: relax these orderings
//             let flag = self.flags[i].load(Ordering::SeqCst);
//             if flag.up_to_date() {
//                 debug_assert!(self.debug_is_init[i].load(Ordering::SeqCst));
//                 debug_assert!(flag.has_mut_handle());
//                 Some(f(&self.mem[i]))
//             } else {
//                 None
//             }
//         }
//     }
// }

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

        // pub(super) fn store(&self, handle: NodeHandle, node: Node, order: Ordering) {
        //     let block = handle.to_block();
        //     let block = unsafe { block.as_ref() };
        //     block.mem[handle.to_index()].store(node, order);
        // }

        // fn insert_leaf_uncolored<const N: usize>(&self, doms: [Domain; N]) -> NodeHandle {
        //     let handle0 = self.alloc::<4>();
        //     // if N == 1, the last three nodes will be uninit
        //     for (offset, dom) in doms.into_iter().enumerate() {
        //         let handle = handle0.siblings()[offset];
        //         self.update_with(handle, |node| {
        //             // node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
        //             //     Some(Node::new_leaf_uncolored(dom))
        //             // })
        //             // .unwrap()
        //             node.store(Node::new_leaf_uncolored(dom), Ordering::SeqCst);
        //         });
        //     }
        //     handle0
        // }
        // pub(super) fn insert_leaf_uncolored1(&self, dom: Domain) -> NodeHandle {
        //     self.insert_leaf_uncolored::<1>([dom])
        // }
        // pub(super) fn insert_leaf_uncolored4(&self, doms: [Domain; 4]) -> NodeHandle {
        //     self.insert_leaf_uncolored::<4>(doms)
        // }

        // fn get_with<Ret, F: Fn(Node) -> Ret>(&self, handle: NodeHandle, f: F) -> Ret {
        //     // we could get the node pointer directly,
        //     // but this is a bit safer.
        //     let block = handle.to_block();
        //     let block = unsafe { block.as_ref() };
        //     let node = block.mem[handle.to_index()].load(Ordering::Relaxed);
        //     f(node)
        // }
        // pub(super) fn get_node(&self, handle: NodeHandle) -> Node {
        //     self.get_with(handle, |node| node)
        // }
        // pub(super) fn get_dom(&self, handle: NodeHandle) -> Domain {
        //     self.get_with(handle, |node| node.dom)
        // }
        // pub(super) fn get_leaf_distance_cache(&self, handle: NodeHandle) -> u32 {
        //     self.get_with(handle, |node| node.leaf_distance_cache)
        // }
        // pub(super) fn get_color(&self, handle: NodeHandle) -> OptionColor {
        //     self.get_with(handle, |node| node.color)
        // }
        // pub(super) fn get_left_child(&self, handle: NodeHandle) -> Option<NodeHandle> {
        //     self.get_with(handle, |node| node.left_child)
        // }

        // /// returns the old value, or maybe just whatever the user wants to.
        // pub(super) fn update_with<Ret, F: Fn(&Atomic<Node>) -> Ret>(
        //     &self,
        //     handle: NodeHandle,
        //     f: F,
        // ) -> Ret {
        //     let block = handle.to_block();
        //     let block = unsafe { block.as_ref() };
        //     f(&block.mem[handle.to_index()])
        // }
    }
}

#[derive(Debug)]
pub(crate) struct Tree {
    dom: Domain,
    alloc: Alloc,
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
    pub(crate) fn new() -> Self {
        let dom = Domain::default();
        let color = OptionColor::new_some(metabrot_sample(dom.mid()).color());
        let alloc = Alloc::default();
        // let root = alloc.insert_leaf_uncolored1(dom);
        // alloc.update_with(root, |node| {
        //     node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
        //         Some(Node {
        //             color: OptionColor::new_some(metabrot_sample(node.dom.mid()).color()),
        //             ..node
        //         })
        //     })
        //     .unwrap();
        // });

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

    // /// must have that leaf_id is a leaf.
    // /// fails if the domain gets too small.
    // /// returns left child.
    // fn try_split(&self, leaf_handle: NodeHandle) -> Option<NodeHandle> {
    //     debug_assert!(
    //         self.alloc.get_color(leaf_handle).is_some(),
    //         "right now we only allow splitting colored leafs"
    //     );

    //     let doms = {
    //         let dom = self.alloc.get_dom(leaf_handle);
    //         debug_assert!(self.alloc.get_left_child(leaf_handle).is_none());
    //         dom.split()?
    //     };
    //     let child_handle = self.alloc.insert_leaf_uncolored4(doms);
    //     self.alloc.update_with(leaf_handle, |node| {
    //         let _ = node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
    //             assert!(node.left_child.is_none(), "child: {:?}", node.left_child);
    //             debug_assert_eq!(node.leaf_distance_cache, 0);
    //             Some(Node {
    //                 left_child: Some(child_handle),
    //                 leaf_distance_cache: 1,
    //                 ..node
    //             })
    //         });
    //     });
    //     Some(child_handle)
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
    pub(crate) fn insert(&self, handle: NodeHandle, color: Color32) {
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
        // let alloc = self.alloc.read().unwrap();
        // let node_id = self.alloc.promote(handle);
        let node = self.alloc.get(handle);
        debug_assert_eq!(node.color.load(Ordering::SeqCst), OptionColor::NONE);
        node.color
            .store(OptionColor::new_some(color), Ordering::SeqCst);

        // self.alloc.update_with(handle, |node| {
        //     node.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |node| {
        //         assert!(
        //             node.color.is_none(),
        //             "node_id: {:?}, node: {:?}",
        //             handle,
        //             node
        //         );
        //         Some(Node {
        //             color: OptionColor::new_some(color),
        //             ..node
        //         })
        //     })
        //     .unwrap();
        // });
        // self.alloc.demote(node_id);
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

        // let alloc = self.alloc.read().unwrap();
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
