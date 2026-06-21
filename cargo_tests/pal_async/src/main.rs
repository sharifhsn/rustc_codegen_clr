//! Async-codegen probe: does async fn / .await (the coroutine state-machine lowering) work on the
//! dotnet PAL? Pure std — a hand-rolled noop-waker block_on executor, no tokio/futures/net.
//! SUCCESS = "== pal_async done ==" with correct values. A crash/ICE localizes the coroutine gap.
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

fn noop_waker() -> Waker {
    fn clone(_: *const ()) -> RawWaker { RawWaker::new(std::ptr::null(), &VTABLE) }
    fn noop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

fn block_on<F: Future>(fut: F) -> F::Output {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut fut = Box::pin(fut);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => {} // all our futures are immediately ready; spin
        }
    }
}

async fn add(a: i32, b: i32) -> i32 { a + b }

// a future that returns Pending once, then Ready — exercises a real suspend/resume across .await
struct YieldOnce(bool);
impl Future for YieldOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 { Poll::Ready(()) } else { self.0 = true; cx.waker().wake_by_ref(); Poll::Pending }
    }
}

async fn compute() -> i32 {
    let x = add(2, 3).await;            // simple await
    let y = add(x, 10).await;           // await with captured local
    YieldOnce(false).await;            // a genuine suspend point (Pending->Ready)
    let mut sum = 0;
    for i in 0..5 { sum += async move { i * 2 }.await; }  // await in a loop, nested async block
    let s = String::from("async");     // a heap value held across .await
    YieldOnce(false).await;
    x + y + sum + s.len() as i32       // s used after a suspend point
}

fn main() {
    println!("== pal_async start ==");
    let r = block_on(compute());
    // 5 + 15 + (0+2+4+6+8=20) + 5 = 45
    println!("1  async compute: {} (expect 45)", r);
    println!("== pal_async done ==");
}
