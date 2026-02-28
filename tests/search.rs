use rmatching::search::{SearchFlooder, SearchGraph};
use rmatching::types::*;

/// Build a 3-node chain: 0 --w=10-- 1 --w=20-- 2
fn make_chain_graph() -> SearchGraph {
    let mut g = SearchGraph::new(3, 2);
    g.add_edge(0, 1, 10, 0b01);
    g.add_edge(1, 2, 20, 0b10);
    g
}

#[test]
fn search_shortest_path() {
    let g = make_chain_graph();
    let mut flooder = SearchFlooder::new(g);

    let edge = flooder.find_shortest_path(0, Some(2));
    assert_eq!(edge.loc_from, Some(NodeIdx(0)));
    assert_eq!(edge.loc_to, Some(NodeIdx(2)));
    // XOR of observables along the path: 0b01 ^ 0b10 = 0b11
    assert_eq!(edge.obs_mask, 0b11);
}

#[test]
fn search_shortest_path_reversed() {
    let g = make_chain_graph();
    let mut flooder = SearchFlooder::new(g);

    let edge = flooder.find_shortest_path(2, Some(0));
    assert_eq!(edge.loc_from, Some(NodeIdx(2)));
    assert_eq!(edge.loc_to, Some(NodeIdx(0)));
    assert_eq!(edge.obs_mask, 0b11);
}

#[test]
fn search_adjacent_nodes() {
    let g = make_chain_graph();
    let mut flooder = SearchFlooder::new(g);

    let edge = flooder.find_shortest_path(0, Some(1));
    assert_eq!(edge.loc_from, Some(NodeIdx(0)));
    assert_eq!(edge.loc_to, Some(NodeIdx(1)));
    assert_eq!(edge.obs_mask, 0b01);
}

#[test]
fn search_boundary_path() {
    let mut g = SearchGraph::new(3, 1);
    // 0 --w=10-- 1 --w=5-- boundary
    g.add_edge(0, 1, 10, 0b01);
    g.add_boundary_edge(1, 5, 0b10);

    let mut flooder = SearchFlooder::new(g);
    let edge = flooder.find_shortest_path(0, None);
    assert_eq!(edge.loc_from, Some(NodeIdx(0)));
    assert_eq!(edge.loc_to, None);
    // Path: 0->1 (obs 0b01) then 1->boundary (obs 0b10) => 0b11
    assert_eq!(edge.obs_mask, 0b11);
}

#[test]
fn search_boundary_direct() {
    let mut g = SearchGraph::new(1, 1);
    g.add_boundary_edge(0, 7, 0b01);

    let mut flooder = SearchFlooder::new(g);
    let edge = flooder.find_shortest_path(0, None);
    assert_eq!(edge.loc_from, Some(NodeIdx(0)));
    assert_eq!(edge.loc_to, None);
    assert_eq!(edge.obs_mask, 0b01);
}

#[test]
fn search_reuse_after_reset() {
    let g = make_chain_graph();
    let mut flooder = SearchFlooder::new(g);

    let e1 = flooder.find_shortest_path(0, Some(2));
    assert_eq!(e1.obs_mask, 0b11);

    // Second search on the same flooder should work after implicit reset.
    let e2 = flooder.find_shortest_path(0, Some(1));
    assert_eq!(e2.obs_mask, 0b01);
}

#[test]
fn search_diamond_picks_shorter_path() {
    // Diamond: 0--1 (w=2), 0--2 (w=10), 1--3 (w=2), 2--3 (w=10)
    // Shortest 0->3 is 0->1->3 with total weight 4, obs = 0b01 ^ 0b10 = 0b11
    let mut g = SearchGraph::new(4, 2);
    g.add_edge(0, 1, 2, 0b01);
    g.add_edge(0, 2, 10, 0b100);
    g.add_edge(1, 3, 2, 0b10);
    g.add_edge(2, 3, 10, 0b1000);

    let mut flooder = SearchFlooder::new(g);
    let edge = flooder.find_shortest_path(0, Some(3));
    assert_eq!(edge.loc_from, Some(NodeIdx(0)));
    assert_eq!(edge.loc_to, Some(NodeIdx(3)));
    // Should take the short path: obs = 0b01 ^ 0b10 = 0b11
    assert_eq!(edge.obs_mask, 0b11);
}
