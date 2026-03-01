# rmatching

[![CI](https://github.com/nzy1997/rmatching/actions/workflows/ci.yml/badge.svg)](https://github.com/nzy1997/rmatching/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/nzy1997/rmatching/branch/main/graph/badge.svg)](https://codecov.io/gh/nzy1997/rmatching)

A Rust implementation of the Sparse Blossom minimum-weight perfect matching (MWPM) decoder for quantum error correction, ported from [PyMatching](https://github.com/oscarhiggott/PyMatching).

## Features

- Full Sparse Blossom algorithm with alternating trees and blossom contraction/shattering
- Standalone DEM (Detector Error Model) text parser — no external dependencies
- Negative edge weight support
- Decode API: `decode`, `decode_batch`, `decode_to_edges`
- Optional [rsinter](https://github.com/nzy1997/rstim) `Decoder` trait integration behind `rsinter` feature flag

## Quick Start

```rust
use rmatching::Matching;

// From a DEM string
let mut m = Matching::from_dem("error(0.1) D0 D1 L0\nerror(0.1) D0\nerror(0.1) D1\n").unwrap();
let prediction = m.decode(&[1, 1]);
assert_eq!(prediction, vec![1]);

// Or build manually
let mut m = Matching::new();
m.add_edge(0, 1, 2.2, &[0], 0.1);
m.add_boundary_edge(0, 2.2, &[0], 0.1);
m.add_boundary_edge(1, 2.2, &[], 0.1);
let prediction = m.decode(&[1, 0]);
```

## Architecture

| Module | Description |
|--------|-------------|
| `util` | Varying (time-varying values), Arena (index-based allocator), RadixHeapQueue |
| `flooder` | DetectorNode, MatchingGraph, GraphFillRegion, GraphFlooder |
| `matcher` | AltTreeNode (alternating trees), Mwpm (MWPM solver) |
| `search` | SearchGraph, SearchFlooder (bidirectional Dijkstra path extraction) |
| `interop` | CompressedEdge, MwpmEvent, FloodCheckEvent, QueuedEventTracker |
| `driver` | UserGraph, DEM parser, Matching (public decode API) |
| `decoder` | rsinter `Decoder` trait impl (feature-gated) |

## License

MIT
