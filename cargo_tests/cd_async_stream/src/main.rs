//! Product-shaped async-stream consumer proof.
//!
//! `ChannelReader<T>.ReadAllAsync()` returns the same `IAsyncEnumerable<T>` shape used by modern
//! .NET streaming APIs. A delayed producer forces the first `MoveNextAsync()` through a genuine
//! incomplete `ValueTask<bool>` before yielding three ordered values and graceful completion.

use std::time::Duration;

use mycorrhiza::sync::channel;

fn main() -> std::process::ExitCode {
    let (sender, receiver) = channel::<i32>();
    let producer = std::thread::spawn(move || {
        for value in [11, 22, 33] {
            std::thread::sleep(Duration::from_millis(10));
            sender.send_blocking(value);
        }
        assert!(sender.close());
    });

    let mut enumerator = receiver.read_all_async().get_async_enumerator();
    let mut values = Vec::new();
    while let Some(value) = enumerator.next_blocking() {
        values.push(value);
    }
    enumerator.dispose_blocking();
    producer.join().expect("async-stream producer panicked");

    let (second_sender, second_receiver) = channel::<i32>();
    second_sender.send_blocking(44);
    second_sender.send_blocking(55);
    assert!(second_sender.close());
    let collected = second_receiver
        .read_all_async()
        .get_async_enumerator()
        .collect_blocking();

    if values == [11, 22, 33] && collected == [44, 55] {
        println!("async stream values: {values:?}");
        println!("async stream collected: {collected:?}");
        println!("== cd_async_stream done ==");
        std::process::ExitCode::SUCCESS
    } else {
        eprintln!("async stream mismatch: {values:?}");
        std::process::ExitCode::FAILURE
    }
}
