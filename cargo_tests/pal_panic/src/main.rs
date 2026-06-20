//! H2 Phase 3 probe: does panic=unwind + catch_unwind work on the REAL dotnet PAL?
//! (WF-6's throw-bridge was only validated on the surrogate target.)
fn main() {
    println!("== pal_panic start ==");
    let r = std::panic::catch_unwind(|| {
        println!("a  inside closure (about to panic)");
        panic!("boom from rust");
    });
    println!("b  caught panic: is_err={}", r.is_err());
    let r2 = std::panic::catch_unwind(|| 6 * 7);
    println!("c  ok path value: {:?}", r2.ok());
    println!("== pal_panic done ==");
}
