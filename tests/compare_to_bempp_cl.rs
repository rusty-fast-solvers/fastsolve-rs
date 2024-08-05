use approx::*;
use bempp::assembly::{batched, batched::BatchedAssembler, batched::BatchedPotentialAssembler};
use bempp::function::SerialFunctionSpace;
use bempp::traits::function::FunctionSpace;
use cauchy::c64;
use ndelement::ciarlet::LagrangeElementFamily;
use ndelement::types::Continuity;
use ndgrid::shapes::regular_sphere;
use rlst::{rlst_dynamic_array2, RandomAccessByRef, RandomAccessMut};

extern crate blas_src;
extern crate lapack_src;

#[test]
fn test_laplace_single_layer_dp0_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<f64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();

    let mut matrix = rlst_dynamic_array2!(f64, [ndofs, ndofs]);

    let a = batched::LaplaceSingleLayerAssembler::<f64>::default();
    a.assemble_into_dense(&mut matrix, &space, &space);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[0.1854538822982487, 0.08755414595678074, 0.05963897421514472, 0.08755414595678074, 0.08755414595678074, 0.05963897421514473, 0.04670742127454548, 0.05963897421514472], [0.08755414595678074, 0.1854538822982487, 0.08755414595678074, 0.05963897421514472, 0.05963897421514472, 0.08755414595678074, 0.05963897421514473, 0.04670742127454548], [0.05963897421514472, 0.08755414595678074, 0.1854538822982487, 0.08755414595678074, 0.04670742127454548, 0.05963897421514472, 0.08755414595678074, 0.05963897421514473], [0.08755414595678074, 0.05963897421514472, 0.08755414595678074, 0.1854538822982487, 0.05963897421514473, 0.04670742127454548, 0.05963897421514472, 0.08755414595678074], [0.08755414595678074, 0.05963897421514472, 0.046707421274545476, 0.05963897421514473, 0.1854538822982487, 0.08755414595678074, 0.05963897421514472, 0.08755414595678074], [0.05963897421514473, 0.08755414595678074, 0.05963897421514472, 0.046707421274545476, 0.08755414595678074, 0.1854538822982487, 0.08755414595678074, 0.05963897421514472], [0.046707421274545476, 0.05963897421514473, 0.08755414595678074, 0.05963897421514472, 0.05963897421514472, 0.08755414595678074, 0.1854538822982487, 0.08755414595678074], [0.05963897421514472, 0.046707421274545476, 0.05963897421514473, 0.08755414595678074, 0.08755414595678074, 0.05963897421514472, 0.08755414595678074, 0.1854538822982487]];

    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            assert_relative_eq!(*matrix.get([i, j]).unwrap(), entry, epsilon = 1e-3);
        }
    }
}

#[test]
fn test_laplace_double_layer_dp0_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<f64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();

    let mut matrix = rlst_dynamic_array2!(f64, [ndofs, ndofs]);
    let a = batched::LaplaceDoubleLayerAssembler::<f64>::default();
    a.assemble_into_dense(&mut matrix, &space, &space);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[-1.9658941517361406e-33, -0.08477786720045567, -0.048343860959178774, -0.08477786720045567, -0.08477786720045566, -0.048343860959178774, -0.033625570841778946, -0.04834386095917877], [-0.08477786720045567, -1.9658941517361406e-33, -0.08477786720045567, -0.048343860959178774, -0.04834386095917877, -0.08477786720045566, -0.048343860959178774, -0.033625570841778946], [-0.048343860959178774, -0.08477786720045567, -1.9658941517361406e-33, -0.08477786720045567, -0.033625570841778946, -0.04834386095917877, -0.08477786720045566, -0.048343860959178774], [-0.08477786720045567, -0.048343860959178774, -0.08477786720045567, -1.9658941517361406e-33, -0.048343860959178774, -0.033625570841778946, -0.04834386095917877, -0.08477786720045566], [-0.08477786720045566, -0.04834386095917877, -0.033625570841778946, -0.04834386095917877, 4.910045345075783e-33, -0.08477786720045566, -0.048343860959178774, -0.08477786720045566], [-0.04834386095917877, -0.08477786720045566, -0.04834386095917877, -0.033625570841778946, -0.08477786720045566, 4.910045345075783e-33, -0.08477786720045566, -0.048343860959178774], [-0.033625570841778946, -0.04834386095917877, -0.08477786720045566, -0.04834386095917877, -0.048343860959178774, -0.08477786720045566, 4.910045345075783e-33, -0.08477786720045566], [-0.04834386095917877, -0.033625570841778946, -0.04834386095917877, -0.08477786720045566, -0.08477786720045566, -0.048343860959178774, -0.08477786720045566, 4.910045345075783e-33]];

    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            println!("{} {entry}", *matrix.get([i, j]).unwrap());
        }
        println!();
    }
    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            assert_relative_eq!(*matrix.get([i, j]).unwrap(), entry, epsilon = 1e-4);
        }
    }
}
/*
#[test]
fn test_laplace_adjoint_double_layer_dp0_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<f64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();

    let mut matrix = rlst_dynamic_array2!(f64, [ndofs, ndofs]);
    let a = batched::LaplaceAdjointDoubleLayerAssembler::<f64>::default();
    a.assemble_into_dense(&mut matrix, &space, &space);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[1.9658941517361406e-33, -0.08478435261011981, -0.048343860959178774, -0.0847843526101198, -0.08478435261011981, -0.04834386095917877, -0.033625570841778946, -0.048343860959178774], [-0.0847843526101198, 1.9658941517361406e-33, -0.08478435261011981, -0.048343860959178774, -0.048343860959178774, -0.08478435261011981, -0.04834386095917877, -0.033625570841778946], [-0.048343860959178774, -0.0847843526101198, 1.9658941517361406e-33, -0.08478435261011981, -0.033625570841778946, -0.048343860959178774, -0.08478435261011981, -0.04834386095917877], [-0.08478435261011981, -0.048343860959178774, -0.0847843526101198, 1.9658941517361406e-33, -0.04834386095917877, -0.033625570841778946, -0.048343860959178774, -0.08478435261011981], [-0.0847843526101198, -0.04834386095917877, -0.033625570841778946, -0.04834386095917877, -4.910045345075783e-33, -0.0847843526101198, -0.048343860959178774, -0.08478435261011981], [-0.04834386095917877, -0.0847843526101198, -0.04834386095917877, -0.033625570841778946, -0.08478435261011981, -4.910045345075783e-33, -0.0847843526101198, -0.048343860959178774], [-0.033625570841778946, -0.04834386095917877, -0.0847843526101198, -0.04834386095917877, -0.048343860959178774, -0.08478435261011981, -4.910045345075783e-33, -0.0847843526101198], [-0.04834386095917877, -0.033625570841778946, -0.04834386095917877, -0.0847843526101198, -0.0847843526101198, -0.048343860959178774, -0.08478435261011981, -4.910045345075783e-33]];

    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            assert_relative_eq!(*matrix.get([i, j]).unwrap(), entry, epsilon = 1e-4);
        }
    }
}
*/

#[test]
fn test_laplace_hypersingular_dp0_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<f64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();

    let mut matrix = rlst_dynamic_array2!(f64, [ndofs, ndofs]);
    let a = batched::LaplaceHypersingularAssembler::<f64>::default();
    a.assemble_into_dense(&mut matrix, &space, &space);

    for i in 0..ndofs {
        for j in 0..ndofs {
            assert_relative_eq!(*matrix.get([i, j]).unwrap(), 0.0, epsilon = 1e-4);
        }
    }
}

#[test]
fn test_laplace_hypersingular_p1_p1() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<f64>::new(1, Continuity::Standard);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();

    let mut matrix = rlst_dynamic_array2!(f64, [ndofs, ndofs]);
    let a = batched::LaplaceHypersingularAssembler::<f64>::default();
    a.assemble_into_dense(&mut matrix, &space, &space);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[0.33550642155494004, -0.10892459915262698, -0.05664545560057827, -0.05664545560057828, -0.0566454556005783, -0.05664545560057828], [-0.10892459915262698, 0.33550642155494004, -0.05664545560057828, -0.05664545560057827, -0.05664545560057828, -0.05664545560057829], [-0.05664545560057828, -0.05664545560057827, 0.33550642155494004, -0.10892459915262698, -0.056645455600578286, -0.05664545560057829], [-0.05664545560057827, -0.05664545560057828, -0.10892459915262698, 0.33550642155494004, -0.05664545560057828, -0.056645455600578286], [-0.05664545560057829, -0.0566454556005783, -0.05664545560057829, -0.05664545560057829, 0.33550642155494004, -0.10892459915262698], [-0.05664545560057829, -0.05664545560057831, -0.05664545560057829, -0.05664545560057829, -0.10892459915262698, 0.33550642155494004]];

    let perm = [0, 5, 2, 4, 3, 1];

    for (i, pi) in perm.iter().enumerate() {
        for (j, pj) in perm.iter().enumerate() {
            assert_relative_eq!(
                *matrix.get([i, j]).unwrap(),
                from_cl[*pi][*pj],
                epsilon = 1e-4
            );
        }
    }
}

#[test]
fn test_helmholtz_single_layer_dp0_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<c64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();
    let mut matrix = rlst_dynamic_array2!(c64, [ndofs, ndofs]);

    let a = batched::HelmholtzSingleLayerAssembler::<c64>::new(3.0);
    a.assemble_into_dense(&mut matrix, &space, &space);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[c64::new(0.08742460357596939, 0.11004203436820102), c64::new(-0.02332791148192136, 0.04919102584271124), c64::new(-0.04211947809894265, 0.003720159902487029), c64::new(-0.02332791148192136, 0.04919102584271125), c64::new(-0.023327911481921364, 0.04919102584271124), c64::new(-0.042119478098942634, 0.003720159902487025), c64::new(-0.03447046598405515, -0.02816544680626108), c64::new(-0.04211947809894265, 0.0037201599024870254)], [c64::new(-0.023327911481921364, 0.04919102584271125), c64::new(0.08742460357596939, 0.11004203436820104), c64::new(-0.02332791148192136, 0.04919102584271124), c64::new(-0.04211947809894265, 0.0037201599024870263), c64::new(-0.04211947809894265, 0.0037201599024870254), c64::new(-0.02332791148192136, 0.04919102584271125), c64::new(-0.042119478098942634, 0.003720159902487025), c64::new(-0.03447046598405515, -0.028165446806261072)], [c64::new(-0.04211947809894265, 0.003720159902487029), c64::new(-0.02332791148192136, 0.04919102584271125), c64::new(0.08742460357596939, 0.11004203436820102), c64::new(-0.02332791148192136, 0.04919102584271124), c64::new(-0.03447046598405515, -0.02816544680626108), c64::new(-0.04211947809894265, 0.0037201599024870254), c64::new(-0.023327911481921364, 0.04919102584271124), c64::new(-0.042119478098942634, 0.003720159902487025)], [c64::new(-0.02332791148192136, 0.04919102584271124), c64::new(-0.04211947809894265, 0.0037201599024870263), c64::new(-0.023327911481921364, 0.04919102584271125), c64::new(0.08742460357596939, 0.11004203436820104), c64::new(-0.042119478098942634, 0.003720159902487025), c64::new(-0.03447046598405515, -0.028165446806261072), c64::new(-0.04211947809894265, 0.0037201599024870254), c64::new(-0.02332791148192136, 0.04919102584271125)], [c64::new(-0.023327911481921364, 0.04919102584271125), c64::new(-0.04211947809894265, 0.0037201599024870263), c64::new(-0.03447046598405515, -0.02816544680626108), c64::new(-0.042119478098942634, 0.003720159902487025), c64::new(0.08742460357596939, 0.11004203436820104), c64::new(-0.02332791148192136, 0.04919102584271124), c64::new(-0.04211947809894265, 0.0037201599024870267), c64::new(-0.023327911481921364, 0.04919102584271125)], [c64::new(-0.042119478098942634, 0.003720159902487025), c64::new(-0.02332791148192136, 0.04919102584271125), c64::new(-0.04211947809894265, 0.0037201599024870263), c64::new(-0.034470465984055156, -0.028165446806261075), c64::new(-0.02332791148192136, 0.04919102584271124), c64::new(0.08742460357596939, 0.11004203436820104), c64::new(-0.023327911481921364, 0.04919102584271125), c64::new(-0.04211947809894265, 0.0037201599024870237)], [c64::new(-0.03447046598405515, -0.02816544680626108), c64::new(-0.042119478098942634, 0.003720159902487025), c64::new(-0.023327911481921364, 0.04919102584271125), c64::new(-0.04211947809894265, 0.0037201599024870263), c64::new(-0.04211947809894265, 0.0037201599024870267), c64::new(-0.023327911481921364, 0.04919102584271125), c64::new(0.08742460357596939, 0.11004203436820104), c64::new(-0.02332791148192136, 0.04919102584271124)], [c64::new(-0.04211947809894265, 0.0037201599024870263), c64::new(-0.034470465984055156, -0.028165446806261075), c64::new(-0.042119478098942634, 0.003720159902487025), c64::new(-0.02332791148192136, 0.04919102584271125), c64::new(-0.023327911481921364, 0.04919102584271125), c64::new(-0.04211947809894265, 0.0037201599024870237), c64::new(-0.02332791148192136, 0.04919102584271124), c64::new(0.08742460357596939, 0.11004203436820104)]];

    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            assert_relative_eq!(*matrix.get([i, j]).unwrap(), entry, epsilon = 1e-4);
        }
    }
}

/*
#[test]
fn test_helmholtz_double_layer_dp0_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<c64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();
    let mut matrix = rlst_dynamic_array2!(c64, [ndofs, ndofs]);

    let a = batched::HelmholtzDoubleLayerAssembler::<c64>::new(3.0);
    a.assemble_into_dense(&mut matrix, &space, &space);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[c64::new(-1.025266688854119e-33, -7.550086433767158e-36), c64::new(-0.07902626473768169, -0.08184681047051735), c64::new(0.01906923918000321, -0.10276858786959298), c64::new(-0.07902626473768172, -0.08184681047051737), c64::new(-0.07902626473768169, -0.08184681047051737), c64::new(0.01906923918000323, -0.10276858786959302), c64::new(0.10089706509966115, -0.07681163409722505), c64::new(0.019069239180003215, -0.10276858786959299)], [c64::new(-0.07902626473768172, -0.08184681047051737), c64::new(-1.025266688854119e-33, 1.0291684702482414e-35), c64::new(-0.0790262647376817, -0.08184681047051737), c64::new(0.019069239180003212, -0.10276858786959299), c64::new(0.019069239180003212, -0.10276858786959298), c64::new(-0.07902626473768168, -0.08184681047051737), c64::new(0.01906923918000323, -0.10276858786959299), c64::new(0.10089706509966115, -0.07681163409722506)], [c64::new(0.01906923918000321, -0.10276858786959298), c64::new(-0.07902626473768172, -0.08184681047051737), c64::new(-1.025266688854119e-33, -7.550086433767158e-36), c64::new(-0.07902626473768169, -0.08184681047051735), c64::new(0.10089706509966115, -0.07681163409722505), c64::new(0.019069239180003215, -0.10276858786959299), c64::new(-0.07902626473768169, -0.08184681047051737), c64::new(0.01906923918000323, -0.10276858786959302)], [c64::new(-0.0790262647376817, -0.08184681047051737), c64::new(0.019069239180003212, -0.10276858786959299), c64::new(-0.07902626473768172, -0.08184681047051737), c64::new(-1.025266688854119e-33, 1.0291684702482414e-35), c64::new(0.01906923918000323, -0.10276858786959299), c64::new(0.10089706509966115, -0.07681163409722506), c64::new(0.019069239180003212, -0.10276858786959298), c64::new(-0.07902626473768168, -0.08184681047051737)], [c64::new(-0.07902626473768172, -0.08184681047051737), c64::new(0.019069239180003215, -0.10276858786959298), c64::new(0.10089706509966115, -0.07681163409722505), c64::new(0.01906923918000323, -0.10276858786959299), c64::new(5.00373588753262e-33, -1.8116810507789718e-36), c64::new(-0.07902626473768169, -0.08184681047051735), c64::new(0.019069239180003212, -0.10276858786959299), c64::new(-0.07902626473768169, -0.08184681047051737)], [c64::new(0.019069239180003222, -0.10276858786959299), c64::new(-0.07902626473768173, -0.08184681047051737), c64::new(0.01906923918000322, -0.10276858786959299), c64::new(0.10089706509966115, -0.07681163409722506), c64::new(-0.07902626473768169, -0.08184681047051735), c64::new(7.314851820797302e-33, -1.088140415641433e-35), c64::new(-0.07902626473768169, -0.08184681047051737), c64::new(0.01906923918000322, -0.10276858786959299)], [c64::new(0.10089706509966115, -0.07681163409722505), c64::new(0.01906923918000323, -0.10276858786959299), c64::new(-0.07902626473768172, -0.08184681047051737), c64::new(0.019069239180003215, -0.10276858786959298), c64::new(0.019069239180003212, -0.10276858786959299), c64::new(-0.07902626473768169, -0.08184681047051737), c64::new(5.00373588753262e-33, -1.8116810507789718e-36), c64::new(-0.07902626473768169, -0.08184681047051735)], [c64::new(0.01906923918000322, -0.10276858786959299), c64::new(0.10089706509966115, -0.07681163409722506), c64::new(0.019069239180003222, -0.10276858786959299), c64::new(-0.07902626473768173, -0.08184681047051737), c64::new(-0.07902626473768169, -0.08184681047051737), c64::new(0.01906923918000322, -0.10276858786959299), c64::new(-0.07902626473768169, -0.08184681047051735), c64::new(7.314851820797302e-33, -1.088140415641433e-35)]];

    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            assert_relative_eq!(matrix.get([i, j]).unwrap(), entry, epsilon = 1e-4);
        }
    }
}
#[test]
fn test_helmholtz_adjoint_double_layer_dp0_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<c64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();
    let mut matrix = rlst_dynamic_array2!(c64, [ndofs, ndofs]);

    let a = batched::HelmholtzAdjointDoubleLayerAssembler::<c64>::new(3.0);
    a.assemble_into_dense(&mut matrix, &space, &space);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[c64::new(1.025266688854119e-33, 7.550086433767158e-36), c64::new(-0.079034545070751, -0.08184700030244885), c64::new(0.019069239180003205, -0.10276858786959298), c64::new(-0.07903454507075097, -0.08184700030244886), c64::new(-0.07903454507075099, -0.08184700030244887), c64::new(0.01906923918000323, -0.10276858786959299), c64::new(0.10089706509966115, -0.07681163409722505), c64::new(0.019069239180003212, -0.10276858786959298)], [c64::new(-0.07903454507075097, -0.08184700030244885), c64::new(1.025266688854119e-33, -1.0291684702482414e-35), c64::new(-0.079034545070751, -0.08184700030244887), c64::new(0.01906923918000321, -0.10276858786959298), c64::new(0.01906923918000321, -0.10276858786959298), c64::new(-0.07903454507075099, -0.08184700030244887), c64::new(0.019069239180003233, -0.10276858786959299), c64::new(0.10089706509966115, -0.07681163409722506)], [c64::new(0.019069239180003205, -0.10276858786959298), c64::new(-0.07903454507075097, -0.08184700030244886), c64::new(1.025266688854119e-33, 7.550086433767158e-36), c64::new(-0.079034545070751, -0.08184700030244885), c64::new(0.10089706509966115, -0.07681163409722505), c64::new(0.019069239180003212, -0.10276858786959298), c64::new(-0.07903454507075099, -0.08184700030244887), c64::new(0.01906923918000323, -0.10276858786959299)], [c64::new(-0.079034545070751, -0.08184700030244887), c64::new(0.01906923918000321, -0.10276858786959298), c64::new(-0.07903454507075097, -0.08184700030244885), c64::new(1.025266688854119e-33, -1.0291684702482414e-35), c64::new(0.019069239180003233, -0.10276858786959299), c64::new(0.10089706509966115, -0.07681163409722506), c64::new(0.01906923918000321, -0.10276858786959298), c64::new(-0.07903454507075099, -0.08184700030244887)], [c64::new(-0.07903454507075099, -0.08184700030244887), c64::new(0.01906923918000321, -0.10276858786959298), c64::new(0.10089706509966115, -0.07681163409722505), c64::new(0.01906923918000323, -0.10276858786959302), c64::new(-5.00373588753262e-33, 1.8116810507789718e-36), c64::new(-0.07903454507075099, -0.08184700030244885), c64::new(0.01906923918000321, -0.10276858786959298), c64::new(-0.07903454507075099, -0.08184700030244886)], [c64::new(0.019069239180003233, -0.10276858786959302), c64::new(-0.07903454507075099, -0.08184700030244886), c64::new(0.019069239180003212, -0.10276858786959298), c64::new(0.10089706509966115, -0.07681163409722506), c64::new(-0.07903454507075099, -0.08184700030244885), c64::new(-7.314851820797302e-33, 1.088140415641433e-35), c64::new(-0.07903454507075099, -0.08184700030244886), c64::new(0.019069239180003215, -0.10276858786959298)], [c64::new(0.10089706509966115, -0.07681163409722505), c64::new(0.01906923918000323, -0.10276858786959302), c64::new(-0.07903454507075099, -0.08184700030244887), c64::new(0.01906923918000321, -0.10276858786959298), c64::new(0.01906923918000321, -0.10276858786959298), c64::new(-0.07903454507075099, -0.08184700030244886), c64::new(-5.00373588753262e-33, 1.8116810507789718e-36), c64::new(-0.07903454507075099, -0.08184700030244885)], [c64::new(0.019069239180003212, -0.10276858786959298), c64::new(0.10089706509966115, -0.07681163409722506), c64::new(0.019069239180003233, -0.10276858786959302), c64::new(-0.07903454507075099, -0.08184700030244886), c64::new(-0.07903454507075099, -0.08184700030244886), c64::new(0.019069239180003215, -0.10276858786959298), c64::new(-0.07903454507075099, -0.08184700030244885), c64::new(-7.314851820797302e-33, 1.088140415641433e-35)]];

    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            assert_relative_eq!(matrix.get([i, j]).unwrap(), entry, epsilon = 1e-4);
        }
    }
}
*/

#[test]
fn test_helmholtz_hypersingular_p1_p1() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<c64>::new(1, Continuity::Standard);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();
    let mut matrix = rlst_dynamic_array2!(c64, [ndofs, ndofs]);

    let a = batched::HelmholtzHypersingularAssembler::<c64>::new(3.0);
    a.assemble_into_dense(&mut matrix, &space, &space);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[c64::new(-0.24054975187128322, -0.37234907871793793), c64::new(-0.2018803657726846, -0.3708486980714607), c64::new(-0.31151549914430937, -0.36517694339435425), c64::new(-0.31146604913280734, -0.3652407688678574), c64::new(-0.3114620814217625, -0.36524076431695807), c64::new(-0.311434147468966, -0.36530056813389983)], [c64::new(-0.2018803657726846, -0.3708486980714607), c64::new(-0.24054975187128322, -0.3723490787179379), c64::new(-0.31146604913280734, -0.3652407688678574), c64::new(-0.31151549914430937, -0.36517694339435425), c64::new(-0.3114620814217625, -0.36524076431695807), c64::new(-0.311434147468966, -0.36530056813389983)], [c64::new(-0.31146604913280734, -0.3652407688678574), c64::new(-0.31151549914430937, -0.36517694339435425), c64::new(-0.24054975187128322, -0.3723490787179379), c64::new(-0.2018803657726846, -0.3708486980714607), c64::new(-0.31146208142176246, -0.36524076431695807), c64::new(-0.31143414746896597, -0.36530056813389983)], [c64::new(-0.31151549914430937, -0.36517694339435425), c64::new(-0.31146604913280734, -0.3652407688678574), c64::new(-0.2018803657726846, -0.3708486980714607), c64::new(-0.24054975187128322, -0.3723490787179379), c64::new(-0.3114620814217625, -0.36524076431695807), c64::new(-0.311434147468966, -0.36530056813389983)], [c64::new(-0.31146208142176257, -0.36524076431695807), c64::new(-0.3114620814217625, -0.3652407643169581), c64::new(-0.3114620814217625, -0.3652407643169581), c64::new(-0.3114620814217625, -0.3652407643169581), c64::new(-0.24056452443903534, -0.37231826606213236), c64::new(-0.20188036577268464, -0.37084869807146076)], [c64::new(-0.3114335658086867, -0.36530052927274986), c64::new(-0.31143356580868675, -0.36530052927274986), c64::new(-0.3114335658086867, -0.36530052927274986), c64::new(-0.3114335658086867, -0.36530052927274986), c64::new(-0.2018803657726846, -0.37084869807146076), c64::new(-0.2402983805938184, -0.37203286968364935)]];

    let perm = [0, 5, 2, 4, 3, 1];

    for (i, pi) in perm.iter().enumerate() {
        for (j, pj) in perm.iter().enumerate() {
            assert_relative_eq!(
                *matrix.get([i, j]).unwrap(),
                from_cl[*pi][*pj],
                epsilon = 1e-3
            );
        }
    }
}

#[test]
fn test_laplace_single_layer_potential_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<f64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();

    let mut matrix = rlst_dynamic_array2!(f64, [3, ndofs]);

    let mut points = rlst_dynamic_array2!(f64, [3, 3]);
    *points.get_mut([0, 0]).unwrap() = 2.0;
    *points.get_mut([1, 1]).unwrap() = 2.0;
    *points.get_mut([2, 2]).unwrap() = 2.0;

    let a = batched::LaplaceSingleLayerPotentialAssembler::<f64>::default();
    a.assemble_into_dense(&mut matrix, &space, &points);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[0.04038047926587569, 0.0403804792658757, 0.04038047926587571], [0.02879904511649957, 0.04038047926587569, 0.04038047926587571], [0.02879904511649957, 0.028799045116499573, 0.04038047926587571], [0.0403804792658757, 0.02879904511649957, 0.04038047926587571], [0.04038047926587569, 0.04038047926587571, 0.028799045116499573], [0.028799045116499562, 0.04038047926587569, 0.028799045116499573], [0.02879904511649957, 0.028799045116499573, 0.028799045116499573], [0.04038047926587571, 0.028799045116499573, 0.028799045116499573]];
    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            assert_relative_eq!(*matrix.get([j, i]).unwrap(), entry, epsilon = 1e-3);
        }
    }
}

#[test]
fn test_helmholtz_single_layer_potential_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<c64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();

    let mut matrix = rlst_dynamic_array2!(c64, [3, ndofs]);

    let mut points = rlst_dynamic_array2!(f64, [3, 3]);
    *points.get_mut([0, 0]).unwrap() = 2.0;
    *points.get_mut([1, 1]).unwrap() = 2.0;
    *points.get_mut([2, 2]).unwrap() = 2.0;

    let a = batched::HelmholtzSingleLayerPotentialAssembler::<c64>::new(3.0);
    a.assemble_into_dense(&mut matrix, &space, &points);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[c64::new(0.011684831539555853, -0.024085085531485414), c64::new(0.01168483153955587, -0.024085085531485407), c64::new(0.011684831539555835, -0.024085085531485424)], [c64::new(0.01584465144950023, 0.018835080109500947), c64::new(0.011684831539555853, -0.024085085531485414), c64::new(0.011684831539555835, -0.024085085531485424)], [c64::new(0.015844651449500223, 0.018835080109500944), c64::new(0.015844651449500233, 0.018835080109500944), c64::new(0.011684831539555835, -0.024085085531485424)], [c64::new(0.01168483153955587, -0.024085085531485407), c64::new(0.015844651449500226, 0.018835080109500944), c64::new(0.011684831539555835, -0.024085085531485424)], [c64::new(0.011684831539555853, -0.024085085531485414), c64::new(0.011684831539555835, -0.024085085531485424), c64::new(0.015844651449500233, 0.018835080109500944)], [c64::new(0.015844651449500216, 0.018835080109500957), c64::new(0.011684831539555853, -0.024085085531485414), c64::new(0.015844651449500233, 0.018835080109500944)], [c64::new(0.015844651449500223, 0.018835080109500944), c64::new(0.01584465144950023, 0.018835080109500947), c64::new(0.015844651449500233, 0.018835080109500944)], [c64::new(0.011684831539555835, -0.024085085531485424), c64::new(0.015844651449500237, 0.01883508010950094), c64::new(0.015844651449500233, 0.018835080109500944)]];
    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            assert_relative_eq!(*matrix.get([j, i]).unwrap(), entry, epsilon = 1e-3);
        }
    }
}

/*
#[test]
fn test_laplace_double_layer_potential_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<f64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();

    let mut matrix = rlst_dynamic_array2!(f64, [3, ndofs]);

    let mut points = rlst_dynamic_array2!(f64, [3, 3]);
    *points.get_mut([0, 0]).unwrap() = 2.0;
    *points.get_mut([1, 1]).unwrap() = 2.0;
    *points.get_mut([2, 2]).unwrap() = 2.0;

    let a = batched::LaplaceDoubleLayerPotentialAssembler::<f64>::default();
    a.assemble_into_dense(&mut matrix, &space, &points);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[0.0088687364674846, 0.008868736467484609, 0.008868736467484612], [-0.008860928325637398, 0.008868736467484602, 0.008868736467484612], [-0.0088609283256374, -0.008860928325637398, 0.008868736467484612], [0.008868736467484609, -0.008860928325637398, 0.008868736467484612], [0.0088687364674846, 0.008868736467484612, -0.008860928325637398], [-0.008860928325637396, 0.0088687364674846, -0.008860928325637398], [-0.0088609283256374, -0.008860928325637398, -0.008860928325637398], [0.008868736467484612, -0.0088609283256374, -0.008860928325637398]];
    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            assert_relative_eq!(*matrix.get([j, i]).unwrap(), entry, epsilon = 1e-3);
        }
    }
}

#[test]
fn test_helmholtz_double_layer_potential_dp0() {
    let grid = regular_sphere(0);
    let element = LagrangeElementFamily::<c64>::new(0, Continuity::Discontinuous);
    let space = SerialFunctionSpace::new(&grid, &element);

    let ndofs = space.global_size();

    let mut matrix = rlst_dynamic_array2!(c64, [3, ndofs]);

    let mut points = rlst_dynamic_array2!(f64, [3, 3]);
    *points.get_mut([0, 0]).unwrap() = 2.0;
    *points.get_mut([1, 1]).unwrap() = 2.0;
    *points.get_mut([2, 2]).unwrap() = 2.0;

    let a = batched::HelmholtzDoubleLayerPotentialAssembler::<c64>::new(3.0);
    a.assemble_into_dense(&mut matrix, &space, &points);

    // Compare to result from bempp-cl
    #[rustfmt::skip]
    let from_cl = [[c64::new(-0.025921206675194482, -0.01265280207508083), c64::new(-0.025921206675194475, -0.012652802075080833), c64::new(-0.025921206675194496, -0.0126528020750808)], [c64::new(-0.045480226003470216, 0.03114053667616141), c64::new(-0.025921206675194486, -0.01265280207508083), c64::new(-0.025921206675194493, -0.0126528020750808)], [c64::new(-0.045480226003470216, 0.03114053667616141), c64::new(-0.04548022600347021, 0.031140536676161422), c64::new(-0.025921206675194496, -0.0126528020750808)], [c64::new(-0.025921206675194475, -0.012652802075080835), c64::new(-0.04548022600347021, 0.03114053667616141), c64::new(-0.025921206675194493, -0.0126528020750808)], [c64::new(-0.025921206675194482, -0.01265280207508083), c64::new(-0.025921206675194493, -0.0126528020750808), c64::new(-0.04548022600347021, 0.031140536676161415)], [c64::new(-0.04548022600347023, 0.031140536676161377), c64::new(-0.025921206675194482, -0.01265280207508083), c64::new(-0.04548022600347021, 0.031140536676161422)], [c64::new(-0.045480226003470216, 0.03114053667616141), c64::new(-0.04548022600347021, 0.031140536676161415), c64::new(-0.04548022600347021, 0.031140536676161415)], [c64::new(-0.025921206675194493, -0.0126528020750808), c64::new(-0.045480226003470195, 0.03114053667616144), c64::new(-0.04548022600347021, 0.031140536676161422)]];
    for (i, row) in from_cl.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            assert_relative_eq!(*matrix.get([j, i]).unwrap(), entry, epsilon = 1e-3);
        }
    }
}
*/
