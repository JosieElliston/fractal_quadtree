# presentation

## uncategorized

- computing each sample itself may seem embarrassingly parallel, but we might want to do things like blob detection for coloring, and we already have parallelism over sampling, so we don't have available threads to do parallelism inside sampling.
- note that the target hardware is "my m1 mac", not a cluster or smt, tho my solution should be fine for any common consumer hardware. note that i didn't end up using the gpu.
- maybe the presentation is building the quadtree and arena allocator and dynamic array from primitives
- epistemic status

## slides

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

## slides for datastructure

- note that i use a single pointer for the leftmost child
    - nodes are always allocated in groups of four
    - the root is annoying and has three uninitialized siblings
- the main loop that the worker does
- the main loop that the main thread
- allocation
    - append only linked list of 64x64 byte blocks, of which one cache line is reserved for metadata
    - storing the allocated but unpublished block in thread local storage
    - note that i never clone entire nodes, and nodes aren't atomic, only their fields.
- reclamation
    - notation
        - i use "reclamation" to refer to both the entire process
            of retire + later reclaim,
            as well as just the latter step.
        - whatever.
    - epoch reclamation: why?
        - i already had a clock for the color pruning.
        - tho i ended up using a disjoint clock for reclamation.
        - this allow for invisible readers.
        - in particular, the handle can just be the pointer.
    - clock
        - there's a central clock,
            threads read it and publish the value they last read,
            the central clock can only tick if it sees that everyone is fully up to date.
        - this maintains that threads can be at most one tick out of sync.
        - (attribution: this was the first thing i thought of and didn't look farther.)
        - in fact, we tick once per frame.
        - (ticking slower means that the buffers will grow larger,
        - not that we can only reclaim once per tick.)
    - epoch reclamation: how?
        - DRAW: ☐ > ☐☐☐☐
        - note that we're reclaiming the children, not the node itself.
        - erase the child pointer, push the children onto a queue with the timestamp.
        - after at least two ticks, reclaim the children.
    - why is one tick not enough?
        - suppose i'm retiring, and it's very late in the tick.
        - i select a node, clear its child pointer, and exit.
        - i find out the tick has happened, so i publish my ack and free the children.
        - but this allows for a use-after-free:
            there's nothing stopping a thread
            from having looked at the child pointer before i cleared it,
            push the children's handles onto a bfs queue,
            and then do a use-after-free once it gets around to them.
    - DRAW: timeline
        - ~~        X      |         |    X           ~~
        - us:     └─────c─┘ └─f─┘
        - them:     └─r───────────X─┘
    - DRAW: timeline
        - ~~        X |       |             X |             ~~
        - us:     └────────c─┘                 └─f─┘
        - them:        └─r─────────────────────────x─┘
    - why is two ticks enough? TODO: three ticks from the start or two ticks from the end? (two ticks from the end is tighter but weirder for my architecture)
        - first, the previous example doesn't disprove this,
            even if the other thread sees and acks the new tick before exploring.
        - we want to prove that between retiring and reclaiming,
            all threads have ever not been inside a subroutine.
        - acking a tick proves to the main thread that you aren't in a subroutine.
        - seeing a tick proves to you that every thread has acked the previous tick.
        - but some of those acks might have from before you retired the node,
            so you need to let an entire tick elapse
            during which no one can look through the child pointer.
    - but do threads ever smuggle handles to nodes across ticks?
    - DRAW: ☐ > ☐☐☐☐ > ☐☐☐☐
        - as i drew this, i implied that the children would be leafs,
            but there's no way to guarantee this, that's the whole problem.
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
- coloring pt 1: definition of color of pixel
- proved that the color hasn't changed (implies each nodes has a color_modified timestamp)
- prove a line hasn't changed
- rendering
    - like what the main thread does with the texture
    - note the inversion of end then begin
- refine (implies storing height)
