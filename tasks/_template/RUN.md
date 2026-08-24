# RUN_<issue>

| field | value |
|---|---|
| issue | #<issue> — <title> |
| input | GitHub issue, read with `gh issue view <issue>` |
| stage | S1 |
| branch | feat/<issue>-<slug> |
| opened | YYYY-MM-DD |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_<issue>.md`
- produced: YYYY-MM-DD

```yaml
gate:
  stage: S1
  artifact: AC_<issue>.md
  reviewer: designer
  verdict: READY | QUESTIONS | BLOCKED
  date: YYYY-MM-DD
```

## Notes

<!-- Anything a later stage needs and the artifacts do not carry. -->
