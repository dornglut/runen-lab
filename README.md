# Runen Lab

Runen Lab is Dornglut's downstream experimental and showcase workspace for building ambitious applications with accepted public Runen ecosystem surfaces.

The repository is intentionally **not** a framework, specification, or cross-framework integration authority. Runen frameworks own their semantics and conformance; Runenwerk owns canonical engine/product integration. Runen Lab consumes those boundaries to explore visuals, interaction, stress, ergonomics, and composition.

## Maturity

Runen Lab is currently **planned**. The repository establishes authority and validation before admitting its first Lab project.

No Lab project is accepted yet, and this repository does not imply that planned Runen frameworks are externally consumable before their owning extraction/release gates are accepted.

## Project model

Future projects are independently runnable downstream applications under `projects/` by default. A project may be exploratory or curated, direct-framework or Runenwerk-integrated, and highly visual or interactive.

Projects do not become framework acceptance authority merely because they exercise a framework successfully. Reusable findings are routed back to the repository that owns the reusable concern.

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
