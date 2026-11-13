# Runen Lab

Runen Lab is Dornglut's downstream experimental and showcase workspace for building ambitious applications with accepted public Runen ecosystem surfaces.

The repository is intentionally **not** a framework, specification, or cross-framework integration authority. Runen frameworks own their semantics and conformance; Runenwerk owns canonical engine/product integration. Runen Lab consumes those boundaries to explore visuals, interaction, stress, ergonomics, and composition.

## Maturity

Runen Lab has an independent downstream PCG inspector project. Its experimental D3
viewer is available for local exploration with an offline pinned D3 build.
Project acceptance remains subject to reviewed exact-head CI and browser proof.

Projects exercise accepted framework APIs but do not thereby establish framework
support or consumer-release claims.

## Projects

- [Desktop Todo](projects/todo/README.md) — independent Runenwerk task manager
  with durable storage. Native acceptance is pending in the draft delivery.
- [PCG Graph Inspector](projects/pcg-inspector/README.md) — inspect generated
  graph topology using D3 forces. Launch with `cargo pcg-inspector`; export
  real RunenPCG candidates using `cargo pcg-inspector generate`.

Each project has its own scope and prerequisites. Other projects may be
experimental or curated, direct-framework or Runenwerk-integrated, and highly
visual or interactive. Source for accepted Runen frameworks remains external.

## Repository authority

- [Architecture](ARCHITECTURE.md) — Lab boundary, dependency direction, and project model.
- [Testing](TESTING.md) — canonical mechanical validation.
- [Agent instructions](AGENTS.md) — automation-specific constraints.
- [Contribution guidance](https://github.com/dornglut/.github/blob/main/CONTRIBUTING.md) — inherited Dornglut contribution defaults.
- [Security policy](https://github.com/dornglut/.github/blob/main/SECURITY.md) — inherited Dornglut security routing.
- [License](LICENSE) — GNU General Public License version 3 only (`GPL-3.0-only`).

Canonical validation:

```text
cargo validate
```
