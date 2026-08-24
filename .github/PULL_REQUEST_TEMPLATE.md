## What and why

<!-- One paragraph: what changes and what problem it solves. Link the issue. -->

Closes #

## Stage artifacts

<!-- Paths under tasks/<issue>/ that this branch was built from. -->

- [ ] `AC_<issue>.md`
- [ ] `DESIGN_<issue>.md`
- [ ] `PLAN_<issue>.md`

## Checklist

- [ ] Tests cover the change, and they pass
- [ ] Behaviour verified by running it, not only by reading the diff
- [ ] Failures are visible: no path exits without an indicator, a toast or a journal line
- [ ] Tab label and sidebar token are restored on every exit path this change can reach
- [ ] Everything in the diff is English
- [ ] Leak gate passes: no employer, client, internal system or personal path
- [ ] `docs/design.md` updated if the design changed
- [ ] `docs/evidence.md` updated if a measurement was made
