# TODO

- refine samples that have children that disagree in color first
- prune tree by double window sizes and doubling allowed domain radius
- garbage collect nodes
- fancy trace thing so i don't need to store the domain in each node
- compute samples in parallel more
- documentation (at least for structs)
- update dependencies
- fix failing to draw pixel at high zoom
- fix texture size mismatch crash on zoom in
- fix crash on zoom out
- if a group of pixels are all inside a node, we can search them together?
- have the pixels live in a quadtree? where if a internal node has a `Some` color, it means all children have that color
    - a `PixelNode` gets a color if its fully contained inside a `FractalNode`
    - we can cache pixels across time if we're not panning lmao
- rewrite `Node` to use typestate?
- Real, Imag, egui::f32 type safety?
- cancel splitting a node if we pan away
