mod allocator;
mod handle;
mod slab;

pub(crate) use allocator::AllocLocal;
pub(crate) use handle::{BlockHandle, NodeHandle};

pub(super) use allocator::Alloc;

use slab::Slab;
