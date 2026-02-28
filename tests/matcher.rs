use rmatching::flooder::graph::MatchingGraph;
use rmatching::flooder::graph_flooder::GraphFlooder;
use rmatching::interop::MwpmEvent;
use rmatching::matcher::mwpm::Mwpm;
use rmatching::types::*;

/// Helper: build a simple 2-node graph with one edge of given weight.
fn two_node_mwpm(weight: i32) -> Mwpm {
    let mut g = MatchingGraph::new(2, 1);
    g.add_edge(0, 1, weight, &[0]);
    Mwpm::new(GraphFlooder::new(g))
}

/// Helper: build a 1-node graph with a boundary edge.
fn one_node_boundary_mwpm(weight: i32) -> Mwpm {
    let mut g = MatchingGraph::new(1, 1);
    g.add_boundary_edge(0, weight, &[0]);
    Mwpm::new(GraphFlooder::new(g))
}

#[test]
fn mwpm_two_nodes_match() {
    let mut mwpm = two_node_mwpm(10);

    // Create detection events at both nodes
    mwpm.create_detection_event(NodeIdx(0));
    mwpm.create_detection_event(NodeIdx(1));

    // Run flooder until we get a region-hit-region event
    let event = mwpm.flooder.run_until_next_mwpm_notification();
    match &event {
        MwpmEvent::RegionHitRegion { region1: _, region2: _, edge } => {
            // Two regions should collide
            assert!(edge.obs_mask != 0 || edge.loc_from.is_some());
        }
        other => panic!("Expected RegionHitRegion, got {:?}", other),
    }

    // Process the event — should match the two regions
    mwpm.process_event(event);

    // Both regions should now be frozen with matches
    let r0 = &mwpm.flooder.region_arena[0];
    let r1 = &mwpm.flooder.region_arena[1];
    assert!(r0.match_.is_some());
    assert!(r1.match_.is_some());
    assert!(r0.radius.is_frozen());
    assert!(r1.radius.is_frozen());
}

#[test]
fn mwpm_boundary_match() {
    let mut mwpm = one_node_boundary_mwpm(5);

    // Create detection event at node 0
    mwpm.create_detection_event(NodeIdx(0));

    // Run flooder — should hit boundary
    let event = mwpm.flooder.run_until_next_mwpm_notification();
    match &event {
        MwpmEvent::RegionHitBoundary { region: _, edge } => {
            assert!(edge.loc_to.is_none()); // boundary
        }
        other => panic!("Expected RegionHitBoundary, got {:?}", other),
    }

    // Process the event
    mwpm.process_event(event);

    // Region should be frozen with a boundary match
    let r = &mwpm.flooder.region_arena[0];
    assert!(r.match_.is_some());
    let m = r.match_.as_ref().unwrap();
    assert!(m.region.is_none()); // boundary match
    assert!(r.radius.is_frozen());
}

#[test]
fn mwpm_blossom_formation() {
    // Triangle graph: 3 nodes, 3 edges, all weight 10
    //   0 -- 1
    //   |  /
    //   2
    let mut g = MatchingGraph::new(3, 1);
    g.add_edge(0, 1, 10, &[0]);
    g.add_edge(1, 2, 10, &[]);
    g.add_edge(0, 2, 10, &[]);
    // Add boundary edges so odd-count detection events can resolve
    g.add_boundary_edge(2, 20, &[]);

    let mut mwpm = Mwpm::new(GraphFlooder::new(g));

    // Create detection events at all 3 nodes
    mwpm.create_detection_event(NodeIdx(0));
    mwpm.create_detection_event(NodeIdx(1));
    mwpm.create_detection_event(NodeIdx(2));

    // Process events until no more
    let mut event_count = 0;
    loop {
        let event = mwpm.flooder.run_until_next_mwpm_notification();
        if event.is_no_event() {
            break;
        }
        mwpm.process_event(event);
        event_count += 1;
        if event_count > 20 {
            break; // safety limit
        }
    }

    // With 3 detection events on a triangle + boundary, the algorithm should
    // resolve all regions (either matched to each other or to boundary).
    // At least some events should have been processed.
    assert!(event_count >= 2, "Expected at least 2 events, got {}", event_count);
}

#[test]
fn mwpm_reset() {
    let mut mwpm = two_node_mwpm(10);
    mwpm.create_detection_event(NodeIdx(0));
    mwpm.create_detection_event(NodeIdx(1));

    // Process one event
    let event = mwpm.flooder.run_until_next_mwpm_notification();
    mwpm.process_event(event);

    // Reset
    mwpm.reset();

    // After reset, arenas should be empty
    assert_eq!(mwpm.node_arena.len(), 0);
    assert!(mwpm.flooder.graph.nodes[0].region_that_arrived.is_none());
}

#[test]
fn alt_tree_node_become_root() {
    use rmatching::matcher::alt_tree::{AltTreeEdge, AltTreeNode};
    use rmatching::interop::CompressedEdge;
    use rmatching::util::arena::Arena;

    let mut arena: Arena<AltTreeNode> = Arena::new();

    // Create root (idx 0) with outer_region = RegionIdx(0)
    let root_idx = AltTreeIdx(arena.alloc());
    arena[root_idx.0] = AltTreeNode::new_root(RegionIdx(0));

    // Create child (idx 1) with inner=RegionIdx(1), outer=RegionIdx(2)
    let child_idx = AltTreeIdx(arena.alloc());
    let edge = CompressedEdge {
        loc_from: Some(NodeIdx(0)),
        loc_to: Some(NodeIdx(1)),
        obs_mask: 0,
    };
    arena[child_idx.0] = AltTreeNode::new_pair(RegionIdx(1), RegionIdx(2), edge);
    let child_edge = AltTreeEdge::new(child_idx, edge);
    arena[root_idx.0].children.push(child_edge);
    arena[child_idx.0].parent = Some(AltTreeEdge::new(root_idx, edge.reversed()));

    // Make child the root
    AltTreeNode::become_root(child_idx, &mut arena);

    // Child should now be root (no parent)
    assert!(arena[child_idx.0].parent.is_none());
    // Old root should be a child of new root
    assert_eq!(arena[child_idx.0].children.len(), 1);
    assert_eq!(arena[child_idx.0].children[0].alt_tree_node, root_idx);
}

#[test]
fn alt_tree_most_recent_common_ancestor() {
    use rmatching::matcher::alt_tree::{AltTreeEdge, AltTreeNode};
    use rmatching::interop::CompressedEdge;
    use rmatching::util::arena::Arena;

    let mut arena: Arena<AltTreeNode> = Arena::new();
    let e = CompressedEdge::empty();

    // Build tree: root -> child1, root -> child2
    let root = AltTreeIdx(arena.alloc());
    arena[root.0] = AltTreeNode::new_root(RegionIdx(0));

    let c1 = AltTreeIdx(arena.alloc());
    arena[c1.0] = AltTreeNode::new_pair(RegionIdx(1), RegionIdx(2), e);
    arena[root.0].children.push(AltTreeEdge::new(c1, e));
    arena[c1.0].parent = Some(AltTreeEdge::new(root, e));

    let c2 = AltTreeIdx(arena.alloc());
    arena[c2.0] = AltTreeNode::new_pair(RegionIdx(3), RegionIdx(4), e);
    arena[root.0].children.push(AltTreeEdge::new(c2, e));
    arena[c2.0].parent = Some(AltTreeEdge::new(root, e));

    // LCA of c1 and c2 should be root
    let lca = AltTreeNode::most_recent_common_ancestor(c1, c2, &mut arena);
    assert_eq!(lca, Some(root));
}
