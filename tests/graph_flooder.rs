use rmatching::flooder::graph::MatchingGraph;
use rmatching::flooder::graph_flooder::GraphFlooder;
use rmatching::interop::MwpmEvent;
use rmatching::types::*;

#[test]
fn flooder_create_detection_event() {
    let mut graph = MatchingGraph::new(3, 1);
    graph.add_edge(0, 1, 10, &[0]);
    graph.add_edge(1, 2, 10, &[]);
    let mut flooder = GraphFlooder::new(graph);
    let _r = flooder.create_detection_event(NodeIdx(0));
    assert!(flooder.graph.nodes[0].region_that_arrived.is_some());
    assert!(flooder.graph.nodes[0].region_that_arrived_top.is_some());
    assert_eq!(flooder.graph.nodes[0].reached_from_source, Some(NodeIdx(0)));
}

#[test]
fn flooder_two_events_collide() {
    let mut graph = MatchingGraph::new(2, 1);
    graph.add_edge(0, 1, 10, &[0]);
    let mut flooder = GraphFlooder::new(graph);
    flooder.create_detection_event(NodeIdx(0));
    flooder.create_detection_event(NodeIdx(1));
    let event = flooder.run_until_next_mwpm_notification();
    match event {
        MwpmEvent::RegionHitRegion { edge, .. } => {
            assert_eq!(edge.obs_mask, 1);
        }
        _ => panic!("Expected RegionHitRegion"),
    }
}

#[test]
fn flooder_boundary_hit() {
    let mut graph = MatchingGraph::new(1, 1);
    graph.add_boundary_edge(0, 5, &[0]);
    let mut flooder = GraphFlooder::new(graph);
    flooder.create_detection_event(NodeIdx(0));
    let event = flooder.run_until_next_mwpm_notification();
    match event {
        MwpmEvent::RegionHitBoundary { edge, .. } => {
            assert_eq!(edge.loc_to, None);
            assert_eq!(edge.obs_mask, 1);
        }
        _ => panic!("Expected RegionHitBoundary"),
    }
}

#[test]
fn flooder_chain_growth() {
    // 3-node chain: 0 --5-- 1 --5-- 2, boundary at 2
    let mut graph = MatchingGraph::new(3, 0);
    graph.add_edge(0, 1, 5, &[]);
    graph.add_edge(1, 2, 5, &[]);
    graph.add_boundary_edge(2, 5, &[]);
    let mut flooder = GraphFlooder::new(graph);
    flooder.create_detection_event(NodeIdx(0));
    let event = flooder.run_until_next_mwpm_notification();
    match event {
        MwpmEvent::RegionHitBoundary { .. } => {}
        _ => panic!("Expected RegionHitBoundary after chain growth"),
    }
}

#[test]
fn flooder_reset() {
    let mut graph = MatchingGraph::new(2, 1);
    graph.add_edge(0, 1, 10, &[0]);
    let mut flooder = GraphFlooder::new(graph);
    flooder.create_detection_event(NodeIdx(0));
    flooder.reset();
    assert!(flooder.graph.nodes[0].region_that_arrived.is_none());
    assert!(flooder.queue.is_empty());
}

#[test]
fn flooder_no_event_on_empty() {
    let graph = MatchingGraph::new(2, 0);
    let mut flooder = GraphFlooder::new(graph);
    let event = flooder.run_until_next_mwpm_notification();
    assert!(event.is_no_event());
}
