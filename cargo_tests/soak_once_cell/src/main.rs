//! H2 real-crate SOAK: once_cell on the dotnet PAL.
//! Exercises sync::Lazy (lazily-computed static), sync::OnceCell::get_or_init, and the
//! unsync variants. sync::* hits atomics-on-static -> category check vs the known
//! sub-word-atomic-static bug. Panic-safe: no unwraps; get_or_init is infallible by API.
//! SUCCESS = "== soak_once_cell done ==" with sane values.
use once_cell::sync::{Lazy, OnceCell};
use once_cell::unsync;

// Lazily-computed sync static: forces the Once/atomic-on-static path.
static GREETING: Lazy<String> = Lazy::new(|| {
    let mut s = String::new();
    for i in 0..5 {
        s.push_str(&i.to_string());
    }
    s
});

static SQUARES: Lazy<Vec<u64>> = Lazy::new(|| (0..10u64).map(|n| n * n).collect());

fn main() {
    println!("== soak_once_cell start ==");

    // 1. sync::Lazy static deref (first access triggers init via atomics).
    println!("1  GREETING={} (len {})", &*GREETING, GREETING.len());

    // 2. second access returns the cached value (no re-init).
    println!("2  GREETING again={}", &*GREETING);

    // 3. sync::Lazy<Vec> static.
    let sum: u64 = SQUARES.iter().copied().sum();
    println!("3  SQUARES.len={} sum={}", SQUARES.len(), sum);

    // 4. sync::OnceCell::get_or_init -> lazily-computed value.
    let cell: OnceCell<u64> = OnceCell::new();
    let v = cell.get_or_init(|| {
        let mut acc = 0u64;
        for n in 1..=10u64 {
            acc += n;
        }
        acc
    });
    println!("4  OnceCell get_or_init={}", v);

    // 5. get_or_init again: closure must NOT re-run; returns the cached 55.
    let v2 = cell.get_or_init(|| 999);
    println!("5  OnceCell cached={}", v2);

    // 6. OnceCell::get on a set cell.
    println!("6  OnceCell get={:?}", cell.get());

    // 7. local sync::Lazy (not a static).
    let local: Lazy<i32> = Lazy::new(|| 6 * 7);
    println!("7  local Lazy={}", *local);

    // 8. unsync::OnceCell + unsync::Lazy (no atomics; sanity vs the sync path).
    let ucell: unsync::OnceCell<&str> = unsync::OnceCell::new();
    let uw = ucell.get_or_init(|| "unsync-init");
    let ulazy: unsync::Lazy<i32> = unsync::Lazy::new(|| 100 + 23);
    println!("8  unsync OnceCell={} Lazy={}", uw, *ulazy);

    println!("== soak_once_cell done ==");
}
