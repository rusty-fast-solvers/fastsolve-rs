//! Implementation of constructors for multi node trees from distributed point data.
use bempp_traits::types::RlstScalar;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use mpi::{
    // collective::SystemOperation,
    topology::UserCommunicator,
    traits::*,
    Rank,
};
use num::traits::Float;

use hyksort::hyksort;

use bempp_traits::tree::Tree;

use crate::constants::{LEVEL_SIZE, N_CRIT};
use crate::types::morton;
use crate::{
    constants::{DEEPEST_LEVEL, DEFAULT_LEVEL},
    implementations::impl_morton::encode_anchor,
    types::{
        domain::Domain,
        morton::{KeyType, MortonKey, MortonKeys},
        multi_node::MultiNodeTree,
        point::{Point, Points},
        single_node::SingleNodeTree,
    },
};

impl<T> MultiNodeTree<T>
where
    T: Float + Default + Equivalence + Debug + RlstScalar<Real = T>,
{
    /// Constructor for uniform trees.
    ///
    /// # Arguments
    /// * `world` - A global communicator for the tree.
    /// * `subcomm_size` - Size of subcommunicator used in Hyksort. Must be a power of 2.
    /// * `points` - Cartesian point data in column major order.
    /// * `domain` - Domain associated with the global point set.
    /// * `depth` - The maximum depth of recursion for the tree.
    /// * `global_idxs` - Globally unique indices for point data.
    pub fn uniform_tree(
        world: &UserCommunicator,
        subcomm_size: i32,
        points: &[T],
        domain: &Domain<T>,
        depth: u64,
        global_idxs: &[usize],
    ) -> MultiNodeTree<T> {
        // Encode points at deepest level, and map to specified depth.
        let dim = 3;
        let npoints = points.len() / dim;
        let rank = world.rank();

        let mut tmp = Points::default();
        for i in 0..npoints {
            let point = [points[i], points[i + npoints], points[i + 2 * npoints]];
            let base_key = MortonKey::from_point(&point, domain, DEEPEST_LEVEL);
            let encoded_key = MortonKey::from_point(&point, domain, depth);
            tmp.points.push(Point {
                coordinate: point,
                base_key,
                encoded_key,
                global_idx: global_idxs[i],
            })
        }
        let mut points = tmp;

        // Perform parallel Morton sort over encoded points
        let comm = world.duplicate();
        hyksort(&mut points.points, subcomm_size, comm);

        // For simplicity, do a top down encoding, in which case number of ranks must be a power of two
        let n_global = 8i32.pow(depth as u32);
        let n_local = n_global / world.size();
        let n_prev = rank * n_local;
        let n_min = n_prev as u64;
        let n_max = (n_prev + n_local) as u64;

        let diameter = 1 << (DEEPEST_LEVEL - depth);
        let steps_per_dimension = LEVEL_SIZE / diameter;
        let steps_per_dimension_2 = steps_per_dimension.pow(2);

        let i_idx = (n_min / steps_per_dimension_2) * diameter;
        let j_idx = ((n_min % steps_per_dimension_2) / steps_per_dimension) * diameter;
        let k_idx = n_min % steps_per_dimension * diameter;
        let anchor = [i_idx, j_idx, k_idx];
        let morton = encode_anchor(&anchor, depth);
        let min = MortonKey { anchor, morton };

        let i_idx = (n_max / steps_per_dimension_2) * diameter;
        let j_idx = ((n_max % steps_per_dimension_2) / steps_per_dimension) * diameter;
        let k_idx = n_max % steps_per_dimension * diameter;
        let anchor = [i_idx, j_idx, k_idx];
        let morton = encode_anchor(&anchor, depth);
        let max = MortonKey { anchor, morton };

        println!(
            "RANK {:?} DEPTH {:?} MIN {:?} MAX {:?}",
            rank,
            depth,
            min.anchor(),
            max.anchor()
        );
        // Find leaf keys on each processor
        // let min = points.points.iter().min().unwrap().encoded_key;
        // let max = points.points.iter().max().unwrap().encoded_key;

        let diameter = 1 << (DEEPEST_LEVEL - depth);

        // Generate complete tree at specified depth within the processor's range
        let leaves = MortonKeys {
            keys: (min.anchor[0]..max.anchor[0])
                .step_by(diameter)
                .flat_map(|i| {
                    (min.anchor[1]..max.anchor[1])
                        .step_by(diameter)
                        .map(move |j| (i, j))
                })
                .flat_map(|(i, j)| {
                    (min.anchor[2]..max.anchor[2])
                        .step_by(diameter)
                        .map(move |k| [i, j, k])
                })
                .map(|anchor| {
                    let morton = encode_anchor(&anchor, depth);
                    MortonKey { anchor, morton }
                })
                .collect(),
            index: 0,
        };

        // Assign keys to points
        let unmapped = SingleNodeTree::assign_nodes_to_points(&leaves, &mut points);

        // Group points by leaves
        points.sort();

        let mut leaves_to_coordinates = HashMap::new();
        let mut curr = points.points[0];
        let mut curr_idx = 0;

        for (i, point) in points.points.iter().enumerate() {
            if point.encoded_key != curr.encoded_key {
                leaves_to_coordinates.insert(curr.encoded_key, (curr_idx, i));
                curr_idx = i;
                curr = *point;
            }
        }
        leaves_to_coordinates.insert(curr.encoded_key, (curr_idx, points.points.len()));

        // Add unmapped leaves
        let leaves = MortonKeys {
            keys: leaves_to_coordinates
                .keys()
                .cloned()
                .chain(unmapped.iter().copied())
                .collect_vec(),
            index: 0,
        };

        // Find all keys in tree
        let tmp: HashSet<MortonKey> = leaves
            .iter()
            .flat_map(|leaf| leaf.ancestors().into_iter())
            .collect();

        let mut keys = MortonKeys {
            keys: tmp.into_iter().collect_vec(),
            index: 0,
        };

        let leaves_set: HashSet<MortonKey> = leaves.iter().cloned().collect();
        let keys_set: HashSet<MortonKey> = keys.iter().cloned().collect();

        let min = leaves.iter().min().unwrap();
        let max = leaves.iter().max().unwrap();
        let range = [world.rank() as KeyType, min.morton, max.morton];

        // Group by level to perform efficient lookup of nodes
        keys.sort_by_key(|a| a.level());

        let mut levels_to_keys = HashMap::new();
        let mut curr = keys[0];
        let mut curr_idx = 0;
        for (i, key) in keys.iter().enumerate() {
            if key.level() != curr.level() {
                levels_to_keys.insert(curr.level(), (curr_idx, i));
                curr_idx = i;
                curr = *key;
            }
        }
        levels_to_keys.insert(curr.level(), (curr_idx, keys.len()));

        // Return tree in sorted order
        for l in 0..=depth {
            let &(l, r) = levels_to_keys.get(&l).unwrap();
            let subset = &mut keys[l..r];
            subset.sort();
        }

        let coordinates = points
            .points
            .iter()
            .map(|p| p.coordinate)
            .flat_map(|[x, y, z]| vec![x, y, z])
            .collect_vec();
        let global_indices = points.points.iter().map(|p| p.global_idx).collect_vec();
        let mut key_to_index = HashMap::new();

        for (i, key) in keys.iter().enumerate() {
            key_to_index.insert(*key, i);
        }

        let mut leaf_to_index = HashMap::new();

        for (i, key) in leaves.iter().enumerate() {
            leaf_to_index.insert(*key, i);
        }

        MultiNodeTree {
            world: world.duplicate(),
            depth,
            domain: *domain,
            points,
            coordinates,
            global_indices,
            leaves,
            keys,
            leaves_to_coordinates,
            levels_to_keys,
            leaves_set,
            keys_set,
            range,
            key_to_index,
            leaf_to_index,
        }

        // MultiNodeTree {}
    }

    pub fn uniform_tree_sparse(
        world: &UserCommunicator,
        subcomm_size: i32,
        points: &[T],
        domain: &Domain<T>,
        depth: u64,
        global_idxs: &[usize],
    ) -> MultiNodeTree<T> {
        // Encode points at deepest level, and map to specified depth.
        let dim = 3;
        let npoints = points.len() / dim;

        let mut tmp = Points::default();
        for i in 0..npoints {
            let point = [points[i], points[i + npoints], points[i + 2 * npoints]];
            let base_key = MortonKey::from_point(&point, domain, DEEPEST_LEVEL);
            let encoded_key = MortonKey::from_point(&point, domain, depth);
            tmp.points.push(Point {
                coordinate: point,
                base_key,
                encoded_key,
                global_idx: global_idxs[i],
            })
        }
        let mut points = tmp;

        // Perform parallel Morton sort over encoded points
        let comm = world.duplicate();
        hyksort(&mut points.points, subcomm_size, comm);

        // Find leaf keys on each processor
        let min = points.points.iter().min().unwrap().encoded_key;
        let max = points.points.iter().max().unwrap().encoded_key;

        let diameter = 1 << (DEEPEST_LEVEL - depth);

        // Generate complete tree at specified depth within the processor's range
        let leaves = MortonKeys {
            keys: (min.anchor[0]..max.anchor[0])
                .step_by(diameter)
                .flat_map(|i| {
                    (min.anchor[1]..max.anchor[1])
                        .step_by(diameter)
                        .map(move |j| (i, j))
                })
                .flat_map(|(i, j)| {
                    (min.anchor[2]..max.anchor[2])
                        .step_by(diameter)
                        .map(move |k| [i, j, k])
                })
                .map(|anchor| {
                    let morton = encode_anchor(&anchor, depth);
                    MortonKey { anchor, morton }
                })
                .collect(),
            index: 0,
        };

        // Assign keys to points
        let unmapped = SingleNodeTree::assign_nodes_to_points(&leaves, &mut points);

        // Group points by leaves
        points.sort();

        let mut leaves_to_coordinates = HashMap::new();
        let mut curr = points.points[0];
        let mut curr_idx = 0;

        for (i, point) in points.points.iter().enumerate() {
            if point.encoded_key != curr.encoded_key {
                leaves_to_coordinates.insert(curr.encoded_key, (curr_idx, i));
                curr_idx = i;
                curr = *point;
            }
        }
        leaves_to_coordinates.insert(curr.encoded_key, (curr_idx, points.points.len()));

        // Add unmapped leaves if they are a sibling of a leaf that is mapped
        let mut leaves = MortonKeys {
            keys: leaves_to_coordinates.keys().cloned().collect_vec(),
            index: 0,
        };
        let mut leaves_set: HashSet<MortonKey> = leaves_to_coordinates.keys().cloned().collect();

        // Add unmapped leaves if they are siblings of mapped leaves
        for leaf in leaves_to_coordinates.keys().into_iter() {
            let siblings = leaf.siblings();
            for sibling in siblings {
                leaves_set.insert(sibling);
            }
        }

        for &leaf in unmapped.iter() {
            if leaves_set.contains(&leaf) {
                leaves.push(leaf)
            }
        }

        // Find all keys in tree
        let tmp: HashSet<MortonKey> = leaves
            .iter()
            .flat_map(|leaf| leaf.ancestors().into_iter())
            .collect();

        let mut keys = MortonKeys {
            keys: tmp.into_iter().collect_vec(),
            index: 0,
        };

        let leaves_set: HashSet<MortonKey> = leaves.iter().cloned().collect();
        let keys_set: HashSet<MortonKey> = keys.iter().cloned().collect();

        let min = leaves.iter().min().unwrap();
        let max = leaves.iter().max().unwrap();
        let range = [world.rank() as KeyType, min.morton, max.morton];

        // Group by level to perform efficient lookup of nodes
        keys.sort_by_key(|a| a.level());

        let mut levels_to_keys = HashMap::new();
        let mut curr = keys[0];
        let mut curr_idx = 0;
        for (i, key) in keys.iter().enumerate() {
            if key.level() != curr.level() {
                levels_to_keys.insert(curr.level(), (curr_idx, i));
                curr_idx = i;
                curr = *key;
            }
        }
        levels_to_keys.insert(curr.level(), (curr_idx, keys.len()));

        // Return tree in sorted order
        for l in 0..=depth {
            let &(l, r) = levels_to_keys.get(&l).unwrap();
            let subset = &mut keys[l..r];
            subset.sort();
        }

        let coordinates = points
            .points
            .iter()
            .map(|p| p.coordinate)
            .flat_map(|[x, y, z]| vec![x, y, z])
            .collect_vec();
        let global_indices = points.points.iter().map(|p| p.global_idx).collect_vec();
        let mut key_to_index = HashMap::new();

        for (i, key) in keys.iter().enumerate() {
            key_to_index.insert(*key, i);
        }

        let mut leaf_to_index = HashMap::new();

        for (i, key) in leaves.iter().enumerate() {
            leaf_to_index.insert(*key, i);
        }

        MultiNodeTree {
            world: world.duplicate(),
            depth,
            domain: *domain,
            points,
            coordinates,
            global_indices,
            leaves,
            keys,
            leaves_to_coordinates,
            levels_to_keys,
            leaves_set,
            keys_set,
            range,
            key_to_index,
            leaf_to_index,
        }
    }

    /// Estimate the minimum depth such that leaf boxes have at most
    /// 'n_crit' particles, using a uniform distribution of particles.
    ///
    /// # Arguments
    /// * `npoints` - Total number of particles
    /// * `n_crit` - Constraint on max number of particles per leaf box
    /// * `world_size` - The size of the global MPI communicator
    pub fn minimum_depth(nglobal_points: u64, n_crit: u64, world_size: u64) -> u64 {
        // Assume that approximately nglobal_points/world_size particles per MPI node
        let mut tmp = nglobal_points / world_size;

        let mut level = 0;
        while tmp > n_crit {
            level += 1;
            tmp /= 8;
        }

        level as u64
    }

    /// Create a new multi-node tree. If non-adaptive (uniform) trees are created, they are specified
    /// by a user defined maximum depth, if an adaptive tree is created it is specified by only by the
    /// user defined maximum leaf maximum occupancy n_crit.
    ///
    /// # Arguments
    /// * `world` - A global communicator for the tree.
    /// * `subcomm_size` - Size of subcommunicator used in Hyksort. Must be a power of 2.
    /// * `points` - Cartesian point data in column major order.
    /// * `domain` - Domain associated with the global point set.
    /// * `global_idxs` - Globally unique indices for point data.
    pub fn new(
        world: &UserCommunicator,
        points: &[T],
        n_crit: Option<u64>,
        sparse: bool,
        subcomm_size: i32,
        global_idxs: &[usize],
    ) -> MultiNodeTree<T> {
        let domain = Domain::from_global_points(points, world);
        let n_crit = n_crit.unwrap_or(N_CRIT);
        let depth =
            MultiNodeTree::<T>::minimum_depth(domain.npoints as u64, n_crit, world.size() as u64);

        if sparse {
            MultiNodeTree::uniform_tree_sparse(
                world,
                subcomm_size,
                points,
                &domain,
                depth,
                global_idxs,
            )
        } else {
            MultiNodeTree::uniform_tree(world, subcomm_size, points, &domain, depth, global_idxs)
        }
    }
}

impl<T> Tree for MultiNodeTree<T>
where
    T: Float + Default + RlstScalar<Real = T>,
{
    type Precision = T;
    type Domain = Domain<T>;
    type Node = MortonKey;
    type NodeSlice<'a> = &'a [MortonKey]
        where T: 'a;
    type Nodes = MortonKeys;

    fn node(&self, idx: usize) -> Option<&Self::Node> {
        Some(&self.keys[idx])
    }

    fn nkeys_tot(&self) -> Option<usize> {
        Some(self.keys.len())
    }

    fn nkeys(&self, level: u64) -> Option<usize> {
        if let Some(&(l, r)) = self.levels_to_keys.get(&level) {
            Some(r - l)
        } else {
            None
        }
    }

    fn nleaves(&self) -> Option<usize> {
        Some(self.leaves.len())
    }

    fn domain(&self) -> &'_ Self::Domain {
        &self.domain
    }

    fn keys(&self, level: u64) -> Option<Self::NodeSlice<'_>> {
        if let Some(&(l, r)) = self.levels_to_keys.get(&level) {
            Some(&self.keys[l..r])
        } else {
            None
        }
    }

    fn all_keys_set(&self) -> Option<&'_ HashSet<Self::Node>> {
        Some(&self.keys_set)
    }

    fn all_leaves_set(&self) -> Option<&'_ HashSet<Self::Node>> {
        Some(&self.leaves_set)
    }

    fn coordinates<'a>(&'a self, key: &Self::Node) -> Option<&'a [Self::Precision]> {
        if let Some(&(l, r)) = self.leaves_to_coordinates.get(key) {
            Some(&self.coordinates[l * 3..r * 3])
        } else {
            None
        }
    }

    fn all_coordinates(&self) -> Option<&[Self::Precision]> {
        Some(&self.coordinates)
    }

    fn all_global_indices(&self) -> Option<&[usize]> {
        Some(&self.global_indices)
    }

    fn index(&self, key: &Self::Node) -> Option<&usize> {
        self.key_to_index.get(key)
    }

    fn leaf_index(&self, key: &Self::Node) -> Option<&usize> {
        self.leaf_to_index.get(key)
    }

    fn all_keys(&self) -> Option<Self::NodeSlice<'_>> {
        Some(&self.keys)
    }

    fn all_leaves(&self) -> Option<Self::NodeSlice<'_>> {
        Some(&self.leaves)
    }

    fn depth(&self) -> u64 {
        self.depth
    }

    fn global_indices<'a>(&'a self, key: &Self::Node) -> Option<&'a [usize]> {
        if let Some(&(l, r)) = self.leaves_to_coordinates.get(key) {
            Some(&self.global_indices[l..r])
        } else {
            None
        }
    }
}
