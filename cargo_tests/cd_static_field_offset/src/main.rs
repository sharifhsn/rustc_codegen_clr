// Faithful repro of the encoding_rs miscompile: a reference to a FIELD at a
// non-zero offset inside ONE big static, stored in another static.
#[repr(C)]
struct Data { t0: [u16; 128], t1: [u16; 128], t2: [u16; 128] }
static BIG: Data = {
    let mut t0 = [0u16; 128]; t0[0] = 0xE000;
    let mut t1 = [0u16; 128]; t1[0] = 0xE001;
    let mut t2 = [0u16; 128]; t2[0] = 0xE002;
    Data { t0, t1, t2 }
};
struct Enc { table: &'static [u16; 128] }
// references to FIELDS at offsets 0, 256, 512 bytes inside BIG:
static ENC0: Enc = Enc { table: &BIG.t0 };
static ENC1: Enc = Enc { table: &BIG.t1 };
static ENC2: Enc = Enc { table: &BIG.t2 };
fn main() {
    // each ENCk.table[0] must equal 0xE00k. If the field offset is dropped,
    // ENC1/ENC2 will wrongly read 0xE000 (the first field, t0).
    println!("ENC0 = {:04X} (ok={})", ENC0.table[0], ENC0.table[0] == 0xE000);
    println!("ENC1 = {:04X} (ok={})", ENC1.table[0], ENC1.table[0] == 0xE001);
    println!("ENC2 = {:04X} (ok={})", ENC2.table[0], ENC2.table[0] == 0xE002);
    println!("== cd_static_field_offset done ==");
}
