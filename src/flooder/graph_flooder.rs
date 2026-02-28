use std::num::Wrapping;

use crate::interop::*;
use crate::types::*;
use crate::util::arena::Arena;
use crate::util::radix_heap::{HasTime, RadixHeapQueue};
use crate::util::varying::VaryingCT;

use super::fill_region::GraphFillRegion;
use super::graph::{MatchingGraph, BOUNDARY_NODE};

pub struct GraphFlooder {
    pub graph: MatchingGraph,
    pub region_arena: Arena<GraphFillRegion>,
    pub queue: RadixHeapQueue<FloodCheckEvent>,
    pub match_edges: Vec<CompressedEdge>,
}

impl GraphFlooder {
    pub fn new(graph: MatchingGraph) -> Self {
        GraphFlooder {
            graph,
            region_arena: Arena::new(),
            queue: RadixHeapQueue::new(),
            match_edges: Vec::new(),
        }
    }

    // ---------------------------------------------------------------
    // Detection event creation
    // ---------------------------------------------------------------

    pub fn create_detection_event(&mut self, node_idx: NodeIdx) -> RegionIdx {
        let region_idx = RegionIdx(self.region_arena.alloc());
        {
            let region = self.region_arena.get_mut(region_idx.0);
            region.radius =
                VaryingCT::growing_varying_with_zero_distance_at_time(self.queue.cur_time);
            region.shell_area.push(node_idx);
        }

        let node = &mut self.graph.nodes[node_idx.0 as usize];
        node.region_that_arrived = Some(region_idx);
        node.region_that_arrived_top = Some(region_idx);
        node.reached_from_source = Some(node_idx);
        node.observables_crossed_from_source = 0;
        node.radius_of_arrival = 0;
        node.wrapped_radius_cached = 0;

        self.reschedule_events_at_detector_node(node_idx);
        region_idx
    }

    // ---------------------------------------------------------------
    // Main loop
    // ---------------------------------------------------------------

    pub fn run_until_next_mwpm_notification(&mut self) -> MwpmEvent {
        loop {
            let event = self.dequeue_valid();
            if event.is_no_event() {
                return MwpmEvent::NoEvent;
            }
            let notification = self.process_tentative_event(event);
            if !notification.is_no_event() {
                return notification;
            }
        }
    }

    /// Dequeue events, skipping stale ones, until we get a valid one or the queue is empty.
    fn dequeue_valid(&mut self) -> FloodCheckEvent {
        loop {
            let ev = self.queue.dequeue();
            if ev.is_no_event() {
                return ev;
            }
            if self.dequeue_decision(&ev) {
                return ev;
            }
        }
    }

    /// Check whether a dequeued event is still valid (not stale).
    fn dequeue_decision(&mut self, ev: &FloodCheckEvent) -> bool {
        match ev {
            FloodCheckEvent::LookAtNode { node, .. } => {
                let node_idx = *node;
                let tracker = &mut self.graph.nodes[node_idx.0 as usize].node_event_tracker;
                tracker.dequeue_decision(ev, &mut self.queue, |time| {
                    FloodCheckEvent::LookAtNode { node: node_idx, time }
                })
            }
            FloodCheckEvent::LookAtShrinkingRegion { region, .. } => {
                let region_idx = *region;
                let tracker = &mut self.region_arena[region_idx.0].shrink_event_tracker;
                tracker.dequeue_decision(ev, &mut self.queue, |time| {
                    FloodCheckEvent::LookAtShrinkingRegion { region: region_idx, time }
                })
            }
            FloodCheckEvent::NoEvent => true,
            _ => false,
        }
    }

    fn process_tentative_event(&mut self, event: FloodCheckEvent) -> MwpmEvent {
        match event {
            FloodCheckEvent::LookAtNode { node, .. } => self.do_look_at_node_event(node),
            FloodCheckEvent::LookAtShrinkingRegion { region, .. } => {
                self.do_region_shrinking(region)
            }
            _ => MwpmEvent::NoEvent,
        }
    }

    // ---------------------------------------------------------------
    // Core node event processing (mirrors PyMatching do_look_at_node_event)
    // ---------------------------------------------------------------

    fn do_look_at_node_event(&mut self, node_idx: NodeIdx) -> MwpmEvent {
        let (best_neighbor, best_time) = self.find_next_event_at_node(node_idx);

        if best_time == self.queue.cur_time {
            // Event is happening NOW. Reschedule immediately so we revisit for other edges.
            let event = FloodCheckEvent::LookAtNode {
                node: node_idx,
                time: Wrapping(self.queue.cur_time as u32),
            };
            self.graph.nodes[node_idx.0 as usize]
                .node_event_tracker
                .set_desired_event(event, &mut self.queue);

            let neighbor_node_idx = self.graph.nodes[node_idx.0 as usize].neighbors[best_neighbor];

            if neighbor_node_idx == BOUNDARY_NODE {
                return self.do_region_hit_boundary(node_idx);
            }
            return self.do_neighbor_interaction(node_idx, best_neighbor, neighbor_node_idx);
        } else if best_neighbor != NO_NEIGHBOR {
            // Future event — schedule it.
            let event = FloodCheckEvent::LookAtNode {
                node: node_idx,
                time: Wrapping(best_time as u32),
            };
            self.graph.nodes[node_idx.0 as usize]
                .node_event_tracker
                .set_desired_event(event, &mut self.queue);
        }

        MwpmEvent::NoEvent
    }

    // ---------------------------------------------------------------
    // Neighbor interaction (grow or collide)
    // ---------------------------------------------------------------

    fn do_neighbor_interaction(
        &mut self,
        src_idx: NodeIdx,
        src_to_dst_index: usize,
        dst_idx: NodeIdx,
    ) -> MwpmEvent {
        let src_has_region = self.graph.nodes[src_idx.0 as usize].region_that_arrived.is_some();
        let dst_has_region = self.graph.nodes[dst_idx.0 as usize].region_that_arrived.is_some();

        if src_has_region && !dst_has_region {
            // Grow into empty neighbor
            self.do_region_arriving_at_empty_node(dst_idx, src_idx, src_to_dst_index);
            return MwpmEvent::NoEvent;
        } else if dst_has_region && !src_has_region {
            // Reverse: dst grows into empty src
            let dst_to_src_index = self.index_of_neighbor(dst_idx, src_idx);
            self.do_region_arriving_at_empty_node(src_idx, dst_idx, dst_to_src_index);
            return MwpmEvent::NoEvent;
        }

        // Two regions colliding
        let src = &self.graph.nodes[src_idx.0 as usize];
        let dst = &self.graph.nodes[dst_idx.0 as usize];
        let obs = src.neighbor_observables[src_to_dst_index];
        let edge = CompressedEdge {
            loc_from: src.reached_from_source,
            loc_to: dst.reached_from_source,
            obs_mask: src.observables_crossed_from_source
                ^ dst.observables_crossed_from_source
                ^ obs,
        };
        MwpmEvent::RegionHitRegion {
            region1: src.region_that_arrived_top.unwrap(),
            region2: dst.region_that_arrived_top.unwrap(),
            edge,
        }
    }

    fn do_region_hit_boundary(&self, node_idx: NodeIdx) -> MwpmEvent {
        let node = &self.graph.nodes[node_idx.0 as usize];
        // Boundary edge is the neighbor whose NodeIdx == BOUNDARY_NODE.
        // Find the boundary neighbor index.
        let boundary_idx = node
            .neighbors
            .iter()
            .position(|n| *n == BOUNDARY_NODE)
            .unwrap();
        let edge = CompressedEdge {
            loc_from: node.reached_from_source,
            loc_to: None,
            obs_mask: node.observables_crossed_from_source
                ^ node.neighbor_observables[boundary_idx],
        };
        MwpmEvent::RegionHitBoundary {
            region: node.region_that_arrived_top.unwrap(),
            edge,
        }
    }

    // ---------------------------------------------------------------
    // Region growth into an empty node
    // ---------------------------------------------------------------

    fn do_region_arriving_at_empty_node(
        &mut self,
        empty_node_idx: NodeIdx,
        from_node_idx: NodeIdx,
        from_to_empty_index: usize,
    ) {
        // Read from the source node
        let from_node = &self.graph.nodes[from_node_idx.0 as usize];
        let obs = from_node.neighbor_observables[from_to_empty_index];
        let obs_crossed = from_node.observables_crossed_from_source ^ obs;
        let source = from_node.reached_from_source;
        let region = from_node.region_that_arrived;
        let region_top = from_node.region_that_arrived_top;

        // Compute radius_of_arrival from the top region's current radius
        let radius_of_arrival = if let Some(top) = region_top {
            self.region_arena[top.0]
                .radius
                .get_distance_at_time(self.queue.cur_time)
        } else {
            0
        };

        // Write to the empty node
        let empty_node = &mut self.graph.nodes[empty_node_idx.0 as usize];
        empty_node.observables_crossed_from_source = obs_crossed;
        empty_node.reached_from_source = source;
        empty_node.radius_of_arrival = radius_of_arrival;
        empty_node.region_that_arrived = region;
        empty_node.region_that_arrived_top = region_top;
        empty_node.wrapped_radius_cached =
            empty_node.compute_wrapped_radius(self.region_arena.items());

        // Add to region's shell area
        if let Some(r) = region_top {
            self.region_arena.get_mut(r.0).shell_area.push(empty_node_idx);
        }

        self.reschedule_events_at_detector_node(empty_node_idx);
    }

    // ---------------------------------------------------------------
    // Find next event at a node
    // ---------------------------------------------------------------

    fn find_next_event_at_node(&self, node_idx: NodeIdx) -> (usize, CumulativeTime) {
        let node = &self.graph.nodes[node_idx.0 as usize];
        let rad1 = node.local_radius(self.region_arena.items());

        if rad1.is_growing() {
            self.find_next_event_growing(node, &rad1)
        } else {
            self.find_next_event_not_growing(node, &rad1)
        }
    }

    /// When the node's top region is growing: check boundary, unoccupied, and other-region neighbors.
    fn find_next_event_growing(
        &self,
        node: &super::detector_node::DetectorNode,
        rad1: &VaryingCT,
    ) -> (usize, CumulativeTime) {
        let mut best_time = i64::MAX;
        let mut best_neighbor = NO_NEIGHBOR;

        for i in 0..node.neighbors.len() {
            let neighbor_idx = node.neighbors[i];
            let weight = node.neighbor_weights[i] as CumulativeTime;

            if neighbor_idx == BOUNDARY_NODE {
                let collision_time = weight - rad1.y_intercept();
                if collision_time < best_time {
                    best_time = collision_time;
                    best_neighbor = i;
                }
                continue;
            }

            let neighbor = &self.graph.nodes[neighbor_idx.0 as usize];
            if node.has_same_owner_as(neighbor) {
                continue;
            }

            let rad2 = neighbor.local_radius(self.region_arena.items());
            if rad2.is_shrinking() {
                continue;
            }

            let mut collision_time = weight - rad1.y_intercept() - rad2.y_intercept();
            if rad2.is_growing() {
                collision_time >>= 1; // Both growing: combined slope = 2
            }
            if collision_time < best_time {
                best_time = collision_time;
                best_neighbor = i;
            }
        }

        (best_neighbor, best_time)
    }

    /// When the node's top region is NOT growing (frozen/shrinking):
    /// only look for growing neighbors colliding into this node.
    fn find_next_event_not_growing(
        &self,
        node: &super::detector_node::DetectorNode,
        _rad1: &VaryingCT,
    ) -> (usize, CumulativeTime) {
        let mut best_time = i64::MAX;
        let mut best_neighbor = NO_NEIGHBOR;

        // Skip boundary neighbors (index 0 if it's boundary) since we're not growing
        let start = if !node.neighbors.is_empty() && node.neighbors[0] == BOUNDARY_NODE {
            1
        } else {
            0
        };

        for i in start..node.neighbors.len() {
            let neighbor_idx = node.neighbors[i];
            if neighbor_idx == BOUNDARY_NODE {
                continue;
            }
            let weight = node.neighbor_weights[i] as CumulativeTime;
            let neighbor = &self.graph.nodes[neighbor_idx.0 as usize];
            let rad2 = neighbor.local_radius(self.region_arena.items());

            if rad2.is_growing() {
                let collision_time = weight - _rad1.y_intercept() - rad2.y_intercept();
                if collision_time < best_time {
                    best_time = collision_time;
                    best_neighbor = i;
                }
            }
        }

        (best_neighbor, best_time)
    }

    // ---------------------------------------------------------------
    // Reschedule events at a detector node
    // ---------------------------------------------------------------

    pub fn reschedule_events_at_detector_node(&mut self, node_idx: NodeIdx) {
        let (best_neighbor, best_time) = self.find_next_event_at_node(node_idx);
        let node = &mut self.graph.nodes[node_idx.0 as usize];
        if best_neighbor == NO_NEIGHBOR {
            node.node_event_tracker.set_no_desired_event();
        } else {
            let event = FloodCheckEvent::LookAtNode {
                node: node_idx,
                time: Wrapping(best_time as u32),
            };
            node.node_event_tracker
                .set_desired_event(event, &mut self.queue);
        }
    }

    // ---------------------------------------------------------------
    // Region state transitions
    // ---------------------------------------------------------------

    pub fn set_region_growing(&mut self, region_idx: RegionIdx) {
        let region = self.region_arena.get_mut(region_idx.0);
        region.radius = region.radius.then_growing_at_time(self.queue.cur_time);
        region.shrink_event_tracker.set_no_desired_event();
        let shell: Vec<NodeIdx> = region.shell_area.clone();
        for node_idx in shell {
            self.reschedule_events_at_detector_node(node_idx);
        }
    }

    pub fn set_region_frozen(&mut self, region_idx: RegionIdx) {
        let region = self.region_arena.get_mut(region_idx.0);
        let was_shrinking = region.radius.is_shrinking();
        region.radius = region.radius.then_frozen_at_time(self.queue.cur_time);
        region.shrink_event_tracker.set_no_desired_event();
        if was_shrinking {
            let shell: Vec<NodeIdx> = region.shell_area.clone();
            for node_idx in shell {
                self.reschedule_events_at_detector_node(node_idx);
            }
        }
    }

    pub fn set_region_shrinking(&mut self, region_idx: RegionIdx) {
        let region = self.region_arena.get_mut(region_idx.0);
        region.radius = region.radius.then_shrinking_at_time(self.queue.cur_time);
        // Schedule tentative shrink event
        self.schedule_tentative_shrink_event(region_idx);
        // No node events while shrinking
        let shell: Vec<NodeIdx> = self.region_arena[region_idx.0].shell_area.clone();
        for node_idx in shell {
            self.graph.nodes[node_idx.0 as usize]
                .node_event_tracker
                .set_no_desired_event();
        }
    }

    fn schedule_tentative_shrink_event(&mut self, region_idx: RegionIdx) {
        let region = &self.region_arena[region_idx.0];
        let t = if region.shell_area.is_empty() {
            region.radius.time_of_x_intercept()
        } else {
            let last_node_idx = *region.shell_area.last().unwrap();
            let last_node = &self.graph.nodes[last_node_idx.0 as usize];
            last_node
                .local_radius(self.region_arena.items())
                .time_of_x_intercept()
        };
        let event = FloodCheckEvent::LookAtShrinkingRegion {
            region: region_idx,
            time: Wrapping(t as u32),
        };
        self.region_arena
            .get_mut(region_idx.0)
            .shrink_event_tracker
            .set_desired_event(event, &mut self.queue);
    }

    // ---------------------------------------------------------------
    // Region shrinking
    // ---------------------------------------------------------------

    fn do_region_shrinking(&mut self, region_idx: RegionIdx) -> MwpmEvent {
        let region = &self.region_arena[region_idx.0];
        if region.shell_area.is_empty() {
            // Blossom shattering — return event for matcher
            return self.do_blossom_shattering(region_idx);
        }

        // Remove the last node from the shell
        let leaving_node_idx = {
            let region = self.region_arena.get_mut(region_idx.0);
            region.shell_area.pop().unwrap()
        };

        let leaving = &mut self.graph.nodes[leaving_node_idx.0 as usize];
        leaving.region_that_arrived = None;
        leaving.region_that_arrived_top = None;
        leaving.wrapped_radius_cached = 0;
        leaving.reached_from_source = None;
        leaving.radius_of_arrival = 0;
        leaving.observables_crossed_from_source = 0;

        self.reschedule_events_at_detector_node(leaving_node_idx);
        self.schedule_tentative_shrink_event(region_idx);

        MwpmEvent::NoEvent
    }

    fn do_blossom_shattering(&self, region_idx: RegionIdx) -> MwpmEvent {
        // Placeholder: full blossom shattering requires AltTreeNode (Task 6).
        // For now, return NoEvent.
        let _ = region_idx;
        MwpmEvent::NoEvent
    }

    // ---------------------------------------------------------------
    // Reset
    // ---------------------------------------------------------------

    pub fn reset(&mut self) {
        for node in &mut self.graph.nodes {
            node.reset();
        }
        self.region_arena.clear();
        self.queue.reset();
        self.match_edges.clear();
    }

    // ---------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------

    fn index_of_neighbor(&self, node_idx: NodeIdx, target: NodeIdx) -> usize {
        self.graph.nodes[node_idx.0 as usize]
            .neighbors
            .iter()
            .position(|n| *n == target)
            .expect("neighbor not found")
    }
}
