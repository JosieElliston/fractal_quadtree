# TODO

## unorganized

- organize `TODO.md`
- search for concurrent/parallel dynamic array/resizable ... instead of arena/linear/bump/heap allocator
- my epoch idea doesn't work bc we might be writing to the old array. actually we can only write after we've called a method on arena so maybe it's workable
- make arena align(16), ie cache_size / 4, bc there's four leaves?
- try to use SOA for cache reasons and partial mut borrowing
- can we have non exclusive mut NodeHandles you can specialize into at most one mut ColorHandle and (not or) multiple ColorHandles?
- so to free stuff, a thread collects all the mut handles. or we can permit multiple mut handles at once, just that the freeing thread needs to prevent issuing new mut handles.
- do raii for the handles?
- texture barrier should just be counters
- debugging: try starting with a large capacity to separate whether it's realloc that's deadlocking
- on successful get in cur, debug_assert that old flag ... is false
- during realloc, if we halt reads until the end, can we get reads to only ever look in cur
- when improving leaf_distance_cache, use try_promote not promote
- maybe have the api never give out mutable handles and make everything be atomic?
- debugging: maybe realloc is deadlocking because the reallocing thread owns a mut handle
- after we fail to get in cur, it might be that old has become null in the meantime, and that's the case in which we should recheck cur
- make leaf_distance_cache exact, find parents by searching dom.mid
- document that leaf_distance_cache is monotonically increasing and guaranteed to never overestimate (except maybe not guaranteed with `Relaxed`)
- between looking in the alloc inner pointer, the reallocing thread might free the buffer, then we read the buffer, and this is a read after free. tho this happens inside get, and isn't the users fault at all. maybe we can use a relaxed fetch add? and store data inside the pointer. maybe we can set the `mem: Box<[Atomic<Node>]>` pointer to null before deallocating it. actually i think get checks that it's up_to_date
- have each thread own a vec that only it can resize/alloc to
    - switch to child pointers?
    - use mmap
- `mem: Vec<Box<Block>>` for BLOCK_SIZE = my guess at the page size, with `Block([T; BLOCK_SIZE / sizeof::<T>()])` align(BLOCK_SIZE)?
- probably should just use child pointers
- allocator is linked list but with big blocks, handles wrap pointers
- bug report on `compare_exchange` returning `current` not `new`
- pausing sampling is broken, also when the fractal is outside the window

## optimization

- try putting `align` on stuff
- review whether things should be `Copy`
- profile with cargo instruments
- fixed point arithmetic with less redundant checks

## sampling

- more precision so we can zoom farther
- find a new, smaller window, and repeat inside that one
- sample at low `WIDTH` for speed, then resample at higher resolution?
- use associative floating point math
- for gradient descent, instead of using a small epsilon for computing the gradient, do something like epsilon = distance_estimate
- make sample stateful to reuse the allocation

## refinement/splitting

- refine samples that have children that disagree in color first
- cancel splitting a node if we pan away?
- note that we're refining 256/4 times with the same window, maybe we can use that
- refine in parallel

## pruning

- prune tree by double window sizes and doubling allowed domain radius

## rendering

- if a group of pixels are all inside a node, we can search them together?
- to interpolate between samples, make a delaunay triangulation on the samples, go to the dual voronoi diagram, then do some interpolation on the vertices of each cell. you could also maybe make a voronoi diagram with the samples as the seeds and do a different interpolation. the delaunay triangulation could include constraints based on parents/children/siblings, if that makes it faster.

## architecture

- fancy trace thing so i don't need to store the domain in each node
- alt quadtree architecture where you store samples at the corners of the domain, not the center, which allows for bilinear interpolation, at the cost of probably redundancy or complexity. like consider a sample on the edge of the domain, it on the corner of multiple leafs that aren't near cousins, and aren't guaranteed to exist in some order.
- refactor: make `ExactFixed` and `Domain`
    - `ExactFixed` guarantees that it's never been rounded, and has been constructed
        - you can only add, sub, mul2, and div2
    - `Domain` is a square but with `ExactFixed`
    - square will probably be unused
- to avoid aliasing artifacts, jitter the samples
    - jitter z0, store both z0 and color, if you get split, give it to the child which contains the sample, and internal nodes don't store samples, (different quadtree architecture)
        - seed the rng from the domain for determinism
    - jitter each c
- split and sample and insert on a parallel datastructure, gc can be really slow, whatever
    - note that the deepest parent of all the active nodes for a given window is kinda deep, this is a pseudo root, maybe we can use this somehow
- when we split a node, instead of filling all the children with a sample/color, only fill the children that intersect the window. (the parent is guaranteed to intersect the window, but it's not guaranteed that all of its children do too)
- have the pixels live in a quadtree? where if a internal node has a `Some` color, it means all children have that color
    - a `PixelNode` gets a color if its fully contained inside a `FractalNode`
    - we can cache pixels across time if we're not panning lmao
- broadcast to all the threads that the texture was just submitted to be drawn and that they should all draw the pixels they're responsible for into the new texture, and after that they can go back to getting new samples
    - have two textures to swap?
    - also tell them the new window
    - what if a thread has been preempted, so it can't render the pixels it owns? does this mean we can't have threads simply own pixels?
    - stagger the rendering threads so they aren't all using the bus at the same time. most should instead be doing alu heavy sampling
- use that nearby samples are relevant to make parallelism harder/more interesting
- parallel arena allocator
- parallel dynamic array
- single-consumer-single-producer (except actually i shouldn't need this for the final version)

## rust style

- update dependencies
- documentation (at least on structs)
- change `lerp` to `lerp_bounded` and `lerp_unbounded` or `lerp<const bounds_check: bool>` for whether do the bounds check
- typestate `Sample`
- rewrite `Node` to use typestate?
- refactor to allow for comparing different fractals
    - metabrots (need same camera)
    - mandelbrots (need different camera)
    - button for "mouse is now controlling camera 1 / 2"
- fixed point `DomainError`
- `Real`, `Imag`, `egui::f32` type safety?
- have a lifetime on `rect` and maybe `camera` in `CameraMap` for better semantics
    - maybe put pan/zoom into `CameraMap`
- use type alias Complex for (Real, Imag)?
- type for `Pixel`
- change children from `[Box<Node>; 4]` to `Box<[Node; 4]>`
- go over what things are `Copy`
- join the threads in the pool at the end so they don't crash
- use `Weak` in worker threads
- fancy [`link`] comments
- try to weaken `sync::atomic::Ordering`
- make sure i'm not using `lock`, i should probably only every be using `try_lock`
- rename node/leaf_id -> node_handle / handle
- rename child_id -> left_child / child_handle

## bugs

- fix failing to draw pixel at high zoom
- fix texture size mismatch crash on zoom in
- fix crash on zoom out

## debugging

- draw debug dots on the centers of each quadtree domain
    - camera_map.fixed_to_vec1(fixed)
    - camera_map.complex_to_vec2((real, imag))
    - don't draw if camera_map.fixed_to_vec1(node.dom().rad()) < 4 * pixel_size

## UX

- does `egui::Frame` have an ugly border?
- gui for keyboard controls for discoverability

## presentation

- reread comments on my project, i think i deleted my notes
- make a writeup, send to iosevich
- 3cycle presentation
- present minimal alloc, then motivate additional features by the problem, then show implementing each feature
