use std::{
    ptr::NonNull,
    sync::atomic::{AtomicPtr, Ordering},
};

use crate::{
    log,
    pool::RenderMoment,
    tree::{Domain, Node, Rgb},
};

use super::{BlockHandle, NodeHandle, Slab};

/// per-thread data for the allocator.
///
/// dropping this will probably leak memory,
/// and perhaps deadlock (in the future).
#[derive(Debug, Default)]
pub(crate) struct AllocLocal {
    /// nodes we have freed.
    /// we look in here before going to the global allocator.
    /// the nodes should be uninit.
    free_list: Vec<BlockHandle>,
    /// the slab we allocated in `realloc` but lost the race to swap in.
    slab: Option<NonNull<Slab>>,
}
impl Drop for AllocLocal {
    fn drop(&mut self) {
        if !self.free_list.is_empty() {
            log!("dropping `AllocLocal` with non-empty free list");
        }
        if let Some(slab) = self.slab.take() {
            unsafe {
                drop(Box::from_raw(slab.as_ptr()));
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct Alloc {
    head: AtomicPtr<Slab>,
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
            head: AtomicPtr::new(Box::into_raw(Box::new(Slab::with_prev(None)))),
            // local_cache: vec![std::ptr::null_mut(); thread_count].into_boxed_slice(),
        }
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    fn realloc(&self, alloc_local: &mut AllocLocal, old_last: NonNull<Slab>) {
        let new_slab = match alloc_local.slab.take() {
            Some(slab) => slab.as_ptr(),
            None => Box::into_raw(Box::new(Slab::with_prev(Some(old_last)))),
        };
        match self.head.compare_exchange_weak(
            old_last.as_ptr(),
            new_slab,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(prev) => {
                assert_eq!(prev, old_last.as_ptr());
                // successfully swapped in the new slab, so we can just use it
                // self.local_cache[thread_i] = std::ptr::null_mut();
            }
            Err(_actual_cur) => {
                // another thread already swapped in a new slab,
                // or we had a spurious failure.
                // reuse the slab we allocated for next time.
                alloc_local.slab = Some(NonNull::new(new_slab).unwrap());
            }
        }
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn alloc(&self, alloc_local: &mut AllocLocal) -> BlockHandle {
        // look in the free list before going to the shared allocator.
        if let Some(block_handle) = alloc_local.free_list.pop() {
            return block_handle;
        }

        // loop bc it's possible that after reallocating
        // or during waiting for another thread to reallocate,
        // we go to sleep and the new slab fills up.
        loop {
            let last = unsafe { self.head.load(Ordering::SeqCst).as_ref().unwrap() };
            let i = last.foot.len.fetch_add(4, Ordering::SeqCst);
            if i + 4 <= Slab::CAPACITY {
                return NodeHandle::new(last.into(), i)
                    .try_into()
                    .expect("we just made sure it's aligned");
            }
            // we could say that whoever got len == Slab::CAPACITY is responsible for reallocating,
            // but what if that thread went to sleep during reallocation?
            // so just have all the threads realloc
            self.realloc(alloc_local, last.into());
        }
    }

    /// puts the siblings block in the free list.
    /// also deinits the fields for debugging.
    ///
    /// SAFETY: the caller must ensure that the siblings are never read after this.
    // TODO: should this be in alloc?
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) unsafe fn free(&self, alloc_local: &mut AllocLocal, block_handle: BlockHandle) {
        for sibling_handle in block_handle.siblings() {
            // TODO: have a feature flag for whether to deinit nodes
            let sibling = self.get(sibling_handle);

            #[cfg(debug_assertions)]
            sibling.assert_not_any_uninit();

            unsafe {
                sibling.write_dom(Domain::uninit());
            }
            sibling
                .children_handle
                .store(Some(BlockHandle::uninit()), Ordering::Relaxed);
            sibling.color.store(Some(Rgb::uninit()), Ordering::Relaxed);
            sibling
                .min_height
                .store(Node::UNINIT_HEIGHT, Ordering::Relaxed);
            sibling
                .max_height
                .store(Node::UNINIT_HEIGHT, Ordering::Relaxed);
            sibling
                .timestamp
                .store(RenderMoment::uninit(), Ordering::Relaxed);

            #[cfg(debug_assertions)]
            sibling.assert_all_uninit();
        }

        alloc_local.free_list.push(block_handle);
    }

    /// gets the partially initialized node.
    /// (debug assert that the handle is init.)
    #[track_caller]
    #[cfg_attr(feature = "profiling", inline(never))]
    fn get_partial_init(&self, node_handle: NodeHandle) -> &Node {
        debug_assert_ne!(
            node_handle,
            NodeHandle::uninit(),
            "uninit node_handle in Alloc::get"
        );

        let ret = unsafe { node_handle.to_ptr().as_ref() };

        #[cfg(debug_assertions)]
        {
            let slab = node_handle.to_slab();
            let slab = unsafe { slab.as_ref() };

            debug_assert_eq!(
                ret as *const Node,
                &slab.mem[node_handle.to_index()] as *const Node
            );
        }

        ret
    }

    /// gets the initialized node.
    #[doc(alias = "get_init")]
    #[track_caller]
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn get(&self, node_handle: NodeHandle) -> &Node {
        let ret = self.get_partial_init(node_handle);

        #[cfg(debug_assertions)]
        ret.assert_not_any_uninit();

        ret
    }

    /// gets the uninitialized node.
    #[track_caller]
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn get_uninit(&self, node_handle: NodeHandle) -> &Node {
        let ret = self.get_partial_init(node_handle);

        #[cfg(debug_assertions)]
        ret.assert_all_uninit();

        ret
    }
}
