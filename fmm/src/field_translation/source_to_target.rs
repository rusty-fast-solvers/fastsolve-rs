//! Multipole to Local field translations for uniform and adaptive Kernel Indepenent FMMs
use itertools::Itertools;
use num::{Complex, Float};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

use bempp_field::{
    fft::Fft,
    types::{FftFieldTranslationKiFmm, SvdFieldTranslationKiFmm},
};

use bempp_traits::{
    field::{FieldTranslation, FieldTranslationData},
    fmm::{Fmm, InteractionLists},
    kernel::{Kernel, ScaleInvariantKernel},
    tree::Tree,
    types::{EvalType, Scalar},
};
use bempp_tree::types::{morton::MortonKey, single_node::SingleNodeTree};

use crate::helpers::find_chunk_size;
use crate::types::{FmmDataAdaptive, FmmDataUniform, KiFmmLinear, SendPtrMut};

use rlst_dense::{
    array::{empty_array, Array},
    base_array::BaseArray,
    data_container::VectorContainer,
    rlst_dynamic_array2,
    traits::{MatrixSvd, MultIntoResize, RawAccess, RawAccessMut, Shape, UnsafeRandomAccessMut},
};

use super::hadamard::matmul8x8;

/// Field translations defined on uniformly refined trees.
pub mod uniform {
    use super::*;

    impl<T, U> FmmDataUniform<KiFmmLinear<SingleNodeTree<U>, T, FftFieldTranslationKiFmm<U, T>, U>, U>
    where
        T: Kernel<T = U>
            + ScaleInvariantKernel<T = U>
            + std::marker::Send
            + std::marker::Sync
            + Default,
        U: Scalar<Real = U> + Float + Default + std::marker::Send + std::marker::Sync + Fft,
        Complex<U>: Scalar,
        Array<U, BaseArray<U, VectorContainer<U>, 2>, 2>: MatrixSvd<Item = U>,
    {
        fn displacements(&self, level: u64) -> Vec<Vec<usize>> {
            let nneighbors = 26;
            let nsiblings = 8;

            let sources = self.fmm.tree().get_keys(level).unwrap();

            let parents: HashSet<MortonKey> =
                sources.iter().map(|source| source.parent()).collect();
            let mut parents = parents.into_iter().collect_vec();
            parents.sort();
            let nparents = parents.len();

            let mut result = vec![Vec::new(); nneighbors];

            let parent_neighbors = parents
                .iter()
                .map(|parent| parent.all_neighbors())
                .collect_vec();

            for i in 0..nneighbors {
                for all_neighbors in parent_neighbors.iter().take(nparents) {
                    // First check if neighbor exists is in the bounds of the tree
                    if let Some(neighbor) = all_neighbors[i] {
                        let first_child = neighbor.first_child();
                        // Then need to check if first child exists in the tree
                        if let Some(first_child_index) =
                            self.level_index_pointer[level as usize].get(&first_child)
                        {
                            result[i].push(*first_child_index)
                        } else {
                            result[i].push(nparents * nsiblings)
                        }
                    } else {
                        result[i].push(nparents * nsiblings);
                    }
                }
            }

            result
        }
    }

    impl<T, U> FieldTranslation<U>
        for FmmDataUniform<KiFmmLinear<SingleNodeTree<U>, T, FftFieldTranslationKiFmm<U, T>, U>, U>
    where
        T: Kernel<T = U>
            + ScaleInvariantKernel<T = U>
            + std::marker::Send
            + std::marker::Sync
            + Default,
        U: Scalar<Real = U> + Float + Default + std::marker::Send + std::marker::Sync + Fft,
        Complex<U>: Scalar,
        Array<U, BaseArray<U, VectorContainer<U>, 2>, 2>: MatrixSvd<Item = U>,
    {
        fn p2l(&self, _level: u64) {}

        fn m2l<'a>(&self, level: u64) {
            let Some(targets) = self.fmm.tree().get_keys(level) else {
                return;
            };

            let n = 2 * self.fmm.order - 1;
            let npad = n + 1;

            // let nparents = self.fmm.tree().get_keys(level - 1).unwrap().len();
            let parents: HashSet<MortonKey> = targets
                .iter()
                .map(|target: &MortonKey| target.parent())
                .collect();
            let mut parents = parents.into_iter().collect_vec();
            parents.sort();
            let nparents = parents.len();

            let ncoeffs = self.fmm.m2l.ncoeffs(self.fmm.order);
            let nsiblings = 8;
            let nzeros = 8;
            let size = npad * npad * npad;
            let size_real = npad * npad * (npad / 2 + 1);
            let all_displacements = self.displacements(level);

            let ntargets = targets.len();
            let min = &targets[0];
            let max = &targets[ntargets - 1];
            let min_idx = self.fmm.tree().key_to_index.get(min).unwrap();
            let max_idx = self.fmm.tree().key_to_index.get(max).unwrap();

            let multipoles = &self.multipoles[min_idx * ncoeffs..(max_idx + 1) * ncoeffs];

            ////////////////////////////////////////////////////////////////////////////////////
            // Pre-process to setup data structures for M2L kernel
            ////////////////////////////////////////////////////////////////////////////////////

            // Allocation of FFT of multipoles on convolution grid, in frequency order
            let mut signals_hat_f_buffer = vec![U::zero(); size_real * (ntargets + nzeros) * 2];
            let signals_hat_f: &mut [Complex<U>];
            unsafe {
                let ptr = signals_hat_f_buffer.as_mut_ptr() as *mut Complex<U>;
                signals_hat_f =
                    std::slice::from_raw_parts_mut(ptr, size_real * (ntargets + nzeros));
            }

            // A thread safe mutable pointer for saving to this vector
            let raw = signals_hat_f.as_mut_ptr();
            let signals_hat_f_ptr = SendPtrMut { raw };

            // Pre processing chunk size, in terms of number of parents
            // let chunk_size = 1;
            let max_chunk_size;
            if level == 2 {
                max_chunk_size = 8
            } else if level == 3 {
                max_chunk_size = 64
            } else {
                max_chunk_size = 128
            }
            let chunk_size = find_chunk_size(nparents, max_chunk_size);

            // Pre-processing to find FFT
            multipoles
                .par_chunks_exact(ncoeffs * nsiblings * chunk_size)
                .enumerate()
                .for_each(|(i, multipole_chunk)| {
                    // Place Signal on convolution grid
                    let mut signal_chunk = vec![U::zero(); size * nsiblings * chunk_size];

                    for i in 0..nsiblings * chunk_size {
                        let multipole = &multipole_chunk[i * ncoeffs..(i + 1) * ncoeffs];
                        let signal = &mut signal_chunk[i * size..(i + 1) * size];
                        for (surf_idx, &conv_idx) in
                            self.fmm.m2l.surf_to_conv_map.iter().enumerate()
                        {
                            signal[conv_idx] = multipole[surf_idx]
                        }
                    }

                    // Temporary buffer to hold results of FFT
                    let signal_hat_chunk_buffer =
                        vec![U::zero(); size_real * nsiblings * chunk_size * 2];
                    let signal_hat_chunk_c;
                    unsafe {
                        let ptr = signal_hat_chunk_buffer.as_ptr() as *mut Complex<U>;
                        signal_hat_chunk_c =
                            std::slice::from_raw_parts_mut(ptr, size_real * nsiblings * chunk_size);
                    }

                    U::rfft3_fftw_slice(&mut signal_chunk, signal_hat_chunk_c, &[npad, npad, npad]);

                    // Re-order the temporary buffer into frequency order before flushing to main memory
                    let signal_hat_chunk_f_buffer =
                        vec![U::zero(); size_real * nsiblings * chunk_size * 2];
                    let signal_hat_chunk_f_c;
                    unsafe {
                        let ptr = signal_hat_chunk_f_buffer.as_ptr() as *mut Complex<U>;
                        signal_hat_chunk_f_c =
                            std::slice::from_raw_parts_mut(ptr, size_real * nsiblings * chunk_size);
                    }

                    for i in 0..size_real {
                        for j in 0..nsiblings * chunk_size {
                            signal_hat_chunk_f_c[nsiblings * chunk_size * i + j] =
                                signal_hat_chunk_c[size_real * j + i]
                        }
                    }

                    // Storing the results of the FFT in frequency order
                    unsafe {
                        let sibling_offset = i * nsiblings * chunk_size;

                        // Pointer to storage buffer for frequency ordered FFT of signals
                        let ptr = signals_hat_f_ptr;

                        for i in 0..size_real {
                            let frequency_offset = i * (ntargets + nzeros);

                            // Head of buffer for each frequency
                            let head = ptr.raw.add(frequency_offset).add(sibling_offset);

                            let signal_hat_f_chunk =
                                std::slice::from_raw_parts_mut(head, nsiblings * chunk_size);

                            // Store results for this frequency for this sibling set chunk
                            let results_i = &signal_hat_chunk_f_c
                                [i * nsiblings * chunk_size..(i + 1) * nsiblings * chunk_size];

                            signal_hat_f_chunk
                                .iter_mut()
                                .zip(results_i)
                                .for_each(|(c, r)| *c += *r);
                        }
                    }
                });

            // Allocate check potentials (implicitly in frequency order)
            let mut check_potentials_hat_f_buffer = vec![U::zero(); 2 * size_real * ntargets];
            let check_potentials_hat_f: &mut [Complex<U>];
            unsafe {
                let ptr = check_potentials_hat_f_buffer.as_mut_ptr() as *mut Complex<U>;
                check_potentials_hat_f = std::slice::from_raw_parts_mut(ptr, size_real * ntargets);
            }

            ////////////////////////////////////////////////////////////////////////////////////
            // M2L Kernel
            ////////////////////////////////////////////////////////////////////////////////////
            let scale = Complex::from(self.m2l_scale(level) * self.fmm.kernel.scale(level));
            let kernel_data_f = &self.fmm.m2l.operator_data.kernel_data_f;

            (0..size_real)
                .into_par_iter()
                .zip(signals_hat_f.par_chunks_exact(ntargets + nzeros))
                .zip(check_potentials_hat_f.par_chunks_exact_mut(ntargets))
                .for_each(|((freq, signal_hat_f), check_potential_hat_f)| {
                    (0..nparents).step_by(chunk_size).for_each(|chunk_start| {
                        let chunk_end = std::cmp::min(chunk_start + chunk_size, nparents);

                        let save_locations = &mut check_potential_hat_f
                            [chunk_start * nsiblings..chunk_end * nsiblings];

                        for (i, kernel_f) in kernel_data_f.iter().enumerate().take(26) {
                            let frequency_offset = 64 * freq;
                            let k_f = &kernel_f[frequency_offset..(frequency_offset + 64)].to_vec();

                            // Lookup signals
                            let displacements = &all_displacements[i][chunk_start..chunk_end];

                            for j in 0..(chunk_end - chunk_start) {
                                let displacement = displacements[j];
                                let s_f = &signal_hat_f[displacement..displacement + nsiblings];

                                matmul8x8(
                                    k_f,
                                    s_f,
                                    &mut save_locations[j * nsiblings..(j + 1) * nsiblings],
                                    scale,
                                )
                            }
                        }
                    });
                });

            ////////////////////////////////////////////////////////////////////////////////////
            // Post processing to find local expansions from check potentials
            ////////////////////////////////////////////////////////////////////////////////////

            // Get check potentials back into target order from frequency order
            let mut check_potential_hat = vec![U::zero(); size_real * ntargets * 2];
            let mut check_potential = vec![U::zero(); size * ntargets];
            let check_potential_hat_c;
            unsafe {
                let ptr = check_potential_hat.as_mut_ptr() as *mut Complex<U>;
                check_potential_hat_c = std::slice::from_raw_parts_mut(ptr, size_real * ntargets)
            }

            check_potential_hat_c
                .par_chunks_exact_mut(size_real)
                .enumerate()
                .for_each(|(i, check_potential_hat_chunk)| {
                    // Lookup all frequencies for this target box
                    for j in 0..size_real {
                        check_potential_hat_chunk[j] = check_potentials_hat_f[j * ntargets + i]
                    }
                });

            // Compute inverse FFT
            U::irfft3_fftw_par_slice(
                check_potential_hat_c,
                &mut check_potential,
                &[npad, npad, npad],
            );

            // TODO: Experiment with chunk size for post processing
            check_potential
                .par_chunks_exact(nsiblings * size)
                .zip(self.level_locals[level as usize].par_chunks_exact(nsiblings))
                .for_each(|(check_potential_chunk, local_ptrs)| {
                    // Map to surface grid
                    let mut potential_chunk = rlst_dynamic_array2!(U, [ncoeffs, nsiblings]);

                    for i in 0..nsiblings {
                        for (surf_idx, &conv_idx) in
                            self.fmm.m2l.conv_to_surf_map.iter().enumerate()
                        {
                            unsafe {
                                *potential_chunk.get_unchecked_mut([surf_idx, i]) =
                                    check_potential_chunk[i * size + conv_idx];
                            }
                        }
                    }

                    // Can now find local expansion coefficients
                    let local_chunk = empty_array::<U, 2>().simple_mult_into_resize(
                        self.fmm.dc2e_inv_1.view(),
                        empty_array::<U, 2>()
                            .simple_mult_into_resize(self.fmm.dc2e_inv_2.view(), potential_chunk),
                    );

                    local_chunk
                        .data()
                        .chunks_exact(ncoeffs)
                        .zip(local_ptrs)
                        .for_each(|(result, local)| {
                            let local =
                                unsafe { std::slice::from_raw_parts_mut(local.raw, ncoeffs) };
                            local.iter_mut().zip(result).for_each(|(l, r)| *l += *r);
                        });
                });
        }

        fn m2l_scale(&self, level: u64) -> U {
            if level < 2 {
                panic!("M2L only perfomed on level 2 and below")
            }

            if level == 2 {
                U::from(1. / 2.).unwrap()
            } else {
                let two = U::from(2.0).unwrap();
                Scalar::powf(two, U::from(level - 3).unwrap())
            }
        }
    }

    /// Implement the multipole to local translation operator for an SVD accelerated KiFMM on a single node.
    impl<T, U> FieldTranslation<U>
        for FmmDataUniform<KiFmmLinear<SingleNodeTree<U>, T, SvdFieldTranslationKiFmm<U, T>, U>, U>
    where
        T: Kernel<T = U>
            + ScaleInvariantKernel<T = U>
            + std::marker::Send
            + std::marker::Sync
            + Default,
        U: Scalar<Real = U>,
        U: Float + Default,
        U: std::marker::Send + std::marker::Sync + Default,
        Array<U, BaseArray<U, VectorContainer<U>, 2>, 2>: MatrixSvd<Item = U>,
    {
        fn p2l(&self, _level: u64) {}

        fn m2l<'a>(&self, level: u64) {
            let Some(sources) = self.fmm.tree().get_keys(level) else {
                return;
            };

            let nsources = sources.len();

            let mut all_displacements = vec![vec![-1i64; nsources]; 316];

            // Need to identify all save locations in a pre-processing step.
            for (j, source) in sources.iter().enumerate() {
                let v_list = source
                    .parent()
                    .neighbors()
                    .iter()
                    .flat_map(|pn| pn.children())
                    .filter(|pnc| {
                        !source.is_adjacent(pnc) && self.fmm.tree().get_all_keys_set().contains(pnc)
                    })
                    .collect_vec();

                let transfer_vectors = v_list
                    .iter()
                    .map(|target| target.find_transfer_vector(source))
                    .collect_vec();

                let mut transfer_vectors_map = HashMap::new();
                for (i, v) in transfer_vectors.iter().enumerate() {
                    transfer_vectors_map.insert(v, i);
                }

                let transfer_vectors_set: HashSet<_> = transfer_vectors.iter().collect();

                for (i, tv) in self.fmm.m2l.transfer_vectors.iter().enumerate() {
                    if transfer_vectors_set.contains(&tv.hash) {
                        let target = &v_list[*transfer_vectors_map.get(&tv.hash).unwrap()];
                        let target_index = self.level_index_pointer[level as usize]
                            .get(target)
                            .unwrap();
                        all_displacements[i][j] = *target_index as i64;
                    }
                }
            }

            // Interpret multipoles as a matrix
            let ncoeffs = self.fmm.m2l.ncoeffs(self.fmm.order);

            // TODO: remove memory assignment?
            let mut multipoles = rlst_dynamic_array2!(U, [ncoeffs, nsources]);
            for j in 0..nsources {
                for i in 0..ncoeffs {
                    unsafe {
                        *multipoles.get_unchecked_mut([i, j]) = *self.level_multipoles
                            [level as usize][0]
                            .raw
                            .add(j * ncoeffs + i);
                    }
                }
            }

            let [nrows, _] = self.fmm.m2l.operator_data.c.shape();
            let c_dim = [nrows, self.fmm.m2l.k];

            let mut compressed_multipoles = empty_array::<U, 2>()
                .simple_mult_into_resize(self.fmm.m2l.operator_data.st_block.view(), multipoles);

            compressed_multipoles
                .data_mut()
                .iter_mut()
                .for_each(|d| *d *= self.fmm.kernel.scale(level) * self.m2l_scale(level));

            (0..316).into_par_iter().for_each(|c_idx| {
                let top_left = [0, c_idx * self.fmm.m2l.k];
                let c_sub = self
                    .fmm
                    .m2l
                    .operator_data
                    .c
                    .view()
                    .into_subview(top_left, c_dim);

                let locals = empty_array::<U, 2>().simple_mult_into_resize(
                    self.fmm.dc2e_inv_1.view(),
                    empty_array::<U, 2>().simple_mult_into_resize(
                        self.fmm.dc2e_inv_2.view(),
                        empty_array::<U, 2>().simple_mult_into_resize(
                            self.fmm.m2l.operator_data.u.view(),
                            empty_array::<U, 2>().simple_mult_into_resize(
                                c_sub.view(),
                                compressed_multipoles.view(),
                            ),
                        ),
                    ),
                );

                let displacements = &all_displacements[c_idx];

                for (result_idx, &save_idx) in displacements.iter().enumerate() {
                    if save_idx > -1 {
                        let save_idx = save_idx as usize;
                        let local_ptr = self.level_locals[level as usize][save_idx].raw;
                        let local = unsafe { std::slice::from_raw_parts_mut(local_ptr, ncoeffs) };

                        let res = &locals.data()[result_idx * ncoeffs..(result_idx + 1) * ncoeffs];
                        local.iter_mut().zip(res).for_each(|(l, r)| *l += *r);
                    }
                }
            })
        }

        fn m2l_scale(&self, level: u64) -> U {
            if level < 2 {
                panic!("M2L only perfomed on level 2 and below")
            }

            if level == 2 {
                U::from(1. / 2.).unwrap()
            } else {
                let two = U::from(2.0).unwrap();
                Scalar::powf(two, U::from(level - 3).unwrap())
            }
        }
    }

    /// Field translations for uniformly refined trees that take matrix input for charges.
    pub mod matrix {
        use crate::types::{FmmDataUniformMatrix, KiFmmLinearMatrix};

        use super::*;

        /// Implement the multipole to local translation operator for an SVD accelerated KiFMM on a single node.
        impl<T, U> FieldTranslation<U>
            for FmmDataUniformMatrix<
                KiFmmLinearMatrix<SingleNodeTree<U>, T, SvdFieldTranslationKiFmm<U, T>, U>,
                U,
            >
        where
            T: Kernel<T = U>
                + ScaleInvariantKernel<T = U>
                + std::marker::Send
                + std::marker::Sync
                + Default,
            U: Scalar<Real = U>,
            U: Float + Default,
            U: std::marker::Send + std::marker::Sync + Default,
            Array<U, BaseArray<U, VectorContainer<U>, 2>, 2>: MatrixSvd<Item = U>,
        {
            fn p2l(&self, _level: u64) {}

            fn m2l<'a>(&self, level: u64) {
                let Some(sources) = self.fmm.tree().get_keys(level) else {
                    return;
                };

                let nsources = sources.len();

                let mut all_displacements = vec![vec![-1i64; nsources]; 316];

                // Need to identify all save locations in a pre-processing step.
                for (j, source) in sources.iter().enumerate() {
                    let v_list = source
                        .parent()
                        .neighbors()
                        .iter()
                        .flat_map(|pn| pn.children())
                        .filter(|pnc| {
                            !source.is_adjacent(pnc)
                                && self.fmm.tree().get_all_keys_set().contains(pnc)
                        })
                        .collect_vec();

                    let transfer_vectors = v_list
                        .iter()
                        .map(|target| target.find_transfer_vector(source))
                        .collect_vec();

                    let mut transfer_vectors_map = HashMap::new();
                    for (i, v) in transfer_vectors.iter().enumerate() {
                        transfer_vectors_map.insert(v, i);
                    }

                    let transfer_vectors_set: HashSet<_> = transfer_vectors.iter().collect();

                    for (i, tv) in self.fmm.m2l.transfer_vectors.iter().enumerate() {
                        if transfer_vectors_set.contains(&tv.hash) {
                            let target = &v_list[*transfer_vectors_map.get(&tv.hash).unwrap()];
                            let target_index = self.level_index_pointer[level as usize]
                                .get(target)
                                .unwrap();
                            all_displacements[i][j] = *target_index as i64;
                        }
                    }
                }

                // Interpret multipoles as a matrix

                // TODO: remove memory assignment
                let mut multipoles =
                    rlst_dynamic_array2!(U, [self.ncoeffs, nsources * self.ncharge_vectors]);
                for j in 0..nsources * self.ncharge_vectors {
                    for i in 0..self.ncoeffs {
                        unsafe {
                            *multipoles.get_unchecked_mut([i, j]) = *self.level_multipoles
                                [level as usize][0][0]
                                .raw
                                .add(j * self.ncoeffs * self.ncharge_vectors + i);
                        }
                    }
                }

                let [nrows, _] = self.fmm.m2l.operator_data.c.shape();
                let c_dim = [nrows, self.fmm.m2l.k];

                let mut compressed_multipoles = empty_array::<U, 2>().simple_mult_into_resize(
                    self.fmm.m2l.operator_data.st_block.view(),
                    multipoles,
                );

                compressed_multipoles
                    .data_mut()
                    .iter_mut()
                    .for_each(|d| *d *= self.fmm.kernel.scale(level) * self.m2l_scale(level));

                (0..316).into_par_iter().for_each(|c_idx| {
                    let top_left = [0, c_idx * self.fmm.m2l.k];
                    let c_sub = self
                        .fmm
                        .m2l
                        .operator_data
                        .c
                        .view()
                        .into_subview(top_left, c_dim);

                    let locals = empty_array::<U, 2>().simple_mult_into_resize(
                        self.fmm.dc2e_inv_1.view(),
                        empty_array::<U, 2>().simple_mult_into_resize(
                            self.fmm.dc2e_inv_2.view(),
                            empty_array::<U, 2>().simple_mult_into_resize(
                                self.fmm.m2l.operator_data.u.view(),
                                empty_array::<U, 2>().simple_mult_into_resize(
                                    c_sub.view(),
                                    compressed_multipoles.view(),
                                ),
                            ),
                        ),
                    );

                    let displacements = &all_displacements[c_idx];

                    for (result_idx, &save_idx) in displacements.iter().enumerate() {
                        if save_idx > -1 {
                            let save_idx = save_idx as usize;
                            for charge_vec_idx in 0..self.ncharge_vectors {
                                let local_ptr =
                                    self.level_locals[level as usize][save_idx][charge_vec_idx].raw;
                                let local = unsafe {
                                    std::slice::from_raw_parts_mut(local_ptr, self.ncoeffs)
                                };

                                let key_displacement =
                                    result_idx * self.ncoeffs * self.ncharge_vectors;
                                let charge_vec_displacement = charge_vec_idx * self.ncoeffs;

                                let result = &locals.data()[key_displacement
                                    + charge_vec_displacement
                                    ..key_displacement + charge_vec_displacement + self.ncoeffs];
                                local.iter_mut().zip(result).for_each(|(l, r)| *l += *r);
                            }
                        }
                    }
                })
            }

            fn m2l_scale(&self, level: u64) -> U {
                if level < 2 {
                    panic!("M2L only perfomed on level 2 and below")
                }

                if level == 2 {
                    U::from(1. / 2.).unwrap()
                } else {
                    let two = U::from(2.0).unwrap();
                    Scalar::powf(two, U::from(level - 3).unwrap())
                }
            }
        }
    }
}

/// Field translations defined on adaptively refined
pub mod adaptive {
    use super::*;

    impl<T, U> FmmDataAdaptive<KiFmmLinear<SingleNodeTree<U>, T, FftFieldTranslationKiFmm<U, T>, U>, U>
    where
        T: Kernel<T = U>
            + ScaleInvariantKernel<T = U>
            + std::marker::Send
            + std::marker::Sync
            + Default,
        U: Scalar<Real = U> + Float + Default + std::marker::Send + std::marker::Sync + Fft,
        Complex<U>: Scalar,
        Array<U, BaseArray<U, VectorContainer<U>, 2>, 2>: MatrixSvd<Item = U>,
    {
        fn displacements(&self, level: u64) -> Vec<Vec<usize>> {
            let nneighbors = 26;
            let nsiblings = 8;

            let sources = self.fmm.tree().get_keys(level).unwrap();

            let parents: HashSet<MortonKey> =
                sources.iter().map(|source| source.parent()).collect();
            let mut parents = parents.into_iter().collect_vec();
            parents.sort();
            let nparents = parents.len();

            let mut result = vec![Vec::new(); nneighbors];

            let parent_neighbors = parents
                .iter()
                .map(|parent| parent.all_neighbors())
                .collect_vec();

            for i in 0..nneighbors {
                for all_neighbors in parent_neighbors.iter().take(nparents) {
                    // First check if neighbor exists is in the bounds of the tree
                    if let Some(neighbor) = all_neighbors[i] {
                        let first_child = neighbor.first_child();
                        // Then need to check if first child exists in the tree
                        if let Some(first_child_index) =
                            self.level_index_pointer[level as usize].get(&first_child)
                        {
                            result[i].push(*first_child_index)
                        } else {
                            result[i].push(nparents * nsiblings)
                        }
                    } else {
                        result[i].push(nparents * nsiblings);
                    }
                }
            }

            result
        }
    }

    impl<T, U> FieldTranslation<U>
        for FmmDataAdaptive<KiFmmLinear<SingleNodeTree<U>, T, FftFieldTranslationKiFmm<U, T>, U>, U>
    where
        T: Kernel<T = U>
            + ScaleInvariantKernel<T = U>
            + std::marker::Send
            + std::marker::Sync
            + Default,
        U: Scalar<Real = U> + Float + Default + std::marker::Send + std::marker::Sync + Fft,
        Complex<U>: Scalar,
        Array<U, BaseArray<U, VectorContainer<U>, 2>, 2>: MatrixSvd<Item = U>,
    {
        fn p2l<'a>(&self, level: u64) {
            let Some(targets) = self.fmm.tree().get_keys(level) else {
                return;
            };

            let ncoeffs = self.fmm.m2l.ncoeffs(self.fmm.order);
            let dim = self.fmm.kernel().space_dimension();
            let surface_size = ncoeffs * dim;
            let min_idx = self.fmm.tree().key_to_index.get(&targets[0]).unwrap();
            let max_idx = self
                .fmm
                .tree()
                .key_to_index
                .get(targets.last().unwrap())
                .unwrap();
            let downward_surfaces =
                &self.downward_surfaces[min_idx * surface_size..(max_idx + 1) * surface_size];
            let coordinates = self.fmm.tree().get_all_coordinates().unwrap();

            let ntargets = targets.len();
            let mut check_potentials = vec![U::zero(); ncoeffs * ntargets];

            // 1. Compute check potentials from x list of each target
            targets
                .par_iter()
                .zip(downward_surfaces.par_chunks_exact(surface_size))
                .zip(check_potentials.par_chunks_exact_mut(ncoeffs))
                .for_each(|((target, downward_surface), check_potential)| {
                    // Find check potential
                    if let Some(x_list) = self.fmm.get_x_list(target) {
                        let x_list_indices = x_list
                            .iter()
                            .filter_map(|k| self.fmm.tree().get_leaf_index(k));
                        let charges = x_list_indices
                            .clone()
                            .map(|&idx| {
                                let index_pointer = &self.charge_index_pointer[idx];
                                &self.charges[index_pointer.0..index_pointer.1]
                            })
                            .collect_vec();

                        let sources_coordinates = x_list_indices
                            .into_iter()
                            .map(|&idx| {
                                let index_pointer = &self.charge_index_pointer[idx];
                                &coordinates[index_pointer.0 * dim..index_pointer.1 * dim]
                            })
                            .collect_vec();

                        for (&charges, sources) in charges.iter().zip(sources_coordinates) {
                            let nsources = sources.len() / dim;

                            if nsources > 0 {
                                self.fmm.kernel.evaluate_st(
                                    EvalType::Value,
                                    &sources,
                                    downward_surface,
                                    charges,
                                    check_potential,
                                );
                            }
                        }
                    }
                });

            // 2. Compute local expansion from check potential
            self.level_locals[level as usize]
                .par_iter()
                .zip(check_potentials.par_chunks_exact(ncoeffs))
                .for_each(|(local_ptr, check_potential)| {
                    let target_local =
                        unsafe { std::slice::from_raw_parts_mut(local_ptr.raw, ncoeffs) };

                    // TODO: remove memory assignment
                    let mut check_potential_mat = rlst_dynamic_array2!(U, [ncoeffs, 1]);
                    check_potential_mat
                        .data_mut()
                        .copy_from_slice(check_potential);

                    let scale = self.fmm.kernel().scale(level);
                    let mut tmp = empty_array::<U, 2>().simple_mult_into_resize(
                        self.fmm.dc2e_inv_1.view(),
                        empty_array::<U, 2>().simple_mult_into_resize(
                            self.fmm.dc2e_inv_2.view(),
                            check_potential_mat,
                        ),
                    );
                    tmp.data_mut().iter_mut().for_each(|val| *val *= scale);

                    target_local
                        .iter_mut()
                        .zip(tmp.data())
                        .for_each(|(r, &t)| *r += t);
                });
        }

        fn m2l<'a>(&self, level: u64) {
            let Some(targets) = self.fmm.tree().get_keys(level) else {
                return;
            };

            let n = 2 * self.fmm.order - 1;
            let npad = n + 1;

            let parents: HashSet<MortonKey> = targets
                .iter()
                .map(|target: &MortonKey| target.parent())
                .collect();
            let mut parents = parents.into_iter().collect_vec();
            parents.sort();
            let nparents = parents.len();

            let ncoeffs = self.fmm.m2l.ncoeffs(self.fmm.order);
            let nsiblings = 8;
            let nzeros = 8;
            let size = npad * npad * npad;
            let size_real = npad * npad * (npad / 2 + 1);
            let all_displacements = self.displacements(level);

            let ntargets = targets.len();
            let min = &targets[0];
            let max = &targets[ntargets - 1];

            let min_idx = self.fmm.tree().key_to_index.get(min).unwrap();
            let max_idx = self.fmm.tree().key_to_index.get(max).unwrap();

            let multipoles = &self.multipoles[min_idx * ncoeffs..(max_idx + 1) * ncoeffs];

            ////////////////////////////////////////////////////////////////////////////////////
            // Pre-process to setup data structures for M2L kernel
            ////////////////////////////////////////////////////////////////////////////////////

            // Allocation of FFT of multipoles on convolution grid, in frequency order
            let mut signals_hat_f_buffer = vec![U::zero(); size_real * (ntargets + nzeros) * 2];
            let signals_hat_f: &mut [Complex<U>];
            unsafe {
                let ptr = signals_hat_f_buffer.as_mut_ptr() as *mut Complex<U>;
                signals_hat_f =
                    std::slice::from_raw_parts_mut(ptr, size_real * (ntargets + nzeros));
            }

            // A thread safe mutable pointer for saving to this vector
            let raw = signals_hat_f.as_mut_ptr();
            let signals_hat_f_ptr = SendPtrMut { raw };

            // Pre processing chunk size, in terms of number of parents
            // let chunk_size = 1;
            let max_chunk_size;
            if level == 2 {
                max_chunk_size = 8
            } else if level == 3 {
                max_chunk_size = 64
            } else {
                max_chunk_size = 128
            }
            let chunk_size = find_chunk_size(nparents, max_chunk_size);

            // Pre-processing to find FFT
            multipoles
                .par_chunks_exact(ncoeffs * nsiblings * chunk_size)
                // .par_chunks_exact(nsiblings * chunk_size)
                .enumerate()
                .for_each(|(i, multipole_chunk)| {
                    // Place Signal on convolution grid
                    let mut signal_chunk = vec![U::zero(); size * nsiblings * chunk_size];

                    for i in 0..nsiblings * chunk_size {
                        let multipole = &multipole_chunk[i * ncoeffs..(i + 1) * ncoeffs];
                        // let multipole = unsafe { std::slice::from_raw_parts(multipole_chunk[i].raw, ncoeffs) };
                        let signal = &mut signal_chunk[i * size..(i + 1) * size];
                        for (surf_idx, &conv_idx) in
                            self.fmm.m2l.surf_to_conv_map.iter().enumerate()
                        {
                            signal[conv_idx] = multipole[surf_idx]
                        }
                    }

                    // Temporary buffer to hold results of FFT
                    let signal_hat_chunk_buffer =
                        vec![U::zero(); size_real * nsiblings * chunk_size * 2];
                    let signal_hat_chunk_c;
                    unsafe {
                        let ptr = signal_hat_chunk_buffer.as_ptr() as *mut Complex<U>;
                        signal_hat_chunk_c =
                            std::slice::from_raw_parts_mut(ptr, size_real * nsiblings * chunk_size);
                    }

                    U::rfft3_fftw_slice(&mut signal_chunk, signal_hat_chunk_c, &[npad, npad, npad]);

                    // Re-order the temporary buffer into frequency order before flushing to main memory
                    let signal_hat_chunk_f_buffer =
                        vec![U::zero(); size_real * nsiblings * chunk_size * 2];
                    let signal_hat_chunk_f_c;
                    unsafe {
                        let ptr = signal_hat_chunk_f_buffer.as_ptr() as *mut Complex<U>;
                        signal_hat_chunk_f_c =
                            std::slice::from_raw_parts_mut(ptr, size_real * nsiblings * chunk_size);
                    }

                    for i in 0..size_real {
                        for j in 0..nsiblings * chunk_size {
                            signal_hat_chunk_f_c[nsiblings * chunk_size * i + j] =
                                signal_hat_chunk_c[size_real * j + i]
                        }
                    }

                    // Storing the results of the FFT in frequency order
                    unsafe {
                        let sibling_offset = i * nsiblings * chunk_size;

                        // Pointer to storage buffer for frequency ordered FFT of signals
                        let ptr = signals_hat_f_ptr;

                        for i in 0..size_real {
                            let frequency_offset = i * (ntargets + nzeros);

                            // Head of buffer for each frequency
                            let head = ptr.raw.add(frequency_offset).add(sibling_offset);

                            let signal_hat_f_chunk =
                                std::slice::from_raw_parts_mut(head, nsiblings * chunk_size);

                            // Store results for this frequency for this sibling set chunk
                            let results_i = &signal_hat_chunk_f_c
                                [i * nsiblings * chunk_size..(i + 1) * nsiblings * chunk_size];

                            signal_hat_f_chunk
                                .iter_mut()
                                .zip(results_i)
                                .for_each(|(c, r)| *c += *r);
                        }
                    }
                });

            // Allocate check potentials (implicitly in frequency order)
            let mut check_potentials_hat_f_buffer = vec![U::zero(); 2 * size_real * ntargets];
            let check_potentials_hat_f: &mut [Complex<U>];
            unsafe {
                let ptr = check_potentials_hat_f_buffer.as_mut_ptr() as *mut Complex<U>;
                check_potentials_hat_f = std::slice::from_raw_parts_mut(ptr, size_real * ntargets);
            }

            ////////////////////////////////////////////////////////////////////////////////////
            // M2L Kernel
            ////////////////////////////////////////////////////////////////////////////////////
            let scale = Complex::from(self.m2l_scale(level) * self.fmm.kernel.scale(level));
            let kernel_data_f = &self.fmm.m2l.operator_data.kernel_data_f;

            (0..size_real)
                .into_par_iter()
                .zip(signals_hat_f.par_chunks_exact(ntargets + nzeros))
                .zip(check_potentials_hat_f.par_chunks_exact_mut(ntargets))
                .for_each(|((freq, signal_hat_f), check_potential_hat_f)| {
                    (0..nparents).step_by(chunk_size).for_each(|chunk_start| {
                        let chunk_end = std::cmp::min(chunk_start + chunk_size, nparents);

                        let save_locations = &mut check_potential_hat_f
                            [chunk_start * nsiblings..chunk_end * nsiblings];

                        for (i, kernel_f) in kernel_data_f.iter().enumerate().take(26) {
                            let frequency_offset = 64 * freq;
                            let k_f = &kernel_f[frequency_offset..(frequency_offset + 64)].to_vec();

                            // Lookup signals
                            let displacements = &all_displacements[i][chunk_start..chunk_end];

                            for j in 0..(chunk_end - chunk_start) {
                                let displacement = displacements[j];
                                let s_f = &signal_hat_f[displacement..displacement + nsiblings];

                                matmul8x8(
                                    k_f,
                                    s_f,
                                    &mut save_locations[j * nsiblings..(j + 1) * nsiblings],
                                    scale,
                                )
                            }
                        }
                    });
                });

            ////////////////////////////////////////////////////////////////////////////////////
            // Post processing to find local expansions from check potentials
            ////////////////////////////////////////////////////////////////////////////////////

            // Get check potentials back into target order from frequency order
            let mut check_potential_hat = vec![U::zero(); size_real * ntargets * 2];
            let mut check_potential = vec![U::zero(); size * ntargets];
            let check_potential_hat_c;
            unsafe {
                let ptr = check_potential_hat.as_mut_ptr() as *mut Complex<U>;
                check_potential_hat_c = std::slice::from_raw_parts_mut(ptr, size_real * ntargets)
            }

            check_potential_hat_c
                .par_chunks_exact_mut(size_real)
                .enumerate()
                .for_each(|(i, check_potential_hat_chunk)| {
                    // Lookup all frequencies for this target box
                    for j in 0..size_real {
                        check_potential_hat_chunk[j] = check_potentials_hat_f[j * ntargets + i]
                    }
                });

            // Compute inverse FFT
            U::irfft3_fftw_par_slice(
                check_potential_hat_c,
                &mut check_potential,
                &[npad, npad, npad],
            );

            // TODO: Experiment with chunk size for post processing
            check_potential
                .par_chunks_exact(nsiblings * size)
                .zip(self.level_locals[level as usize].par_chunks_exact(nsiblings))
                .for_each(|(check_potential_chunk, local_ptrs)| {
                    // Map to surface grid
                    let mut potential_chunk = rlst_dynamic_array2!(U, [ncoeffs, nsiblings]);
                    for i in 0..nsiblings {
                        for (surf_idx, &conv_idx) in
                            self.fmm.m2l.conv_to_surf_map.iter().enumerate()
                        {
                            unsafe {
                                *potential_chunk.get_unchecked_mut([surf_idx, i]) =
                                    check_potential_chunk[i * size + conv_idx];
                            }
                        }
                    }

                    let local_chunk = empty_array::<U, 2>().simple_mult_into_resize(
                        self.fmm.dc2e_inv_1.view(),
                        empty_array::<U, 2>()
                            .simple_mult_into_resize(self.fmm.dc2e_inv_2.view(), potential_chunk),
                    );

                    local_chunk
                        .data()
                        .chunks_exact(ncoeffs)
                        .zip(local_ptrs)
                        .for_each(|(result, local)| {
                            let local =
                                unsafe { std::slice::from_raw_parts_mut(local.raw, ncoeffs) };
                            local.iter_mut().zip(result).for_each(|(l, r)| *l += *r);
                        });
                });
        }

        fn m2l_scale(&self, level: u64) -> U {
            if level < 2 {
                panic!("M2L only perfomed on level 2 and below")
            }

            if level == 2 {
                U::from(1. / 2.).unwrap()
            } else {
                let two = U::from(2.0).unwrap();
                Scalar::powf(two, U::from(level - 3).unwrap())
            }
        }
    }

    /// Implement the multipole to local translation operator for an SVD accelerated KiFMM on a single node.
    impl<T, U> FieldTranslation<U>
        for FmmDataAdaptive<KiFmmLinear<SingleNodeTree<U>, T, SvdFieldTranslationKiFmm<U, T>, U>, U>
    where
        T: Kernel<T = U>
            + ScaleInvariantKernel<T = U>
            + std::marker::Send
            + std::marker::Sync
            + Default,
        U: Scalar<Real = U>,
        U: Float + Default,
        U: std::marker::Send + std::marker::Sync + Default,
        Array<U, BaseArray<U, VectorContainer<U>, 2>, 2>: MatrixSvd<Item = U>,
    {
        fn p2l<'a>(&self, level: u64) {
            let Some(targets) = self.fmm.tree().get_keys(level) else {
                return;
            };

            let ncoeffs = self.fmm.m2l.ncoeffs(self.fmm.order);
            let dim = self.fmm.kernel().space_dimension();
            let surface_size = ncoeffs * dim;
            let min_idx = self.fmm.tree().key_to_index.get(&targets[0]).unwrap();
            let max_idx = self
                .fmm
                .tree()
                .key_to_index
                .get(targets.last().unwrap())
                .unwrap();
            let downward_surfaces =
                &self.downward_surfaces[min_idx * surface_size..(max_idx + 1) * surface_size];
            let coordinates = self.fmm.tree().get_all_coordinates().unwrap();

            let ntargets = targets.len();
            let mut check_potentials = vec![U::zero(); ncoeffs * ntargets];

            // 1. Compute check potentials from x list of each target
            targets
                .par_iter()
                .zip(downward_surfaces.par_chunks_exact(surface_size))
                .zip(check_potentials.par_chunks_exact_mut(ncoeffs))
                .for_each(|((target, downward_surface), check_potential)| {
                    // Find check potential
                    if let Some(x_list) = self.fmm.get_x_list(target) {
                        let x_list_indices = x_list
                            .iter()
                            .filter_map(|k| self.fmm.tree().get_leaf_index(k));
                        let charges = x_list_indices
                            .clone()
                            .map(|&idx| {
                                let index_pointer = &self.charge_index_pointer[idx];
                                &self.charges[index_pointer.0..index_pointer.1]
                            })
                            .collect_vec();

                        let sources_coordinates = x_list_indices
                            .into_iter()
                            .map(|&idx| {
                                let index_pointer = &self.charge_index_pointer[idx];
                                &coordinates[index_pointer.0 * dim..index_pointer.1 * dim]
                            })
                            .collect_vec();

                        for (&charges, sources) in charges.iter().zip(sources_coordinates) {
                            let nsources = sources.len() / dim;

                            if nsources > 0 {
                                self.fmm.kernel.evaluate_st(
                                    EvalType::Value,
                                    &sources,
                                    downward_surface,
                                    charges,
                                    check_potential,
                                );
                            }
                        }
                    }
                });

            // 2. Compute local expansion from check potential
            self.level_locals[level as usize]
                .par_iter()
                .zip(check_potentials.par_chunks_exact(ncoeffs))
                .for_each(|(local_ptr, check_potential)| {
                    let target_local =
                        unsafe { std::slice::from_raw_parts_mut(local_ptr.raw, ncoeffs) };

                    // TODO: remove memory assignment
                    let mut check_potential_mat = rlst_dynamic_array2!(U, [ncoeffs, 1]);
                    for i in 0..ncoeffs {
                        unsafe {
                            *check_potential_mat.get_unchecked_mut([i, 0]) = check_potential[i];
                        }
                    }

                    let scale = self.fmm.kernel().scale(level);
                    let mut tmp = empty_array::<U, 2>().simple_mult_into_resize(
                        self.fmm.dc2e_inv_1.view(),
                        empty_array::<U, 2>().simple_mult_into_resize(
                            self.fmm.dc2e_inv_2.view(),
                            check_potential_mat,
                        ),
                    );
                    tmp.data_mut().iter_mut().for_each(|val| *val *= scale);

                    target_local
                        .iter_mut()
                        .zip(tmp.data())
                        .for_each(|(r, &t)| *r += t);
                });
        }

        fn m2l<'a>(&self, level: u64) {
            let Some(sources) = self.fmm.tree().get_keys(level) else {
                return;
            };

            let nsources = sources.len();

            let mut source_map = HashMap::new();

            for (i, t) in sources.iter().enumerate() {
                source_map.insert(t, i);
            }

            let mut target_indices = vec![vec![-1i64; nsources]; 316];

            // Need to identify all save locations in a pre-processing step.
            for (j, source) in sources.iter().enumerate() {
                let v_list = source
                    .parent()
                    .neighbors()
                    .iter()
                    .flat_map(|pn| pn.children())
                    .filter(|pnc| {
                        !source.is_adjacent(pnc) && self.fmm.tree().get_all_keys_set().contains(pnc)
                    })
                    .collect_vec();

                let transfer_vectors = v_list
                    .iter()
                    .map(|target| target.find_transfer_vector(source))
                    .collect_vec();

                let mut transfer_vectors_map = HashMap::new();
                for (i, v) in transfer_vectors.iter().enumerate() {
                    transfer_vectors_map.insert(v, i);
                }

                let transfer_vectors_set: HashSet<_> = transfer_vectors.iter().collect();

                for (i, tv) in self.fmm.m2l.transfer_vectors.iter().enumerate() {
                    if transfer_vectors_set.contains(&tv.hash) {
                        let target = &v_list[*transfer_vectors_map.get(&tv.hash).unwrap()];
                        let target_index = source_map.get(target).unwrap();
                        target_indices[i][j] = *target_index as i64;
                    }
                }
            }

            // Interpret multipoles as a matrix
            let ncoeffs = self.fmm.m2l.ncoeffs(self.fmm.order);
            let mut multipoles = rlst_dynamic_array2!(U, [ncoeffs, nsources]);
            for j in 0..nsources {
                for i in 0..ncoeffs {
                    unsafe {
                        *multipoles.get_unchecked_mut([i, j]) = *self.level_multipoles
                            [level as usize][0]
                            .raw
                            .add(j * ncoeffs + i);
                    }
                }
            }

            let [nrows, _] = self.fmm.m2l.operator_data.c.shape();
            let dim = [nrows, self.fmm.m2l.k];

            let mut compressed_multipoles = empty_array::<U, 2>().simple_mult_into_resize(
                self.fmm.m2l.operator_data.st_block.view(),
                multipoles.view(),
            );

            compressed_multipoles
                .data_mut()
                .iter_mut()
                .for_each(|d| *d *= self.fmm.kernel.scale(level) * self.m2l_scale(level));

            (0..316).into_par_iter().for_each(|c_idx| {
                let top_left = [0, c_idx * self.fmm.m2l.k];
                let c_sub = self
                    .fmm
                    .m2l
                    .operator_data
                    .c
                    .view()
                    .into_subview(top_left, dim);

                let locals = empty_array::<U, 2>().simple_mult_into_resize(
                    self.fmm.dc2e_inv_1.view(),
                    empty_array::<U, 2>().simple_mult_into_resize(
                        self.fmm.dc2e_inv_2.view(),
                        empty_array::<U, 2>().simple_mult_into_resize(
                            self.fmm.m2l.operator_data.u.view(),
                            empty_array::<U, 2>().simple_mult_into_resize(
                                c_sub.view(),
                                compressed_multipoles.view(),
                            ),
                        ),
                    ),
                );

                let displacements = &target_indices[c_idx];

                for (result_idx, &save_idx) in displacements.iter().enumerate() {
                    if save_idx > -1 {
                        let save_idx = save_idx as usize;
                        let local_ptr = self.level_locals[(level) as usize][save_idx].raw;
                        let local = unsafe { std::slice::from_raw_parts_mut(local_ptr, ncoeffs) };

                        let res = &locals.data()[result_idx * ncoeffs..(result_idx + 1) * ncoeffs];
                        local.iter_mut().zip(res).for_each(|(l, r)| *l += *r);
                    }
                }
            })
        }

        fn m2l_scale(&self, level: u64) -> U {
            if level < 2 {
                panic!("M2L only perfomed on level 2 and below")
            }

            if level == 2 {
                U::from(1. / 2.).unwrap()
            } else {
                let two = U::from(2.0).unwrap();
                Scalar::powf(two, U::from(level - 3).unwrap())
            }
        }
    }
}
