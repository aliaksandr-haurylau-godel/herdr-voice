# RUN_13

| field | value |
|---|---|
| issue | #13 — Recognition: transcribe a take with candle, and let the person choose the model |
| input | GitHub issue, read with `gh issue view 13` |
| stage | S2 |
| branch | feat/13-recognition |
| opened | 2026-08-25 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_13.md`
- produced: 2026-08-25

Two facts were checked on the machine rather than assumed, and they decided the
shape of the work: `whisper-cli` is installed, and the model the prototype used —
`ggml-large-v3-turbo.bin`, 1.5 GB — has been on disk since April. An engine that
runs an external command therefore reaches a working chain tonight without a
download and without inference code of ours.

The issue is rescoped to the two thin engines plus the model contract. The built-in
`candle` engine becomes issue #15. This is a **scope cut that touches a request the
owner made himself** — a plugin that needs nothing else installed — so it is
flagged in `docs/decisions.md` for his confirmation rather than recorded as settled.
Two things follow from it and are in the criteria: no release is tagged while the
built-in engine is missing, and the default stays `candle`, which reports that it is
not built rather than quietly becoming a different engine.

```yaml
gate:
  stage: S1
  artifact: AC_13.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-08-25
  questions:
    - "AC-3 and AC-12: how do the take's path, the language and the model reach the external program's argument list, and under which configuration keys? `whisper-cli` takes the file behind `-f` and needs `-m` and `-l` too, and `[stt]` has no key for a program or its arguments. A fixed 'append the path' contract and a configured argument template are two different engines with two different schemas."
    - "AC-9: is `[stt] model` the file name or an identifier mapped to one? The default is `large-v3-turbo` and the file on disk is `ggml-large-v3-turbo.bin`, so under an exact-name rule the default can never be present."
    - "AC-10: what is an integrity check compared against? Nothing holds an expected size or digest, and chosen reading 3 removes the download that would carry one."
    - "AC-8 refuses a name that is 'none of the four' while everything else names three. Which is the fourth?"
  blocker: null
  notes:
    - "Separate note, not a reason to withhold READY: the scope cut to `command` and `http` is stated, reasoned and flagged for the owner, and I can design against it. The as-is claims about `src/config.rs` and `src/doctor.rs` check out against the code on the branch."
```

### S1 Assess — answers to the gate
- artifact: `AC_13.md`, a new section and five criteria
- produced: 2026-08-25

All four are answered in the artifact, and one of them found a plain error.

The external program takes a configured argument list with `{audio}`, `{model}`
and `{language}` placeholders, under a new key `[stt] command`. That key is a
user-visible name that `docs/design.md` section 7 does not contain, so it is in
`docs/decisions.md` as the owner's to confirm, alongside the scope cut.

`[stt] model` is an identifier and the file is `ggml-<model>.bin`. The reviewer
caught that an exact-name rule against the identifier could never match, since the
default is `large-v3-turbo` and the file is `ggml-large-v3-turbo.bin`.

Integrity is the exact name, a plausible size, and the four leading bytes
`6C 6D 67 67` — the `ggml` magic, read off the model on disk rather than assumed.
That catches a truncated download without a digest nobody has yet; a `.sha256`
sidecar is checked when present, and #15 writes it.

There is no fourth engine. The criterion said four and was wrong.

A revised artifact needs a new verdict; the gate runs again.

```yaml
gate:
  stage: S1
  artifact: AC_13.md
  reviewer: designer
  verdict: QUESTIONS
  date: 2026-08-25
  questions:
    - "AC-6: under which configuration key is the `http` endpoint set, and what is the request and response contract? The artifact names and defaults `[stt] command` and records it as the owner's to confirm, but the endpoint has no key anywhere, while AC-6 requires 'an endpoint that is not configured' to be a distinct named failure. Designing it means inventing a user-visible name, its default, whether a token and a model ride with it, what is posted and where the transcript is read from — the same class of decision the artifact just refused to take alone for `command`."
    - "AC-3 against AC-12 for a command with `language = \"auto\"`: AC-3 replaces every placeholder, AC-12 says auto means the engine decides rather than a literal value being sent. For `[\"-l\", \"{language}\"]` that is three different programs — substitute `auto`, drop the argument, or drop it and the one before it — and nothing chooses."
  blocker: null
  notes:
    - "Not a question: the `http` engine will need a blocking HTTP client crate, which meets the standing dependency decision. I can take and record that myself; only the user-visible key is out of my hands."
    - "Everything the first gate asked was answered and the answers hold against the code and the design: `[stt] model` as an identifier with the file `ggml-<model>.bin` matches `models_directory()` in `src/doctor.rs:260` and the default in `src/config.rs:64`; the `ggml` magic is the byte order a whisper.cpp model actually begins with; the engine set is three and agrees with `docs/design.md:112-117`."
```

### S1 Assess — the endpoint engine leaves the issue
- artifact: `AC_13.md`, chosen readings and five criteria
- produced: 2026-08-25

The first question is answered by removing its subject rather than by inventing
what it asks for. Everything the endpoint engine needs — a key, a default, whether
a token and a model travel with it, what is posted, where the transcript is read
from — is a user-visible decision of the same class as `[stt] command`, and it
needs an HTTP client dependency besides. None of it is on the path to a working
chain, which the external command already reaches. It becomes issue #16, and this
issue builds one engine.

That is the third narrowing of this issue tonight, and it is the only one that
makes the night's work smaller in every direction at once: fewer names invented
without the owner, one fewer dependency, and the same chain reached.

The second question is answered by not inventing a rule: `auto` is substituted like
any other value, because `auto` is what these programs already understand.
Dropping the placeholder, or the argument before it, would be a rule nothing
states.

A revised artifact needs a new verdict; the gate runs again.

```yaml
gate:
  stage: S1
  artifact: AC_13.md
  reviewer: designer
  verdict: READY
  date: 2026-08-25
  questions: []
  blocker: null
  notes:
    - "The narrowing to one engine is consistent where it matters. AC-6 no longer demands a named failure for an unconfigured endpoint, only 'not built in this version', which needs no key, no wire contract and no HTTP client. Two leftover mentions of `http` are statements about that engine's nature, not build obligations."
    - "The `auto` answer is designable: substitution is unconditional, so `{language}` is one string replacement with no neighbouring-argument rule, and the owner-written argument list is where the omission case lives."
    - "As-is claims check out against the branch: `src/config.rs:50`, `src/config.rs:64`, `src/doctor.rs:192` matching by `contains`, `src/doctor.rs:260`, and `dictate` answering with path, level and target at `src/daemon.rs:85-90`."
    - "One thing I will decide in the design rather than ask about: whether the model contract gates a command whose argument list contains no `{model}` placeholder."
    - "Separate note: with the default engine `candle` and an empty command list, a fresh install transcribes nothing until two keys are set. The criteria state that deliberately, so it is the owner's call and not a gap."
```

### S2 Design
- artifact: `DESIGN_13.md`
- produced: 2026-08-25

Four decisions. The interface takes only the path, because the path is the only
thing that changes between takes — which also puts the model check at the moment
somebody can still be told what to fix, rather than in the middle of a recording.

The command is an argument list with placeholders, and the reviewer's open point is
answered rather than deferred: a list with no `{model}` placeholder brings its own
model, so the model contract does not gate it. Demanding a model this plugin manages
would refuse a working setup.

A model is usable after four checks in order — the exact name, a plausible size,
the `ggml` magic, and a `.sha256` sidecar when one exists — and each catches
something the next cannot. `doctor` and the engine call the same function so they
cannot disagree.

An engine that cannot run gives one of four answers, each naming what to set, and
never falls back to an engine that works. One "recognition is not available" would
make somebody guess which of four things to fix.

One thing the design adds that the criteria did not ask for, because it comes from
the environment rather than the requirements: herdr starts plugin commands with a
minimal `PATH`, and the daemon is started by herdr. A program found in an
interactive shell may be absent from the daemon's, so "not found" reports the `PATH`
it actually searched.

```yaml
gate:
  stage: S2
  artifact: DESIGN_13.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-08-25
  questions:
    - "Section 5 transcribes before replying, and nothing touches the client's bound. `src/client.rs:18` waits two seconds and a test asserts it stays at or under five, while `docs/evidence.md:29` measures two seconds of transcription for a two-second take. So AC-11's done-criterion would usually become 'the daemon did not answer within 2 seconds', with the failure landing on a take that succeeded. Whether the fix is a larger bound, a per-command bound, a configured one, or a reply that does not wait is undecided, and it decides whether `src/client.rs` is in this plan at all."
    - "Section 3 says the model check does not apply without a `{model}` placeholder, and that `doctor` and the engine cannot disagree. That leaves the `doctor` model line undefined for the state a fresh install is in — engine not built, command empty, no argument list to ask for a model — so the task rewriting `model_finding` has no checkable done-criterion there."
  blocker: null
  notes:
    - "Everything else cuts: four independent tasks with real edges and a test list each that needs no model, microphone or network. The dependency graph is unambiguous."
    - "Separate note: the design replaces the `dictate` reply with the transcript without saying whether the measured level survives, which #8 kept deliberately. I can plan either way."
```

### S2 Design — answers to the gate
- artifact: `DESIGN_13.md`, a new section 4a and two additions
- produced: 2026-08-25

The first question is the best catch of the night, because it would have shipped: a
take that worked would have been reported as a daemon that never answered, and the
failure would have landed on the successful case. The bound now belongs to the
command — two seconds for `cancel`, two minutes for the `dictate` that finishes a
take — and the test changes from "the bound is short" to "the short one stays short
and the long one is bounded". A bound exists to turn a hang into a message, not to
cap work somebody asked for.

The second is answered by a sentence `doctor` did not have: when the configuration
cannot ask for a model, the model line says it is not used by this configuration
rather than missing. "Missing" would send somebody to download a file that nothing
would read, and a fresh install is exactly that state.

The separate note is taken as well: the level stays beside the transcript, as issue
8 decided.

A revised artifact needs a new verdict; the gate runs again.

```yaml
gate:
  stage: S2
  artifact: DESIGN_13.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-08-25
  questions:
    - "Section 4a and the module table disagree about what the long bound applies to, and the code cannot express the narrower one. The client picks the bound before it sends — `src/main.rs:145` passes a command name only — while which half a `dictate` press is is daemon state. So the per-half rule needs something the design does not state, and the per-command rule means the first press also waits two minutes before reporting a hang. The client task has two done-criteria and no way to choose."
  blocker: null
  notes:
    - "Separate note: the new `doctor` paragraph is plannable and checkable, though that state is decided before the shared check runs rather than by it, and it needs a `State` variant `src/doctor.rs` does not have."
    - "Separate note: AC-14 maps to section 4, which says nothing about `README.md`, and no module row covers it. I can still cut that task from AC-14."
    - "Everything else cuts: four tasks with real edges and an unambiguous graph once the bound is settled."
```

### S2 Design — the bound goes to the command name
- artifact: `DESIGN_13.md`, section 4a and the module table
- produced: 2026-08-25

The reviewer is right that the narrower rule cannot be written: the client chooses
its bound before it sends, and which half of the toggle a press turns out to be
lives in the daemon. Telling them apart would take a second round trip, a second
command name, or a protocol that answers twice, and none of those is worth it.

So `dictate` gets two minutes on both halves. The cost is named in the design rather
than left for somebody to discover: the press that starts a take is answered
immediately, so the long bound never elapses in normal use, but a wedged daemon now
takes two minutes to say so on that press instead of two seconds. A late message
about a stuck daemon beats a prompt lie about a take that was working, and the take
is the case that happens.

Both separate notes are taken too: `doctor` gains a fourth state beside `ok`,
`default` and `missing`, and the README line has a module row of its own.

A revised artifact needs a new verdict; the gate runs again.

## Notes

The prototype passed a bias prompt to `whisper-cli`. That prompt is context, and
context is its own task, so nothing here passes one — which `docs/evidence.md` says
costs punctuation and capitalisation but not terms.
