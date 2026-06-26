// Decisive diagnostic: is a panic in a SPAWNED thread caught (→ join() Err) on the .NET PAL, or does
// it cross the nounwind thread_start boundary and ABORT the process? libtest runs each test in a
// spawned thread; if spawned-thread panics aren't caught, EVERY panicking test aborts the whole run.
use std::panic;
use std::thread;
fn main() {
    panic::set_hook(Box::new(|_| {})); // silence the panic message
    let r = panic::catch_unwind(|| panic!("main"));
    println!("main_thread_catch_unwind_caught={}", r.is_err());
    let h = thread::spawn(|| panic!("spawned"));
    let jr = h.join();
    println!("spawned_thread_panic_caught={}", jr.is_err());
    println!("PROBE_DONE");
}
