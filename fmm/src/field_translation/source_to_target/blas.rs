//! Multipole to Local field translations for uniform and adaptive Kernel Indepenent FMMs
use itertools::Itertools;
use num::Float;
use rayon::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};

use bempp_field::types::BlasFieldTranslationKiFmm;
use bempp_traits::{
    fmm::SourceToTargetTranslation,
    kernel::Kernel,
    tree::{FmmTree, Tree},
};
use bempp_tree::{constants::NTRANSFER_VECTORS_KIFMM, types::single_node::SingleNodeTree};

use crate::{
    helpers::{homogenous_kernel_scale, m2l_scale},
    types::{FmmEvalType, KiFmm, SendPtrMut},
};

use rlst_dense::{
    array::{empty_array, Array},
    base_array::BaseArray,
    data_container::VectorContainer,
    rlst_array_from_slice2, rlst_dynamic_array2,
    traits::{MatrixSvd, MultIntoResize, RawAccess, RawAccessMut},
    types::RlstScalar,
};

impl<T, U, V> KiFmm<V, BlasFieldTranslationKiFmm<U, T>, T, U>
where
    T: Kernel<T = U> + std::marker::Send + std::marker::Sync + Default,
    U: RlstScalar<Real = U> + Float + Default,
    Array<U, BaseArray<U, VectorContainer<U>, 2>, 2>: MatrixSvd<Item = U>,
    V: FmmTree<Tree = SingleNodeTree<U>> + Send + Sync,
{
    /// Displacements
    pub fn displacements(&self, level: u64) -> Vec<Mutex<Vec<i64>>> {
        let sources = self.tree.source_tree().keys(level).unwrap();
        let nsources = sources.len();

        let all_displacements = vec![vec![-1i64; nsources]; 316];
        let all_displacements = all_displacements.into_iter().map(Mutex::new).collect_vec();

        sources
            .into_par_iter()
            .enumerate()
            .for_each(|(source_idx, source)| {
                // Find interaction list of each source, as this defines scatter locations
                let interaction_list = source
                    .parent()
                    .neighbors()
                    .iter()
                    .flat_map(|pn| pn.children())
                    .filter(|pnc| {
                        !source.is_adjacent(pnc)
                            && self
                                .tree
                                .target_tree()
                                .all_keys_set()
                                .unwrap()
                                .contains(pnc)
                    })
                    .collect_vec();

                let transfer_vectors = interaction_list
                    .iter()
                    .map(|target| target.find_transfer_vector(source))
                    .collect_vec();

                let mut transfer_vectors_map = HashMap::new();
                for (i, v) in transfer_vectors.iter().enumerate() {
                    transfer_vectors_map.insert(v, i);
                }

                let transfer_vectors_set: HashSet<_> = transfer_vectors.iter().cloned().collect();

                // Mark items in interaction list for scattering
                for (tv_idx, tv) in self
                    .source_to_target_translation_data
                    .transfer_vectors
                    .iter()
                    .enumerate()
                {
                    let mut all_displacements_lock = all_displacements[tv_idx].lock().unwrap();
                    if transfer_vectors_set.contains(&tv.hash) {
                        // Look up scatter location in target tree
                        let target =
                            &interaction_list[*transfer_vectors_map.get(&tv.hash).unwrap()];
                        let &target_idx = self.level_index_pointer_locals[level as usize]
                            .get(target)
                            .unwrap();
                        all_displacements_lock[source_idx] = target_idx as i64;
                    }
                }
            });
        all_displacements
    }
}

/// Implement the multipole to local translation operator for an SVD accelerated KiFMM on a single node.
impl<T, U, V> SourceToTargetTranslation for KiFmm<V, BlasFieldTranslationKiFmm<U, T>, T, U>
where
    T: Kernel<T = U> + std::marker::Send + std::marker::Sync + Default,
    U: RlstScalar<Real = U> + Float + Default,
    Array<U, BaseArray<U, VectorContainer<U>, 2>, 2>: MatrixSvd<Item = U>,
    V: FmmTree<Tree = SingleNodeTree<U>> + Send + Sync,
{
    fn m2l(&self, level: u64) {
        let Some(sources) = self.tree.source_tree().keys(level) else {
            return;
        };
        let Some(targets) = self.tree.target_tree().keys(level) else {
            return;
        };

        // Compute the displacements
        let all_displacements = self.displacements(level);

        let multipole_idxs = all_displacements
            .iter()
            .map(|displacement| {
                displacement
                    .lock()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .filter(|(_, &d)| d != -1)
                    .map(|(i, _)| i)
                    .collect_vec()
            })
            .collect_vec();

        let local_idxs = all_displacements
            .iter()
            .map(|displacements| {
                displacements
                    .lock()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .filter(|(_, &d)| d != -1)
                    .map(|(_, &j)| j as usize)
                    .collect_vec()
            })
            .collect_vec();

        // Number of sources at this level
        let nsources = sources.len();
        let ntargets = targets.len();

        match self.fmm_eval_type {
            FmmEvalType::Vector => {
                // Lookup multipole data from source tree
                let multipoles = rlst_array_from_slice2!(
                    U,
                    unsafe {
                        std::slice::from_raw_parts(
                            self.level_multipoles[level as usize][0][0].raw,
                            self.ncoeffs * nsources,
                        )
                    },
                    [self.ncoeffs, nsources]
                );

                // Allocate buffer to store compressed check potentials
                let compressed_check_potentials = rlst_dynamic_array2!(
                    U,
                    [self.source_to_target_translation_data.cutoff_rank, ntargets]
                );
                let mut compressed_check_potentials_ptrs = Vec::new();

                for i in 0..ntargets {
                    let raw = unsafe {
                        compressed_check_potentials
                            .data()
                            .as_ptr()
                            .add(i * self.source_to_target_translation_data.cutoff_rank)
                            as *mut U
                    };
                    let send_ptr = SendPtrMut { raw };
                    compressed_check_potentials_ptrs.push(send_ptr);
                }

                let compressed_level_check_potentials = compressed_check_potentials_ptrs
                    .iter()
                    .map(Mutex::new)
                    .collect_vec();

                // 1. Compute the SVD compressed multipole expansions at this level
                let mut compressed_multipoles;
                {
                    rlst_blis::interface::threading::enable_threading();
                    compressed_multipoles = empty_array::<U, 2>().simple_mult_into_resize(
                        self.source_to_target_translation_data
                            .operator_data
                            .st_block
                            .view(),
                        multipoles,
                    );
                    rlst_blis::interface::threading::disable_threading();

                    compressed_multipoles.data_mut().iter_mut().for_each(|d| {
                        *d *= homogenous_kernel_scale::<U>(level) * m2l_scale::<U>(level)
                    });
                }

                // 2. Apply BLAS operation
                {
                    (0..NTRANSFER_VECTORS_KIFMM)
                        .into_par_iter()
                        .zip(multipole_idxs)
                        .zip(local_idxs)
                        .for_each(|((c_idx, multipole_idxs), local_idxs)| {
                            let c_u_sub =
                                &self.source_to_target_translation_data.operator_data.c_u[c_idx];
                            let c_vt_sub =
                                &self.source_to_target_translation_data.operator_data.c_vt[c_idx];

                            let mut compressed_multipoles_subset = rlst_dynamic_array2!(
                                U,
                                [
                                    self.source_to_target_translation_data.cutoff_rank,
                                    multipole_idxs.len()
                                ]
                            );

                            for (i, &multipole_idx) in multipole_idxs.iter().enumerate() {
                                compressed_multipoles_subset.data_mut()[i * self
                                    .source_to_target_translation_data
                                    .cutoff_rank
                                    ..(i + 1) * self.source_to_target_translation_data.cutoff_rank]
                                    .copy_from_slice(
                                        &compressed_multipoles.data()[multipole_idx
                                            * self.source_to_target_translation_data.cutoff_rank
                                            ..(multipole_idx + 1)
                                                * self
                                                    .source_to_target_translation_data
                                                    .cutoff_rank],
                                    );
                            }

                            let compressed_check_potential = empty_array::<U, 2>()
                                .simple_mult_into_resize(
                                    c_u_sub.view(),
                                    empty_array::<U, 2>().simple_mult_into_resize(
                                        c_vt_sub.view(),
                                        compressed_multipoles_subset.view(),
                                    ),
                                );

                            for (multipole_idx, &local_idx) in local_idxs.iter().enumerate() {
                                let check_potential_lock =
                                    compressed_level_check_potentials[local_idx].lock().unwrap();
                                let check_potential_ptr = check_potential_lock.raw;
                                let check_potential = unsafe {
                                    std::slice::from_raw_parts_mut(
                                        check_potential_ptr,
                                        self.source_to_target_translation_data.cutoff_rank,
                                    )
                                };
                                let tmp = &compressed_check_potential.data()[multipole_idx
                                    * self.source_to_target_translation_data.cutoff_rank
                                    ..(multipole_idx + 1)
                                        * self.source_to_target_translation_data.cutoff_rank];
                                check_potential
                                    .iter_mut()
                                    .zip(tmp)
                                    .for_each(|(l, r)| *l += *r);
                            }
                        });
                }

                // 3. Compute local expansions from compressed check potentials
                {
                    rlst_blis::interface::threading::enable_threading();
                    let locals = empty_array::<U, 2>().simple_mult_into_resize(
                        self.dc2e_inv_1.view(),
                        empty_array::<U, 2>().simple_mult_into_resize(
                            self.dc2e_inv_2.view(),
                            empty_array::<U, 2>().simple_mult_into_resize(
                                self.source_to_target_translation_data
                                    .operator_data
                                    .u
                                    .view(),
                                compressed_check_potentials,
                            ),
                        ),
                    );
                    rlst_blis::interface::threading::disable_threading();

                    let ptr = self.level_locals[level as usize][0][0].raw;
                    let all_locals =
                        unsafe { std::slice::from_raw_parts_mut(ptr, ntargets * self.ncoeffs) };
                    all_locals
                        .iter_mut()
                        .zip(locals.data().iter())
                        .for_each(|(l, r)| *l += *r);
                }
            }
            FmmEvalType::Matrix(nmatvecs) => {
                // Lookup multipole data from source tree
                let multipoles = rlst_array_from_slice2!(
                    U,
                    unsafe {
                        std::slice::from_raw_parts(
                            self.level_multipoles[level as usize][0][0].raw,
                            self.ncoeffs * nsources * nmatvecs,
                        )
                    },
                    [self.ncoeffs, nsources * nmatvecs]
                );

                let compressed_check_potentials = rlst_dynamic_array2!(
                    U,
                    [
                        self.source_to_target_translation_data.cutoff_rank,
                        nsources * nmatvecs
                    ]
                );
                let mut compressed_check_potentials_ptrs = Vec::new();

                for i in 0..ntargets {
                    let key_displacement =
                        i * self.source_to_target_translation_data.cutoff_rank * nmatvecs;
                    let mut tmp = Vec::new();
                    for charge_vec_idx in 0..nmatvecs {
                        let charge_vec_displacement =
                            charge_vec_idx * self.source_to_target_translation_data.cutoff_rank;

                        let raw = unsafe {
                            compressed_check_potentials
                                .data()
                                .as_ptr()
                                .add(key_displacement + charge_vec_displacement)
                                as *mut U
                        };
                        let send_ptr = SendPtrMut { raw };
                        tmp.push(send_ptr)
                    }
                    compressed_check_potentials_ptrs.push(tmp);
                }

                let compressed_level_check_potentials = compressed_check_potentials_ptrs
                    .iter()
                    .map(Mutex::new)
                    .collect_vec();

                // 1. Compute the SVD compressed multipole expansions at this level
                let mut compressed_multipoles;
                {
                    rlst_blis::interface::threading::enable_threading();
                    compressed_multipoles = empty_array::<U, 2>().simple_mult_into_resize(
                        self.source_to_target_translation_data
                            .operator_data
                            .st_block
                            .view(),
                        multipoles,
                    );
                    rlst_blis::interface::threading::disable_threading();

                    compressed_multipoles.data_mut().iter_mut().for_each(|d| {
                        *d *= homogenous_kernel_scale::<U>(level) * m2l_scale::<U>(level)
                    });
                }

                // 2. Apply the BLAS operation
                {
                    (0..NTRANSFER_VECTORS_KIFMM)
                        .into_par_iter()
                        .zip(multipole_idxs)
                        .zip(local_idxs)
                        .for_each(|((c_idx, multipole_idxs), local_idxs)| {
                            let c_u_sub =
                                &self.source_to_target_translation_data.operator_data.c_u[c_idx];
                            let c_vt_sub =
                                &self.source_to_target_translation_data.operator_data.c_vt[c_idx];

                            let mut compressed_multipoles_subset = rlst_dynamic_array2!(
                                U,
                                [
                                    self.source_to_target_translation_data.cutoff_rank,
                                    multipole_idxs.len() * nmatvecs
                                ]
                            );

                            for (local_multipole_idx, &global_multipole_idx) in
                                multipole_idxs.iter().enumerate()
                            {
                                let key_displacement_global = global_multipole_idx
                                    * self.source_to_target_translation_data.cutoff_rank
                                    * nmatvecs;

                                let key_displacement_local = local_multipole_idx
                                    * self.source_to_target_translation_data.cutoff_rank
                                    * nmatvecs;

                                for charge_vec_idx in 0..nmatvecs {
                                    let charge_vec_displacement = charge_vec_idx
                                        * self.source_to_target_translation_data.cutoff_rank;

                                    compressed_multipoles_subset.data_mut()[key_displacement_local
                                        + charge_vec_displacement
                                        ..key_displacement_local
                                            + charge_vec_displacement
                                            + self.source_to_target_translation_data.cutoff_rank]
                                        .copy_from_slice(
                                            &compressed_multipoles.data()[key_displacement_global
                                                + charge_vec_displacement
                                                ..key_displacement_global
                                                    + charge_vec_displacement
                                                    + self
                                                        .source_to_target_translation_data
                                                        .cutoff_rank],
                                        );
                                }
                            }

                            let compressed_check_potential = empty_array::<U, 2>()
                                .simple_mult_into_resize(
                                    c_u_sub.view(),
                                    empty_array::<U, 2>().simple_mult_into_resize(
                                        c_vt_sub.view(),
                                        compressed_multipoles_subset.view(),
                                    ),
                                );

                            for (local_multipole_idx, &global_local_idx) in
                                local_idxs.iter().enumerate()
                            {
                                let check_potential_lock = compressed_level_check_potentials
                                    [global_local_idx]
                                    .lock()
                                    .unwrap();

                                for charge_vec_idx in 0..nmatvecs {
                                    let check_potential_ptr =
                                        check_potential_lock[charge_vec_idx].raw;
                                    let check_potential = unsafe {
                                        std::slice::from_raw_parts_mut(
                                            check_potential_ptr,
                                            self.source_to_target_translation_data.cutoff_rank,
                                        )
                                    };

                                    let key_displacement = local_multipole_idx
                                        * self.source_to_target_translation_data.cutoff_rank
                                        * nmatvecs;
                                    let charge_vec_displacement = charge_vec_idx
                                        * self.source_to_target_translation_data.cutoff_rank;

                                    let tmp = &compressed_check_potential.data()[key_displacement
                                        + charge_vec_displacement
                                        ..key_displacement
                                            + charge_vec_displacement
                                            + self.source_to_target_translation_data.cutoff_rank];
                                    check_potential
                                        .iter_mut()
                                        .zip(tmp)
                                        .for_each(|(l, r)| *l += *r);
                                }
                            }
                        });
                }

                // 3. Compute local expansions from compressed check potentials
                {
                    rlst_blis::interface::threading::enable_threading();
                    let locals = empty_array::<U, 2>().simple_mult_into_resize(
                        self.dc2e_inv_1.view(),
                        empty_array::<U, 2>().simple_mult_into_resize(
                            self.dc2e_inv_2.view(),
                            empty_array::<U, 2>().simple_mult_into_resize(
                                self.source_to_target_translation_data
                                    .operator_data
                                    .u
                                    .view(),
                                compressed_check_potentials,
                            ),
                        ),
                    );
                    rlst_blis::interface::threading::disable_threading();
                    let ptr = self.level_locals[level as usize][0][0].raw;
                    let all_locals = unsafe {
                        std::slice::from_raw_parts_mut(ptr, ntargets * self.ncoeffs * nmatvecs)
                    };
                    all_locals
                        .iter_mut()
                        .zip(locals.data().iter())
                        .for_each(|(l, r)| *l += *r);
                }
            }
        }
    }
    fn p2l(&self, _level: u64) {}
}
