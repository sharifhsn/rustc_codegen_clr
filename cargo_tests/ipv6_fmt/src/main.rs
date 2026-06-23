// IPv6 address Display / Debug / parse codegen repro.
// Mirrors VERBATIM the upstream coretests ipv6_addr_to_string body plus
// from_str roundtrips (ipv6_properties), padding, {:#?}, and socket_addr fmt.
// Each `eq` prints got|expected and asserts equality.
use std::net::{Ipv6Addr, SocketAddrV6};
use std::str::FromStr;

fn eq(label: &str, got: String, expected: &str) {
    println!("{label}: {got}|{expected}");
    assert_eq!(got, expected, "MISMATCH {label}");
}

fn main() {
    // ===== ipv6_addr_to_string (verbatim upstream) =====
    eq("v4mapped", Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc000, 0x280).to_string(), "::ffff:192.0.2.128");
    eq("v4compat", Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0xc000, 0x280).to_string(), "::c000:280");
    eq("nozero", Ipv6Addr::new(8, 9, 10, 11, 12, 13, 14, 15).to_string(), "8:9:a:b:c:d:e:f");
    eq("longest", Ipv6Addr::new(0x1111, 0x2222, 0x3333, 0x4444, 0x5555, 0x6666, 0x7777, 0x8888).to_string(),
        "1111:2222:3333:4444:5555:6666:7777:8888");
    // padding
    eq("pad_left", format!("{:20}", Ipv6Addr::new(1, 2, 3, 4, 5, 6, 7, 8)), "1:2:3:4:5:6:7:8     ");
    eq("pad_right", format!("{:>20}", Ipv6Addr::new(1, 2, 3, 4, 5, 6, 7, 8)), "     1:2:3:4:5:6:7:8");
    // reduce a single run of zeros
    eq("single_run", Ipv6Addr::new(0xae, 0, 0, 0, 0, 0xffff, 0x0102, 0x0304).to_string(), "ae::ffff:102:304");
    // don't reduce just a single zero segment
    eq("single_zero", Ipv6Addr::new(1, 2, 3, 4, 5, 6, 0, 8).to_string(), "1:2:3:4:5:6:0:8");
    eq("any", Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0).to_string(), "::");
    eq("loopback", Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1).to_string(), "::1");
    eq("ends_zeros", Ipv6Addr::new(1, 0, 0, 0, 0, 0, 0, 0).to_string(), "1::");
    // two runs of zeros, second one is longer
    eq("two_runs_2nd", Ipv6Addr::new(1, 0, 0, 4, 0, 0, 0, 8).to_string(), "1:0:0:4::8");
    // two runs of zeros, equal length -> first wins
    eq("two_runs_eq", Ipv6Addr::new(1, 0, 0, 4, 5, 0, 0, 8).to_string(), "1::4:5:0:0:8");
    // debug, no 0x prefix
    eq("debug_hash", format!("{:#?}", Ipv6Addr::new(1, 0, 0, 4, 5, 0, 0, 8)), "1::4:5:0:0:8");
    eq("debug_plain", format!("{:?}", Ipv6Addr::new(1, 0, 0, 4, 5, 0, 0, 8)), "1::4:5:0:0:8");

    // ===== from_str roundtrips (ipv6_properties core) =====
    for s in ["::", "::1", "1::", "2a02:6b8::11:11",
              "1:2:3:4:5:6:7:8", "fe80::", "fc00::", "ff01::", "ff02::", "2001:db8::",
              "ff00::", "::ffff:127.0.0.1"] {
        let ip = Ipv6Addr::from_str(s).unwrap();
        eq(&format!("roundtrip[{s}]"), ip.to_string(), s);
    }

    // octets roundtrip
    let ip = Ipv6Addr::from_str("2001:db8::1").unwrap();
    let oct = ip.octets();
    assert_eq!(Ipv6Addr::from(oct), ip, "octets roundtrip");
    println!("octets_roundtrip: ok");

    // ===== socket_addr formatting =====
    eq("sa_v4mapped",
        SocketAddrV6::new(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc000, 0x280), 8080, 0, 0).to_string(),
        "[::ffff:192.0.2.128]:8080");
    eq("sa_loopback", SocketAddrV6::new(Ipv6Addr::LOCALHOST, 53, 0, 0).to_string(), "[::1]:53");
    eq("sa_scope",
        SocketAddrV6::new(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1), 8080, 0, 5).to_string(),
        "[2001:db8::1%5]:8080");
    // width padding on socket addr
    eq("sa_pad",
        format!("{:24}", SocketAddrV6::new(Ipv6Addr::LOCALHOST, 53, 0, 0)),
        "[::1]:53                ");

    println!("ipv6_fmt OK");
}
