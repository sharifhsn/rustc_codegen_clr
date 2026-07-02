#![forbid(unsafe_op_in_unsafe_fn)]

use std::alloc::{alloc, dealloc, realloc, Layout};
use std::collections::HashSet;
use std::ptr;
use std::sync::mpsc;
use std::thread;

const POOL_CEILING: usize = 8192;
const STORM_THREADS: usize = 8;
const STORM_ITERS: usize = 50_000;
const CROSS_BLOCKS: usize = 2048;
const CROSS_LIVE: usize = 128;
const CROSS_SIZE: usize = 4096;
const ZERO_SIZE: usize = 4096;

fn main() {
    storm();
    cross_thread_free();
    realloc_chains();
    alignment_asserts();
    zeroing_contract();
    println!("pal_allocstress PASS all scenarios");
}

fn storm() {
    let mut handles = Vec::new();
    for tid in 0..STORM_THREADS {
        handles.push(thread::spawn(move || {
            let mut seed = 0x9E37_79B9_7F4A_7C15u64 ^ ((tid as u64) << 32);
            for iter in 0..STORM_ITERS {
                let size = random_size(&mut seed, iter);
                let mut v = vec![0u8; size];
                let pat = pattern(tid, iter);
                for (idx, b) in v.iter_mut().enumerate() {
                    *b = pat.wrapping_add(idx as u8);
                }
                for (idx, b) in v.iter().enumerate() {
                    assert_eq!(
                        *b,
                        pat.wrapping_add(idx as u8),
                        "storm tid={tid} iter={iter} idx={idx} size={size}"
                    );
                }
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    println!(
        "storm PASS threads={STORM_THREADS} iters_per_thread={STORM_ITERS} ceiling={POOL_CEILING}"
    );
}

fn cross_thread_free() {
    let layout = Layout::from_size_align(CROSS_SIZE, 8).unwrap();
    let mut live = Vec::new();
    let mut live_set = HashSet::new();
    for idx in 0..CROSS_LIVE {
        let ptr = checked_alloc(layout, "cross live");
        unsafe {
            ptr::write_bytes(ptr, 0xC0u8.wrapping_add(idx as u8), CROSS_SIZE);
        }
        assert!(live_set.insert(ptr as usize), "duplicate live pointer {ptr:p}");
        live.push(ptr);
    }

    let (tx, rx) = mpsc::channel::<usize>();
    let verifier = thread::spawn(move || {
        for idx in 0..CROSS_BLOCKS {
            let raw = rx.recv().unwrap();
            let ptr = raw as *mut u8;
            let pat = 0x40u8.wrapping_add(idx as u8);
            unsafe {
                for off in 0..CROSS_SIZE {
                    assert_eq!(
                        ptr.add(off).read(),
                        pat.wrapping_add(off as u8),
                        "cross-thread readback idx={idx} off={off}"
                    );
                }
                dealloc(ptr, layout);
            }
        }
    });

    for idx in 0..CROSS_BLOCKS {
        let ptr = checked_alloc(layout, "cross transfer");
        let pat = 0x40u8.wrapping_add(idx as u8);
        unsafe {
            for off in 0..CROSS_SIZE {
                ptr.add(off).write(pat.wrapping_add(off as u8));
            }
        }
        tx.send(ptr as usize).unwrap();
    }
    drop(tx);
    verifier.join().unwrap();

    let mut reused = Vec::new();
    for _ in 0..CROSS_BLOCKS {
        let ptr = checked_alloc(layout, "cross reuse");
        assert!(
            !live_set.contains(&(ptr as usize)),
            "new allocation aliased still-live pointer {ptr:p}"
        );
        reused.push(ptr);
    }
    for ptr in reused {
        unsafe {
            dealloc(ptr, layout);
        }
    }
    for ptr in live {
        unsafe {
            dealloc(ptr, layout);
        }
    }
    println!("cross_thread_free PASS transferred={CROSS_BLOCKS} live={CROSS_LIVE}");
}

fn realloc_chains() {
    let counts = [8usize, 4000, 16, 2048, 4, 512, 1200, 32];
    let mut layout = Layout::array::<u64>(counts[0]).unwrap();
    let mut ptr = checked_alloc(layout, "realloc initial") as *mut u64;
    let mut old_count = counts[0];
    let mut fill = 0x1111_0000_0000_0000u64;
    unsafe {
        for idx in 0..old_count {
            ptr.add(idx).write(fill ^ idx as u64);
        }
    }

    for (step, &new_count) in counts.iter().enumerate().skip(1) {
        let new_layout = Layout::array::<u64>(new_count).unwrap();
        let new_bytes = new_layout.size();
        let new_ptr = unsafe { realloc(ptr.cast::<u8>(), layout, new_bytes) as *mut u64 };
        assert!(
            !new_ptr.is_null(),
            "realloc step={step} returned null for {new_bytes} bytes"
        );
        let prefix = old_count.min(new_count);
        unsafe {
            for idx in 0..prefix {
                assert_eq!(
                    new_ptr.add(idx).read(),
                    fill ^ idx as u64,
                    "realloc prefix mismatch step={step} idx={idx}"
                );
            }
        }
        ptr = new_ptr;
        layout = new_layout;
        old_count = new_count;
        fill = fill.wrapping_add(0x0101_0101_0101_0101);
        unsafe {
            for idx in 0..old_count {
                ptr.add(idx).write(fill ^ idx as u64);
            }
        }
    }

    unsafe {
        dealloc(ptr.cast::<u8>(), layout);
    }
    println!("realloc_chains PASS steps={}", counts.len());
}

fn alignment_asserts() {
    #[derive(Clone, Copy, Default)]
    #[repr(align(8))]
    #[allow(dead_code)]
    struct A8(u8);
    #[derive(Clone, Copy, Default)]
    #[repr(align(16))]
    #[allow(dead_code)]
    struct A16(u8);
    #[derive(Clone, Copy, Default)]
    #[repr(align(32))]
    #[allow(dead_code)]
    struct A32(u8);
    #[derive(Clone, Copy, Default)]
    #[repr(align(64))]
    #[allow(dead_code)]
    struct A64(u8);

    fn check<T: Clone + Default>(name: &str, align: usize, lens: &[usize]) {
        for round in 0..2048 {
            for &len in lens {
                let mut v = vec![T::default(); len];
                let ptr = v.as_mut_ptr() as usize;
                assert_eq!(ptr % align, 0, "alignment {name} len={len} round={round}");
                drop(v);
            }
        }
    }

    check::<u8>("A1", 1, &[1, 64, 4096, 12_000]);
    check::<u64>("A8", 8, &[1, 64, 512, 2048]);
    check::<A8>("repr8", 8, &[1, 64, 512, 2048]);
    check::<A16>("repr16", 16, &[1, 64, 256, 1024]);
    check::<A32>("repr32", 32, &[1, 64, 128, 512]);
    check::<A64>("repr64", 64, &[1, 64, 128, 256]);
    println!("alignment_asserts PASS");
}

fn zeroing_contract() {
    let layout = Layout::from_size_align(ZERO_SIZE, 8).unwrap();
    for iter in 0..5000 {
        let ptr = checked_alloc(layout, "zero dirty");
        unsafe {
            ptr::write_bytes(ptr, 0xA5, ZERO_SIZE);
            dealloc(ptr, layout);
        }

        let v = vec![0u8; ZERO_SIZE];
        assert!(
            v.iter().all(|&b| b == 0),
            "vec![0] reused dirty bytes at iter={iter}"
        );
    }
    println!("zeroing_contract PASS size={ZERO_SIZE}");
}

fn random_size(seed: &mut u64, iter: usize) -> usize {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    let mixed = *seed as usize ^ iter.wrapping_mul(0x45D9_F3B);
    match mixed & 7 {
        0 => 1 + (mixed % 128),
        1 => 129 + (mixed % 896),
        2 => 1025 + (mixed % 3072),
        3 => 4096,
        4 => POOL_CEILING,
        5 => POOL_CEILING + 1 + (mixed % 1024),
        6 => 16_384 + (mixed % 4096),
        _ => 32 + (mixed % 6000),
    }
}

fn pattern(tid: usize, iter: usize) -> u8 {
    ((tid.wrapping_mul(37) ^ iter.wrapping_mul(131)) & 0xff) as u8
}

fn checked_alloc(layout: Layout, context: &str) -> *mut u8 {
    let ptr = unsafe { alloc(layout) };
    assert!(
        !ptr.is_null(),
        "{context}: allocation returned null for size={} align={}",
        layout.size(),
        layout.align()
    );
    ptr
}
