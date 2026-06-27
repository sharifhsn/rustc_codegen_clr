fn main() {
    // NO black_box: let the CIL optimizer const-propagate the NaN through is_finite / comparisons.
    let nan: f32 = f32::NAN;
    let inf: f32 = f32::INFINITY;
    println!("f32 is_finite(NAN): {} (want false)", nan.is_finite());
    println!("f32 NAN < INF     : {} (want false)", nan < inf);
    println!("f32 NAN.abs()<INF : {} (want false)", nan.abs() < inf);
    println!("f32 NAN > INF     : {} (want false)", nan > inf);
    println!("f32 NAN <= INF    : {} (want false)", nan <= inf);
    println!("f32 NAN >= INF    : {} (want false)", nan >= inf);
    println!("f32 NAN == NAN    : {} (want false)", nan == nan);
    println!("f32 NAN != NAN    : {} (want true)", nan != nan);
    let nd: f64 = f64::NAN; let idd: f64 = f64::INFINITY;
    println!("f64 is_finite(NAN): {} (want false)", nd.is_finite());
    println!("f64 NAN < INF     : {} (want false)", nd < idd);
    println!("done");
}
