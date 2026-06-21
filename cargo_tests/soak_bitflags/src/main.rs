use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Perms: u32 {
        const READ    = 0b0000_0001;
        const WRITE   = 0b0000_0010;
        const EXECUTE = 0b0000_0100;
        const RW      = Self::READ.bits() | Self::WRITE.bits();
    }
}

fn main() {
    // Construct a flags value.
    let mut p = Perms::READ | Perms::WRITE;
    println!("p bits=0b{:08b}", p.bits());

    // contains
    println!("contains READ={}", p.contains(Perms::READ));
    println!("contains EXECUTE={}", p.contains(Perms::EXECUTE));
    println!("contains RW={}", p.contains(Perms::RW));

    // insert / set
    p.insert(Perms::EXECUTE);
    println!("after insert EXECUTE: bits=0b{:08b}", p.bits());

    // intersection
    let inter = p.intersection(Perms::RW);
    println!("intersection with RW: bits=0b{:08b}", inter.bits());

    // union
    let u = Perms::READ | Perms::EXECUTE;
    println!("union READ|EXECUTE bits=0b{:08b}", u.bits());

    // difference / remove
    p.remove(Perms::WRITE);
    println!("after remove WRITE: bits=0b{:08b}", p.bits());

    // toggle
    p.toggle(Perms::READ);
    println!("after toggle READ: bits=0b{:08b}", p.bits());

    // is_empty / empty / all
    println!("empty is_empty={}", Perms::empty().is_empty());
    println!("all bits=0b{:08b}", Perms::all().bits());

    // Iterate over set flags (non-panicking).
    let it = Perms::RW | Perms::EXECUTE;
    let mut count = 0u32;
    for _flag in it.iter() {
        count += 1;
    }
    println!("RW|EXECUTE flag count={}", count);

    // Debug formatting.
    println!("debug={:?}", Perms::RW);

    println!("== soak_bitflags done ==");
}
