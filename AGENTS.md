# Agent Instructions

Automated contributors must begin with [ARCHITECTURE.md](ARCHITECTURE.md), [TESTING.md](TESTING.md), and Dornglut's [Authority and work](https://github.com/dornglut/engineering/blob/main/governance/authority-and-work.md) rules.

Runen Lab is a downstream application repository. Before changing a project, identify the accepted public Runen surfaces it consumes and keep framework semantics, conformance, release claims, and Runenwerk integration authority in their owning repositories.

Do not create a shared Lab runtime, compatibility facade, copied framework source, path/submodule dependency, moving-branch dependency, backend escape hatch, or speculative cross-project abstraction unless accepted repository authority explicitly requires it.

Projects are independent downstream applications by default. Prefer project-local code over premature shared infrastructure. Reusable findings are routed to the repository that owns the reusable concern.

For iterative continuation, re-establish current issue/PR authority before selecting work. A useful experiment or prior proposal is not by itself implementation authority.

Before proposing acceptance, run the canonical validation defined by [TESTING.md](TESTING.md) and review the exact changed head.
