# Repository Testing

This document owns the mechanical repository validation contract. Project-specific visual, performance, hardware, or interactive verification remains additional evidence owned by the relevant project or issue.

## Canonical gate

The repository acceptance command is:

```text
cargo validate
```

The current repository-owned command verifies the maintained bootstrap surface:

1. required authority, validation, toolchain, workflow, and license files;
2. Markdown link integrity for repository-relative links;
3. locked root Cargo metadata;
4. root workspace formatting;
5. locked all-target root workspace tests;
6. all-target root workspace Clippy with warnings denied;
7. Git diff hygiene;
8. checkout-state preservation.

The first accepted project, and later project types when they materially differ, must extend the canonical gate in the same accepted change with the minimum mechanical checks needed to enforce that project's maintained repository contract. Do not pre-build project validation machinery before a real project establishes the shape being validated.

Focused project checks may be used during development but do not replace `cargo validate` before acceptance once the canonical gate owns that project class.

GPU execution, visual captures, performance measurements, browser runs, and other environment-dependent project proofs are not silently included in the baseline unless the repository later has reliable infrastructure and accepted authority to make them merge requirements.

GitHub Actions invokes the same repository-owned command through Dornglut's pinned reusable Rust validation workflow and validates the exact reviewed feature-head revision.
