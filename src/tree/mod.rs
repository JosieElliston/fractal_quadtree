pub(crate) mod alloc;
mod domain;
mod node;
mod quadtree;
mod rgb;

pub(crate) use domain::Domain;
pub(crate) use quadtree::{
    DRAW_UNCOLORED_NODES, RETIRE_MAX_WIDTH, SPLIT_RETIRABLE_NODES, Tree, TreeLocal,
};

use node::Node;
use rgb::Rgb;

/// in 0..4.
/// used to index into a block of siblings,
/// and into a split `Domain`.
pub(crate) type Offset = usize;
