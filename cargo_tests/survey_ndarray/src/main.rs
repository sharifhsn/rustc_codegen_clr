use ndarray::{array, Array2, Axis};

fn main() {
    // Deterministic 3x3 f64 matrix, fixed entries.
    let a: Array2<f64> = array![
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0],
        [7.0, 8.0, 10.0],
    ];

    // Shape (rows, cols) -> derived ints, no panic.
    let (rows, cols) = a.dim();
    println!("rows = {}", rows);
    println!("cols = {}", cols);

    // Construction helpers: zeros / from_elem / identity-like via diagonal fill.
    let zeros: Array2<f64> = Array2::zeros((3, 3));
    println!("zeros_sum = {:.6}", zeros.sum());

    let filled: Array2<f64> = Array2::from_elem((2, 4), 2.5);
    println!("filled_sum = {:.6}", filled.sum());

    // Reductions: sum, mean (Option), max-tracking via fold.
    println!("sum = {:.6}", a.sum());
    match a.mean() {
        Some(m) => println!("mean = {:.6}", m),
        None => println!("mean = <none>"),
    }

    // Per-axis sums (column sums = sum over rows = Axis(0)).
    let col_sums = a.sum_axis(Axis(0));
    println!("col_sum_0 = {:.6}", col_sums[0]);
    println!("col_sum_1 = {:.6}", col_sums[1]);
    println!("col_sum_2 = {:.6}", col_sums[2]);

    // Row sums (sum over columns = Axis(1)).
    let row_sums = a.sum_axis(Axis(1));
    println!("row_sum_0 = {:.6}", row_sums[0]);
    println!("row_sum_1 = {:.6}", row_sums[1]);
    println!("row_sum_2 = {:.6}", row_sums[2]);

    // Matrix multiply (dot). A * A -> deterministic 3x3.
    let prod = a.dot(&a);
    println!("prod_sum = {:.6}", prod.sum());
    println!("prod_0_0 = {:.6}", prod[[0, 0]]);
    println!("prod_2_2 = {:.6}", prod[[2, 2]]);

    // Matrix * vector via dot.
    let v = array![1.0_f64, 0.0, -1.0];
    let av = a.dot(&v);
    println!("av_0 = {:.6}", av[0]);
    println!("av_1 = {:.6}", av[1]);
    println!("av_2 = {:.6}", av[2]);

    // Element-wise ops: scale, add, multiply.
    let scaled = &a * 2.0;
    println!("scaled_sum = {:.6}", scaled.sum());

    let summed = &a + &a;
    println!("summed_sum = {:.6}", summed.sum());

    let hadamard = &a * &a;
    println!("hadamard_sum = {:.6}", hadamard.sum());

    // mapv: apply f64 method element-wise (sqrt of squares == abs of a).
    let roots = hadamard.mapv(f64::sqrt);
    println!("roots_sum = {:.6}", roots.sum());

    // Transpose (view, no copy) -> sum invariant, but check a transposed entry.
    let at = a.t();
    println!("transposed_0_2 = {:.6}", at[[0, 2]]);
    println!("transposed_2_0 = {:.6}", at[[2, 0]]);

    // Slicing: first two rows, last two cols -> 2x2 sub-block sum.
    let sub = a.slice(ndarray::s![0..2, 1..3]);
    println!("sub_sum = {:.6}", sub.sum());
    println!("sub_0_0 = {:.6}", sub[[0, 0]]);
    println!("sub_1_1 = {:.6}", sub[[1, 1]]);

    // A column view sum (slice a single column).
    let col0 = a.column(0);
    println!("col0_sum = {:.6}", col0.sum());

    // Dot product of two 1-D arrays.
    let x = array![1.0_f64, 2.0, 3.0];
    let y = array![4.0_f64, 5.0, 6.0];
    println!("vec_dot = {:.6}", x.dot(&y));

    // L2 norm derived from dot (no .norm() needed) -> deterministic scalar.
    let norm_sq = x.dot(&x);
    println!("x_norm = {:.6}", norm_sq.sqrt());

    println!("== survey_ndarray done ==");
}
