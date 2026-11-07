# Todo — independent Runenwerk desktop consumer

This project is the first ordinary desktop application trial in [Runen Lab issue #7](https://github.com/dornglut/runen-lab/issues/7).

The application owns task state and operations locally in `src/model.rs`, rather than adding
a command registry or product behavior to Runenwerk. The typed RunenUI application dispatches
the same task operations through `TodoList::execute` as any in-process caller.

## Build and run

From this directory:

```sh
cargo test --locked
cargo run --locked
```

Use the Rust toolchain selected by the repository `rust-toolchain.toml`.
The application pins accepted Runenwerk `b4683b99939464c3bc79e85f04c752687f01bc86` and uses its explicit
`UiRuntimeFontConfiguration::with_system_fonts()` policy rather than a Windows-only font-file path.
This is a temporary accepted-API consumer baseline, **not** the future fluent App-model cutover.
Native display/input/resize/shutdown still require physical platform qualification; compilation
and unit tests alone do **not** establish that the GUI works.

## Task interactions

Enter a title and press Enter or select **Add**. Each task has a **Complete** / **Reopen**
control and **Remove**. Changes are validated and saved before the in-memory task state is
committed. Errors appear in the UI instead of silently erasing saved state.

Task data is written to a per-user location:

- Windows: `%APPDATA%\RunenLab\Todo\tasks.json`
- macOS: `~/Library/Application Support/RunenLab/Todo/tasks.json`
- Linux: `$XDG_DATA_HOME/runen-lab/todo/tasks.json`, or `~/.local/share/runen-lab/todo/tasks.json`

Set `RUNEN_LAB_TODO_DATA_FILE` to an explicit file path to override this for testing or development.
An existing malformed or unsupported file causes startup to fail rather than being overwritten.

This first iteration is **single-writer**. Running concurrent instances against the same data
file is not supported; a future terminal or agent controller must operate through a single live
application owner, not independently modify its backing file.

## Terminal-only development workflow

The independent Cargo package has its own manifest, lockfile, sources and tests. UI and
behavior can be changed without an editor GUI: task commands belong in `src/model.rs`,
durable mutations in `src/service.rs` and `src/storage.rs`, and views/actions in
`src/ui/mod.rs`. Native composition stays in `src/application.rs`,
`src/ui/host.rs`, `src/rendering.rs` and `src/window.rs`.

From the repository root, the maintenance commands are:

```sh
cargo test --manifest-path projects/todo/Cargo.toml --all-targets --locked
cargo clippy --manifest-path projects/todo/Cargo.toml --all-targets --locked -- -D warnings
cargo +stable validate
cargo run --manifest-path projects/todo/Cargo.toml --locked
```

The accepted Engine requires explicit plugin selection, native-primary UI placement via
`mount_app_on_primary`, native input recipient selection via
`bind_native_primary_input`, and an explicit close policy. Todo still authors its
clear → UI composite → single Present RenderFlow and opts into system fonts; these
are concrete integration steps to evaluate, not justification for a second Lab runtime
or speculative CLI. Use a unique `RUNEN_LAB_TODO_DATA_FILE` for native trials.
Source checks and hosted Cargo validation do not establish physical keyboard/pointer
operation, resize, GPU settlement or persistence across a real process restart.

## Architecture and limits

- `model.rs`: typed task commands, stable task IDs, validation, and read-only task inspection.
- `service.rs`: single application-owned execution and persistence transaction boundary.
- `storage.rs`: bounded versioned JSON persistence and replacement writes.
- `ui/mod.rs`: typed RunenUI actions and UI-only draft editing.
- `ui/host.rs`, `rendering.rs`, `window.rs`: accepted Engine-managed UI placement and native input selection with the still-explicit RenderFlow and close policy; broader fluent App construction remains separate under Runenwerk #1423/#1405.

The synchronous small-file save is a bounded first-consumer choice, not a general Runenwerk
persistence or scheduler contract. An external MCP/IPC endpoint, shared command registry,
and cross-process live-state synchronization are **not** implemented here.
