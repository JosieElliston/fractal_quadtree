use std::{ptr::NonNull, sync::atomic::AtomicUsize};

use crate::tree::Node;

/// footer rather than header because then indexing the node array
/// is an offset from the slab pointer,
/// rather than the slab pointer + header size.
#[derive(Debug)]
pub(super) struct SlabFooter {
    // TODO: consider having the head point to itself instead of `None` / null
    // prev: *mut Slab,
    pub(super) prev: Option<NonNull<Slab>>,
    /// how many nodes are allocated in this slab.
    /// note that this can be > capacity.
    pub(super) len: AtomicUsize,
}

// align requires a literal, otherwise i would use `Slab::SIZE`
#[repr(C, align(4096))]
#[derive(Debug)]
pub(super) struct Slab {
    /// not wrapped in `UnsafeCell` because we don't actually write to the nodes, only their fields.
    // TODO: maybe this should be `[MaybeUninit<Node>; CAPACITY]`
    // TODO: should i be using `Pin` somewhere?
    // TODO: we should be able to free slabs so we can use caching on mandelbrots.
    pub(super) mem: [Node; Self::CAPACITY],
    pub(super) foot: SlabFooter,
}
const _: () = assert!(size_of::<Slab>() == Slab::SIZE);
const _: () = assert!(align_of::<Slab>() == Slab::SIZE);
impl Slab {
    pub(super) const SIZE: usize = 4096;
    pub(super) const CAPACITY: usize = (Self::SIZE - size_of::<SlabFooter>()) / size_of::<Node>();

    pub(super) fn with_prev(prev: Option<NonNull<Slab>>) -> Self {
        Self {
            mem: std::array::from_fn(|_| Node::uninit()),
            foot: SlabFooter {
                prev,
                len: AtomicUsize::new(0),
            },
        }
    }
}
