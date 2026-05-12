# README

drawing the metabrot set, caching samples in a quadtree. [some](https://github.com/JosieElliston/fractal_egui) [previous](https://github.com/JosieElliston/fractal_explorer) iterations. [video](https://youtu.be/wz_QIwfb6oA) of looking around. you may find the [report](report.md) enlightening.

## controls

- pan/zoom with mouse
- various controls in info menu, descriptions and keybinds shown on hover

<!-- ## implementation

### sampling

### caching/quadtree

#### simplest

```rs
struct Node {
    color: Option<Color>,
    children: Option<[Box<Node>; 4]>,
}

struct Tree {
    root: Node,
}

impl Tree {
    fn refine(&mut self) -> Complex { ... }
    fn insert(&mut self, z0: Complex, color: Color) { ... }
    fn color(&self, z0: Complex) -> Color { ... }
}
```

#### pruning

refine is O(n) because we need to find the shallowest leaf.
if we store the distance to nearest/shallowest descendant leaf at each node,


#### concurrency

#### reclamation / block allocator -->
