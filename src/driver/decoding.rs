use crate::driver::dem_parse::parse_dem;
use crate::driver::user_graph::UserGraph;
use crate::matcher::mwpm::{MatchingResult, Mwpm};
use crate::types::*;

/// Public-facing decoder wrapping a `UserGraph` and its cached `Mwpm`.
pub struct Matching {
    user_graph: UserGraph,
}

impl Matching {
    /// Build a `Matching` from a Stim DEM text string.
    pub fn from_dem(dem_text: &str) -> Result<Self, String> {
        let user_graph = parse_dem(dem_text)?;
        Ok(Matching { user_graph })
    }

    /// Create an empty `Matching` (edges added manually).
    pub fn new() -> Self {
        Matching {
            user_graph: UserGraph::new(),
        }
    }

    pub fn add_edge(
        &mut self,
        n1: usize,
        n2: usize,
        weight: f64,
        observables: &[usize],
        error_probability: f64,
    ) {
        self.user_graph
            .add_edge(n1, n2, observables.to_vec(), weight, error_probability);
    }

    pub fn add_boundary_edge(
        &mut self,
        node: usize,
        weight: f64,
        observables: &[usize],
        error_probability: f64,
    ) {
        self.user_graph
            .add_boundary_edge(node, observables.to_vec(), weight, error_probability);
    }

    pub fn set_boundary(&mut self, boundary: &[usize]) {
        self.user_graph
            .set_boundary(boundary.iter().copied().collect());
    }

    /// Decode a syndrome bit-vector into observable predictions.
    ///
    /// `syndrome` has one byte per detector; non-zero means that detector fired.
    /// Returns one byte per observable (0 or 1).
    pub fn decode(&mut self, syndrome: &[u8]) -> Vec<u8> {
        let mwpm = self.user_graph.get_mwpm();
        let num_observables = mwpm.flooder.graph.num_observables;

        // 1. Convert syndrome bytes to detection event indices
        let detection_events = syndrome_to_detection_events(syndrome);

        // 2. Compute negative-weight obs mask from the set
        let neg_obs_mask = compute_neg_obs_mask(&mwpm.flooder.graph.negative_weight_observables_set);

        // 3. XOR detection events with negative weight detection events (symmetric difference)
        let effective_events = apply_negative_weight_events(
            &detection_events,
            &mwpm.flooder.graph.negative_weight_detection_events_set,
            &mwpm.flooder.graph.is_user_graph_boundary_node,
        );

        // 4. Run the matching
        process_timeline_until_completion(mwpm, &effective_events);

        // 5. Extract obs_mask from matched regions
        let mut res = shatter_and_extract(mwpm, &effective_events);

        // 6. XOR with negative weight obs mask
        res.obs_mask ^= neg_obs_mask;

        // 7. Convert obs_mask to byte vector
        let predictions = obs_mask_to_predictions(res.obs_mask, num_observables);

        // 8. Reset for next decode
        mwpm.reset();

        predictions
    }

    /// Decode multiple syndromes. Each result matches `decode` on the same input.
    pub fn decode_batch(&mut self, syndromes: &[Vec<u8>]) -> Vec<Vec<u8>> {
        syndromes.iter().map(|s| self.decode(s)).collect()
    }

    /// Decode a syndrome and return matched pairs as `(node1, node2)`.
    /// Boundary matches use `-1` for the boundary node.
    pub fn decode_to_edges(&mut self, syndrome: &[u8]) -> Vec<(i64, i64)> {
        let mwpm = self.user_graph.get_mwpm();

        let detection_events = syndrome_to_detection_events(syndrome);

        let effective_events = apply_negative_weight_events(
            &detection_events,
            &mwpm.flooder.graph.negative_weight_detection_events_set,
            &mwpm.flooder.graph.is_user_graph_boundary_node,
        );

        process_timeline_until_completion(mwpm, &effective_events);

        let edges = extract_match_edges(mwpm, &effective_events);

        mwpm.reset();

        edges
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn syndrome_to_detection_events(syndrome: &[u8]) -> Vec<usize> {
    syndrome
        .iter()
        .enumerate()
        .filter(|(_, v)| **v != 0)
        .map(|(i, _)| i)
        .collect()
}

fn compute_neg_obs_mask(neg_obs_set: &std::collections::HashSet<usize>) -> ObsMask {
    let mut mask: ObsMask = 0;
    for &obs in neg_obs_set {
        mask ^= 1u64 << obs;
    }
    mask
}

/// Compute the symmetric difference of detection events and negative-weight
/// detection events, filtering out user-graph boundary nodes.
fn apply_negative_weight_events(
    detection_events: &[usize],
    neg_det_set: &std::collections::HashSet<usize>,
    is_boundary: &[bool],
) -> Vec<usize> {
    if neg_det_set.is_empty() {
        // Fast path: filter out boundary nodes only
        return detection_events
            .iter()
            .copied()
            .filter(|&d| d >= is_boundary.len() || !is_boundary[d])
            .collect();
    }

    // Symmetric difference via XOR-toggle in a set
    let mut active: std::collections::HashSet<usize> = detection_events.iter().copied().collect();
    for &d in neg_det_set {
        if !active.remove(&d) {
            active.insert(d);
        }
    }

    let mut result: Vec<usize> = active
        .into_iter()
        .filter(|&d| d >= is_boundary.len() || !is_boundary[d])
        .collect();
    result.sort_unstable();
    result
}

fn process_timeline_until_completion(mwpm: &mut Mwpm, detection_events: &[usize]) {
    // Reset queue time
    mwpm.flooder.queue.cur_time = 0;

    let num_nodes = mwpm.flooder.graph.nodes.len();

    for &det in detection_events {
        if det >= num_nodes {
            // Skip out-of-range detection events
            continue;
        }
        mwpm.create_detection_event(NodeIdx(det as u32));
    }

    loop {
        let event = mwpm.flooder.run_until_next_mwpm_notification();
        if event.is_no_event() {
            break;
        }
        mwpm.process_event(event);
    }
}

fn shatter_and_extract(mwpm: &mut Mwpm, detection_events: &[usize]) -> MatchingResult {
    let mut res = MatchingResult::new();
    for &i in detection_events {
        if i < mwpm.flooder.graph.nodes.len()
            && mwpm.flooder.graph.nodes[i].region_that_arrived.is_some()
        {
            let top = mwpm.flooder.graph.nodes[i].region_that_arrived_top.unwrap();
            // Collect shell-area nodes to reset *after* shattering, since
            // pair_and_shatter_subblossoms needs region_that_arrived_top to
            // locate sub-blossoms.
            let mut nodes_to_clean = collect_shell_nodes(mwpm, top);
            let match_region = mwpm.flooder.region_arena[top.0]
                .match_
                .as_ref()
                .and_then(|m| m.region);
            if let Some(mr) = match_region {
                nodes_to_clean.extend(collect_shell_nodes(mwpm, mr));
            }
            // Shattering reads region_that_arrived_top, so run it first.
            res += mwpm.shatter_blossom_and_extract_matches(top);
            // Now reset the nodes to prevent double-processing.
            for node_idx in nodes_to_clean {
                mwpm.flooder.graph.nodes[node_idx.0 as usize].reset();
            }
        }
    }
    res
}

/// Collect all detector-node indices in a region's shell area (and its
/// blossom children, recursively) so they can be reset after shattering.
fn collect_shell_nodes(mwpm: &Mwpm, region: RegionIdx) -> Vec<NodeIdx> {
    let mut nodes = Vec::new();
    collect_shell_nodes_recursive(mwpm, region, &mut nodes);
    nodes
}

fn collect_shell_nodes_recursive(mwpm: &Mwpm, region: RegionIdx, out: &mut Vec<NodeIdx>) {
    out.extend(mwpm.flooder.region_arena[region.0].shell_area.iter().copied());
    for child in &mwpm.flooder.region_arena[region.0].blossom_children {
        collect_shell_nodes_recursive(mwpm, child.region, out);
    }
}

fn extract_match_edges(mwpm: &mut Mwpm, detection_events: &[usize]) -> Vec<(i64, i64)> {
    let mut edges = Vec::new();
    for &i in detection_events {
        if i < mwpm.flooder.graph.nodes.len()
            && mwpm.flooder.graph.nodes[i].region_that_arrived.is_some()
        {
            let top = mwpm.flooder.graph.nodes[i].region_that_arrived_top.unwrap();
            let region = &mwpm.flooder.region_arena[top.0];
            if let Some(ref m) = region.match_ {
                let from = i as i64;
                let to = match m.edge.loc_to {
                    Some(node_idx) => node_idx.0 as i64,
                    None => -1,
                };
                // Avoid duplicate edges: only add if from <= to (or boundary)
                if to == -1 || from <= to {
                    edges.push((from, to));
                }
            }
        }
    }
    edges
}

fn obs_mask_to_predictions(obs_mask: ObsMask, num_observables: usize) -> Vec<u8> {
    (0..num_observables)
        .map(|i| ((obs_mask >> i) & 1) as u8)
        .collect()
}
