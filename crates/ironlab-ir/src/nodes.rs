//! The nodes of a figure: the single declaration of the kinds of node, and the walk of the
//! tree that they form.
//!
//! A figure is a tree. Its root is the figure, which holds one list of each top-level
//! kind of node (its data arrays, its colour scales, its axes and its panels), and an axes
//! holds the list of its artists. Every kind of node is declared exactly once, in the
//! `figure_nodes!` invocation below. From that declaration the macro generates:
//!
//! - [`Node`] and [`NodeKind`], with [`NodeKind::ALL`];
//! - the lookup of a node of each top-level kind by identifier (such as
//!   [`Figure::data_array`]);
//! - the dispatch to the settable properties of each kind;
//! - the methods of [`Artist`] that read or write a field that every kind of artist has
//!   ([`Artist::id`], [`Artist::label`], [`Artist::visible`] and [`Artist::set_visible`]);
//! - the encoding and decoding of [`Node`] and of [`Artist`] as wire messages, which
//!   set and read the variant of the wire oneof that each kind is encoded as;
//! - the walk of validation, which calls the check of every node of a figure with its
//!   parent, and the dispatch to the usage of each kind of artist, through which every
//!   artist is validated;
//! - the glue through which the code below sees every node as a [`TreeNode`] and every
//!   list of nodes as a [`NodeList`].
//!
//! Everything that walks the tree is written once, over those two traits: finding a node
//! by identifier, reading and writing its properties, listing the identifiers of the
//! figure, inserting, removing and moving nodes, and following the shape of the tree
//! through a transaction. A kind of node added to the declaration is therefore reached by
//! all of it at once. Besides its line in the declaration, it needs only its own
//! substance: its type, its `ir_node!` declaration of properties, its wire message with
//! the [`Encode`] and [`Decode`] implementations that convert it, and its validation (an
//! implementation of [`Check`] for a top-level kind, or of [`Usage`] for a kind of
//! artist). A kind that lacks any of them does not compile.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artist::{
    Artist, Bar, Cells, Contour, Image, Quiver, ScalarImage, Series, Stairs, Surface,
};
use crate::axes::Axes;
use crate::color_scale::ColorScale;
use crate::data::DataArray;
use crate::edit::walk::{Step, WalkNode};
use crate::edit::{Edit, Property, Value};
use crate::error::ProtobufError;
use crate::figure::Figure;
use crate::ids::NodeId;
use crate::panel::Panel;
use crate::text::Text;
use crate::validate::{ArtistUsage, Check, Usage, Validator};
use crate::wire::{self as w, Decode, Encode};

/// A node of the figure tree, seen without its type.
pub(crate) trait TreeNode {
    /// Returns the identifier of the node.
    fn id(&self) -> NodeId;

    /// Returns the kind of the node.
    fn kind(&self) -> NodeKind;

    /// Returns the property at the path within the node.
    fn get(&self, path: &[String]) -> Result<Value, Step>;

    /// Replaces the property at the path within the node.
    fn set(&mut self, path: &[String], value: Value) -> Result<(), Step>;

    /// Returns the lists of the children of the node, in tree order; a node without
    /// children has none.
    fn lists(&self) -> Vec<&dyn NodeList>;

    /// Returns the lists of the children of the node for modification, in the order of
    /// [`TreeNode::lists`].
    fn lists_mut(&mut self) -> Vec<&mut dyn NodeList>;
}

/// A list of the children of a node, whose members are all of the kinds that the list
/// holds.
pub(crate) trait NodeList {
    /// Returns the number of nodes in the list.
    fn count(&self) -> usize;

    /// Returns the node at a position of the list.
    fn at(&self, position: usize) -> &dyn TreeNode;

    /// Returns the node at a position of the list, for modification.
    fn at_mut(&mut self, position: usize) -> &mut dyn TreeNode;

    /// Returns whether the list holds nodes of a kind.
    fn holds(&self, kind: NodeKind) -> bool;

    /// Inserts a node, of a kind that the list holds, at a position from zero to the
    /// length of the list.
    fn insert(&mut self, position: usize, node: Node);

    /// Removes the node at a position of the list, and returns it.
    fn remove(&mut self, position: usize) -> Node;
}

/// A node that a list of the tree holds by value, and that an edit carries as a [`Node`].
pub(crate) trait Listed: TreeNode + Sized {
    /// Returns whether a list of this type holds nodes of a kind.
    fn holds(kind: NodeKind) -> bool;

    /// Returns the node as an edit carries it.
    fn into_node(self) -> Node;

    /// Returns the node that an edit carries, or `None` when it is of another type.
    fn from_node(node: Node) -> Option<Self>;
}

impl<T: Listed> NodeList for Vec<T> {
    fn count(&self) -> usize {
        self.len()
    }

    fn at(&self, position: usize) -> &dyn TreeNode {
        &self[position]
    }

    fn at_mut(&mut self, position: usize) -> &mut dyn TreeNode {
        &mut self[position]
    }

    fn holds(&self, kind: NodeKind) -> bool {
        T::holds(kind)
    }

    fn insert(&mut self, position: usize, node: Node) {
        let node = T::from_node(node).expect("the list holds the kind of the node");
        Vec::insert(self, position, node);
    }

    fn remove(&mut self, position: usize) -> Node {
        Vec::remove(self, position).into_node()
    }
}

/// Carries a node of a top-level kind in a [`Node`], boxed when the kind is declared
/// `boxed`.
macro_rules! node_payload {
    ($kind:ident) => {
        $kind
    };
    ($kind:ident boxed) => {
        Box<$kind>
    };
}

/// Wraps a node of a top-level kind as a [`Node`] carries it.
macro_rules! node_wrap {
    ($node:expr) => {
        $node
    };
    ($node:expr, boxed) => {
        Box::new($node)
    };
}

/// Unwraps a node of a top-level kind from a [`Node`].
macro_rules! node_unwrap {
    ($node:expr) => {
        $node
    };
    ($node:expr, boxed) => {
        *$node
    };
}

/// Borrows a node of a top-level kind from the payload of a [`Node`].
macro_rules! node_ref {
    ($node:expr) => {
        $node
    };
    ($node:expr, boxed) => {
        &**$node
    };
}

/// Declares every kind of node of a figure.
///
/// The first block lists the top-level kinds, each held by the figure in a list of its
/// own: the kind (whose type has the same name and an `id` field), whether an edit
/// carries it boxed, the field of the figure that holds the list, the names of the
/// lookups by identifier, the noun that their documentation uses, the field of the wire
/// oneof of [`w::Node`] that the kind is encoded as, and, for a kind whose nodes have
/// children, the field that holds them. The second block lists the kinds of artist, each
/// the variant of [`Artist`] of the same name, whose type has the same name and the
/// fields `id`, `label` and `visible`, with the field of the wire oneof of [`w::Artist`]
/// that the kind is encoded as. The variant of a wire oneof that holds a kind has the
/// name of the kind.
macro_rules! figure_nodes {
    (@one $kind:ident) => {
        1
    };

    (
        figure {
            $(
                $(#[$doc:meta])*
                $kind:ident $(($boxed:ident))? in $field:ident,
                    found by $get:ident and $get_mut:ident as $noun:literal,
                    encoded as $wire:ident
                    $(, parent of $children:ident)?;
            )*
        }
        artists {
            $(
                $(#[$artist_doc:meta])*
                $artist:ident encoded as $artist_wire:ident,
            )*
        }
    ) => {
        /// A node that can be inserted into a figure, with its subtree.
        ///
        /// In JSON a node is an object with one property, named after its kind in
        /// `snake_case`, that holds the node, such as `{"panel": {"id": 6, ...}}`.
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum Node {
            $( $(#[$doc])* $kind(node_payload!($kind $($boxed)?)), )*
            /// An artist, which an axes holds.
            Artist(Artist),
        }

        /// The kind of a node, which determines the properties it has.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum NodeKind {
            /// The figure, the root node.
            Figure,
            $( $(#[$doc])* $kind, )*
            $( $(#[$artist_doc])* $artist, )*
        }

        impl NodeKind {
            /// Every kind of node, in declaration order.
            pub const ALL: [NodeKind; 1 $(+ figure_nodes!(@one $kind))* $(+ figure_nodes!(@one $artist))*] = [
                NodeKind::Figure,
                $( NodeKind::$kind, )*
                $( NodeKind::$artist, )*
            ];
        }

        impl Node {
            /// Returns the node as the walk of the tree sees it.
            pub(crate) fn tree(&self) -> &dyn TreeNode {
                match self {
                    $( Node::$kind(node) => node_ref!(node $(, $boxed)?), )*
                    Node::Artist(artist) => artist,
                }
            }
        }

        /// Appends every settable property of a kind of node to `out`, in declaration
        /// order.
        pub(crate) fn describe(kind: NodeKind, out: &mut Vec<Property>) {
            match kind {
                NodeKind::Figure => Figure::node_describe(out),
                $( NodeKind::$kind => $kind::node_describe(out), )*
                $( NodeKind::$artist => $artist::node_describe(out), )*
            }
        }

        impl TreeNode for Figure {
            fn id(&self) -> NodeId {
                self.id
            }

            fn kind(&self) -> NodeKind {
                NodeKind::Figure
            }

            fn get(&self, path: &[String]) -> Result<Value, Step> {
                self.node_get(path)
            }

            fn set(&mut self, path: &[String], value: Value) -> Result<(), Step> {
                self.node_set(path, value)
            }

            fn lists(&self) -> Vec<&dyn NodeList> {
                vec![$( &self.$field as &dyn NodeList ),*]
            }

            fn lists_mut(&mut self) -> Vec<&mut dyn NodeList> {
                vec![$( &mut self.$field as &mut dyn NodeList ),*]
            }
        }

        impl Figure {
            $(
                #[doc = concat!("Returns the ", $noun, " with the given identifier.")]
                pub fn $get(&self, id: NodeId) -> Option<&$kind> {
                    self.$field.iter().find(|node| node.id == id)
                }

                #[doc = concat!("Returns the ", $noun, " with the given identifier, mutably.")]
                pub fn $get_mut(&mut self, id: NodeId) -> Option<&mut $kind> {
                    self.$field.iter_mut().find(|node| node.id == id)
                }
            )*
        }

        $(
            impl TreeNode for $kind {
                fn id(&self) -> NodeId {
                    self.id
                }

                fn kind(&self) -> NodeKind {
                    NodeKind::$kind
                }

                fn get(&self, path: &[String]) -> Result<Value, Step> {
                    self.node_get(path)
                }

                fn set(&mut self, path: &[String], value: Value) -> Result<(), Step> {
                    self.node_set(path, value)
                }

                fn lists(&self) -> Vec<&dyn NodeList> {
                    vec![$( &self.$children as &dyn NodeList )?]
                }

                fn lists_mut(&mut self) -> Vec<&mut dyn NodeList> {
                    vec![$( &mut self.$children as &mut dyn NodeList )?]
                }
            }

            impl Listed for $kind {
                fn holds(kind: NodeKind) -> bool {
                    kind == NodeKind::$kind
                }

                fn into_node(self) -> Node {
                    Node::$kind(node_wrap!(self $(, $boxed)?))
                }

                fn from_node(node: Node) -> Option<Self> {
                    match node {
                        Node::$kind(node) => Some(node_unwrap!(node $(, $boxed)?)),
                        _ => None,
                    }
                }
            }
        )*

        impl Artist {
            /// Returns the node identifier of the artist.
            pub fn id(&self) -> NodeId {
                match self {
                    $( Artist::$artist(artist) => artist.id, )*
                }
            }

            /// Returns the label drawn beside the artist in a legend.
            pub fn label(&self) -> Option<&Text> {
                match self {
                    $( Artist::$artist(artist) => artist.label.as_ref(), )*
                }
            }

            /// Returns whether the artist is drawn.
            pub fn visible(&self) -> bool {
                match self {
                    $( Artist::$artist(artist) => artist.visible, )*
                }
            }

            /// Sets whether the artist is drawn.
            pub fn set_visible(&mut self, visible: bool) {
                match self {
                    $( Artist::$artist(artist) => artist.visible = visible, )*
                }
            }

            /// Returns how the artist uses its data and the dimensions of its axes, as
            /// the usage of its kind describes it.
            pub(crate) fn usage(&self) -> ArtistUsage<'_> {
                match self {
                    $( Artist::$artist(artist) => Usage::usage(artist), )*
                }
            }
        }

        impl Figure {
            /// Calls the check of every node of the figure with its parent, in tree
            /// order: the figure, then each node of each top-level kind followed by its
            /// children.
            pub(crate) fn check_nodes(&self, validator: &mut Validator<'_>) {
                Check::check(self, validator, &());
                $(
                    for node in &self.$field {
                        Check::check(node, validator, self);
                        $(
                            for child in &node.$children {
                                Check::check(child, validator, node);
                            }
                        )?
                    }
                )*
            }
        }

        impl Encode for Node {
            type Message = w::Node;

            fn encode(&self) -> w::Node {
                let kind = match self {
                    $( Node::$kind(node) => w::NodeKind::$kind(node.encode()), )*
                    Node::Artist(artist) => w::NodeKind::Artist(artist.encode()),
                };
                w::Node { kind: Some(kind) }
            }
        }

        impl Decode for Node {
            type Message = w::Node;

            fn decode(wire: w::Node, at: &str) -> Result<Self, ProtobufError> {
                Ok(match w::required(wire.kind, at, "kind")? {
                    $(
                        w::NodeKind::$kind(node) => Node::$kind(node_wrap!(
                            $kind::decode_field(node, at, stringify!($wire))?
                            $(, $boxed)?
                        )),
                    )*
                    w::NodeKind::Artist(artist) => {
                        Node::Artist(Artist::decode_field(artist, at, "artist")?)
                    }
                })
            }
        }

        impl Encode for Artist {
            type Message = w::Artist;

            fn encode(&self) -> w::Artist {
                let kind = match self {
                    $( Artist::$artist(artist) => w::ArtistKind::$artist(artist.encode()), )*
                };
                w::Artist { kind: Some(kind) }
            }
        }

        impl Decode for Artist {
            type Message = w::Artist;

            fn decode(wire: w::Artist, at: &str) -> Result<Self, ProtobufError> {
                Ok(match w::required(wire.kind, at, "kind")? {
                    $(
                        w::ArtistKind::$artist(artist) => Artist::$artist(
                            $artist::decode_field(artist, at, stringify!($artist_wire))?,
                        ),
                    )*
                })
            }
        }

        impl TreeNode for Artist {
            fn id(&self) -> NodeId {
                Artist::id(self)
            }

            fn kind(&self) -> NodeKind {
                match self {
                    $( Artist::$artist(_) => NodeKind::$artist, )*
                }
            }

            fn get(&self, path: &[String]) -> Result<Value, Step> {
                match self {
                    $( Artist::$artist(artist) => artist.node_get(path), )*
                }
            }

            fn set(&mut self, path: &[String], value: Value) -> Result<(), Step> {
                match self {
                    $( Artist::$artist(artist) => artist.node_set(path, value), )*
                }
            }

            fn lists(&self) -> Vec<&dyn NodeList> {
                Vec::new()
            }

            fn lists_mut(&mut self) -> Vec<&mut dyn NodeList> {
                Vec::new()
            }
        }

        impl Listed for Artist {
            fn holds(kind: NodeKind) -> bool {
                matches!(kind, $( NodeKind::$artist )|*)
            }

            fn into_node(self) -> Node {
                Node::Artist(self)
            }

            fn from_node(node: Node) -> Option<Self> {
                match node {
                    Node::Artist(artist) => Some(artist),
                    _ => None,
                }
            }
        }
    };
}

figure_nodes! {
    figure {
        /// A data array, which artists refer to by its identifier.
        DataArray in data, found by data_array and data_array_mut as "data array",
            encoded as data_array;
        /// A colour scale, which colour-mapped artists refer to by its identifier.
        ColorScale in color_scales, found by color_scale and color_scale_mut
            as "colour scale", encoded as color_scale;
        /// An axes with its artists, boxed in a [`Node`] because an axes is much larger
        /// than any other node.
        Axes(boxed) in axes, found by axes and axes_mut as "axes",
            encoded as axes, parent of artists;
        /// A panel, which occupies cells of the tile layout and holds one key.
        Panel in panels, found by panel and panel_mut as "panel",
            encoded as panel;
    }
    artists {
        /// A series artist: data points drawn as a line, as markers or as both.
        Series encoded as series,
        /// A contour artist.
        Contour encoded as contour,
        /// A quiver artist.
        Quiver encoded as quiver,
        /// A surface artist.
        Surface encoded as surface,
        /// A true-colour image artist.
        Image encoded as image,
        /// A scalar image artist, whose pixels each hold one number mapped through a
        /// colour scale.
        ScalarImage encoded as scalar_image,
        /// A bar artist: one rectangle per position from a base to each value.
        Bar encoded as bar,
        /// A stairs artist: a stepped outline through values between edges or at samples.
        Stairs encoded as stairs,
        /// A cells artist: a grid of rectangles, each coloured by its value through a
        /// colour scale.
        Cells encoded as cells,
    }
}

// ---------------------------------------------------------------------------------
// The walk of the tree, written once for every kind of node
// ---------------------------------------------------------------------------------

/// The address of a node below the root: at each level, the position of the list among
/// the lists of the parent and the position of the node in that list.
pub(crate) type Address = Vec<(usize, usize)>;

/// Returns the address of the node with the identifier in the subtree of `root`, or `None`
/// when the subtree has no such node. When several nodes share the identifier, the first
/// in tree order is found.
pub(crate) fn address(root: &dyn TreeNode, id: NodeId) -> Option<Address> {
    if root.id() == id {
        return Some(Address::new());
    }
    for (l, list) in root.lists().into_iter().enumerate() {
        for p in 0..list.count() {
            if let Some(mut below) = address(list.at(p), id) {
                below.insert(0, (l, p));
                return Some(below);
            }
        }
    }
    None
}

/// Returns the node at an address in the subtree of `root`.
pub(crate) fn at<'a>(root: &'a dyn TreeNode, address: &[(usize, usize)]) -> &'a dyn TreeNode {
    let mut node = root;
    for &(l, p) in address {
        let list: &'a dyn NodeList = node.lists()[l];
        node = list.at(p);
    }
    node
}

/// Returns the node at an address in the subtree of `root`, for modification.
pub(crate) fn at_mut<'a>(
    root: &'a mut dyn TreeNode,
    address: &[(usize, usize)],
) -> &'a mut dyn TreeNode {
    let mut node = root;
    for &(l, p) in address {
        node = node.lists_mut().swap_remove(l).at_mut(p);
    }
    node
}

/// Calls `visit` with every node of the subtree of `root` and its parent (`None` for
/// `root`), in tree order.
pub(crate) fn visit(
    root: &dyn TreeNode,
    parent: Option<NodeId>,
    visit_node: &mut dyn FnMut(&dyn TreeNode, Option<NodeId>),
) {
    visit_node(root, parent);
    for list in root.lists() {
        for p in 0..list.count() {
            visit(list.at(p), Some(root.id()), visit_node);
        }
    }
}

/// Returns the identifiers of the subtree of `root`, `root` first, in tree order.
pub(crate) fn subtree_ids(root: &dyn TreeNode) -> Vec<NodeId> {
    let mut ids = Vec::new();
    visit(root, None, &mut |node, _| ids.push(node.id()));
    ids
}

impl Node {
    /// Returns the identifier of the node.
    pub fn id(&self) -> NodeId {
        self.tree().id()
    }

    /// Returns the kind of the node.
    pub fn kind(&self) -> NodeKind {
        self.tree().kind()
    }
}

impl Figure {
    /// Returns the identifiers of every node, in tree order: the figure, its data arrays,
    /// its colour scales, its axes each followed by its artists, and its panels.
    pub(crate) fn node_ids(&self) -> Vec<NodeId> {
        subtree_ids(self)
    }

    /// Returns the node with the identifier, or `None` when it is not a node of the
    /// figure.
    pub(crate) fn tree_node(&self, id: NodeId) -> Option<&dyn TreeNode> {
        address(self, id).map(|address| at(self, &address))
    }

    /// Returns the node with the identifier for modification, or `None` when it is not a
    /// node of the figure.
    pub(crate) fn tree_node_mut(&mut self, id: NodeId) -> Option<&mut dyn TreeNode> {
        let address = address(&*self, id)?;
        Some(at_mut(self, &address))
    }
}

/// The tree of a figure's nodes, as the structural edits of a transaction leave it.
///
/// The overlay and selections need to know which nodes a removal takes with it at the
/// moment of the removal, so that an artist moved out of an axes earlier in the same
/// transaction is not counted among its descendants. Only the shape of the tree is
/// tracked, so following a transaction costs nothing like a copy of the figure.
pub(crate) struct NodeTree {
    /// The parent of every node other than the root.
    parents: BTreeMap<NodeId, NodeId>,
}

impl NodeTree {
    /// Returns the tree of a figure.
    pub(crate) fn of(figure: &Figure) -> Self {
        let mut tree = NodeTree {
            parents: BTreeMap::new(),
        };
        tree.add(figure, None);
        tree
    }

    /// Adds a subtree whose root has the given parent.
    fn add(&mut self, root: &dyn TreeNode, parent: Option<NodeId>) {
        visit(root, parent, &mut |node, parent| {
            if let Some(parent) = parent {
                self.parents.insert(node.id(), parent);
            }
        });
    }

    /// Returns the node and its descendants, the node first, or nothing when the tree
    /// does not hold the node below its root.
    pub(crate) fn subtree(&self, node: NodeId) -> Vec<NodeId> {
        if !self.parents.contains_key(&node) {
            return Vec::new();
        }
        let mut subtree = vec![node];
        let mut k = 0;
        while k < subtree.len() {
            let parent = subtree[k];
            subtree.extend(
                self.parents
                    .iter()
                    .filter(|&(_, p)| *p == parent)
                    .map(|(child, _)| *child),
            );
            k += 1;
        }
        subtree
    }

    /// Applies the structural effect of an edit; every other edit leaves the tree
    /// unchanged. Positions within a list are not tracked, because only membership and
    /// descent matter.
    pub(crate) fn apply(&mut self, edit: &Edit) {
        match edit {
            Edit::Insert { parent, node, .. } => self.add(node.tree(), Some(*parent)),
            Edit::Remove { node } => {
                for removed in self.subtree(*node) {
                    self.parents.remove(&removed);
                }
            }
            Edit::Move { node, parent, .. } => {
                if let Some(old) = self.parents.get_mut(node) {
                    *old = *parent;
                }
            }
            _ => {}
        }
    }
}
