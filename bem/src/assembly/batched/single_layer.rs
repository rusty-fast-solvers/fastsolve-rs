//! Single layer assemblers
use super::{BatchedAssembler, EvalType, RlstArray};
use bempp_kernel::{helmholtz_3d::Helmholtz3dKernel, laplace_3d::Laplace3dKernel};
use bempp_traits::kernel::Kernel;
use rlst::{RlstScalar, UnsafeRandomAccessByRef};

/// Assembler for a Laplace single layer operator
pub struct LaplaceSingleLayerAssembler<const BATCHSIZE: usize, T: RlstScalar> {
    kernel: Laplace3dKernel<T>,
}
impl<const BATCHSIZE: usize, T: RlstScalar> Default for LaplaceSingleLayerAssembler<BATCHSIZE, T> {
    fn default() -> Self {
        Self {
            kernel: Laplace3dKernel::<T>::new(),
        }
    }
}
impl<const BATCHSIZE: usize, T: RlstScalar> BatchedAssembler
    for LaplaceSingleLayerAssembler<BATCHSIZE, T>
{
    const DERIV_SIZE: usize = 1;
    const TABLE_DERIVS: usize = 0;
    const BATCHSIZE: usize = BATCHSIZE;
    type T = T;
    unsafe fn singular_kernel_value(
        &self,
        k: &RlstArray<T, 2>,
        _test_normals: &RlstArray<T::Real, 2>,
        _trial_normals: &RlstArray<T::Real, 2>,
        index: usize,
    ) -> T {
        *k.get_unchecked([0, index])
    }
    unsafe fn nonsingular_kernel_value(
        &self,
        k: &RlstArray<T, 3>,
        _test_normals: &RlstArray<T::Real, 2>,
        _trial_normals: &RlstArray<T::Real, 2>,
        test_index: usize,
        trial_index: usize,
    ) -> T {
        *k.get_unchecked([test_index, 0, trial_index])
    }
    fn kernel_assemble_diagonal_st(
        &self,
        sources: &[T::Real],
        targets: &[T::Real],
        result: &mut [T],
    ) {
        self.kernel
            .assemble_diagonal_st(EvalType::Value, sources, targets, result);
    }
    fn kernel_assemble_st(&self, sources: &[T::Real], targets: &[T::Real], result: &mut [T]) {
        self.kernel
            .assemble_st(EvalType::Value, sources, targets, result);
    }
}

/// Assembler for a Helmholtz single layer boundary operator
pub struct HelmholtzSingleLayerAssembler<const BATCHSIZE: usize, T: RlstScalar<Complex = T>> {
    kernel: Helmholtz3dKernel<T>,
}
impl<const BATCHSIZE: usize, T: RlstScalar<Complex = T>>
    HelmholtzSingleLayerAssembler<BATCHSIZE, T>
{
    /// Create a new assembler
    pub fn new(wavenumber: T::Real) -> Self {
        Self {
            kernel: Helmholtz3dKernel::<T>::new(wavenumber),
        }
    }
}
impl<const BATCHSIZE: usize, T: RlstScalar<Complex = T>> BatchedAssembler
    for HelmholtzSingleLayerAssembler<BATCHSIZE, T>
{
    const DERIV_SIZE: usize = 1;
    const TABLE_DERIVS: usize = 0;
    const BATCHSIZE: usize = BATCHSIZE;
    type T = T;
    unsafe fn singular_kernel_value(
        &self,
        k: &RlstArray<T, 2>,
        _test_normals: &RlstArray<T::Real, 2>,
        _trial_normals: &RlstArray<T::Real, 2>,
        index: usize,
    ) -> T {
        *k.get_unchecked([0, index])
    }
    unsafe fn nonsingular_kernel_value(
        &self,
        k: &RlstArray<T, 3>,
        _test_normals: &RlstArray<T::Real, 2>,
        _trial_normals: &RlstArray<T::Real, 2>,
        test_index: usize,
        trial_index: usize,
    ) -> T {
        *k.get_unchecked([test_index, 0, trial_index])
    }
    fn kernel_assemble_diagonal_st(
        &self,
        sources: &[T::Real],
        targets: &[T::Real],
        result: &mut [T],
    ) {
        self.kernel
            .assemble_diagonal_st(EvalType::Value, sources, targets, result);
    }
    fn kernel_assemble_st(&self, sources: &[T::Real], targets: &[T::Real], result: &mut [T]) {
        self.kernel
            .assemble_st(EvalType::Value, sources, targets, result);
    }
}
