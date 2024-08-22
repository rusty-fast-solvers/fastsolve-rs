//! Trait definitions

mod assembly;
mod function;

#[cfg(feature = "mpi")]
pub use assembly::ParallelBoundaryAssembly;
pub use assembly::{
    BoundaryAssembly, BoundaryIntegrand, CellAssembler, CellGeometry, CellPairAssembler,
    KernelEvaluator, PotentialAssembly, PotentialIntegrand,
};
pub use function::FunctionSpace;
#[cfg(feature = "mpi")]
pub use function::ParallelFunctionSpace;
