# Repository Testing

This document owns the mechanical repository validation contract. Project-specific
visual, performance, hardware, or interactive verification remains additional
evidence owned by the relevant project or issue.

## Canonical gate

The repository acceptance command is:

```text
cargo validate
```

The repository-owned command verifies:

1. required authority, validation, toolchain, workflow, license and PCG project files;
2. Markdown link integrity for repository-relative links;
3. locked root Cargo metadata;
4. root workspace formatting, locked all-target tests and Clippy with warnings denied;
5. PCG inspector's independently locked Rust exporter formatting, tests and Clippy;
6. JavaScript syntax, vendored D3 SHA-256/license integrity, and Node built-in graph-data/HTTP server tests using `npm run check`;
7. Todo's independently locked Cargo metadata, formatting, all-target tests and strict Clippy;
8. Git diff hygiene and checkout-state preservation.

The PCG inspector's Rust project uses the exact accepted RunenPCG Git revision.
Its consumer tests do not claim historical Runenwerk replay.

A separate, exact-head [browser workflow](.github/workflows/pcg-inspector-browser.yml)
runs Chromium with Playwright 1.57.0 against the actual D3 SVG viewer and its
local files. It checks rendering, selection, dragging, pinning, zooming, force
controls, file import, rejection, SVG export and reduced-motion behavior.

The repository-owned Node check verifies D3's vendored bytes, provenance hash,
license presence and local script reference. There is no runtime CDN dependency.
Browser and mechanical checks do not establish physical 3D cave geometry,
performance for arbitrary graph sizes, or historical Runenwerk replay.

The first accepted project, and later types when they materially differ, must
extend the canonical gate with the minimum checks needed to enforce their
maintained repository contract. Do not pre-build project validation machinery
before real projects establish a need.

GitHub Actions invokes the same repository-owned `cargo validate` command
through Dornglut's pinned reusable Rust validation workflow on the exact head.
