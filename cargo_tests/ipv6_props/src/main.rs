// Ipv6Addr property predicates (ipv6_properties test core).
// Exercises the bit-pattern classification logic that the upstream property test
// hammers, including the unstable feature(ip) predicates.
#![feature(ip)]
use std::net::Ipv6Addr;
use std::str::FromStr;

fn b(label: &str, got: bool, exp: bool) {
    println!("{label}: {got}|{exp}");
    assert_eq!(got, exp, "MISMATCH {label}");
}

fn main() {
    let unspec = Ipv6Addr::from_str("::").unwrap();
    b("unspec.is_unspecified", unspec.is_unspecified(), true);
    b("unspec.is_loopback", unspec.is_loopback(), false);
    b("unspec.is_multicast", unspec.is_multicast(), false);

    let lo = Ipv6Addr::from_str("::1").unwrap();
    b("lo.is_loopback", lo.is_loopback(), true);
    b("lo.is_unspecified", lo.is_unspecified(), false);
    b("lo.is_multicast", lo.is_multicast(), false);

    let ll = Ipv6Addr::from_str("fe80::1").unwrap();
    b("ll.is_unicast_link_local", ll.is_unicast_link_local(), true);
    b("ll.is_multicast", ll.is_multicast(), false);

    let ula = Ipv6Addr::from_str("fc00::1").unwrap();
    b("ula.is_unique_local", ula.is_unique_local(), true);
    b("ula.is_multicast", ula.is_multicast(), false);

    let mc = Ipv6Addr::from_str("ff01::1").unwrap();
    b("mc.is_multicast", mc.is_multicast(), true);
    b("mc.is_loopback", mc.is_loopback(), false);
    // multicast_scope is Some for multicast
    println!("mc.multicast_scope_some: {}|true", mc.multicast_scope().is_some());
    assert!(mc.multicast_scope().is_some());

    let mc2 = Ipv6Addr::from_str("ff02::1").unwrap();
    b("mc2.is_multicast", mc2.is_multicast(), true);

    let g = Ipv6Addr::from_str("2606:4700:4700::1111").unwrap();
    b("g.is_multicast", g.is_multicast(), false);
    b("g.is_loopback", g.is_loopback(), false);

    // segments roundtrip / octets bit-layout
    let ip = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
    let segs = ip.segments();
    println!("segs0: {:#x}|0x2001", segs[0]);
    assert_eq!(segs[0], 0x2001);
    assert_eq!(segs[7], 1);
    let oct = ip.octets();
    assert_eq!(oct[0], 0x20);
    assert_eq!(oct[1], 0x01);
    assert_eq!(oct[15], 0x01);
    assert_eq!(Ipv6Addr::from(oct), ip);
    println!("octets_layout: ok");

    println!("ipv6_props OK");
}
