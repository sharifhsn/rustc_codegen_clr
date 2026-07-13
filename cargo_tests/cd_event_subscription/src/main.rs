//! Rust-side subscription to an ordinary BCL event, including deterministic removal.

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use std::sync::atomic::{AtomicUsize, Ordering};

use mycorrhiza::bindings::System::ComponentModel::Component;
use mycorrhiza::bindings::System::{EventArgs, EventHandler as RawEventHandler, Object};
use mycorrhiza::delegate::{EventHandler, EventSubscription};

static HITS: AtomicUsize = AtomicUsize::new(0);

extern "C" fn on_disposed(_sender: Object, _args: EventArgs) {
    HITS.fetch_add(1, Ordering::SeqCst);
}

fn add_disposed(owner: Component, handler: RawEventHandler) {
    owner.add_disposed(handler);
}

fn remove_disposed(owner: Component, handler: RawEventHandler) {
    owner.remove_disposed(handler);
}

fn check(name: &str, ok: bool, passed: &mut usize, checks: &mut usize) {
    *checks += 1;
    if ok {
        *passed += 1;
    }
    println!("  [{}] {name}", if ok { "OK" } else { "FAIL" });
}

fn main() {
    let mut passed = 0;
    let mut checks = 0;
    let handler = EventHandler::from_fn(on_disposed);

    let first = Component::new();
    let subscription = EventSubscription::subscribe(
        first,
        handler.handle(),
        add_disposed,
        remove_disposed,
    );
    check(
        "new subscription reports active",
        subscription.is_active(),
        &mut passed,
        &mut checks,
    );
    first.dispose();
    check(
        "Component.Disposed invokes Rust callback",
        HITS.load(Ordering::SeqCst) == 1,
        &mut passed,
        &mut checks,
    );
    subscription.unsubscribe();

    let explicit = Component::new();
    EventSubscription::subscribe(
        explicit,
        handler.handle(),
        add_disposed,
        remove_disposed,
    )
    .unsubscribe();
    explicit.dispose();
    check(
        "explicit unsubscribe prevents callback",
        HITS.load(Ordering::SeqCst) == 1,
        &mut passed,
        &mut checks,
    );

    let dropped = Component::new();
    {
        let _subscription = EventSubscription::subscribe(
            dropped,
            handler.handle(),
            add_disposed,
            remove_disposed,
        );
    }
    dropped.dispose();
    check(
        "dropping subscription prevents callback",
        HITS.load(Ordering::SeqCst) == 1,
        &mut passed,
        &mut checks,
    );

    println!("cd_event_subscription: {passed}/{checks} checks passed");
    std::process::exit(if passed == checks { 0 } else { 1 });
}
