//! The Matrix type and the sampling helpers linspace, logspace and meshgrid.

use ironlab::Matrix;
use ironlab::ir::IrError;
use ironlab::{linspace, logspace, meshgrid};

// WHY: every gridded plot relies on row-major storage with rows = y and cols = x; if
// from_fn filled column-major, all surfaces and contours would be transposed.
#[test]
fn from_fn_fills_row_major() {
    let m = Matrix::from_fn(2, 3, |row, col| (10 * row + col) as f64);
    assert_eq!((m.rows(), m.cols()), (2, 3));
    assert_eq!(m.values(), &[0.0, 1.0, 2.0, 10.0, 11.0, 12.0]);
    assert_eq!(m[(0, 2)], 2.0);
    assert_eq!(m[(1, 0)], 10.0);
    assert_eq!(m.row(1), &[10.0, 11.0, 12.0]);
}

// WHY: from_rows is the literal-matrix constructor used in examples; it must keep row
// order and agree with from_fn.
#[test]
fn from_rows_agrees_with_from_fn() {
    let m = Matrix::from_rows(&[vec![0.0, 1.0, 2.0], vec![10.0, 11.0, 12.0]]);
    assert_eq!(m, Matrix::from_fn(2, 3, |row, col| (10 * row + col) as f64));
}

// WHY: ragged rows cannot form a matrix; the documented behaviour is a panic, because
// this is a programming error in literal data rather than a data-dependent condition.
#[test]
#[should_panic(expected = "matrix rows have different lengths")]
fn from_rows_panics_on_ragged_rows() {
    let _ = Matrix::from_rows(&[vec![0.0, 1.0], vec![2.0]]);
}

// WHY: from_vec accepts data computed elsewhere (for example read from a file), so a
// length mismatch is a runtime condition and must be an error, not a panic.
#[test]
fn from_vec_checks_the_length() {
    let m = Matrix::from_vec(2, 2, vec![1.0, 2.0, 3.0, 4.0]).unwrap();
    assert_eq!(m[(1, 0)], 3.0);
    assert!(matches!(
        Matrix::from_vec(2, 2, vec![1.0, 2.0, 3.0]),
        Err(IrError::InvalidShape { .. })
    ));
}

// WHY: zeros and IndexMut together let users fill a matrix in loops, MATLAB style.
#[test]
fn zeros_and_index_mut() {
    let mut m = Matrix::zeros(2, 2);
    assert_eq!(m.values(), &[0.0; 4]);
    m[(0, 1)] = 5.0;
    assert_eq!(m.values(), &[0.0, 5.0, 0.0, 0.0]);
}

// WHY: indexing outside the matrix must panic rather than silently read a value from
// another row. On a 2 by 2 matrix the flat index of (0, 2) is 2, which is a valid
// position in the value vector, so only an explicit column check panics here; the
// expected message distinguishes that check from any unrelated panic.
#[test]
#[should_panic(expected = "out of range")]
fn index_past_the_last_column_panics() {
    let m = Matrix::from_fn(2, 2, |_, _| 0.0);
    let _ = m[(0, 2)];
}

// WHY: see index_past_the_last_column_panics; writes must be checked in the same way,
// or a loop with an off-by-one column bound silently corrupts the next row.
#[test]
#[should_panic(expected = "out of range")]
fn index_mut_past_the_last_column_panics() {
    let mut m = Matrix::zeros(2, 2);
    m[(0, 2)] = 1.0;
}

// WHY: a row past the end must be reported with the matrix's own message rather than
// returning an empty or partial slice.
#[test]
#[should_panic(expected = "out of range")]
fn row_past_the_last_row_panics() {
    let m = Matrix::zeros(2, 3);
    let _ = m.row(2);
}

// WHY: map and zip_map are how fields are evaluated on meshgrid output; they must keep
// the shape and pair values element by element.
#[test]
fn map_and_zip_map_preserve_shape_and_pairing() {
    let a = Matrix::from_fn(2, 3, |row, col| (row + col) as f64);
    let b = Matrix::from_fn(2, 3, |row, _| row as f64 * 100.0);
    let doubled = a.map(|v| 2.0 * v);
    assert_eq!(
        doubled,
        Matrix::from_fn(2, 3, |row, col| 2.0 * (row + col) as f64)
    );
    let sum = a.zip_map(&b, |p, q| p + q);
    assert_eq!(
        sum,
        Matrix::from_fn(2, 3, |row, col| (row + col) as f64 + row as f64 * 100.0)
    );
}

// WHY: combining matrices of different shapes is a programming error that must not
// produce a silently truncated or transposed result. A 2 by 3 and a 3 by 2 matrix
// have the same number of values, so an implementation that only compares lengths
// would zip them without panicking.
#[test]
#[should_panic(expected = "matrices have different shapes")]
fn zip_map_panics_on_shape_mismatch() {
    let a = Matrix::zeros(2, 3);
    let b = Matrix::zeros(3, 2);
    let _ = a.zip_map(&b, |p, q| p + q);
}

// WHY: MATLAB's meshgrid convention (x along columns, y along rows, shape ny by nx) is
// what makes `surf(X, Y, Z)` agree with `surf(x, y, Z)`; getting it backwards
// transposes every curvilinear plot.
#[test]
fn meshgrid_varies_x_along_columns_and_y_along_rows() {
    let x = [1.0, 2.0, 3.0];
    let y = [-1.0, 1.0];
    let (xx, yy) = meshgrid(&x, &y);
    assert_eq!((xx.rows(), xx.cols()), (2, 3));
    assert_eq!((yy.rows(), yy.cols()), (2, 3));
    for row in 0..2 {
        for col in 0..3 {
            assert_eq!(xx[(row, col)], x[col]);
            assert_eq!(yy[(row, col)], y[row]);
        }
    }
}

// WHY: axis limits are computed from data extremes, so linspace endpoints must be
// exactly the requested values (accumulated floating-point steps would give, for
// example, 0.30000000000000004 and an unexpected tick).
#[test]
fn linspace_has_exact_endpoints_and_count() {
    let v = linspace(0.1, 0.3, 7);
    assert_eq!(v.len(), 7);
    assert_eq!(v[0], 0.1);
    assert_eq!(v[6], 0.3);

    let pi = std::f64::consts::PI;
    let w = linspace(-pi, 2.0 * pi, 1001);
    assert_eq!(w.len(), 1001);
    assert_eq!(w[0], -pi);
    assert_eq!(w[1000], 2.0 * pi);
}

// WHY: interior values must be evenly spaced, and a decreasing range must be supported.
#[test]
fn linspace_is_evenly_spaced_including_decreasing_ranges() {
    assert_eq!(linspace(0.0, 1.0, 5), vec![0.0, 0.25, 0.5, 0.75, 1.0]);
    assert_eq!(linspace(1.0, -1.0, 3), vec![1.0, 0.0, -1.0]);
}

// WHY: degenerate counts follow MATLAB (n = 1 gives the end value, n = 0 gives nothing)
// rather than dividing by zero and producing NaN.
#[test]
fn linspace_degenerate_counts() {
    assert_eq!(linspace(2.0, 5.0, 1), vec![5.0]);
    assert!(linspace(2.0, 5.0, 0).is_empty());
}

// WHY: logspace generates the x data of the loglog gallery figure. Values at integer
// exponents must be exactly the nearest floating-point numbers to the decades, so that
// log axis limits and ticks land on decades (`powf` is not guaranteed to be correctly
// rounded, and a decade one unit in the last place too large clips the last point).
#[test]
fn logspace_is_exact_at_integer_exponents() {
    assert_eq!(
        logspace(-3.0, 3.0, 7),
        vec![0.001, 0.01, 0.1, 1.0, 10.0, 100.0, 1000.0]
    );
}

// WHY: between decades the exponents are evenly spaced, so the ratio of successive
// values is constant, and degenerate counts behave as in linspace.
#[test]
fn logspace_has_a_constant_ratio_and_follows_linspace_counts() {
    let v = logspace(0.0, 1.0, 3);
    assert_eq!(v[0], 1.0);
    assert!((v[1] - 10f64.sqrt()).abs() < 1e-12, "{}", v[1]);
    assert_eq!(v[2], 10.0);
    assert_eq!(logspace(0.0, 2.0, 1), vec![100.0]);
    assert!(logspace(0.0, 1.0, 0).is_empty());
}
