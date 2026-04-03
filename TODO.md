# TODO

## unorganized

- organize `TODO.md`

## optimization

- try putting `align` on stuff
- review whether things should be `Copy`
- profile with cargo instruments
- fixed point arithmetic with less redundant checks

## sampling

- more precision so we can zoom farther
- find a new, smaller window, and repeat inside that one
- sample at low `WIDTH` for speed, then resample at higher resolution?

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
    - jitter each c
- split and sample and insert on a parallel datastructure, gc can be really slow, whatever
    - note that the deepest parent of all the active nodes for a given window is kinda deep, this is a pseudo root, maybe we can use this somehow
- when we split a node, instead of filling all the children with a sample/color, only fill the children that intersect the window. (the parent is guaranteed to intersect the window, but it's not guaranteed that all of its children do too)
- have the pixels live in a quadtree? where if a internal node has a `Some` color, it means all children have that color
    - a `PixelNode` gets a color if its fully contained inside a `FractalNode`
    - we can cache pixels across time if we're not panning lmao
- broadcast to all the threads that the texture was just submitted to be drawn and that they should all draw the pixels they're responsible for into the new texture, and after that they can go back to getting new samples
    - have two textures to swap?
- use that nearby samples are relevant to make parallelism harder/more interesting

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
