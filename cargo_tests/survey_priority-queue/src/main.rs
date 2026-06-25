use priority_queue::PriorityQueue;

fn main() {
    // Build a max-priority queue of (item, priority) pairs. PriorityQueue pops
    // the HIGHEST-priority element first, which is a deterministic order.
    // NOTE: PriorityQueue is backed by a hash map, so iteration order is NOT
    // deterministic — we deliberately avoid iterating; we only push/peek/pop,
    // all of which are priority-ordered and therefore stable across runs.
    let mut pq: PriorityQueue<&'static str, i32> = PriorityQueue::new();

    // push returns the old priority if the item was already present (None here).
    pq.push("task_a", 5);
    pq.push("task_b", 9);
    pq.push("task_c", 1);
    pq.push("task_d", 7);
    pq.push("task_e", 3);

    println!("len_after_push = {}", pq.len());
    println!("is_empty = {}", pq.is_empty());

    // peek the current maximum (highest priority) without removing it.
    match pq.peek() {
        Some((item, prio)) => println!("peek_max = {} {}", item, prio),
        None => println!("peek_max = <empty>"),
    }

    // change_priority bumps an existing item; returns the old priority.
    match pq.change_priority("task_c", 100) {
        Some(old) => println!("task_c_old_priority = {}", old),
        None => println!("task_c_old_priority = <absent>"),
    }

    // After the bump, task_c (now 100) should be the new max.
    match pq.peek() {
        Some((item, prio)) => println!("peek_after_bump = {} {}", item, prio),
        None => println!("peek_after_bump = <empty>"),
    }

    // get_priority is a deterministic point lookup.
    match pq.get_priority("task_b") {
        Some(p) => println!("get_priority_task_b = {}", p),
        None => println!("get_priority_task_b = <absent>"),
    }

    // Pop everything in priority order (highest first). This sequence is the
    // canonical deterministic observable of a priority queue.
    let mut order: Vec<String> = Vec::new();
    while let Some((item, prio)) = pq.pop() {
        order.push(format!("{}:{}", item, prio));
    }
    println!("pop_order = {}", order.join(","));
    println!("len_after_drain = {}", pq.len());
    println!("is_empty_after_drain = {}", pq.is_empty());

    // Demonstrate that a re-push of a duplicate key keeps the MAX priority
    // semantics: pushing the same key twice updates rather than duplicates.
    let mut pq2: PriorityQueue<u32, u32> = PriorityQueue::new();
    pq2.push(42, 10);
    let prev = pq2.push(42, 20); // duplicate key -> returns old priority
    match prev {
        Some(old) => println!("dup_push_old_priority = {}", old),
        None => println!("dup_push_old_priority = <none>"),
    }
    println!("dup_len = {}", pq2.len());
    match pq2.pop() {
        Some((item, prio)) => println!("dup_pop = {} {}", item, prio),
        None => println!("dup_pop = <empty>"),
    }

    println!("== survey_priority-queue done ==");
}
