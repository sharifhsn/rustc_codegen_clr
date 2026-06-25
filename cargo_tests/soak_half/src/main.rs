use half::{bf16, f16};

fn main() {
    // f16 from f32, arithmetic, back to f32.
    let a = f16::from_f32(1.5_f32);
    let b = f16::from_f32(2.25_f32);
    let sum = a + b;
    let prod = a * b;
    let diff = b - a;
    println!(
        "f16: a={} b={} sum={} prod={} diff={}",
        a.to_f32(),
        b.to_f32(),
        sum.to_f32(),
        prod.to_f32(),
        diff.to_f32()
    );

    // bf16 from f32, arithmetic, back to f32.
    let c = bf16::from_f32(3.0_f32);
    let d = bf16::from_f32(0.5_f32);
    let bsum = c + d;
    let bprod = c * d;
    println!(
        "bf16: c={} d={} sum={} prod={}",
        c.to_f32(),
        d.to_f32(),
        bsum.to_f32(),
        bprod.to_f32()
    );

    // Some constants / conversions exercising the bit-level codegen.
    let one = f16::from_f32(1.0_f32);
    let two = f16::from_f32(2.0_f32);
    println!("f16 one bits=0x{:04x} two bits=0x{:04x}", one.to_bits(), two.to_bits());

    let bone = bf16::from_f32(1.0_f32);
    println!("bf16 one bits=0x{:04x}", bone.to_bits());

    // Round-trip a handful of values without any panicking ops.
    let vals = [0.0_f32, 1.0, -1.0, 0.25, 100.0];
    let mut acc = 0.0_f32;
    for &v in vals.iter() {
        let h = f16::from_f32(v);
        acc += h.to_f32();
    }
    println!("f16 round-trip acc={}", acc);

    println!("== soak_half done ==");
}
