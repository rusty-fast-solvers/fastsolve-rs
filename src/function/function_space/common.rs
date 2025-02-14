//! Serial function space

use ndelement::ciarlet::CiarletElement;
use ndelement::traits::{ElementFamily, FiniteElement};
use ndelement::types::ReferenceCellType;
use ndgrid::{
    traits::{Entity, Grid, Topology},
    types::Ownership,
};
use rlst::{MatrixInverse, RlstScalar};
use std::collections::HashMap;

type DofList = Vec<Vec<usize>>;
type OwnerData = Vec<(usize, usize, usize, usize)>;

pub(crate) fn assign_dofs<
    T: RlstScalar + MatrixInverse,
    GridImpl: Grid<T = T::Real, EntityDescriptor = ReferenceCellType> + Sync,
>(
    rank: usize,
    grid: &GridImpl,
    e_family: &impl ElementFamily<
        T = T,
        FiniteElement = CiarletElement<T>,
        CellType = ReferenceCellType,
    >,
) -> (DofList, [DofList; 4], usize, OwnerData) {
    let mut size = 0;
    let mut entity_dofs: [Vec<Vec<usize>>; 4] = [vec![], vec![], vec![], vec![]];
    let mut owner_data = vec![];
    let tdim = grid.topology_dim();

    let mut elements = HashMap::new();
    let mut element_dims = HashMap::new();
    for cell in grid.entity_types(2) {
        elements.insert(*cell, e_family.element(*cell));
        element_dims.insert(*cell, elements[cell].dim());
    }

    let entity_counts = (0..tdim + 1)
        .map(|d| {
            grid.entity_types(d)
                .iter()
                .map(|&i| grid.entity_count(i))
                .sum::<usize>()
        })
        .collect::<Vec<_>>();
    if tdim > 2 {
        unimplemented!("DOF maps not implemented for cells with tdim > 2.");
    }

    for d in 0..tdim + 1 {
        entity_dofs[d] = vec![vec![]; entity_counts[d]];
    }
    let mut cell_dofs = vec![vec![]; entity_counts[tdim]];

    let mut max_rank = rank;
    for cell in grid.entity_iter(tdim) {
        if let Ownership::Ghost(process, _index) = cell.ownership() {
            if process > max_rank {
                max_rank = process;
            }
        }
    }
    for cell in grid.entity_iter(tdim) {
        cell_dofs[cell.local_index()] = vec![0; element_dims[&cell.entity_type()]];
        let element = &elements[&cell.entity_type()];
        let topology = cell.topology();

        // Assign DOFs to entities
        for (d, edofs_d) in entity_dofs.iter_mut().take(tdim + 1).enumerate() {
            for (i, e) in topology.sub_entity_iter(d).enumerate() {
                let e_dofs = element.entity_dofs(d, i).unwrap();
                if !e_dofs.is_empty() {
                    if edofs_d[e].is_empty() {
                        for (dof_i, _d) in e_dofs.iter().enumerate() {
                            edofs_d[e].push(size);
                            if let Ownership::Ghost(process, index) =
                                grid.entity(d, e).unwrap().ownership()
                            {
                                owner_data.push((process, d, index, dof_i));
                            } else {
                                owner_data.push((rank, d, e, dof_i));
                            }
                            size += 1;
                        }
                    }
                    for (local_dof, dof) in e_dofs.iter().zip(&edofs_d[e]) {
                        cell_dofs[cell.local_index()][*local_dof] = *dof;
                    }
                }
            }
        }
    }
    (cell_dofs, entity_dofs, size, owner_data)
}
