use crate::interop::QueuedEventTracker;
use crate::types::*;
use crate::util::varying::VaryingCT;

use super::fill_region::GraphFillRegion;

#[derive(Debug, Clone)]
pub struct DetectorNode {
    // Permanent (graph structure)
    pub neighbors: Vec<NodeIdx>,
    pub neighbor_weights: Vec<Weight>,
    pub neighbor_observables: Vec<ObsMask>,
    // Ephemeral (reset between decodes)
    pub region_that_arrived: Option<RegionIdx>,
    pub region_that_arrived_top: Option<RegionIdx>,
    pub reached_from_source: Option<NodeIdx>,
    pub observables_crossed_from_source: ObsMask,
    pub radius_of_arrival: CumulativeTime,
    pub wrapped_radius_cached: i32,
    pub node_event_tracker: QueuedEventTracker,
}

impl Default for DetectorNode {
    fn default() -> Self {
        DetectorNode {
            neighbors: Vec::new(),
            neighbor_weights: Vec::new(),
            neighbor_observables: Vec::new(),
            region_that_arrived: None,
            region_that_arrived_top: None,
            reached_from_source: None,
            observables_crossed_from_source: 0,
            radius_of_arrival: 0,
            wrapped_radius_cached: 0,
            node_event_tracker: QueuedEventTracker::default(),
        }
    }
}

impl DetectorNode {
    pub fn new() -> Self {
        Self::default()
    }

    /// The local radius at this node = top_region.radius + wrapped_radius_cached
    pub fn local_radius(&self, regions: &[GraphFillRegion]) -> VaryingCT {
        match self.region_that_arrived_top {
            None => VaryingCT::frozen(0),
            Some(top_idx) => {
                regions[top_idx.0 as usize].radius + self.wrapped_radius_cached as i64
            }
        }
    }

    /// Walk blossom hierarchy to compute wrapped radius
    pub fn compute_wrapped_radius(&self, regions: &[GraphFillRegion]) -> i32 {
        if self.reached_from_source.is_none() {
            return 0;
        }
        let mut total: i32 = 0;
        let mut r = self.region_that_arrived;
        while r != self.region_that_arrived_top {
            if let Some(idx) = r {
                total += regions[idx.0 as usize].radius.y_intercept() as i32;
                r = regions[idx.0 as usize].blossom_parent;
            } else {
                break;
            }
        }
        total - self.radius_of_arrival as i32
    }

    pub fn has_same_owner_as(&self, other: &DetectorNode) -> bool {
        self.region_that_arrived_top.is_some()
            && self.region_that_arrived_top == other.region_that_arrived_top
    }

    pub fn reset(&mut self) {
        self.region_that_arrived = None;
        self.region_that_arrived_top = None;
        self.reached_from_source = None;
        self.observables_crossed_from_source = 0;
        self.radius_of_arrival = 0;
        self.wrapped_radius_cached = 0;
        self.node_event_tracker.clear();
    }
}
