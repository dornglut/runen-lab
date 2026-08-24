# Repository Architecture

This document owns the structure and dependency boundaries of the Runen Lab repository. It does not define Runen framework semantics, framework conformance, cross-framework compatibility, or Runenwerk integration architecture.

## Product boundary

Runen Lab is a downstream collection of experimental and showcase applications built with accepted public Runen ecosystem surfaces.

Runen Lab owns:

- project-specific application behavior and interaction;
- project-specific visual and presentation choices;
- project-local assets and application glue;
- experimental downstream composition and stress exploration;
- curated showcase applications;
- non-authoritative observations about downstream friction.

Runen Lab does not own:

- Runen language or framework semantics;
- framework conformance or release/support claims;
- canonical cross-framework compatibility;
- Runenwerk engine/product integration architecture;
- reusable GPU, rendering, UI, spatial, ECS, networking, or language abstractions merely because a Lab project needs them;
- broad raw-backend escape hatches or compatibility facades.

Reusable findings are routed to the repository that owns the reusable concern. Application-specific behavior remains local to the Lab project.

## Dependency direction

```text
accepted public Runen surfaces
            |
            v
       Runen Lab projects
```

Runen framework and Runenwerk production packages MUST NOT depend on Runen Lab.

A Lab project may consume a framework directly when the framework exposes an accepted independently consumable public surface. A Lab project may consume Runenwerk when it intentionally exercises the canonical engine/integration product. The Lab itself does not introduce a second framework-family integration authority.

## Project model

Projects are independent downstream applications by default.

The directional layout is:

```text
projects/
  <project-a>/
    Cargo.toml
    Cargo.lock
    README.md
    src/
  <project-b>/
    ...
```

Each maintained Rust project owns its own dependency resolution and lockfile unless accepted evidence later justifies a different boundary. Projects SHOULD NOT depend on other Lab projects by default.

The repository root is reserved for repository authority and validation tooling. Lab projects are not automatically members of the root Cargo workspace.

## Dependency policy

Maintained projects consume immutable accepted dependencies:

- released packages use an accepted SemVer requirement plus the committed project lockfile;
- unreleased public dependencies use an exact accepted Git revision plus the committed project lockfile.

Maintained `main` projects MUST NOT depend on moving branches, pull-request heads, sibling path checkouts, copied framework source, Git submodules containing framework source, or source-include/forwarding compatibility layers.

A temporary candidate branch may test an exact upstream candidate revision when an owning issue explicitly requires integration evidence. Such a candidate is not accepted dependency authority and must be reconciled to accepted upstream authority before Lab acceptance.

## Shared code

Do not create a `runen_lab_core`, Lab engine, renderer, scene system, resource system, or equivalent shared framework merely to reduce project duplication.

Project-local duplication is preferred until repeated independent evidence identifies a stable Lab-only concern. If the repeated concern is reusable Runen semantics or integration architecture, route it upstream instead of extracting it into Lab shared code.

Repository tooling under `tools/` may be shared when it serves repository validation or project orchestration without wrapping framework semantics.

## Top-level artifact areas

- root Markdown entrypoints — repository purpose, automation contract, architecture, and testing;
- `tools/` — repository tooling only;
- future `projects/` — independently runnable downstream applications when accepted work creates them.

Do not pre-create taxonomy, `spec/`, a durable roadmap, a gallery site, shared asset infrastructure, or shared runtime packages without accepted evidence that they are needed.
