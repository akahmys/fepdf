//! Text matrix bookkeeping tests.

// The matrix is assigned, not computed, so exact comparison is the right check.
#![allow(clippy::float_cmp)]

use fepdf_model::graphics::{Matrix, TextMatrices};

#[test]
fn test_text_matrix_init() {
    let tm = TextMatrices::default();
    assert_eq!(tm.tm, Matrix::default());
    assert_eq!(tm.tlm, Matrix::default());
}

#[test]
fn test_text_positioning() {
    let mut tm = TextMatrices::default();
    let tx = 10.0;
    let ty = 20.0;

    let move_matrix = Matrix::new(1.0, 0.0, 0.0, 1.0, tx, ty);
    tm.tlm = tm.tlm.concat(&move_matrix);
    tm.tm = tm.tlm;

    assert_eq!(tm.tm.0[4], 10.0);
    assert_eq!(tm.tm.0[5], 20.0);
}
