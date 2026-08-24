# Repository Testing

This document owns the mechanical repository validation contract. Project-specific visual, performance, hardware, or interactive verification remains additional evidence owned by the relevant project or issue.

## Canonical gate

The repository acceptance command is:

```text
cargo validate
```

The command is repository-owned and verifies the maintained root repository surface, including:

1. required authority files and repository structure;
2. Markdown link integrity for repository-relative links;
3. Lab dependency-policy checks for maintained projects when projects exist;
4. locked Cargo metadata;
5. workspace formatting;
6. locked all-target workspace tests;
7. all-target Clippy with warnings denied;
8. Git diff hygiene;
9. checkout-state preservation.

Focused project checks may be used during development but do not replace `cargo validate` before acceptance.

GPU execution, visual captures, performance measurements, browser runs, and other environment-dependent project proofs are not silently included in the baseline unless the repository later has reliable infrastructure and accepted authority to make them merge requirements.

GitHub Actions invokes the same repository-owned command through Dornglut's pinned reusable Rust validation workflow and validates the exact reviewed feature-head revision.
