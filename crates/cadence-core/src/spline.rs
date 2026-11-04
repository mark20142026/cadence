//! The cubic spline keyframe curves use: Tino Kluge's `tk::spline` as compiled into the game
//! (`set_points` 0x10043abc0, `operator()` 0x10043c490), only the case the game builds
//! (`KeyframeObject::setupSpline` 0xcb680): a C² cubic spline (`cspline`) with zero second
//! derivatives at both ends, its tridiagonal system solved by the library's band-matrix LU
//! decomposition. The fused multiply-adds are where the compiled code has them.

/// f(x) = y_i + b_i·h + c_i·h² + d_i·h³ on [x_i, x_{i+1}), h = x − x_i.
#[derive(Clone, Debug, PartialEq)]
pub struct Spline {
    x: Vec<f64>,
    y: Vec<f64>,
    b: Vec<f64>,
    c: Vec<f64>,
    d: Vec<f64>,
    c0: f64,
}

const THIRD: f64 = 1.0 / 3.0;
const TWO_THIRDS: f64 = 2.0 / 3.0;

impl Spline {
    /// `set_points(x, y, cspline)` with second-derivative boundaries of 0. `x` must increase
    /// strictly and hold at least 3 points (setupSpline makes no spline from fewer).
    pub fn new(x: &[f64], y: &[f64]) -> Spline {
        let n = x.len();
        assert!(n >= 3 && y.len() == n);
        // The band matrix (one diagonal above and below): lower[i] = A(i, i−1), upper[i] =
        // A(i, i+1).
        let mut lower = vec![0.0f64; n];
        let mut diag = vec![0.0f64; n];
        let mut upper = vec![0.0f64; n];
        let mut rhs = vec![0.0f64; n];
        for i in 1..n - 1 {
            lower[i] = (x[i] - x[i - 1]) * THIRD;
            diag[i] = (x[i + 1] - x[i - 1]) * TWO_THIRDS;
            upper[i] = (x[i + 1] - x[i]) * THIRD;
            rhs[i] = (y[i + 1] - y[i]) / (x[i + 1] - x[i]) - (y[i] - y[i - 1]) / (x[i] - x[i - 1]);
        }
        // second_deriv ends: 2·c[0] = 0, 2·c[n−1] = 0.
        diag[0] = 2.0;
        upper[0] = 0.0;
        rhs[0] = 0.0;
        diag[n - 1] = 2.0;
        lower[n - 1] = 0.0;
        rhs[n - 1] = 0.0;
        // band_matrix::lu_decompose: rows scaled so the diagonal is 1 (saved_diag = 1/a_ii)...
        let mut saved = vec![0.0f64; n];
        for i in 0..n {
            saved[i] = 1.0 / diag[i];
            if i > 0 {
                lower[i] *= saved[i];
            }
            if i + 1 < n {
                upper[i] *= saved[i];
            }
            diag[i] = 1.0;
        }
        // ...then Gauss elimination (L below the diagonal, R on and above it).
        for k in 0..n - 1 {
            let i = k + 1;
            let f = -lower[i] / diag[k];
            lower[i] = -f;
            diag[i] = f.mul_add(upper[k], diag[i]);
        }
        // l_solve (x_i = b_i·saved_i − Σ, fused), r_solve.
        let mut yv = vec![0.0f64; n];
        for i in 0..n {
            let sum = if i > 0 { lower[i].mul_add(yv[i - 1], 0.0) } else { 0.0 };
            yv[i] = rhs[i].mul_add(saved[i], -sum);
        }
        let mut c = vec![0.0f64; n];
        for i in (0..n).rev() {
            let sum = if i + 1 < n { upper[i].mul_add(c[i + 1], 0.0) } else { 0.0 };
            c[i] = (yv[i] - sum) / diag[i];
        }
        let mut b = vec![0.0f64; n];
        let mut d = vec![0.0f64; n];
        for i in 0..n - 1 {
            let h = x[i + 1] - x[i];
            d[i] = ((c[i + 1] - c[i]) * THIRD) / h;
            let slope = (y[i + 1] - y[i]) / h;
            b[i] = (c[i].mul_add(2.0, c[i + 1]) * -THIRD).mul_add(h, slope);
        }
        // Right extrapolation: no cubic term, the slope at the last point.
        let h = x[n - 1] - x[n - 2];
        d[n - 1] = 0.0;
        b[n - 1] = b[n - 2] + (h * (d[n - 2] * 3.0)).mul_add(h, h * (c[n - 2] + c[n - 2]));
        let c0 = c[0];
        Spline { x: x.to_vec(), y: y.to_vec(), b, c, d, c0 }
    }

    /// `operator()(x)`: Horner's scheme from the last point at or before x (the first segment
    /// extrapolates to the left, the last one to the right).
    pub fn at(&self, x: f64) -> f64 {
        let n = self.x.len();
        let ub = self.x.partition_point(|&v| v <= x);
        let idx = ((ub as i32 - 1) as f64).max(0.0) as usize;
        let h = x - self.x[idx];
        if x < self.x[0] {
            self.c0.mul_add(h, self.b[0]).mul_add(h, self.y[0])
        } else if self.x[n - 1] < x {
            self.c[n - 1].mul_add(h, self.b[n - 1]).mul_add(h, self.y[n - 1])
        } else {
            self.d[idx].mul_add(h, self.c[idx]).mul_add(h, self.b[idx]).mul_add(h, self.y[idx])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Spline;

    #[test]
    fn natural_cubic_through_its_points() {
        let x = [0.0, 1.0, 2.5, 3.0];
        let y = [0.0, 100.0, 40.0, 120.0];
        let s = Spline::new(&x, &y);
        for (a, b) in x.iter().zip(y) {
            assert!((s.at(*a) - b).abs() < 1e-9, "{a}: {} vs {b}", s.at(*a));
        }
        // Zero second derivative at the ends: c[0] = c[n−1] = 0.
        assert_eq!(s.c[0], 0.0);
        assert_eq!(s.c[3], 0.0);
        // A straight line stays straight.
        let line = Spline::new(&[0.0, 1.0, 2.0], &[0.0, 10.0, 20.0]);
        assert!((line.at(0.5) - 5.0).abs() < 1e-12);
        assert!((line.at(1.75) - 17.5).abs() < 1e-12);
    }
}
