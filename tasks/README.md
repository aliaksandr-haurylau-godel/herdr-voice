# Runs

One directory per issue: `tasks/<issue-number>/`, holding that run's artifacts —
`RUN_<issue>.md`, `AC_<issue>.md`, `DESIGN_<issue>.md`, `PLAN_<issue>.md`.

Start a run by copying `_template/RUN.md` into the new directory and filling the
table. The stages, the reviewers and the verdict format are in `../CLAUDE.md`.

Artifacts stay in the repository after the issue closes: they are the record of
why the code looks the way it does, and the design document is written from them.
