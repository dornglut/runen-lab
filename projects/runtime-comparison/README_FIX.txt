Runtime comparison Rust 2024 and RunenECS public API source correction.

Apply from the runen-lab/projects/runtime-comparison directory:
  Expand-Archive -LiteralPath "$HOME\Downloads\runtime-comparison-compile-fix-01.zip" -DestinationPath . -Force
  cargo fmt --all
  cargo test --all-targets --locked

Only src/contract.rs, src/tde.rs, src/bevy_backend.rs, and src/runen_backend.rs change.
The existing Cargo.lock, fixtures, generated.rs, other source, and project structure remain intact.

This is an uncompiled source fix; the receiving Rust host must compile and test it.
Known TODO: compiler may expose further errors; semantic oracle may expose runtime issues.
