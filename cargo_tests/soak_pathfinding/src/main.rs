use pathfinding::directed::bfs::bfs;
use pathfinding::directed::dijkstra::dijkstra;

// Deterministic, hand-built directed integer-weighted graph over nodes 0..=5.
// Adjacency given by a `match` returning (neighbor, cost) pairs in a fixed
// order. No hashing, no RNG, no float — fully reproducible.
//
//   0 --2--> 1 --3--> 3 --1--> 5
//   0 --5--> 2 --1--> 3
//   1 --1--> 2
//   2 --7--> 4 --2--> 5
//   3 --6--> 4
//
// Shortest 0 -> 5 by cost: 0 -1-> ... let's see paths:
//   0->1->3->5 : 2+3+1 = 6
//   0->1->2->3->5 : 2+1+1+1 = 5   (cheapest)
//   0->2->3->5 : 5+1+1 = 7
// So dijkstra cost = 5, path = [0,1,2,3,5].

type Node = u32;

fn successors(n: Node) -> Vec<(Node, u32)> {
    match n {
        0 => vec![(1, 2), (2, 5)],
        1 => vec![(3, 3), (2, 1)],
        2 => vec![(3, 1), (4, 7)],
        3 => vec![(5, 1), (4, 6)],
        4 => vec![(5, 2)],
        _ => vec![], // node 5 (and any other) is a sink
    }
}

// Unweighted neighbors (same edges, edge-count metric) for BFS.
fn neighbors(n: Node) -> Vec<Node> {
    successors(n).into_iter().map(|(to, _cost)| to).collect()
}

fn fmt_path(path: &[Node]) -> String {
    // Deterministic "a>b>c" rendering; no separator trailing.
    let mut s = String::new();
    for (i, node) in path.iter().enumerate() {
        if i != 0 {
            s.push('>');
        }
        s.push_str(&node.to_string());
    }
    s
}

fn main() {
    let start: Node = 0;
    let goal: Node = 5;

    // --- Dijkstra: shortest path by accumulated integer cost. ---
    match dijkstra(&start, |&n| successors(n), |&n| n == goal) {
        Some((path, cost)) => {
            println!("dijkstra_cost = {}", cost);
            println!("dijkstra_path = {}", fmt_path(&path));
            println!("dijkstra_len = {}", path.len());
        }
        None => {
            println!("dijkstra_cost = none");
            println!("dijkstra_path = none");
            println!("dijkstra_len = 0");
        }
    }

    // --- BFS: fewest-edges path (ignores weights). ---
    match bfs(&start, |&n| neighbors(n), |&n| n == goal) {
        Some(path) => {
            // BFS returns a path with the minimum number of edges.
            let edges = path.len().saturating_sub(1);
            println!("bfs_edges = {}", edges);
            println!("bfs_path = {}", fmt_path(&path));
        }
        None => {
            println!("bfs_edges = none");
            println!("bfs_path = none");
        }
    }

    // --- Unreachable goal: prove the None branch is deterministic too. ---
    // Node 4 has no edge to node 0, so searching for 0 from 4 fails.
    match dijkstra(&4u32, |&n| successors(n), |&n| n == 0) {
        Some((_p, c)) => println!("unreachable_cost = {}", c),
        None => println!("unreachable_cost = none"),
    }

    // --- A second cost sum: walk the cheapest path and re-add costs by hand,
    //     cross-checking dijkstra's reported cost via the adjacency table. ---
    // Cheapest 0->5 path is [0,1,2,3,5]; recompute its cost edge-by-edge.
    let chosen: [Node; 5] = [0, 1, 2, 3, 5];
    let mut hand_cost: u32 = 0;
    let mut ok = true;
    for w in chosen.windows(2) {
        let from = w[0];
        let to = w[1];
        match successors(from).into_iter().find(|(t, _)| *t == to) {
            Some((_t, c)) => hand_cost = hand_cost.saturating_add(c),
            None => ok = false,
        }
    }
    println!("hand_cost = {}", hand_cost);
    println!("hand_path_valid = {}", ok);

    println!("== soak_pathfinding done ==");
}
