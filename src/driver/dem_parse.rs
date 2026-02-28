use crate::driver::user_graph::UserGraph;

/// Parse a Stim Detector Error Model (DEM) text into a `UserGraph`.
///
/// Handles: `error(p) D<i> ...`, `detector D<i>`, `repeat N { ... }`,
/// comments (`#`), blank lines, `^` separator, and unknown instructions.
pub fn parse_dem(text: &str) -> Result<UserGraph, String> {
    let mut graph = UserGraph::new();
    let lines: Vec<&str> = text.lines().collect();
    parse_block(&lines, &mut graph, 0)?;
    Ok(graph)
}

/// Parse a slice of lines into `graph`, applying `detector_offset` to all D indices.
/// Returns the max detector index seen (before offset) so repeat can compute shifts.
fn parse_block(
    lines: &[&str],
    graph: &mut UserGraph,
    detector_offset: usize,
) -> Result<usize, String> {
    let mut max_detector: usize = 0;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        // Skip blank lines and comments
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        if line.starts_with("error") {
            let det = parse_error_line(line, graph, detector_offset)?;
            max_detector = max_detector.max(det);
        } else if line.starts_with("detector") {
            let det = parse_detector_line(line, graph, detector_offset)?;
            max_detector = max_detector.max(det);
        } else if line.starts_with("repeat") {
            let (det, consumed) =
                parse_repeat(lines, i, graph, detector_offset)?;
            max_detector = max_detector.max(det);
            i += consumed;
            continue;
        }
        // All other instructions (shift_detectors, logical_observable, etc.) are skipped.
        i += 1;
    }
    Ok(max_detector)
}

/// Parse an `error(p) D<i> [D<j>] [L<k>...] [^ ...]` line.
/// Returns the max raw detector index seen (before offset).
fn parse_error_line(
    line: &str,
    graph: &mut UserGraph,
    detector_offset: usize,
) -> Result<usize, String> {
    // Truncate at `^` separator (correlated errors — we ignore the second half)
    let line = line.split('^').next().unwrap_or(line);

    // Extract probability from error(p)
    let open = line.find('(').ok_or("error line missing '('")?;
    let close = line.find(')').ok_or("error line missing ')'")?;
    let p: f64 = line[open + 1..close]
        .trim()
        .parse()
        .map_err(|e| format!("bad probability: {e}"))?;

    let mut detectors = Vec::new();
    let mut observables = Vec::new();
    let mut max_det: usize = 0;

    for token in line[close + 1..].split_whitespace() {
        if let Some(rest) = token.strip_prefix('D') {
            let idx: usize = rest.parse().map_err(|e| format!("bad detector index: {e}"))?;
            max_det = max_det.max(idx);
            detectors.push(idx + detector_offset);
        } else if let Some(rest) = token.strip_prefix('L') {
            let idx: usize = rest.parse().map_err(|e| format!("bad observable index: {e}"))?;
            observables.push(idx);
        }
    }

    graph.handle_dem_instruction(p, &detectors, observables);
    Ok(max_det)
}

/// Parse a `detector D<i> [coords...]` line. Ensures the node exists.
/// Returns the raw detector index (before offset).
fn parse_detector_line(
    line: &str,
    graph: &mut UserGraph,
    detector_offset: usize,
) -> Result<usize, String> {
    for token in line.split_whitespace().skip(1) {
        if let Some(rest) = token.strip_prefix('D') {
            let idx: usize = rest.parse().map_err(|e| format!("bad detector index: {e}"))?;
            // Ensure the node exists in the graph (coordinates are ignored)
            let shifted = idx + detector_offset;
            if shifted >= graph.nodes.len() {
                graph.nodes.resize_with(shifted + 1, Default::default);
            }
            return Ok(idx);
        }
    }
    Ok(0)
}

/// Parse a `repeat N { ... }` block starting at `lines[start]`.
/// Returns (max_detector_in_block, number_of_lines_consumed).
fn parse_repeat(
    lines: &[&str],
    start: usize,
    graph: &mut UserGraph,
    detector_offset: usize,
) -> Result<(usize, usize), String> {
    let header = lines[start].trim();
    // Parse repeat count
    let count: usize = header
        .split_whitespace()
        .nth(1)
        .ok_or("repeat missing count")?
        .parse()
        .map_err(|e| format!("bad repeat count: {e}"))?;

    // Find the matching closing brace, collecting body lines
    let mut body_lines = Vec::new();
    let mut depth = 0u32;
    let mut end = start;

    for (j, &l) in lines[start..].iter().enumerate() {
        let trimmed = l.trim();
        if trimmed.contains('{') {
            depth += 1;
        }
        if trimmed.contains('}') {
            depth -= 1;
            if depth == 0 {
                end = start + j;
                break;
            }
        }
        // Collect lines inside the braces (skip the header line itself)
        if j > 0 && depth > 0 {
            body_lines.push(l);
        }
    }

    // Determine the shift per iteration: find max detector in one pass of the body,
    // then use shift = max_det_in_body + 1.
    // We also look for explicit `shift_detectors` instructions inside the body.
    let explicit_shift = find_shift_detectors(&body_lines);

    // First pass to discover max detector in the body (at offset 0)
    let mut scratch = UserGraph::new();
    let max_det_in_body = parse_block(&body_lines, &mut scratch, 0)?;

    let shift_per_iter = explicit_shift.unwrap_or(max_det_in_body + 1);

    // Now replay the body `count` times with increasing offset
    let mut overall_max = 0usize;
    for iteration in 0..count {
        let iter_offset = detector_offset + iteration * shift_per_iter;
        let det = parse_block(&body_lines, graph, iter_offset)?;
        let absolute = det + iter_offset;
        overall_max = overall_max.max(absolute);
    }

    // Lines consumed = from start to end (inclusive)
    Ok((overall_max, end - start + 1))
}

/// Look for `shift_detectors N` inside a block and return the value if found.
fn find_shift_detectors(lines: &[&str]) -> Option<usize> {
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("shift_detectors") {
            if let Some(val) = trimmed.split_whitespace().nth(1) {
                return val.parse().ok();
            }
        }
    }
    None
}
