//! Functions to create simple example grids

use crate::flat_triangle_grid::{SerialFlatTriangleGrid, SerialFlatTriangleGridBuilder};
use crate::traits_impl::WrappedGrid;
use bempp_traits::grid::Builder;
use num::Float;
use rlst_dense::{
    array::{views::ArrayViewMut, Array},
    base_array::BaseArray,
    data_container::VectorContainer,
    traits::MatrixInverse,
    types::RlstScalar,
};
use std::collections::HashMap;

/// Create a regular sphere
///
/// A regular sphere is created by starting with a regular octahedron. The shape is then refined `refinement_level` times.
/// Each time the grid is refined, each triangle is split into four triangles (by adding lines connecting the midpoints of
/// each edge). The new points are then scaled so that they are a distance of 1 from the origin.
pub fn regular_sphere<T: Float + RlstScalar<Real = T>>(
    refinement_level: u32,
) -> WrappedGrid<SerialFlatTriangleGrid<T>>
where
    for<'a> Array<T, ArrayViewMut<'a, T, BaseArray<T, VectorContainer<T>, 2>, 2>, 2>: MatrixInverse,
{
    let mut b = SerialFlatTriangleGridBuilder::new_with_capacity(
        2 + 4 * usize::pow(6, refinement_level),
        8 * usize::pow(6, refinement_level),
        (),
    );
    let zero = T::from(0.0).unwrap();
    let one = T::from(1.0).unwrap();
    let half = T::from(0.5).unwrap();
    let three = T::from(3.0).unwrap();
    b.add_point(0, [zero, zero, one]);
    b.add_point(1, [one, zero, zero]);
    b.add_point(2, [zero, one, zero]);
    b.add_point(3, [-one, zero, zero]);
    b.add_point(4, [zero, -one, zero]);
    b.add_point(5, [zero, zero, -one]);
    let mut point_n = 6;

    let mut cells = vec![
        [0, 1, 2],
        [0, 2, 3],
        [0, 3, 4],
        [0, 4, 1],
        [5, 2, 1],
        [5, 3, 2],
        [5, 4, 3],
        [5, 1, 4],
    ];
    let mut v = [[zero, zero, zero], [zero, zero, zero], [zero, zero, zero]];

    for level in 0..refinement_level {
        let mut edge_points = HashMap::new();
        let mut new_cells = Vec::with_capacity(8 * usize::pow(6, level));
        for c in &cells {
            for i in 0..3 {
                for j in 0..3 {
                    v[i][j] = b.points[3 * c[i] + j];
                }
            }
            let edges = [[1, 2], [0, 2], [0, 1]]
                .iter()
                .map(|[i, j]| {
                    let mut pt_i = c[*i];
                    let mut pt_j = c[*j];
                    if pt_i > pt_j {
                        std::mem::swap(&mut pt_i, &mut pt_j);
                    }
                    if !edge_points.contains_key(&(pt_i, pt_j)) {
                        let v_i = v[*i];
                        let v_j = v[*j];
                        let mut new_pt = [
                            half * (v_i[0] + v_j[0]),
                            half * (v_i[1] + v_j[1]),
                            half * (v_i[2] + v_j[2]),
                        ];
                        let size =
                            Float::sqrt(new_pt.iter().map(|x| Float::powi(*x, 2)).sum::<T>());
                        for i in new_pt.iter_mut() {
                            *i /= size;
                        }
                        b.add_point(point_n, new_pt);
                        edge_points.insert((pt_i, pt_j), point_n);
                        point_n += 1;
                    }
                    edge_points[&(pt_i, pt_j)]
                })
                .collect::<Vec<_>>();
            let mid = point_n;
            let mut new_pt = [
                (v[0][0] + v[1][0] + v[2][0]) / three,
                (v[0][1] + v[1][1] + v[2][1]) / three,
                (v[0][2] + v[1][2] + v[2][2]) / three,
            ];
            let size = Float::sqrt(new_pt.iter().map(|x| Float::powi(*x, 2)).sum::<T>());
            for i in new_pt.iter_mut() {
                *i /= size;
            }
            b.add_point(point_n, new_pt);
            point_n += 1;
            new_cells.push([c[0], edges[2], mid]);
            new_cells.push([c[0], mid, edges[1]]);
            new_cells.push([c[1], edges[0], mid]);
            new_cells.push([c[1], mid, edges[2]]);
            new_cells.push([c[2], edges[1], mid]);
            new_cells.push([c[2], mid, edges[0]]);
        }
        cells = new_cells;
    }
    for (i, v) in cells.iter().enumerate() {
        b.add_cell(i, *v);
    }

    b.create_grid()
}

#[cfg(test)]
mod test {
    use crate::shapes::*;
    use bempp_traits::grid::{GridType, ReferenceMapType};

    #[test]
    fn test_regular_sphere_0() {
        let _g = regular_sphere::<f64>(0);
    }

    #[test]
    fn test_regular_spheres() {
        let _g1 = regular_sphere::<f64>(1);
        let _g2 = regular_sphere::<f64>(2);
        let _g3 = regular_sphere::<f64>(3);
    }

    #[test]
    fn test_normal_is_outward() {
        for i in 0..3 {
            let g = regular_sphere::<f64>(i);
            let points = vec![1.0 / 3.0, 1.0 / 3.0];
            let map = g.reference_to_physical_map(&points);
            let mut mapped_pt = vec![0.0; 3];
            let mut normal = vec![0.0; 3];
            for i in 0..g.number_of_cells() {
                map.reference_to_physical(i, 0, &mut mapped_pt);
                map.normal(i, 0, &mut normal);
                let dot = mapped_pt
                    .iter()
                    .zip(&normal)
                    .map(|(i, j)| i * j)
                    .sum::<f64>();
                assert!(dot > 0.0);
            }
        }
    }
}
