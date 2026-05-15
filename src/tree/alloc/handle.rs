use std::{fmt, num::NonZeroUsize, ptr::NonNull};

use crate::tree::{Node, Offset};

use super::Slab;

// const _: () = assert!(size_of::<Node>() == 64);
const _: () = assert!(align_of::<Node>() == 64);
/// bits 0..6: unused (for epoch stuff maybe?).
///
/// bits 6..12: index of node within the slab.
///
/// bits 12..: `Slab` pointer.
///
/// bits 6..: `Node` pointer (due to the high alignment of `Slab`).
// TODO: we can store [Node; 4] and get two more bits in the pointer
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
pub(crate) struct NodeHandle(pub(super) NonZeroUsize);
unsafe impl bytemuck::ZeroableInOption for NodeHandle {}
unsafe impl bytemuck::PodInOption for NodeHandle {}
impl NodeHandle {
    pub(crate) fn uninit() -> Self {
        Self(NonZeroUsize::new(0xABCDEFABCDEFABCD_u64 as usize).unwrap())
    }

    /// index is the index of the node in the slab
    pub(super) fn new(slab: NonNull<Slab>, index: usize) -> Self {
        debug_assert!(index < Slab::CAPACITY);
        let slab = slab.as_ptr() as usize;
        debug_assert_eq!(slab % Slab::SIZE, 0);
        let offset = index * size_of::<Node>();
        debug_assert_eq!(slab & offset, 0);
        let ret = slab + offset;
        debug_assert_eq!(
            ret % size_of::<Node>(),
            0,
            "ret: {:?}",
            NodeHandle(NonZeroUsize::new(ret).unwrap())
        );
        debug_assert_ne!(ret, 0);
        unsafe { NodeHandle(NonZeroUsize::new_unchecked(ret)) }
    }

    pub(super) fn to_slab(self) -> NonNull<Slab> {
        let slab = self.0.get() & !(Slab::SIZE - 1);
        debug_assert_eq!(slab % Slab::SIZE, 0);
        NonNull::new(slab as *mut Slab).unwrap()
    }

    pub(super) fn to_index(self) -> usize {
        let index = (self.0.get() % Slab::SIZE) / size_of::<Node>();
        debug_assert!(index < Slab::CAPACITY);
        index
    }

    pub(super) fn to_ptr(self) -> NonNull<Node> {
        let ptr = self.0.get() as *mut Node;
        debug_assert_ne!(ptr, std::ptr::null_mut());
        debug_assert_eq!(ptr as usize % size_of::<Node>(), 0);
        unsafe { NonNull::new_unchecked(ptr) }
    }

    /// because the root is leftmost in its block,
    /// this is actually fine to call on the root.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn to_block(self) -> BlockHandle {
        let slab = self.to_slab();
        let i = self.to_index();
        BlockHandle::try_from(NodeHandle::new(slab, i - (i % 4))).unwrap()
    }
}
impl fmt::Debug for NodeHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NodeHandle")
            .field(&format_args!("hex: {:x}", self.0.get()))
            .field(&format_args!(
                "slab: {:x}",
                self.to_slab().as_ptr() as usize
            ))
            .field(&format_args!("index: {}", self.to_index()))
            .finish()
    }
}
impl From<BlockHandle> for NodeHandle {
    fn from(value: BlockHandle) -> Self {
        value.0
    }
}

/// represents a block of four sibling nodes.
///
/// bc they're contiguous in memory,
/// we actually just store the handle to the leftmost sibling.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
pub(crate) struct BlockHandle(NodeHandle);
unsafe impl bytemuck::ZeroableInOption for BlockHandle {}
unsafe impl bytemuck::PodInOption for BlockHandle {}
impl BlockHandle {
    // const ARITY: usize = 4;

    pub(crate) fn uninit() -> Self {
        Self(NodeHandle::uninit())
    }

    /// equivalent to `self.siblings()[offset]`
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn offset(self, offset: Offset) -> NodeHandle {
        debug_assert!(offset < 4);
        #[cfg(debug_assertions)]
        let oracle = {
            let slab = self.0.to_slab();
            let i = self.0.to_index();
            debug_assert_eq!(i % 4, 0, "unaligned handle in siblings");
            NodeHandle::new(slab, i + offset)
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

    /// note that if you call this on the root, you'll get handles to uninitialized nodes.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn siblings(self) -> [NodeHandle; 4] {
        let slab = self.0.to_slab();
        let i = self.0.to_index();
        debug_assert_eq!(i % 4, 0, "unaligned handle in siblings");
        let oracle = [
            NodeHandle::new(slab, i),
            NodeHandle::new(slab, i + 1),
            NodeHandle::new(slab, i + 2),
            NodeHandle::new(slab, i + 3),
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
impl fmt::Debug for BlockHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NodeBlockHandle")
            .field(&format_args!("hex: {:x}", self.0.0.get()))
            .field(&format_args!(
                "slab: {:x}",
                self.0.to_slab().as_ptr() as usize
            ))
            .field(&format_args!("index: {}", self.0.to_index()))
            .finish()
    }
}
impl TryFrom<NodeHandle> for BlockHandle {
    type Error = &'static str;

    fn try_from(value: NodeHandle) -> Result<Self, Self::Error> {
        if value.to_index() % 4 != 0 {
            return Err("index is not a multiple of 4");
        }
        Ok(Self(value))
    }
}
