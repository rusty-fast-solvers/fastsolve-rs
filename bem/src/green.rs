use num::complex::Complex;
use num::Num;

pub trait Scalar: Num + std::ops::AddAssign {
    /// Get 1 over 4*pi as this scalar type
    fn inv_4pi() -> Self;
    /// Get -1 over 4*pi as this scalar type
    fn neg_inv_4pi() -> Self;
    /// Get the distance between x and y as this scalar type
    fn dist(x: &[f64], y: &[f64]) -> Self;
    /// Get the square of the distance between x and y as this scalar type
    fn dist_squared(x: &[f64], y: &[f64]) -> Self;
    /// Get the cube of the distance between x and y as this scalar type
    fn dist_cubed(x: &[f64], y: &[f64]) -> Self;
    /// Get x.y as this scalar type
    fn dot(x: &[f64], y: &[f64]) -> Self;
    /// Get (x-y).n as this scalar type
    fn subdot(x: &[f64], y: &[f64], n: &[f64]) -> Self;
    /// Convert a f64 to this type
    fn from_f64(v: f64) -> Self;
    /// Get e^(i*x) as this scalar type
    fn eix(x: f64) -> Self;
    /// Get i*e^(i*x) as this scalar type
    fn ieix(x: f64) -> Self;
}

impl Scalar for f64 {
    fn inv_4pi() -> Self {
        0.25 * std::f64::consts::FRAC_1_PI
    }
    fn neg_inv_4pi() -> Self {
        -0.25 * std::f64::consts::FRAC_1_PI
    }
    fn dist(x: &[f64], y: &[f64]) -> Self {
        f64::sqrt(
            (x[0] - y[0]) * (x[0] - y[0])
                + (x[1] - y[1]) * (x[1] - y[1])
                + (x[2] - y[2]) * (x[2] - y[2]),
        )
    }
    fn dist_squared(x: &[f64], y: &[f64]) -> Self {
        (x[0] - y[0]) * (x[0] - y[0])
            + (x[1] - y[1]) * (x[1] - y[1])
            + (x[2] - y[2]) * (x[2] - y[2])
    }
    fn dist_cubed(x: &[f64], y: &[f64]) -> Self {
        let sq = Self::dist_squared(x, y);
        sq * f64::sqrt(sq)
    }
    fn dot(x: &[f64], y: &[f64]) -> Self {
        x[0] * y[0] + x[1] * y[1] + x[2] * y[2]
    }
    fn subdot(x: &[f64], y: &[f64], n: &[f64]) -> Self {
        (x[0] - y[0]) * n[0] + (x[1] - y[1]) * n[1] + (x[2] - y[2]) * n[2]
    }
    fn from_f64(v: f64) -> Self {
        v
    }
    fn eix(x: f64) -> Self {
        Complex::<f64>::eix(x).re
    }
    fn ieix(x: f64) -> Self {
        -Complex::<f64>::eix(x).im
    }
}

impl Scalar for Complex<f64> {
    fn inv_4pi() -> Self {
        Complex::<f64>::new(f64::inv_4pi(), 0.0)
    }
    fn neg_inv_4pi() -> Self {
        Complex::<f64>::new(f64::neg_inv_4pi(), 0.0)
    }
    fn dist(x: &[f64], y: &[f64]) -> Self {
        Complex::<f64>::new(f64::dist(x, y), 0.0)
    }
    fn dist_squared(x: &[f64], y: &[f64]) -> Self {
        Complex::<f64>::new(f64::dist_squared(x, y), 0.0)
    }
    fn dist_cubed(x: &[f64], y: &[f64]) -> Self {
        Complex::<f64>::new(f64::dist_cubed(x, y), 0.0)
    }
    fn dot(x: &[f64], y: &[f64]) -> Self {
        Complex::<f64>::new(f64::dot(x, y), 0.0)
    }
    fn subdot(x: &[f64], y: &[f64], n: &[f64]) -> Self {
        Complex::<f64>::new(f64::subdot(x, y, n), 0.0)
    }
    fn eix(x: f64) -> Self {
        Complex::<f64>::new(0.0, x).exp()
    }
    fn ieix(x: f64) -> Self {
        Complex::<f64>::new(0.0, 1.0) * Complex::<f64>::new(0.0, x).exp()
    }
    fn from_f64(v: f64) -> Self {
        Complex::<f64>::new(v, 0.0)
    }
}

pub trait SingularKernel {
    fn eval<T: Scalar>(&self, x: &[f64], y: &[f64], nx: &[f64], ny: &[f64]) -> T;
}

pub struct LaplaceGreenKernel {}
impl SingularKernel for LaplaceGreenKernel {
    fn eval<T: Scalar>(&self, x: &[f64], y: &[f64], _nx: &[f64], _ny: &[f64]) -> T {
        T::inv_4pi() / T::dist(x, y)
    }
}

pub struct LaplaceGreenDxKernel {}
impl SingularKernel for LaplaceGreenDxKernel {
    fn eval<T: Scalar>(&self, x: &[f64], y: &[f64], nx: &[f64], _ny: &[f64]) -> T {
        T::inv_4pi() * T::subdot(y, x, nx) / T::dist_cubed(x, y)
    }
}

pub struct LaplaceGreenDyKernel {}
impl SingularKernel for LaplaceGreenDyKernel {
    fn eval<T: Scalar>(&self, x: &[f64], y: &[f64], _nx: &[f64], ny: &[f64]) -> T {
        T::inv_4pi() * T::subdot(x, y, ny) / T::dist_cubed(x, y)
    }
}

pub struct HelmholtzGreenKernel {
    pub k: f64,
}
impl SingularKernel for HelmholtzGreenKernel {
    fn eval<T: Scalar>(&self, x: &[f64], y: &[f64], _nx: &[f64], _ny: &[f64]) -> T {
        let dist = f64::dist(x, y);
        T::inv_4pi() * T::eix(self.k * dist) / T::from_f64(dist)
    }
}

pub struct HelmholtzGreenDxKernel {
    pub k: f64,
}
impl SingularKernel for HelmholtzGreenDxKernel {
    fn eval<T: Scalar>(&self, x: &[f64], y: &[f64], nx: &[f64], _ny: &[f64]) -> T {
        let sq = f64::dist_squared(x, y);
        let dist = sq.sqrt();
        T::inv_4pi()
            * T::subdot(x, y, nx)
            * (T::from_f64(self.k) * T::ieix(self.k * dist)
                - T::eix(self.k * dist) / T::from_f64(dist))
            / T::from_f64(sq)
    }
}

pub struct HelmholtzGreenDyKernel {
    pub k: f64,
}
impl SingularKernel for HelmholtzGreenDyKernel {
    fn eval<T: Scalar>(&self, x: &[f64], y: &[f64], _nx: &[f64], ny: &[f64]) -> T {
        let sq = f64::dist_squared(x, y);
        let dist = sq.sqrt();
        T::inv_4pi()
            * T::subdot(y, x, ny)
            * (T::from_f64(self.k) * T::ieix(self.k * dist)
                - T::eix(self.k * dist) / T::from_f64(dist))
            / T::from_f64(sq)
    }
}

pub struct HelmholtzGreenHypersingularTermKernel {
    pub k: f64,
}
impl SingularKernel for HelmholtzGreenHypersingularTermKernel {
    fn eval<T: Scalar>(&self, x: &[f64], y: &[f64], nx: &[f64], ny: &[f64]) -> T {
        let dist = f64::dist(x, y);
        T::from_f64(self.k) * T::from_f64(self.k) * T::neg_inv_4pi() * T::eix(self.k * dist)
            / T::from_f64(dist)
            * T::dot(nx, ny)
    }
}
