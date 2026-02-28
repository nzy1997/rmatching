use rmatching::flooder::detector_node::DetectorNode;
use rmatching::flooder::graph::{MatchingGraph, BOUNDARY_NODE};
use rmatching::types::*;

#[test]
fn matching_graph_add_edge() {
    let mut g = MatchingGraph::new(3, 1);
    g.add_edge(0, 1, 10, &[0]);
    assert_eq!(g.nodes[0].neighbors.len(), 1);
    assert_eq!(g.nodes[1].neighbors.len(), 1);
    assert_eq!(g.nodes[0].neighbor_weights[0], 10);
    assert_eq!(g.nodes[0].neighbor_observables[0], 1);
}

#[test]
fn matching_graph_boundary_edge() {
    let mut g = MatchingGraph::new(2, 1);
    g.add_boundary_edge(0, 5, &[0]);
    assert_eq!(g.nodes[0].neighbors.len(), 1);
    assert_eq!(g.nodes[0].neighbors[0], BOUNDARY_NODE);
}

#[test]
fn matching_graph_negative_weight() {
    let mut g = MatchingGraph::new(2, 1);
    g.add_edge(0, 1, -5, &[0]);
    assert!(g.negative_weight_detection_events_set.contains(&0));
    assert!(g.negative_weight_detection_events_set.contains(&1));
    assert!(g.negative_weight_observables_set.contains(&0));
    assert_eq!(g.negative_weight_sum, -5);
    // Weight stored as absolute value
    assert_eq!(g.nodes[0].neighbor_weights[0], 5);
}

#[test]
fn detector_node_reset() {
    let mut n = DetectorNode::new();
    n.region_that_arrived = Some(RegionIdx(1));
    n.reached_from_source = Some(NodeIdx(0));
    n.reset();
    assert!(n.region_that_arrived.is_none());
    assert!(n.reached_from_source.is_none());
}

#[test]
fn detector_node_same_owner() {
    let mut a = DetectorNode::new();
    let mut b = DetectorNode::new();
    a.region_that_arrived_top = Some(RegionIdx(5));
    b.region_that_arrived_top = Some(RegionIdx(5));
    assert!(a.has_same_owner_as(&b));
    b.region_that_arrived_top = Some(RegionIdx(6));
    assert!(!a.has_same_owner_as(&b));
}
