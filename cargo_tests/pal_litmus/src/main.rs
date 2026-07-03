use std::env;
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

const DEFAULT_ITERATIONS: usize = 1_000_000;

struct Args {
    iterations: usize,
    only: Option<String>,
}

#[derive(Clone, Copy)]
struct Counts {
    iterations: usize,
    violations: usize,
    observed: usize,
}

impl Counts {
    fn new(iterations: usize, violations: usize, observed: usize) -> Self {
        Self {
            iterations,
            violations,
            observed,
        }
    }
}

fn parse_args() -> Args {
    let mut args = env::args().skip(1);
    let mut iterations = DEFAULT_ITERATIONS;
    let mut only = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iterations" => {
                let Some(value) = args.next() else {
                    usage_and_exit("--iterations needs a value");
                };
                iterations = value
                    .parse()
                    .unwrap_or_else(|_| usage_and_exit("--iterations must be a positive integer"));
                if iterations == 0 {
                    usage_and_exit("--iterations must be greater than zero");
                }
            }
            "--only" => {
                let Some(value) = args.next() else {
                    usage_and_exit("--only needs a value");
                };
                only = Some(value);
            }
            "--help" | "-h" => {
                println!("usage: pal_litmus [--iterations N] [--only CASE]");
                println!("cases: mp-relaxed, mp-ra, sb-relaxed, sb-seqcst, lb-relaxed, lb-seqcst, iriw-seqcst");
                std::process::exit(0);
            }
            other => usage_and_exit(&format!("unknown argument: {other}")),
        }
    }
    Args { iterations, only }
}

fn usage_and_exit(message: &str) -> ! {
    eprintln!("{message}");
    eprintln!("usage: pal_litmus [--iterations N] [--only CASE]");
    std::process::exit(2);
}

fn join(handle: thread::JoinHandle<()>) {
    handle.join().expect("litmus worker panicked");
}

fn run_mp(iterations: usize, flag_store: Ordering, flag_load: Ordering) -> Counts {
    let data = Arc::new(AtomicUsize::new(0));
    let flag = Arc::new(AtomicUsize::new(0));
    let r_flag = Arc::new(AtomicUsize::new(usize::MAX));
    let r_data = Arc::new(AtomicUsize::new(usize::MAX));
    let start_barrier = Arc::new(Barrier::new(3));
    let end_barrier = Arc::new(Barrier::new(3));

    let t0 = {
        let data = Arc::clone(&data);
        let flag = Arc::clone(&flag);
        let start_barrier = Arc::clone(&start_barrier);
        let end_barrier = Arc::clone(&end_barrier);
        thread::spawn(move || {
            for _ in 0..iterations {
                start_barrier.wait();
                data.store(1, Ordering::Relaxed);
                flag.store(1, flag_store);
                end_barrier.wait();
            }
        })
    };

    let t1 = {
        let data = Arc::clone(&data);
        let flag = Arc::clone(&flag);
        let r_flag = Arc::clone(&r_flag);
        let r_data = Arc::clone(&r_data);
        let start_barrier = Arc::clone(&start_barrier);
        let end_barrier = Arc::clone(&end_barrier);
        thread::spawn(move || {
            for _ in 0..iterations {
                start_barrier.wait();
                let seen_flag = flag.load(flag_load);
                let seen_data = data.load(Ordering::Relaxed);
                r_flag.store(seen_flag, Ordering::Relaxed);
                r_data.store(seen_data, Ordering::Relaxed);
                end_barrier.wait();
            }
        })
    };

    let mut observed = 0;
    let mut violations = 0;
    for _ in 0..iterations {
        data.store(0, Ordering::Relaxed);
        flag.store(0, Ordering::Relaxed);
        r_flag.store(usize::MAX, Ordering::Relaxed);
        r_data.store(usize::MAX, Ordering::Relaxed);
        start_barrier.wait();
        end_barrier.wait();

        let seen_flag = r_flag.load(Ordering::Relaxed);
        let seen_data = r_data.load(Ordering::Relaxed);
        if seen_flag == 1 {
            observed += 1;
            if seen_data == 0 {
                violations += 1;
            }
        }
    }

    join(t0);
    join(t1);
    Counts::new(iterations, violations, observed)
}

fn run_sb(iterations: usize, ordering: Ordering) -> Counts {
    let x = Arc::new(AtomicUsize::new(0));
    let y = Arc::new(AtomicUsize::new(0));
    let r1 = Arc::new(AtomicUsize::new(usize::MAX));
    let r2 = Arc::new(AtomicUsize::new(usize::MAX));
    let start_barrier = Arc::new(Barrier::new(3));
    let end_barrier = Arc::new(Barrier::new(3));

    let t0 = {
        let x = Arc::clone(&x);
        let y = Arc::clone(&y);
        let r1 = Arc::clone(&r1);
        let start_barrier = Arc::clone(&start_barrier);
        let end_barrier = Arc::clone(&end_barrier);
        thread::spawn(move || {
            for _ in 0..iterations {
                start_barrier.wait();
                x.store(1, ordering);
                let seen = y.load(ordering);
                r1.store(seen, Ordering::Relaxed);
                end_barrier.wait();
            }
        })
    };

    let t1 = {
        let x = Arc::clone(&x);
        let y = Arc::clone(&y);
        let r2 = Arc::clone(&r2);
        let start_barrier = Arc::clone(&start_barrier);
        let end_barrier = Arc::clone(&end_barrier);
        thread::spawn(move || {
            for _ in 0..iterations {
                start_barrier.wait();
                y.store(1, ordering);
                let seen = x.load(ordering);
                r2.store(seen, Ordering::Relaxed);
                end_barrier.wait();
            }
        })
    };

    let mut violations = 0;
    for _ in 0..iterations {
        x.store(0, Ordering::Relaxed);
        y.store(0, Ordering::Relaxed);
        r1.store(usize::MAX, Ordering::Relaxed);
        r2.store(usize::MAX, Ordering::Relaxed);
        start_barrier.wait();
        end_barrier.wait();

        if r1.load(Ordering::Relaxed) == 0 && r2.load(Ordering::Relaxed) == 0 {
            violations += 1;
        }
    }

    join(t0);
    join(t1);
    Counts::new(iterations, violations, iterations)
}

fn run_lb(iterations: usize, ordering: Ordering) -> Counts {
    let x = Arc::new(AtomicUsize::new(0));
    let y = Arc::new(AtomicUsize::new(0));
    let r1 = Arc::new(AtomicUsize::new(usize::MAX));
    let r2 = Arc::new(AtomicUsize::new(usize::MAX));
    let start_barrier = Arc::new(Barrier::new(3));
    let end_barrier = Arc::new(Barrier::new(3));

    let t0 = {
        let x = Arc::clone(&x);
        let y = Arc::clone(&y);
        let r1 = Arc::clone(&r1);
        let start_barrier = Arc::clone(&start_barrier);
        let end_barrier = Arc::clone(&end_barrier);
        thread::spawn(move || {
            for _ in 0..iterations {
                start_barrier.wait();
                let seen = y.load(ordering);
                x.store(1, ordering);
                r1.store(seen, Ordering::Relaxed);
                end_barrier.wait();
            }
        })
    };

    let t1 = {
        let x = Arc::clone(&x);
        let y = Arc::clone(&y);
        let r2 = Arc::clone(&r2);
        let start_barrier = Arc::clone(&start_barrier);
        let end_barrier = Arc::clone(&end_barrier);
        thread::spawn(move || {
            for _ in 0..iterations {
                start_barrier.wait();
                let seen = x.load(ordering);
                y.store(1, ordering);
                r2.store(seen, Ordering::Relaxed);
                end_barrier.wait();
            }
        })
    };

    let mut violations = 0;
    for _ in 0..iterations {
        x.store(0, Ordering::Relaxed);
        y.store(0, Ordering::Relaxed);
        r1.store(usize::MAX, Ordering::Relaxed);
        r2.store(usize::MAX, Ordering::Relaxed);
        start_barrier.wait();
        end_barrier.wait();

        if r1.load(Ordering::Relaxed) == 1 && r2.load(Ordering::Relaxed) == 1 {
            violations += 1;
        }
    }

    join(t0);
    join(t1);
    Counts::new(iterations, violations, iterations)
}

fn run_iriw(iterations: usize, ordering: Ordering) -> Counts {
    let x = Arc::new(AtomicUsize::new(0));
    let y = Arc::new(AtomicUsize::new(0));
    let r1 = Arc::new(AtomicUsize::new(usize::MAX));
    let r2 = Arc::new(AtomicUsize::new(usize::MAX));
    let r3 = Arc::new(AtomicUsize::new(usize::MAX));
    let r4 = Arc::new(AtomicUsize::new(usize::MAX));
    let start_barrier = Arc::new(Barrier::new(5));
    let end_barrier = Arc::new(Barrier::new(5));

    let writer_x = {
        let x = Arc::clone(&x);
        let start_barrier = Arc::clone(&start_barrier);
        let end_barrier = Arc::clone(&end_barrier);
        thread::spawn(move || {
            for _ in 0..iterations {
                start_barrier.wait();
                x.store(1, ordering);
                end_barrier.wait();
            }
        })
    };

    let writer_y = {
        let y = Arc::clone(&y);
        let start_barrier = Arc::clone(&start_barrier);
        let end_barrier = Arc::clone(&end_barrier);
        thread::spawn(move || {
            for _ in 0..iterations {
                start_barrier.wait();
                y.store(1, ordering);
                end_barrier.wait();
            }
        })
    };

    let reader_xy = {
        let x = Arc::clone(&x);
        let y = Arc::clone(&y);
        let r1 = Arc::clone(&r1);
        let r2 = Arc::clone(&r2);
        let start_barrier = Arc::clone(&start_barrier);
        let end_barrier = Arc::clone(&end_barrier);
        thread::spawn(move || {
            for _ in 0..iterations {
                start_barrier.wait();
                let first = x.load(ordering);
                let second = y.load(ordering);
                r1.store(first, Ordering::Relaxed);
                r2.store(second, Ordering::Relaxed);
                end_barrier.wait();
            }
        })
    };

    let reader_yx = {
        let x = Arc::clone(&x);
        let y = Arc::clone(&y);
        let r3 = Arc::clone(&r3);
        let r4 = Arc::clone(&r4);
        let start_barrier = Arc::clone(&start_barrier);
        let end_barrier = Arc::clone(&end_barrier);
        thread::spawn(move || {
            for _ in 0..iterations {
                start_barrier.wait();
                let first = y.load(ordering);
                let second = x.load(ordering);
                r3.store(first, Ordering::Relaxed);
                r4.store(second, Ordering::Relaxed);
                end_barrier.wait();
            }
        })
    };

    let mut observed = 0;
    let mut violations = 0;
    for _ in 0..iterations {
        x.store(0, Ordering::Relaxed);
        y.store(0, Ordering::Relaxed);
        r1.store(usize::MAX, Ordering::Relaxed);
        r2.store(usize::MAX, Ordering::Relaxed);
        r3.store(usize::MAX, Ordering::Relaxed);
        r4.store(usize::MAX, Ordering::Relaxed);
        start_barrier.wait();
        end_barrier.wait();

        let xy_first = r1.load(Ordering::Relaxed);
        let xy_second = r2.load(Ordering::Relaxed);
        let yx_first = r3.load(Ordering::Relaxed);
        let yx_second = r4.load(Ordering::Relaxed);
        if xy_first == 1 && yx_first == 1 {
            observed += 1;
        }
        if xy_first == 1 && xy_second == 0 && yx_first == 1 && yx_second == 0 {
            violations += 1;
        }
    }

    join(writer_x);
    join(writer_y);
    join(reader_xy);
    join(reader_yx);
    Counts::new(iterations, violations, observed)
}

fn print_result(name: &str, ordering: &str, forbidden: &str, counts: Counts, elapsed_ms: u128) {
    println!(
        "RESULT name={name} ordering={ordering} forbidden=\"{forbidden}\" iterations={} violations={} observed={} elapsed_ms={elapsed_ms}",
        counts.iterations, counts.violations, counts.observed
    );
}

fn should_run(only: Option<&str>, case: &str) -> bool {
    only.map_or(true, |selected| selected == case)
}

fn print_start(case: &str, iterations: usize) {
    println!("START case={case} iterations={iterations}");
    io::stdout().flush().expect("stdout flush failed");
}

fn main() {
    let args = parse_args();
    let iterations = args.iterations;
    let only = args.only.as_deref();
    println!("pal_litmus iterations={iterations}");

    if should_run(only, "mp-relaxed") {
        print_start("mp-relaxed", iterations);
        let start = Instant::now();
        let counts = run_mp(iterations, Ordering::Relaxed, Ordering::Relaxed);
        print_result(
            "MP",
            "Relaxed",
            "flag=1,data=0 allowed control",
            counts,
            start.elapsed().as_millis(),
        );
    }

    if should_run(only, "mp-ra") {
        print_start("mp-ra", iterations);
        let start = Instant::now();
        let counts = run_mp(iterations, Ordering::Release, Ordering::Acquire);
        print_result(
            "MP",
            "Release/Acquire",
            "flag=1,data=0",
            counts,
            start.elapsed().as_millis(),
        );
    }

    if should_run(only, "sb-relaxed") {
        print_start("sb-relaxed", iterations);
        let start = Instant::now();
        let counts = run_sb(iterations, Ordering::Relaxed);
        print_result(
            "SB",
            "Relaxed",
            "r1=0,r2=0 allowed control",
            counts,
            start.elapsed().as_millis(),
        );
    }

    if should_run(only, "sb-seqcst") {
        print_start("sb-seqcst", iterations);
        let start = Instant::now();
        let counts = run_sb(iterations, Ordering::SeqCst);
        print_result(
            "SB",
            "SeqCst",
            "r1=0,r2=0",
            counts,
            start.elapsed().as_millis(),
        );
    }

    if should_run(only, "lb-relaxed") {
        print_start("lb-relaxed", iterations);
        let start = Instant::now();
        let counts = run_lb(iterations, Ordering::Relaxed);
        print_result(
            "LB",
            "Relaxed",
            "r1=1,r2=1 allowed",
            counts,
            start.elapsed().as_millis(),
        );
    }

    if should_run(only, "lb-seqcst") {
        print_start("lb-seqcst", iterations);
        let start = Instant::now();
        let counts = run_lb(iterations, Ordering::SeqCst);
        print_result(
            "LB",
            "SeqCst",
            "r1=1,r2=1",
            counts,
            start.elapsed().as_millis(),
        );
    }

    if should_run(only, "iriw-seqcst") {
        print_start("iriw-seqcst", iterations);
        let start = Instant::now();
        let counts = run_iriw(iterations, Ordering::SeqCst);
        print_result(
            "IRIW",
            "SeqCst",
            "xy=1,0 and yx=1,0",
            counts,
            start.elapsed().as_millis(),
        );
    }
}
