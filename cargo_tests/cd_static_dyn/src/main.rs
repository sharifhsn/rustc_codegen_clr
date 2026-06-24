// P3-S1 regression probe for the unified const/static GlobalAlloc relocation resolver.
//
// P1: `static OBJ: &dyn Speak` — the fat pointer's vtable field is a GlobalAlloc::VTable
//     relocation. Before the fix it routed to a NULL vtable static; OBJ.say() faulted
//     (NullReferenceException). Must now print 7.
// P2: `static OBJS: [&dyn Speak; 2]` — two VTable relocations inside one array static,
//     dispatched through a loop. Must print 7, 7.
// P3: `static F: fn() -> u32` — a GlobalAlloc::Function relocation in a fn-ptr static.
//     Already worked via the reloc-loop Function branch; guards the Function arm. Must
//     print 42.
//
// The trait must be `: Sync` for the `&dyn` static to be legal (the referent A is Sync).

trait Speak: Sync {
    fn say(&self) -> u32;
}

struct A;
impl Speak for A {
    fn say(&self) -> u32 {
        7
    }
}

static A_INST: A = A;

// P1: const/static trait object.
static OBJ: &dyn Speak = &A_INST;

// P2: array of const/static trait objects.
static OBJS: [&dyn Speak; 2] = [&A_INST, &A_INST];

// P3: fn-pointer static (GlobalAlloc::Function reloc).
fn g() -> u32 {
    42
}
static F: fn() -> u32 = g;

fn main() {
    // P1
    let p1 = OBJ.say();
    println!("P1 {}", p1);
    assert_eq!(p1, 7, "static &dyn vtable dispatch wrong");

    // P2
    for o in OBJS {
        let v = o.say();
        println!("P2 {}", v);
        assert_eq!(v, 7, "array-of-&dyn vtable dispatch wrong");
    }

    // P3
    let p3 = F();
    println!("P3 {}", p3);
    assert_eq!(p3, 42, "fn-ptr static call wrong");

    println!("OK");
}
