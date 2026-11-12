# RunenPCG Graph Inspector

An independent D3-powered, force-directed browser inspector for accepted RunenPCG
structural graphs. This is **Runen Lab presentation code**, not a RunenPCG
framework module, generator or physical-geometry implementation.

## Launch

From the Runen Lab repository root (Node.js 20+ and Rust stable required):

```sh
cargo pcg-inspector
```

Open the printed local URL, normally `http://127.0.0.1:4173`. Use
`PCG_INSPECTOR_PORT=4174 cargo pcg-inspector` to choose another port. The server
binds to loopback, has an explicit static-file allowlist and uses no remote
backend. It prints a useful error if its port is occupied.

**Current prototype limitation:** D3 7.9.0 loads from an exact-version CDN URL;
network access is still required. Offline packaging, supply-chain verification,
and successful browser interaction tests remain acceptance gates for this project.

## Generate actual RunenPCG data

The exporter is a separately built Rust consumer pinned to the accepted RunenPCG
commit `7de282455492959ff1bacd0c3e8a78ad37215306`:

```sh
cargo pcg-inspector generate --nodes 12 --cycles 2 --max-edges 24 --seed 42 > graph.json
```

Import `graph.json` in the browser. This is a **one-off CLI export**, not a
live generator backend, automatic refresh or RunenPCG-owned serialization.
Output goes to standard output; Cargo build diagnostics appear on standard error.
The exported result is the same structural candidate returned by the Rust API.

## Viewer features

Drag or click nodes, double-click to pin, pan/zoom, adjust repulsion, link
length, collision spacing and damping, pause/restart layout, and export SVG.
Use **Import JSON** or drop a JSON file onto the graph. A reduced-motion
preference uses static layout, and SVG nodes are keyboard-selectable and pinnable.
Presentation coordinates are not world positions or physically verified corridors.

## JSON view contract

```json
{
  "format": "runen-lab.graph-view.v1",
  "algorithm": "runen-pcg.graph.spanning-tree.v1",
  "seed": "42",
  "node_count": 4,
  "edges": [[0, 1], [1, 2], [1, 3], [2, 3]]
}
```

This schema is **consumer-owned**, not a universal PCG interchange or a
RunenPCG public compatibility promise. Node indices are zero-based. Endpoints
must be unique, sorted and canonical. The viewer independently computes
connectedness, cycle rank, degrees and branching vertices. Numeric seeds must
be represented as canonical decimal strings so the full unsigned 64-bit domain
is preserved. Other metric claims are ignored.

The browser SVG view admits **at most 500 nodes and 2,500 edges**; limits are
independent of RunenPCG's generation maxima. Imports are limited to 512 KB and
rejected instead of silently truncated.

## Verification

```sh
node --version
npm run check --prefix projects/pcg-inspector
cargo validate
```

The Node tests cover schema, negative cases, deterministic presentation seed,
local HTTP serving and static asset restrictions. A separate locked Rust
exporter project has public-API golden and failure tests. `cargo validate`
must include both. These checks do **not** replace browser UI or third-party
dependency verification.

## Ownership and dependency direction

```text
RunenPCG (external, pinned Rust dependency)
  -> project-local Rust JSON exporter
  -> user-imported JSON view document
  -> graph-data.mjs (strict validation and measured metrics)
  -> main.mjs (D3 SVG visualization only)
```

The consumer owns visualization, placement, and presentation. No framework
runtime, graph backend, adapter trait, 3D layout, persistence or plugin is added.
The project is licensed GPL-3.0-only as part of Runen Lab. Third-party D3 is ISC.
