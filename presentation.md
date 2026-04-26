# presentation

## uncategorized

- computing each sample itself may seem embarrassingly parallel, but we might want to do things like blob detection for coloring, and we already have parallelism over sampling, so we don't have available threads to do parallelism inside sampling.
- note that the target hardware is "my m1 mac", not a cluster or smt, tho my solution should be fine for any common consumer hardware. note that i didn't end up using the gpu.
- maybe the presentation is building the quadtree and arena allocator and dynamic array from primitives
- epistemic status
- why i use AOS and not SOA (each struct is one cache line, SOA would get false sharing)

## speaker notes for datastructure

- TODO: section on why do i have min_height and max_height
- TODO: more DRAW commands

- intro
    - show off program if it's easy.
    - we get samples from a function,
    - but these samples are expensive,
    - so we cache them,
    - but we still want to pan/zoom, so we can't just have a pixel array,
    - so we use a quadtree.
- operations
    - color_of_pixel: screen space pixel center -> color
    - refine/split: select a node to split, split it, and return its children (who we need to insert colors into)
    - insert_sample: insert a sample into the previously reserved node
    - stuff for memory reclamation (because if you zoom in really far then leave, those samples would otherwise stay)
- **DRAW**: struct Node (persist over presentation):
    - `dom: UnsafeCell<Domain>,` // note that this is kinda unnecessary and is implicit in the structure of the tree, but it's really nice to have
    - `parent: UnsafeCell<Option<NodeHandle>>,`
    - `left_child: Atomic<Option<NodeHandle4>>,` // nodes are always allocated in groups of four
    - `color: Atomic<Option<Rgb>>,` // color is sampled at the center of the node
    - `timestamp: Atomic<RenderMoment>,`
    - `min_height: AtomicU16,`
    - `max_height: AtomicU16,`
    - `_pad: [u8; 8],`
- workers have a main loop, where they go though operations in a static order.
    - this order is chosen to not deadlock/livelock.
    - stuff like
        - we don't get new nodes to sample before the queue is emptied.
        - rendering is highest priority, so the main thread can progress.
- coloring
    - main thread rendering
        - resize the texture if needed
        - clears begin_count and finish_count
        - does other stuff
        - blocks until finish_count matches the texture height
        - give the texture to egui
    - worker threads rendering
        - check if begin_lock is less than height, do a fetch_and_add, check again that its less than height
        - render a line to the texture
        - increment the finish_count
    - **DRAW**
        - 1d ~quadtree, binary tree but nodes as their doms which are horizontal lines
        - vertical line representing the path of the pixel, with the closest center not being a leaf
    - def color of a pixel (bad):
        - get the center of the pixel.
        - find the leaf the center is in.
        - return the leafs color.
    - this is bad because it doesn't use the colors of internal nodes.
    - def color of a pixel:
        - get the center of the pixel, nothing else about the pixel matters.
        - follow the path down to the leaf the center is in.
        - take the color of the node whose center is closest to the pixel center.
    - we can optimize this.
        - **DRAW**: nodes with new timestamps? X on the centers if they're new?
        - if we're not moving the camera, very little of the image is changing; it would be nice to reuse most of the texture across frames.
        - the color of a pixel needs an update only when you insert a sample (or reclaim a node, which just worsens the image, so i don't do this).
        - we introduce a global render clock.
        - the main thread updates the global timestamp every frame, and worker threads fetch it at the start of operations.
        - so we have nodes store a timestamp of when it or any of its descendants had their color changed.
        - (so eg the root will probably have a timestamp of almost exactly now, because any update wil update the root)
        - when going down the path to the leaf, if we ever encounter a timestamp that's old enough, we exit early and don't update the texture.
        - note that (contrary to how i've drawn it) we can't have the timestamp updates for inserting a sample happen instantly, so we  sometimes return early even though the node has been colored. this just means the texture will be slightly stale, which is fine.
    - we can optimize this.
        - we're still doing a lot of work per pixel.
        - it would be nice to prove a region of pixels haven't changed.
        - we explore nodes that intersect the region,
            - if a node is definitely good, we don't explore its children.
            - for a node to be definitely good, it simply needs a old timestamp.
            - if a node is definitely bad, we fail.
            - for a node to be definitely bad, it must be a leaf.
        - (i do this for regions that are lines because i render line by line)
- allocation
    - **DRAW**
        - struct Alloc { head: Atomic<*Block> (: 1 word) }
        <!-- - struct Block { mem: [Node; 63], prev: Atomic<*Block>, len: usize  } -->
        - struct Block { mem: [Node; 63], len: usize } (don't draw prev)
        - draw these as boxes with members (as opposed to member list (which i might never use))
        - draw two `Block`s, draw `Alloc` underneath.
    - 64x64 byte blocks, of which one cache line is reserved for metadata.
    - note that i don't every free `Block`s, or even have a shared free-list for reclaimed nodes.
    - note that nodes aren't atomic, only their fields.
    - the handle i give out is just a pointer, touches are just looking though the pointer to the fields.
    <!-- - i made a concurrent linked list, but i don't use it.
        - just storing a (shared) pointer to the current block we're allocating in is sufficient.
        - for reclamation, i put the reclaimed node into a thread-local free-list.
        - my idea for a global free-list via bitset -->
    - fn alloc
        - head_ptr := alloc.head.load()
        - i := head_ptr.len.fetch_add(1)
        - if i < CAPACITY: return &head_ptr.mem[i]
        - else we need to append a block
        - **DRAW**: doing this
        - new_block_ptr := global_alloc(block) // also check the thread-local cache
        - alloc.head.cas(head_ptr, new_block_ptr)
        - if the cas failed, put new_block_ptr into thread-local cache
        - but in both cases, *someone* appended a block, so we go to the top of alloc and retry.
    - mem ordering: i think that they can all be relaxed, but i'm not sure
    - why does len need to be stored in each block and not just in alloc?
        - (the len going out of bounds makes me uncomfortable)
        - in the case where we realloc, i := alloc.len.load() is too large so we need a new block.
        - so we do the realloc, then do head_ptr := alloc.head.load(), and return `&head_ptr.mem[i % BLOCK_CAP]`.
        - but this block isn't guaranteed to be immediate next block.
        - we could have gone to sleep, and many blocks could have been appended, and now we have two logically different pointers that are actually the same.
- reclamation
    - notation
        - i use "reclamation" to refer to both the entire process
            of retire + later reclaim,
            as well as just the latter step.
        - whatever.
    - i have that handles don't live across subroutine calls, except for ones in the nursing_home (and the root)
    - epoch reclamation: why?
        - i already had a clock for the color pruning.
        - tho i ended up using a disjoint clock for reclamation.
        - this allow for invisible readers.
        - in particular, the handle can just be the pointer.
    - clock
        - **DRAW**: main clock above, array of four thread clocks below, tally marks inside
        - there's a central clock,
            threads read it and publish the value they last read,
            the central clock can only tick if it sees that everyone is fully up to date.
        - this maintains that threads can be at most one tick out of sync.
        - (attribution: this was the first thing i thought of and didn't look farther.)
        - in fact, we tick once per frame.
        - (ticking slower means that the buffers will grow larger,
        - not that we can only reclaim once per tick.)
    - epoch reclamation: how?
        - **DRAW**: nodes ☐ > ☐☐☐☐
        - note that we're reclaiming the children, not the node itself.
        - erase the child pointer, push the children/siblings onto a queue with the timestamp.
        - after a few ticks, reclaim the siblings.
    - fn retire
        - select a node, which should be internal
        - do an atomic get-and-set on the child pointer to clear it
        - if the child pointer was `None`, someone else got there first, whatever
        - if it wasn't `None`, we put the siblings into the thread-local nursing home.
        - (we change to calling them siblings at this point)
        - we are now responsible for reclaiming the siblings after some grace period.
    - how long a grace period? we need to wait two ticks from the end (or three ticks from the start).
    - lower bound on grace period
        - **DRAW**: timeline
            - ~~         X |       |             X                          ~~
            - free:    └────────c─┘                 └─f─┘
            - touch:        └─r─────────────────────────x─┘
        - suppose i'm retiring, and it's very late in the tick.
        - i select a node, clear its child pointer, and exit.
        - i find out the tick has happened, so i publish my ack and free the siblings.
        - but this allows for a use-after-free:
            there's nothing stopping a thread
            from having looked at the child pointer before i cleared it,
            push the children's handles onto a bfs queue,
            and then do a use-after-free once it gets around to them.
    - upper bound proof
        - first, the previous example doesn't disprove this,
            even if the other thread sees and acks the new tick before exploring.
        <!-- - we want to prove that between retiring and reclaiming,
            all threads have ever not been inside a subroutine
            (because handles don't persist across subroutine calls). -->
        - what do we know?
            - acking a tick proves to the main thread that you aren't in a subroutine.
            - seeing a tick proves to you that every thread has acked the previous tick.
        - what we we want?
            - we need an entire tick to elapse during which no one can look through the child pointer (bc it's None).
        - the tick after exiting retire is the start of this period, and the next tick ends it.
    - ok so we've waited the grace period, can we now reclaim the nodes?
        - are we sure no one has handles to them?
            - only reclamation stores handles across acks
            - we get our handles from an atomic get-and-set, so we're confident that no one has pointers to the siblings
        - but what about the siblings' children?
            - my picture is misleading, we can't guarantee that the picture look like ☐ > ☐☐☐☐
                - like we could try to select a node with height one, but we can't guarantee that it remains height one, that's like the whole problem
            - **DRAW**: ☐ > ☐☐☐☐ > ☐☐☐☐
            - obviously we shouldn't leak them.
            - we can retire them, put them in the nursing home, and wait the grace period.
            - but do we actually need to wait or can we reclaim them now?
            - it turns out that we can!
          - can we reclaim them now?

    - but do threads ever smuggle handles to nodes across ticks?
        - yes, but only for reclamation.
        - we don't ever do a double free because we take the child pointer atomically.
    - but what about the children of the siblings?
        - we can't guarantee that the picture look like ☐ > ☐☐☐☐ (that's like the whole problem of non-blocking algorithms)
        - **DRAW**: ☐ > ☐☐☐☐ > ☐☐☐☐
        - we recurse, atomically retiring each sibling and sending their children to the thread's nursing home
        - TODO: why can't we free them?
        - and now we can reclaim the siblings, and say that they're deinitialized and reading them is UB
    - problem
        - or rather, no one can find them via following child pointers
        - but you can find them via direct pointer.
        - and who stores pointers across ticks?
        - us! or rather, other retirement buffers.
        - but actually, these nodes are only in our buffer,
        - which guarantees no other pointers to them exist,
        - it's only their children that might be in other buffers.
        - so this just means we need to try to retire each node
        - and their into our buffer to be reclaimed in another two ticks.
    - problem with
    - one problem
        - suppose we've selected a node whose children we want to retire
        - we erase the child pointer
        - we have no way to guarantee that no one else is looking at one of the children
    - i have ideas about how to put them back into the global free-list (have each block store a bitset of free nodes), but right now they're put into a thread-local free-list for reuse before you request an allocation from the global free-list.
- coloring pt 1: definition of color of pixel
- proved that the color hasn't changed (implies each nodes has a color_modified timestamp)
- prove a line hasn't changed
- rendering
    - like what the main thread does with the texture
    - note the inversion of end then begin
- refine (implies storing height)

## slides for sampling

- slide: intro
    - the premiss is that i came up with a particular fractal to try to render, selected for being expensive, more than being mathematically interesting. (i enjoy performance engineering)
    - bc i have time, i'll define what it in some detail, tho this detail isn't very relevant to this class.
- slide: lecture structure
    - i'll give a brief overview of sampling,
    - then i'll talk about parallelism,
    - then i'll pad with more sampling
    - (sampling (TODO: right now at least) is really the hard part of this project)
- slide: the quadratic map
    - we're interested in iterating the function z^2 + c
    - iterating meaning z\_{n+1} = z_n^2 + c
    - if we instead iterated c, we'll get something much less interesting
    - embed gif of varying c with lines between iterations (desmos)
- slide: mandelbrot set
    - note how some initial values for c stay bounded, while others diverge
    - a c value is inside the mandelbrot set if it stays bounded, and color it black
    - and otherwise color the c value based on how quickly it diverges
    - embed gif of varying c with lines between iterations (desmos) on top of the mandelbrot set
        - note that screen space is the c plane
- slide: z_0 != 0 - you'll notice i never specified a base case for the function we're iterating - the previous slide used z_0 = 0, which is natural choice - but we can in fact pick other values - there are various things you might observe about these other mandelbrots, but what we care about is that some values of z_0 have area, while other don't - embed gif of the mandelbrot set with varying z_0 (desmos)
<!-- - slide: pt 2
    - desmos was bad, so i made my own renderer
    - but this was trivial and not that interesting
    - what i found interesting was to try render these quickly
    - but this turned out to be easy, this is nearly the canonical into to GPU project, and it's slow on the CPU either (TODO: find actual fps)
    - embed gif of the mandelbrot set with varying z_0 (fractal_explorer) -->
- slide: metabrot
    - well, lets draw this
    - instead of screen space being the c plane, screen space will be the z_0 plane
    - for each pixel (ie z_0 value), we'll color it black if the corresponding mandelbrot has area, otherwise we'll color it based on the maximum depth any point achieved before escaping
    - note you do a meta-julia set, you just get the mandelbrot set
    - this is incredibly slow, i can get a decent image in about a minute
    - but it would be nice to be able to pan/zoom in real time
    - the rest of this talk is on various speedups / the current architecture
