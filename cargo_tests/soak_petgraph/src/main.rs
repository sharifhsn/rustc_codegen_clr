//! H2 real-crate SOAK: petgraph on the dotnet PAL.
//! Build a small Graph<&str,i32>, add nodes/edges, run BFS + dijkstra, print.
//! Exercises Vec/HashMap-heavy code, generics, the graph indexing types, and a binary-heap-based
//! shortest-path. Panic-safe (no unwraps on fallible data; node lookups handle None).
//! SUCCESS = "== soak_petgraph done ==" with sane values.
use petgraph::algo::dijkstra;
use petgraph::graph::Graph;
use petgraph::visit::Bfs;

fn main() {
    println!("== soak_petgraph start ==");

    // Directed graph with &str node weights and i32 edge weights.
    let mut g: Graph<&str, i32> = Graph::new();
    let a = g.add_node("A");
    let b = g.add_node("B");
    let c = g.add_node("C");
    let d = g.add_node("D");
    let e = g.add_node("E");

    // Edges with i32 costs.
    g.add_edge(a, b, 4);
    g.add_edge(a, c, 1);
    g.add_edge(c, b, 1);
    g.add_edge(b, d, 1);
    g.add_edge(c, d, 5);
    g.add_edge(d, e, 3);

    println!("1  nodes={} edges={}", g.node_count(), g.edge_count());

    // BFS traversal from A, collecting visited node weights.
    let mut order: Vec<&str> = Vec::new();
    let mut bfs = Bfs::new(&g, a);
    while let Some(nx) = bfs.next(&g) {
        if let Some(w) = g.node_weight(nx) {
            order.push(*w);
        }
    }
    println!("2  bfs order: {order:?}");

    // Dijkstra from A: shortest path cost to every reachable node.
    let costs = dijkstra(&g, a, None, |edge| *edge.weight());
    // Sort by node weight for deterministic output.
    let mut pairs: Vec<(&str, i32)> = Vec::new();
    for (node, cost) in &costs {
        if let Some(w) = g.node_weight(*node) {
            pairs.push((*w, *cost));
        }
    }
    pairs.sort_by(|x, y| x.0.cmp(y.0));
    println!("3  dijkstra from A: {pairs:?}");

    // Dijkstra cost to E specifically (expected 4: A->C(1)->B(1)->D(1)->E(3) = 6, or A->C->D->E = 9; min path A->C->B->D->E = 6).
    let to_e = costs.get(&e).copied().unwrap_or(-1);
    println!("4  cost A->E = {to_e}");

    // Sum of all dijkstra costs as a simple aggregate check.
    let total: i32 = costs.values().sum();
    println!("5  total reachable cost = {total}");

    println!("== soak_petgraph done ==");
}
