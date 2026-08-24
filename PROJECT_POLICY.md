# Project Policy

This document owns Runen Lab project-admission and dependency rules. It does not define framework semantics or a durable Lab roadmap.

## Admission

A project belongs in Runen Lab when its primary purpose is downstream experimentation, demonstration, stress exploration, visual/interactive study, or ecosystem composition using accepted public Runen surfaces.

A project should use a separate repository instead when it becomes an independently versioned product, service, library, game, or tool with its own lifecycle and audience beyond the Lab.

## Independence

Projects are independently runnable and reproducible by default. A maintained Rust project owns its `Cargo.toml` and committed `Cargo.lock` and is not automatically part of the root validation-tool workspace.

Project-to-project dependencies require explicit accepted evidence. Shared application/framework layers are not introduced merely to reduce duplication.

## Upstream dependencies

Maintained projects consume accepted immutable public dependencies. Released packages use accepted SemVer requirements; unreleased public packages use exact accepted Git revisions when their owning repository permits downstream use.

Moving branch dependencies, pull-request heads, sibling path dependencies, copied framework source, framework-source submodules, and forwarding/source-include compatibility layers are not accepted on `main`.

A project must not consume an internal pre-extraction package merely to make a planned standalone framework appear externally available.

## Evidence and authority

A Lab project may provide valuable downstream evidence, including integration friction, usability observations, visual results, stress behavior, or independent-consumer proof. That evidence does not itself change framework semantics, compatibility policy, support claims, or acceptance state.

Reusable findings are raised in the repository that owns the reusable concern. Project-local needs remain local.
