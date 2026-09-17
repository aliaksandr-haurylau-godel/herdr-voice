# PLAN_42 — the build entries fetch the release archive

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans`
> with `superpowers:test-driven-development` to implement this plan task by task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `herdr plugin install` puts a working binary on a machine with no Rust
toolchain, by fetching the release archive for the platform and verifying it
against a digest published beside it.

**Architecture:** The manifest's `[[build]]` entries stop running
`cargo build --release` and call a script this repository ships — `scripts/install.sh`
on macOS and Linux, `scripts/install.ps1` on Windows. The script reads `version`
from `herdr-plugin.toml`, asks for the release tagged `v<version>`, fetches the
archive and its `.sha256`, verifies, and unpacks to `target/release/herdr-voice`.
`.github/workflows/release.yml` starts publishing those digests and learns which
tags publish as prereleases. A container check proves the whole thing with no
toolchain present.

**Tech stack:** POSIX shell, PowerShell 5.1, GitHub Actions, `curl`, `tar`,
`sha256sum` / `shasum`.

**Spec:** `tasks/42/DESIGN_42.md`, which the planner gate passed on 2026-09-17.
Criteria: `tasks/42/AC_42.md`, designer gate passed the same day. Read both; this
plan argues from them and does not restate their reasoning.

## Global constraints

- **Everything in the repository is English** — code, comments, output strings,
  commit messages.
- **No employer, client, internal-system or personal name, and no absolute home
  path**, anywhere. Cite paths relative to the repository root. The leak gate
  enforces it.
- **Four gates green before every commit**, never commit red:
  `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
  `python3 scripts/check_manifest.py`.
- **Grep what you write** for `<new_string>`, `</new_string>`, `<old_string>`,
  `</old_string>` and line-start conflict markers. `check.yml`'s `debris` job
  fails the build on them.
- **Commits end with** `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- **The repository is public.** The owner and repository name
  `aliaksandr-haurylau-godel/herdr-voice` already appear in `docs/design.md` and
  `README.md` and are not a leak.
- **No version number moves.** `herdr-plugin.toml` and `Cargo.toml` both stay at
  `0.0.0`. `scripts/check_manifest.py` fails if they disagree.
- **Do not cut the `v0.0.0` tag.** It is outward-facing, it is the owner's call,
  and it happens after this plan's last task. Tasks 1 to 7 push no tag and
  publish nothing.

## File structure

| File | Status | Responsible for |
|---|---|---|
| `scripts/release-kind.sh` | create | One rule: does this tag publish as a prerelease or as a release |
| `scripts/test-release-kind.sh` | create | Drives that rule over every tag shape |
| `.github/workflows/release.yml` | modify | Also publishes a `.sha256` per archive; asks `release-kind.sh` how to publish |
| `scripts/install.sh` | create | The macOS and Linux build entry: platform, tag, fetch, verify, unpack, fallback |
| `scripts/test-install.sh` | create | Drives every decision in `install.sh` with no network |
| `scripts/install.ps1` | create | The Windows build entry, same sequence, one target |
| `scripts/test-install.ps1` | create | Drives its target detection |
| `.github/workflows/check.yml` | modify | Runs the two shell test scripts and the PowerShell one |
| `herdr-plugin.toml` | modify | `[[build]]` entries call the scripts; the comment becomes true |
| `docs/design.md` | modify | Section 8 describes what the entries now do |
| `scripts/install-check.sh` | create | Proves an install in a container with no toolchain |

`scripts/install.sh` holds the whole macOS-and-Linux sequence in one file because
its parts are one decision tree — splitting fetch from classification would put
both halves of one rule in two places. It stays readable because each step is a
function of a few lines.

---

### Task 1: the release publishes a digest, and knows which tags are prereleases

**Files:**
- Create: `scripts/release-kind.sh`
- Create: `scripts/test-release-kind.sh`
- Modify: `.github/workflows/release.yml` (package step at `:32-44`, upload at
  `:45-48`, publish job at `:50-61`)
- Modify: `.github/workflows/check.yml` (a new job)

**Interfaces:**
- Consumes: nothing.
- Produces: `scripts/release-kind.sh`, invoked as `sh scripts/release-kind.sh <tag>`,
  printing exactly `prerelease` or `release` on standard output and exiting 0.
  Task 5 does not use it; nothing else does.

- [ ] **Step 1: Write the failing test**

Create `scripts/test-release-kind.sh`:

```sh
#!/bin/sh
#
# Drives scripts/release-kind.sh over every tag shape that matters. A wrong
# answer here publishes a throwaway tag as the repository's latest release,
# where anyone's `herdr plugin install` finds it.

set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="${ROOT}/scripts/release-kind.sh"
failures=0

expect() {
    # tag, expected answer
    actual="$(sh "${SCRIPT}" "$1")"
    if [ "${actual}" = "$2" ]; then
        printf 'ok    %-16s -> %s\n' "$1" "${actual}"
    else
        printf 'FAIL  %-16s -> %s (expected %s)\n' "$1" "${actual}" "$2"
        failures=$((failures + 1))
    fi
}

# 0.0.0 is this repository's placeholder for "unreleased": both the manifest and
# the crate sit there, and a tag cut from it is never a version of the plugin.
expect v0.0.0        prerelease
# A hyphen is semver's own prerelease marker.
expect v1.0.0-rc.1   prerelease
expect v0.2.0-test   prerelease
# Everything else is a real release.
expect v0.1.0        release
expect v1.0.0        release
expect v10.20.30     release

if [ "${failures}" -ne 0 ]; then
    printf '\n%s assertion(s) failed\n' "${failures}" >&2
    exit 1
fi
printf '\nall release-kind assertions passed\n'
```

Make it executable: `chmod +x scripts/test-release-kind.sh`.

- [ ] **Step 2: Run it to make sure it fails**

Run: `sh scripts/test-release-kind.sh`
Expected: FAIL — `sh: .../scripts/release-kind.sh: No such file or directory`.

- [ ] **Step 3: Write the minimal implementation**

Create `scripts/release-kind.sh`:

```sh
#!/bin/sh
#
# Answers one question for .github/workflows/release.yml: does this tag publish
# as a prerelease, or as a release?
#
# It exists as a script rather than as a `case` inside the workflow so that it
# can be tested. Getting it wrong is not visible until a throwaway tag is sitting
# at the top of the releases page being installed by strangers.
#
# Usage: sh scripts/release-kind.sh v1.2.3  ->  prints `prerelease` or `release`

set -eu

case "${1:?usage: release-kind.sh <tag>}" in
    # The repository's placeholder version. Nothing cut from it is a version of
    # the plugin; herdr-plugin.toml and Cargo.toml both say 0.0.0.
    v0.0.0) echo prerelease ;;
    # Semver's own prerelease marker.
    *-*)    echo prerelease ;;
    *)      echo release ;;
esac
```

Make it executable: `chmod +x scripts/release-kind.sh`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `sh scripts/test-release-kind.sh`
Expected: six `ok` lines, then `all release-kind assertions passed`.

- [ ] **Step 5: Make the packaging step publish a digest**

In `.github/workflows/release.yml`, replace the `package` step's `run:` block so
it ends with the digest. The step currently finishes at
`tar -C dist -czf "dist/$name.tar.gz" "$name"`; append:

```yaml
          # macOS runners have shasum and not sha256sum; Linux and Windows
          # runners have sha256sum. scripts/install.sh makes the same choice when
          # it verifies what it fetched.
          if command -v sha256sum >/dev/null 2>&1; then
            ( cd dist && sha256sum "$name.tar.gz" >"$name.tar.gz.sha256" )
          else
            ( cd dist && shasum -a 256 "$name.tar.gz" >"$name.tar.gz.sha256" )
          fi
```

The subshell and `cd` matter: the sidecar must name the archive by its bare
filename, not by `dist/...`, because the install script reads the first field and
a person reading the file should see the name it belongs to.

- [ ] **Step 6: Upload the sidecars as artifacts**

In the same file, replace the upload step's single `path: dist/*.tar.gz` with:

```yaml
          path: |
            dist/*.tar.gz
            dist/*.tar.gz.sha256
```

- [ ] **Step 7: Give the publish job a checkout, and the prerelease rule**

The `publish` job downloads artifacts and has no `actions/checkout`, so
`scripts/release-kind.sh` is not on disk there. Add the checkout as the job's
first step, before `actions/download-artifact`:

```yaml
      - uses: actions/checkout@v4
```

Then replace the `publish the release` step's `run:` with:

```yaml
        shell: bash
        run: |
          set -euo pipefail
          # Two globs, no overlap: `*.tar.gz` cannot match a name ending
          # `.sha256`. Checked, and it yields each archive once and each sidecar
          # once.
          assets=(dist/*.tar.gz dist/*.tar.gz.sha256)
          if [ "$(sh scripts/release-kind.sh "$GITHUB_REF_NAME")" = prerelease ]; then
            gh release create "$GITHUB_REF_NAME" "${assets[@]}" \
              --repo "$GITHUB_REPOSITORY" \
              --prerelease --latest=false \
              --title "$GITHUB_REF_NAME — install-path check" \
              --notes "Not a version of the plugin. This tag exists so the install path can be verified: the manifest's build entries fetch these archives and check them against the .sha256 files published beside them. It is deleted once that check is recorded in docs/evidence.md."
          else
            gh release create "$GITHUB_REF_NAME" "${assets[@]}" \
              --repo "$GITHUB_REPOSITORY" \
              --generate-notes
          fi
```

Fixed notes rather than generated ones for a prerelease, because the point of the
text is to say what the tag is for.

- [ ] **Step 8: Add the CI job that runs the shell tests**

In `.github/workflows/check.yml`, add a job after `manifest`:

```yaml
  scripts:
    name: shell scripts
    runs-on: ${{ matrix.os }}
    timeout-minutes: 5
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - name: the release-kind rule
        run: sh scripts/test-release-kind.sh
```

Both operating systems, because the digest branch in the install script — and in
the packaging step above — differs between them, and task 2 adds its tests to
this same job.

- [ ] **Step 9: Check the workflows still parse**

Run:

```sh
python3 -c "import sys,yaml;[yaml.safe_load(open(p)) for p in ('.github/workflows/release.yml','.github/workflows/check.yml')];print('both parse')"
```

Expected: `both parse`. If `yaml` is not installed, run
`python3 -m pip install --user pyyaml` first.

- [ ] **Step 10: Run the four gates**

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py
```

Expected: all four pass. None of them touches what this task changed; they are
run because the rule is to never commit red.

- [ ] **Step 11: Commit**

```sh
git add scripts/release-kind.sh scripts/test-release-kind.sh \
        .github/workflows/release.yml .github/workflows/check.yml
git commit -m "$(cat <<'EOF'
Publish a digest beside each archive, and say which tags are not releases

A release published archives and nothing to check them against, so an install
had no way to tell a good download from a bad one. Each archive now travels with
a .sha256 written by the same runner that built it.

The prerelease rule is a script rather than a case inside the workflow because it
can then be tested. Getting it wrong is invisible until a throwaway tag is
sitting at the top of the releases page being installed by strangers.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: the install script knows where it is, which release it belongs to, and what it runs on

**Files:**
- Create: `scripts/install.sh`
- Create: `scripts/test-install.sh`
- Modify: `.github/workflows/check.yml` (the `scripts` job from task 1)

**Interfaces:**
- Consumes: nothing from task 1.
- Produces, all defined in `scripts/install.sh` and relied on by tasks 3 and 4:
  - `HV_NAME` — `herdr-voice`
  - `HV_REPO` — `aliaksandr-haurylau-godel/herdr-voice`
  - `hv_root` — prints the repository root, absolute
  - `hv_manifest_version ROOT` — prints the manifest's `version`
  - `hv_target OS ARCH` — prints the target triple for `uname -s` and `uname -m`,
    or prints nothing and returns 1
  - The guard: sourcing the file with `HERDR_VOICE_INSTALL_LIB=1` defines the
    functions and runs nothing.

- [ ] **Step 1: Write the failing test**

Create `scripts/test-install.sh`:

```sh
#!/bin/sh
#
# Drives every decision scripts/install.sh makes, with no network, no release and
# no herdr. The script is sourced as a library — see the guard at the bottom of
# it — so each function can be called directly and the fetching one replaced.

set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
failures=0

check() {
    # label, actual, expected
    if [ "$2" = "$3" ]; then
        printf 'ok    %s\n' "$1"
    else
        printf 'FAIL  %s: got [%s], expected [%s]\n' "$1" "$2" "$3"
        failures=$((failures + 1))
    fi
}

HERDR_VOICE_INSTALL_LIB=1
export HERDR_VOICE_INSTALL_LIB
# shellcheck source=./install.sh
. "${ROOT}/scripts/install.sh"

# --- the platform map -------------------------------------------------------
check "Darwin arm64"    "$(hv_target Darwin arm64)"    aarch64-apple-darwin
check "Darwin x86_64"   "$(hv_target Darwin x86_64)"   x86_64-apple-darwin
check "Linux x86_64"    "$(hv_target Linux x86_64)"    x86_64-unknown-linux-gnu
check "Linux aarch64"   "$(hv_target Linux aarch64)"   aarch64-unknown-linux-gnu
check "Linux arm64"     "$(hv_target Linux arm64)"     aarch64-unknown-linux-gnu

# A pair the release matrix has no build for prints nothing and reports failure,
# which is what sends the run to the source build.
if hv_target Linux riscv64 >/dev/null 2>&1; then
    printf 'FAIL  Linux riscv64: reported a target it has no build for\n'
    failures=$((failures + 1))
else
    printf 'ok    Linux riscv64 has no target\n'
fi
if hv_target FreeBSD x86_64 >/dev/null 2>&1; then
    printf 'FAIL  FreeBSD x86_64: reported a target it has no build for\n'
    failures=$((failures + 1))
else
    printf 'ok    FreeBSD x86_64 has no target\n'
fi

# --- the version, out of a manifest -----------------------------------------
# A fixture rather than the real manifest, so that the test still means something
# when the real version moves.
fixture="$(mktemp -d)"
trap 'rm -rf "${fixture}"' EXIT
cat >"${fixture}/herdr-plugin.toml" <<'TOML'
id = "haurylau.voice"
name = "Voice"
version = "0.4.2"
min_herdr_version = "0.8.0"

[[build]]
platforms = ["macos", "linux"]
command = ["sh", "scripts/install.sh"]
TOML
check "version from the manifest" "$(hv_manifest_version "${fixture}")" 0.4.2

# The real manifest is read too: if this stops answering, the URL the script
# builds is wrong and nothing else in the suite would notice.
if [ -z "$(hv_manifest_version "${ROOT}")" ]; then
    printf 'FAIL  the repository manifest yielded no version\n'
    failures=$((failures + 1))
else
    printf 'ok    the repository manifest yields %s\n' "$(hv_manifest_version "${ROOT}")"
fi

if [ "${failures}" -ne 0 ]; then
    printf '\n%s assertion(s) failed\n' "${failures}" >&2
    exit 1
fi
printf '\nall install assertions passed\n'
```

Make it executable: `chmod +x scripts/test-install.sh`.

- [ ] **Step 2: Run it to make sure it fails**

Run: `sh scripts/test-install.sh`
Expected: FAIL — `No such file or directory` for `scripts/install.sh`.

- [ ] **Step 3: Write the minimal implementation**

Create `scripts/install.sh`:

```sh
#!/bin/sh
#
# The herdr [[build]] step for macOS and Linux: put the released binary at
# target/release/herdr-voice without compiling it.
#
# herdr runs this on `herdr plugin install` and never on `herdr plugin link`, and
# it runs it before the plugin is registered. A non-zero exit aborts the install
# and leaves nothing behind, so every message that ends in one says so.
#
# Build commands receive none of herdr's environment — there is no
# HERDR_PLUGIN_ROOT here — so the repository root comes from this script's own
# location. Nothing here writes to herdr-plugin.toml: changing the manifest during
# a build aborts the install.
#
# Sourcing this file with HERDR_VOICE_INSTALL_LIB=1 defines the functions and runs
# nothing. scripts/test-install.sh does that.

set -eu

HV_NAME=herdr-voice
HV_REPO=aliaksandr-haurylau-godel/herdr-voice

# ---------------------------------------------------------------------------
# What this checkout is
# ---------------------------------------------------------------------------

hv_root() {
    ( cd "$(dirname "$0")/.." && pwd )
}

hv_manifest_version() {
    # $1 = repository root. The first `version = "..."` in the manifest is the
    # package's; the tables below it have no version key of their own.
    sed -n 's/^version = "\([^"]*\)".*/\1/p' "$1/herdr-plugin.toml" | head -n 1
}

# ---------------------------------------------------------------------------
# What it is running on
#
# The pairs below are exactly the targets .github/workflows/release.yml builds.
# Anything else has no archive, which is not an error — it is the source build.
# ---------------------------------------------------------------------------

hv_target() {
    # $1 = uname -s, $2 = uname -m
    case "$1:$2" in
        Darwin:arm64)              echo aarch64-apple-darwin ;;
        Darwin:x86_64)             echo x86_64-apple-darwin ;;
        Linux:x86_64)              echo x86_64-unknown-linux-gnu ;;
        Linux:aarch64|Linux:arm64) echo aarch64-unknown-linux-gnu ;;
        *)                         return 1 ;;
    esac
}

# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

hv_main() {
    root="$(hv_root)"
    version="$(hv_manifest_version "${root}")"
    tag="v${version}"
    printf '%s: %s wants the release %s\n' "${HV_NAME}" "${root}" "${tag}"
}

if [ "${HERDR_VOICE_INSTALL_LIB:-0}" != 1 ]; then
    hv_main "$@"
fi
```

Make it executable: `chmod +x scripts/install.sh`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `sh scripts/test-install.sh`
Expected: nine `ok` lines, then `all install assertions passed`.

- [ ] **Step 5: Add it to the CI job**

In `.github/workflows/check.yml`, in the `scripts` job created by task 1, add a
step after `the release-kind rule`:

```yaml
      - name: the install script's decisions
        run: sh scripts/test-install.sh
```

- [ ] **Step 6: Run the four gates, then commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add scripts/install.sh scripts/test-install.sh .github/workflows/check.yml
git commit -m "$(cat <<'EOF'
Work out which release archive this checkout is entitled to

The checkout herdr installs from is a shallow clone with no tags, and a build
command receives none of herdr's environment, so neither git nor the environment
can say which release the code belongs to. The manifest's own version is the only
statement of it inside the checkout, and this reads it.

The platform map lists exactly the targets the release workflow builds. A pair
that is not there is not an error; it is the source build, which a later commit
adds.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: fetching, and telling the three answers apart

**Files:**
- Modify: `scripts/install.sh` (add functions before `hv_main`)
- Modify: `scripts/test-install.sh` (add assertions before the failure count)

**Interfaces:**
- Consumes: `HV_NAME` and the guard from task 2.
- Produces, relied on by task 4:
  - `HV_RETRY_ATTEMPTS` (default 5) and `HV_RETRY_DELAY` (default 3), both
    overridable from the environment so the tests do not sleep.
  - `hv_fetch_once URL DEST` — prints exactly one of `ok`, `missing`, `error`,
    always exits 0.
  - `hv_fetch URL DEST` — the same three words, after retrying. Task 3's tests
    replace `hv_fetch_once`; task 4's replace `hv_fetch` itself.

  `hv_fetch` requires `fixture` from task 2's test, which is defined above the
  point task 3 inserts at.

- [ ] **Step 1: Write the failing test**

In `scripts/test-install.sh`, insert before the `if [ "${failures}" -ne 0 ]` block:

```sh
# --- the retry, and what settles it -----------------------------------------
# hv_fetch_once is replaced so no network is touched. It reads its answers from
# HV_TEST_ANSWERS, one per attempt.
#
# The call count lives in a FILE, not a variable. hv_fetch calls hv_fetch_once
# inside `$( )`, and the suite calls hv_fetch inside `$( )` again, so each runs
# in its own subshell and an incremented variable never reaches either caller.
# A variable here does not fail loudly: every attempt would read answer 1, so
# "a 404 that later succeeds" would settle on `missing` and the suite would
# assert the opposite of the rule it is there to protect.
HV_TEST_COUNT="${fixture}/calls"
hv_fetch_once() {
    n=$(( $(cat "${HV_TEST_COUNT}") + 1 ))
    echo "${n}" >"${HV_TEST_COUNT}"
    echo "${HV_TEST_ANSWERS}" | cut -d' ' -f"${n}"
}

HV_RETRY_ATTEMPTS=3
HV_RETRY_DELAY=0

echo 0 >"${HV_TEST_COUNT}"; HV_TEST_ANSWERS="ok ok ok"
check "a first-attempt success settles at once" "$(hv_fetch u d)" ok
check "and it asked only once" "$(cat "${HV_TEST_COUNT}")" 1

# GitHub answers 404 for some minutes after a release publishes. A 404 that
# later succeeds is the CDN catching up, not a missing target, and treating it
# as one would start the compile this whole change exists to avoid.
echo 0 >"${HV_TEST_COUNT}"; HV_TEST_ANSWERS="missing missing ok"
check "a 404 that later succeeds is not missing" "$(hv_fetch u d)" ok

echo 0 >"${HV_TEST_COUNT}"; HV_TEST_ANSWERS="missing missing missing"
check "a 404 through every attempt is missing" "$(hv_fetch u d)" missing
# It must stop after the budget rather than looping.
check "it stops after the budget" "$(cat "${HV_TEST_COUNT}")" 3

echo 0 >"${HV_TEST_COUNT}"; HV_TEST_ANSWERS="error error error"
check "a transport failure throughout is an error" "$(hv_fetch u d)" error

echo 0 >"${HV_TEST_COUNT}"; HV_TEST_ANSWERS="error error ok"
check "a transport failure that recovers is ok" "$(hv_fetch u d)" ok
```

- [ ] **Step 2: Run it to make sure it fails**

Run: `sh scripts/test-install.sh`
Expected: FAIL — `hv_fetch: not found`.

- [ ] **Step 3: Write the minimal implementation**

In `scripts/install.sh`, insert after the `hv_target` function and before the
`Entry point` banner:

```sh
# ---------------------------------------------------------------------------
# Fetching
#
# Two rules, not one, and they answer different questions.
#
# The retry decides whether we know: GitHub's content delivery network answers
# 404 for some minutes after a release publishes, so a single 404 is not evidence
# that a target is missing.
#
# The three words below decide what we do once we know, and the caller acts on
# them: `missing` is the source build, `error` is a stop. Collapsing the two puts
# a machine whose network hiccupped into an hour of compiling nobody asked for.
# ---------------------------------------------------------------------------

HV_RETRY_ATTEMPTS="${HV_RETRY_ATTEMPTS:-5}"
HV_RETRY_DELAY="${HV_RETRY_DELAY:-3}"

hv_fetch_once() {
    # $1 = url, $2 = destination. Prints ok, missing or error. Always exits 0:
    # the answer is the word, not the status, so that `set -e` cannot turn a
    # 404 into an abort before the caller has decided what it means.
    if ! http="$(curl -sSL -o "$2" -w '%{http_code}' "$1" 2>/dev/null)"; then
        echo error
        return 0
    fi
    case "${http}" in
        200) echo ok ;;
        404) echo missing ;;
        *)   echo error ;;
    esac
}

hv_fetch() {
    # $1 = url, $2 = destination. Prints the settled answer.
    attempt=1
    while :; do
        outcome="$(hv_fetch_once "$1" "$2")"
        if [ "${outcome}" = ok ] || [ "${attempt}" -ge "${HV_RETRY_ATTEMPTS}" ]; then
            echo "${outcome}"
            return 0
        fi
        attempt=$((attempt + 1))
        sleep "${HV_RETRY_DELAY}"
    done
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `sh scripts/test-install.sh`
Expected: the six new `ok` lines among the rest, then
`all install assertions passed`.

- [ ] **Step 5: Run the four gates, then commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add scripts/install.sh scripts/test-install.sh
git commit -m "$(cat <<'EOF'
Tell a missing archive from a network that did not answer

A 404 and a timeout want opposite treatment. A 404 that survives the retries
means this platform has no archive, and a source build is the right answer. A
timeout means we do not know, and answering it with a source build starts
compiling the candle crates for someone who only wanted to run the install again.

The retry sits under both because a 404 is not reliable evidence on its own:
GitHub answers one for some minutes after a release publishes. So the retry
decides whether we know, and the three words decide what we do once we know.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: verify, unpack, fall back — and the messages a person actually reads

**Files:**
- Modify: `scripts/install.sh` (add functions, replace `hv_main`)
- Modify: `scripts/test-install.sh` (add assertions)

**Interfaces:**
- Consumes: everything from tasks 2 and 3.
- Produces:
  - `hv_digest FILE` — prints the lowercase hexadecimal SHA-256, nothing else.
  - `hv_die REASON NEXT` — prints both lines to standard error and exits 1.
  - `hv_have_cargo` — succeeds when `cargo` is on PATH. A function so the suite
    can replace it.
  - `hv_fallback REASON` — prints why, then `cargo build --release`, or `hv_die`
    when `hv_have_cargo` fails.
  - `hv_main` — the whole sequence.

- [ ] **Step 1: Write the failing test**

In `scripts/test-install.sh`, insert before the `if [ "${failures}" -ne 0 ]` block:

```sh
# --- the digest --------------------------------------------------------------
# The empty string's SHA-256 is a fixed, well-known value, so this asserts the
# function against arithmetic rather than against itself.
: >"${fixture}/empty"
check "the digest of an empty file" "$(hv_digest "${fixture}/empty")" \
    e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855

# --- the whole sequence, with a fake release ---------------------------------
# A real archive of the shape release.yml builds: one top-level directory named
# after the tag and the target, holding the binary.
stage="${fixture}/stage"
mkdir -p "${stage}/herdr-voice-v0.4.2-x86_64-unknown-linux-gnu"
printf '#!/bin/sh\necho fake\n' \
    >"${stage}/herdr-voice-v0.4.2-x86_64-unknown-linux-gnu/herdr-voice"
tar -C "${stage}" -czf "${fixture}/archive.tar.gz" \
    herdr-voice-v0.4.2-x86_64-unknown-linux-gnu
good_digest="$(hv_digest "${fixture}/archive.tar.gz")"

# A checkout that looks like the one herdr installs into.
checkout="${fixture}/checkout"
mkdir -p "${checkout}"
cp "${fixture}/herdr-plugin.toml" "${checkout}/herdr-plugin.toml"

# hv_root and the platform are pinned so the sequence is the same on every
# machine that runs this suite; only the answers under test vary.
hv_root() { echo "${checkout}"; }
uname() { if [ "$1" = -s ]; then echo Linux; else echo x86_64; fi; }

# HV_TEST_ARCHIVE / HV_TEST_SIDECAR say what each fetch answers; when it is `ok`
# the file is produced, the way a real fetch would.
hv_fetch() {
    case "$2" in
        *.sha256)
            [ "${HV_TEST_SIDECAR}" = ok ] || { echo "${HV_TEST_SIDECAR}"; return 0; }
            printf '%s  archive.tar.gz\n' "${HV_TEST_DIGEST}" >"$2"
            echo ok ;;
        *)
            [ "${HV_TEST_ARCHIVE}" = ok ] || { echo "${HV_TEST_ARCHIVE}"; return 0; }
            cp "${fixture}/archive.tar.gz" "$2"
            echo ok ;;
    esac
}

run_main() {
    # Runs hv_main in a subshell so that hv_die's `exit 1` ends the subshell and
    # not the suite, and prints the code it exited with.
    #
    # It has to be `if`, and the code has to come from `$?` directly. Writing the
    # code to a file from inside the subshell does not work: hv_die calls `exit`,
    # so a trailing `echo "$?" >file` in the same subshell never runs and the
    # assertion would read a stale code from the previous case. `if` also keeps
    # `set -e` from ending the suite on the failing cases, which are most of them.
    if ( hv_main >"${fixture}/out" 2>&1 ); then printf 0; else printf '%s' "$?"; fi
}
said() { cat "${fixture}/out"; }

# The good path: it installs, and nothing is compiled.
rm -rf "${checkout}/target"
HV_TEST_ARCHIVE=ok HV_TEST_SIDECAR=ok HV_TEST_DIGEST="${good_digest}"
check "a good release installs" "$(run_main)" 0
if [ -x "${checkout}/target/release/herdr-voice" ]; then
    printf 'ok    the binary landed at target/release/herdr-voice, executable\n'
else
    printf 'FAIL  no executable at target/release/herdr-voice\n'
    failures=$((failures + 1))
fi

# A digest that disagrees stops, names both digests, and installs nothing.
rm -rf "${checkout}/target"
HV_TEST_ARCHIVE=ok HV_TEST_SIDECAR=ok HV_TEST_DIGEST=0000000000000000000000000000000000000000000000000000000000000000
check "a wrong digest stops" "$(run_main)" 1
case "$(said)" in
    *0000000000000000*"${good_digest}"*|*"${good_digest}"*0000000000000000*)
        printf 'ok    it named both digests\n' ;;
    *)  printf 'FAIL  it did not name both digests: %s\n' "$(said)"
        failures=$((failures + 1)) ;;
esac
if [ -e "${checkout}/target/release/herdr-voice" ]; then
    printf 'FAIL  a mismatched archive was unpacked anyway\n'
    failures=$((failures + 1))
else
    printf 'ok    a mismatched archive is not unpacked\n'
fi

# An archive with no digest beside it stops. The release exists and this platform
# is built; only the means to check the bytes is missing, and unpacking anyway is
# the one thing the digest was added to prevent.
rm -rf "${checkout}/target"
HV_TEST_ARCHIVE=ok HV_TEST_SIDECAR=missing HV_TEST_DIGEST="${good_digest}"
check "a missing digest stops" "$(run_main)" 1
if [ -e "${checkout}/target/release/herdr-voice" ]; then
    printf 'FAIL  an unverifiable archive was unpacked\n'
    failures=$((failures + 1))
else
    printf 'ok    an unverifiable archive is not unpacked\n'
fi

# A network that does not answer stops rather than falling back.
HV_TEST_ARCHIVE=error HV_TEST_SIDECAR=ok HV_TEST_DIGEST="${good_digest}"
check "an unreachable archive stops" "$(run_main)" 1
case "$(said)" in
    *cargo*) printf 'FAIL  it fell back to a source build on a network error\n'
             failures=$((failures + 1)) ;;
    *)       printf 'ok    it did not fall back on a network error\n' ;;
esac

# Every stopping message says the machine was left alone.
case "$(said)" in
    *"nothing was installed"*) printf 'ok    a stop says nothing was installed\n' ;;
    *) printf 'FAIL  a stop did not say nothing was installed: %s\n' "$(said)"
       failures=$((failures + 1)) ;;
esac

# A 404 falls back, and the message carries the URL, the tag, the target and both
# readings. `cargo` is replaced so the suite compiles nothing.
fakebin="${fixture}/bin"
mkdir -p "${fakebin}"
printf '#!/bin/sh\necho "fake cargo $*"\n' >"${fakebin}/cargo"
chmod +x "${fakebin}/cargo"
PATH="${fakebin}:${PATH}"
HV_TEST_ARCHIVE=missing HV_TEST_SIDECAR=ok HV_TEST_DIGEST="${good_digest}"
check "a missing archive falls back" "$(run_main)" 0
for fragment in v0.4.2 x86_64-unknown-linux-gnu https://github.com/; do
    case "$(said)" in
        *"${fragment}"*) printf 'ok    the fallback message carries %s\n' "${fragment}" ;;
        *) printf 'FAIL  the fallback message lacks %s: %s\n' "${fragment}" "$(said)"
           failures=$((failures + 1)) ;;
    esac
done

# The case this whole change exists for: no archive and no toolchain either.
#
# Overriding `command` itself does not work — there is no portable way to call
# through to the real builtin afterwards, and hv_main needs `command -v` for the
# digest tool. Emptying PATH does not work either: hv_main runs `mktemp` before
# it ever reaches the fallback. So install.sh asks through hv_have_cargo, and the
# test replaces that one function.
hv_have_cargo() { return 1; }
HV_TEST_ARCHIVE=missing HV_TEST_SIDECAR=ok HV_TEST_DIGEST="${good_digest}"
check "no archive and no cargo stops" "$(run_main)" 1
case "$(said)" in
    *rustup.rs*) printf 'ok    it says where to get a toolchain\n' ;;
    *) printf 'FAIL  it did not say where to get a toolchain: %s\n' "$(said)"
       failures=$((failures + 1)) ;;
esac
case "$(said)" in
    *"nothing was installed"*) printf 'ok    and says nothing was installed\n' ;;
    *) printf 'FAIL  it did not say nothing was installed: %s\n' "$(said)"
       failures=$((failures + 1)) ;;
esac
```

- [ ] **Step 2: Run it to make sure it fails**

Run: `sh scripts/test-install.sh`
Expected: FAIL — `hv_digest: not found`.

- [ ] **Step 3: Write the implementation**

In `scripts/install.sh`, insert after the fetching section:

```sh
# ---------------------------------------------------------------------------
# Verifying
# ---------------------------------------------------------------------------

hv_digest() {
    # $1 = file. macOS has shasum and not sha256sum; Linux has both or the
    # first. .github/workflows/release.yml makes the same choice when it writes
    # the sidecar this is compared against.
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# ---------------------------------------------------------------------------
# How this ends
#
# A non-zero exit from a build command aborts the install and registers no
# plugin, so a person reading any of these messages has a machine with nothing
# new on it. Each one says that, and says what to do next.
# ---------------------------------------------------------------------------

hv_die() {
    # $1 = what went wrong, $2 = what to do about it.
    printf '%s: %s\n' "${HV_NAME}" "$1" >&2
    printf '%s: nothing was installed. %s\n' "${HV_NAME}" "$2" >&2
    exit 1
}

hv_have_cargo() {
    # A function rather than an inline `command -v` so the suite can answer it
    # without emptying PATH, which would also take away the mktemp and tar this
    # script needs before it ever reaches the fallback.
    command -v cargo >/dev/null 2>&1
}

hv_fallback() {
    # $1 = why there is no archive to use.
    printf '%s: %s\n' "${HV_NAME}" "$1" >&2
    printf '%s: building from source instead, which compiles the candle crates and takes a while.\n' \
        "${HV_NAME}" >&2
    if ! hv_have_cargo; then
        hv_die "there is no archive for this platform and no cargo to build one" \
               "install a Rust toolchain from https://rustup.rs and run the install again, or install on a platform a release archive is published for."
    fi
    cargo build --release
}
```

Then replace `hv_main` in its entirety with:

```sh
hv_main() {
    root="$(hv_root)"
    cd "${root}"
    version="$(hv_manifest_version "${root}")"
    tag="v${version}"

    if ! target="$(hv_target "$(uname -s)" "$(uname -m)")"; then
        hv_fallback "no release archive is built for $(uname -s) $(uname -m)"
        return 0
    fi

    archive="${HV_NAME}-${tag}-${target}.tar.gz"
    base="https://github.com/${HV_REPO}/releases/download/${tag}"
    tmp="$(mktemp -d)"
    trap 'rm -rf "${tmp}"' EXIT

    case "$(hv_fetch "${base}/${archive}" "${tmp}/${archive}")" in
        ok) ;;
        missing)
            # A 404 here is what a missing release and a missing target both look
            # like, and nothing else fetched tells them apart. Telling them apart
            # needs a release-level API request, which brings its own retry, its
            # own failure mode and a rate limit a shared address can exhaust — a
            # fifth way to fail, bought for one clause. So the message names both.
            hv_fallback "no archive at ${base}/${archive} — either no release is tagged ${tag}, or that release has no build for ${target}"
            return 0 ;;
        error)
            hv_die "could not reach ${base}/${archive} after ${HV_RETRY_ATTEMPTS} attempts" \
                   "check the network and run the install again." ;;
    esac

    case "$(hv_fetch "${base}/${archive}.sha256" "${tmp}/${archive}.sha256")" in
        ok) ;;
        missing)
            hv_die "the release ${tag} publishes ${archive} but no ${archive}.sha256, so its bytes cannot be checked" \
                   "report it against the release; an unverified archive is not installed." ;;
        error)
            hv_die "could not reach ${base}/${archive}.sha256 after ${HV_RETRY_ATTEMPTS} attempts" \
                   "check the network and run the install again." ;;
    esac

    expected="$(cut -d' ' -f1 <"${tmp}/${archive}.sha256")"
    actual="$(hv_digest "${tmp}/${archive}")"
    if [ "${expected}" != "${actual}" ]; then
        hv_die "${archive} does not match the digest published with it (expected ${expected}, got ${actual})" \
               "the download is damaged or the release was changed after it was published; run the install again, and report it if it repeats."
    fi

    tar -xzf "${tmp}/${archive}" -C "${tmp}"
    mkdir -p "${root}/target/release"
    cp "${tmp}/${HV_NAME}-${tag}-${target}/${HV_NAME}" "${root}/target/release/${HV_NAME}"
    chmod 0755 "${root}/target/release/${HV_NAME}"
    printf '%s: installed %s from %s, verified against its published digest\n' \
        "${HV_NAME}" "target/release/${HV_NAME}" "${tag}"
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `sh scripts/test-install.sh`
Expected: every assertion `ok`, then `all install assertions passed`.

The suite leaves `hv_have_cargo` overridden after the last case. That is the end
of the file, so nothing later sees it; if you add cases after it, restore the
real one first.

- [ ] **Step 5: Run the four gates, then commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add scripts/install.sh scripts/test-install.sh
git commit -m "$(cat <<'EOF'
Check the bytes before trusting them, and say what happened when we cannot

The archive is compared against the digest published beside it before anything
is unpacked. A disagreement stops; so does an archive that arrives with no digest
at all, because the release exists and this platform is built, and only the means
to check is missing — unpacking anyway is the one thing the digest was added to
prevent.

Every stop says the install was aborted and nothing was registered, because that
is the state the person reading it is actually in. The one that matters most is
the last: no archive for this platform and no cargo either, which is the case this
whole change exists for, and it names where to get a toolchain rather than
printing "command not found".

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: the manifest calls the script, and the documents stop lying

**Files:**
- Modify: `herdr-plugin.toml:8-17`
- Modify: `docs/design.md:344-350`

**Interfaces:**
- Consumes: `scripts/install.sh` from task 4, `scripts/install.ps1` from task 6.
  **`scripts/install.ps1` does not exist yet.** The Windows entry is pointed at it
  here anyway, because `scripts/check_manifest.py` does not check that a build
  command's file exists and the two edits belong in one commit; task 6 creates
  the file. If you prefer, do task 6 first — nothing else depends on the order.
- Produces: nothing later tasks consume, except that task 7's container check
  exercises these entries.

- [ ] **Step 1: Replace the build entries and the comment**

In `herdr-plugin.toml`, replace lines 8 to 17 — the three comment lines and both
`[[build]]` entries — with the text below. Line 7 is the blank separator after
`platforms` and stays as it is.

```toml
# Installing from GitHub fetches the release archive built for this platform and
# checks it against the digest published beside it; only when there is no such
# archive does it compile. The archive is the one tagged `v` plus the `version`
# above, so a release tag and that version have to agree — nothing enforces it,
# and the fetch is what breaks if they drift. `herdr plugin link` runs neither
# entry: a local checkout is built with `cargo build --release` by hand.
[[build]]
platforms = ["macos", "linux"]
command = ["sh", "scripts/install.sh"]

[[build]]
platforms = ["windows"]
command = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/install.ps1"]
```

- [ ] **Step 2: Rewrite section 8 of the design**

In `docs/design.md`, replace the first paragraph of `## 8. Distribution` — the
sentence ending "fetch the archive that matches the platform" — with:

```markdown
Installed with `herdr plugin install aliaksandr-haurylau-godel/herdr-voice`. A
tagged release publishes archives for macOS on arm64 and x86_64, Linux on x86_64
and arm64, and Windows on x86_64, each with a `.sha256` beside it. The manifest's
`build` entries call `scripts/install.sh` on macOS and Linux and
`scripts/install.ps1` on Windows, which work out the archive for the running
platform from the manifest's own `version`, fetch it, refuse it if its bytes do
not match the published digest, and unpack it to `target/release`. Where no
archive is published for the platform they compile instead, saying so; where the
archive cannot be reached at all they stop rather than compiling unasked. A tag
`vX.Y.Z` therefore has to be cut from a commit whose manifest says `X.Y.Z`: the
installed checkout is a shallow clone with no tags, so the manifest is the only
place the script can learn what to ask for.
```

- [ ] **Step 3: Check the manifest still satisfies its own gate**

Run: `python3 scripts/check_manifest.py`
Expected: it passes. It skips build commands that do not name the `herdr-voice`
binary, so the new entries are not checked against the binary's subcommands — and
the version equality it does check is untouched.

- [ ] **Step 4: Check nothing still claims the old behaviour**

Run:

```sh
grep -rn "cargo build --release" --include='*.toml' --include='*.md' . | grep -v target/
```

Expected: matches in `README.md`, `CLAUDE.md`, `docs/` and `tasks/` describing the
**local** build, which is still true, and none in `herdr-plugin.toml`.

- [ ] **Step 5: Run the four gates, then commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add herdr-plugin.toml docs/design.md
git commit -m "$(cat <<'EOF'
Point the build entries at the fetch, and make the comment above them true

The comment claimed that a released copy fetches an archive while both entries
ran cargo, and section 8 of the design said the same. Both described something
nobody had written. They now describe what happens, and the comment carries the
rule the mechanism creates: the tag and the manifest version have to agree,
because a shallow tagless clone leaves the manifest as the only place the script
can learn what to ask for.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: the Windows entry

**Files:**
- Create: `scripts/install.ps1`
- Create: `scripts/test-install.ps1`
- Modify: `.github/workflows/check.yml`

**Interfaces:**
- Consumes: nothing from the shell script; this is a separate implementation.
- Produces: `Get-HvTarget` taking `-Architecture`, returning the triple or `$null`.

This task makes the Windows archive fetchable. It does not claim it then runs —
that is #1, which the issue puts out of bounds, and no Windows machine is
available here.

- [ ] **Step 1: Write the failing test**

Create `scripts/test-install.ps1`:

```powershell
# Drives what can be driven without a Windows machine to install on: the file
# parses, and it maps the architecture the way the release matrix does.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$script = Join-Path $root 'scripts/install.ps1'
$failures = 0

$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile($script, [ref]$null, [ref]$errors) | Out-Null
if ($errors.Count -gt 0) {
    Write-Host "FAIL  install.ps1 does not parse: $($errors[0].Message)"
    $failures++
} else {
    Write-Host 'ok    install.ps1 parses'
}

$env:HERDR_VOICE_INSTALL_LIB = '1'
. $script

function Check($label, $actual, $expected) {
    if ($actual -eq $expected) { Write-Host "ok    $label" }
    else { Write-Host "FAIL  $label`: got [$actual], expected [$expected]"; $script:failures++ }
}

Check 'AMD64 maps to the built target' (Get-HvTarget -Architecture 'AMD64') 'x86_64-pc-windows-msvc'
# The release matrix has no ARM64 Windows build, so there is no archive and the
# run goes to the source build.
Check 'ARM64 has no target' (Get-HvTarget -Architecture 'ARM64') $null

if ($failures -gt 0) { Write-Error "$failures assertion(s) failed"; exit 1 }
Write-Host "`nall install.ps1 assertions passed"
```

- [ ] **Step 2: Run it to make sure it fails**

On a Windows runner, or locally if PowerShell is installed:
`pwsh -File scripts/test-install.ps1`
Expected: FAIL — the script file does not exist. If no PowerShell is available on
this machine, push the branch and let the CI job added in step 5 be the run that
shows red first; say so in the commit rather than claiming a local run.

- [ ] **Step 3: Write the implementation**

Create `scripts/install.ps1`:

```powershell
# The herdr [[build]] step for Windows: put the released binary at
# target\release\herdr-voice.exe without compiling it.
#
# Same sequence as scripts/install.sh, and the same rules: a 404 that survives the
# retries is the source build, anything else that stops us reaching the archive is
# a stop, an archive whose digest is missing or wrong is a stop, and every stop
# says the install was aborted and nothing registered.
#
# Dot-sourcing this with HERDR_VOICE_INSTALL_LIB=1 defines the functions and runs
# nothing. scripts/test-install.ps1 does that.

$ErrorActionPreference = 'Stop'

$HvName = 'herdr-voice'
$HvRepo = 'aliaksandr-haurylau-godel/herdr-voice'
$HvRetryAttempts = if ($env:HV_RETRY_ATTEMPTS) { [int]$env:HV_RETRY_ATTEMPTS } else { 5 }
$HvRetryDelay    = if ($env:HV_RETRY_DELAY)    { [int]$env:HV_RETRY_DELAY }    else { 3 }

function Get-HvRoot { Split-Path -Parent $PSScriptRoot }

function Get-HvManifestVersion([string]$Root) {
    $line = Select-String -Path (Join-Path $Root 'herdr-plugin.toml') `
        -Pattern '^version = "([^"]*)"' | Select-Object -First 1
    $line.Matches[0].Groups[1].Value
}

function Get-HvTarget([string]$Architecture) {
    # The release matrix builds one Windows target. ARM64 has no archive, which
    # is not an error — it is the source build.
    switch ($Architecture) {
        'AMD64' { 'x86_64-pc-windows-msvc' }
        default { $null }
    }
}

function Invoke-HvFetch([string]$Url, [string]$Destination) {
    # Returns 'ok', 'missing' or 'error', after retrying. GitHub answers 404 for
    # some minutes after a release publishes, so a single 404 settles nothing.
    for ($attempt = 1; ; $attempt++) {
        $outcome = 'error'
        try {
            Invoke-WebRequest -Uri $Url -OutFile $Destination -UseBasicParsing
            $outcome = 'ok'
        } catch {
            $code = $_.Exception.Response.StatusCode.value__
            if ($code -eq 404) { $outcome = 'missing' }
        }
        if ($outcome -eq 'ok' -or $attempt -ge $HvRetryAttempts) { return $outcome }
        Start-Sleep -Seconds $HvRetryDelay
    }
}

function Stop-Hv([string]$Reason, [string]$Next) {
    Write-Host "${HvName}: $Reason"
    Write-Host "${HvName}: nothing was installed. $Next"
    exit 1
}

function Invoke-HvFallback([string]$Reason) {
    Write-Host "${HvName}: $Reason"
    Write-Host "${HvName}: building from source instead, which compiles the candle crates and takes a while."
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Stop-Hv 'there is no archive for this platform and no cargo to build one' `
                'install a Rust toolchain from https://rustup.rs and run the install again, or install on a platform a release archive is published for.'
    }
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Stop-Hv 'the source build failed' 'read the compiler output above.'
    }
}

function Invoke-HvMain {
    $root = Get-HvRoot
    Set-Location $root
    $tag = 'v' + (Get-HvManifestVersion $root)
    $target = Get-HvTarget -Architecture $env:PROCESSOR_ARCHITECTURE
    if (-not $target) {
        Invoke-HvFallback "no release archive is built for Windows on $env:PROCESSOR_ARCHITECTURE"
        return
    }

    $archive = "$HvName-$tag-$target.tar.gz"
    $base = "https://github.com/$HvRepo/releases/download/$tag"
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $tmp | Out-Null

    switch (Invoke-HvFetch "$base/$archive" "$tmp\$archive") {
        'missing' {
            Invoke-HvFallback "no archive at $base/$archive - either no release is tagged $tag, or that release has no build for $target"
            return
        }
        'error' {
            Stop-Hv "could not reach $base/$archive after $HvRetryAttempts attempts" `
                    'check the network and run the install again.'
        }
    }

    switch (Invoke-HvFetch "$base/$archive.sha256" "$tmp\$archive.sha256") {
        'missing' {
            Stop-Hv "the release $tag publishes $archive but no $archive.sha256, so its bytes cannot be checked" `
                    'report it against the release; an unverified archive is not installed.'
        }
        'error' {
            Stop-Hv "could not reach $base/$archive.sha256 after $HvRetryAttempts attempts" `
                    'check the network and run the install again.'
        }
    }

    $expected = ((Get-Content "$tmp\$archive.sha256" -Raw).Trim() -split '\s+')[0]
    $actual = (Get-FileHash "$tmp\$archive" -Algorithm SHA256).Hash.ToLower()
    if ($expected.ToLower() -ne $actual) {
        Stop-Hv "$archive does not match the digest published with it (expected $expected, got $actual)" `
                'the download is damaged or the release was changed after it was published; run the install again, and report it if it repeats.'
    }

    tar -xzf "$tmp\$archive" -C $tmp
    New-Item -ItemType Directory -Force -Path (Join-Path $root 'target\release') | Out-Null
    Copy-Item (Join-Path $tmp "$HvName-$tag-$target\$HvName.exe") `
              (Join-Path $root "target\release\$HvName.exe") -Force
    Write-Host "${HvName}: installed target\release\$HvName.exe from $tag, verified against its published digest"
}

if ($env:HERDR_VOICE_INSTALL_LIB -ne '1') { Invoke-HvMain }
```

- [ ] **Step 4: Run the test to verify it passes**

`pwsh -File scripts/test-install.ps1`
Expected: three `ok` lines, then `all install.ps1 assertions passed`.

- [ ] **Step 5: Add the Windows CI job**

In `.github/workflows/check.yml`, add after the `scripts` job:

```yaml
  scripts-windows:
    name: the Windows install script
    runs-on: windows-latest
    timeout-minutes: 5
    steps:
      - uses: actions/checkout@v4
      - name: it parses, and maps the architecture
        shell: pwsh
        run: pwsh -File scripts/test-install.ps1
```

- [ ] **Step 6: Check the workflow parses, run the four gates, then commit**

```sh
python3 -c "import yaml;yaml.safe_load(open('.github/workflows/check.yml'));print('parses')"
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add scripts/install.ps1 scripts/test-install.ps1 .github/workflows/check.yml
git commit -m "$(cat <<'EOF'
Fetch the archive on Windows too, without claiming it then runs

The same sequence and the same four rules as the shell script. It is a second
implementation rather than a shared one because there is nothing to share: the
release matrix builds exactly one Windows target, so where the shell script needs
a four-way map this needs a constant.

What CI can check without a Windows machine to install on is that the file parses
and that it maps the architecture the way the matrix does. Whether the fetched
binary runs is #1, which this issue leaves alone.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: the container check

**Files:**
- Create: `scripts/install-check.sh`

**Interfaces:**
- Consumes: everything above; it exercises the real manifest entries.
- Produces: the output that becomes a section of `docs/evidence.md` at S5.

This task **writes** the check. Running it needs a published release, which does
not exist and is not this plan's to create. Running it is S5.

- [ ] **Step 1: Write the script**

Create `scripts/install-check.sh`:

```sh
#!/usr/bin/env bash
#
# Verify that `herdr plugin install` works on a machine with no Rust toolchain,
# inside a throwaway container.
#
# This is the check `scripts/linux-check.sh` cannot be. That one installs rustup
# and cargo and then links a local checkout, which never runs a build entry at
# all. This one asserts that no toolchain is present and installs from GitHub, so
# the build entry under test is the thing being exercised.
#
# Run it inside a clean glibc Linux container as root, with network access:
#
#   bash scripts/install-check.sh
#
# A musl image (Alpine) will not do: the published Linux archives are built
# against glibc.
#
# Environment it honours:
#
#   HERDR_VOICE_REF    the tag or branch to install (default: v0.0.0)
#   HERDR_VOICE_REPO   the owner/repo shorthand to install from
#   HERDR_VOICE_WORK   where the logs go (default: /tmp/herdr-voice-install-check)

set -eu

PLUGIN_ID="haurylau.voice"
REPO="${HERDR_VOICE_REPO:-aliaksandr-haurylau-godel/herdr-voice}"
REF="${HERDR_VOICE_REF:-v0.0.0}"
WORK="${HERDR_VOICE_WORK:-/tmp/herdr-voice-install-check}"
REPORT="${WORK}/report.tsv"
LOGS="${WORK}/logs"

mkdir -p "${WORK}" "${LOGS}"
: >"${REPORT}"

record() { printf '%s\t%s\t%s\n' "$1" "$2" "$3" >>"${REPORT}"; }

print_report() {
    printf '\n== report ==\n'
    printf '%-14s %-5s %s\n' "step" "exit" "outcome"
    while IFS=$'\t' read -r step code outcome; do
        printf '%-14s %-5s %s\n' "${step}" "${code}" "${outcome}"
    done <"${REPORT}"
    printf '\nlogs: %s\n' "${LOGS}"
}

fail() {
    record "$1" "$2" "$3"
    printf '\nFAILED at step %s (exit %s): %s\n' "$1" "$2" "$3" >&2
    printf 'next: %s\n' "$4" >&2
    print_report
    exit 1
}

cleanup() {
    if command -v herdr >/dev/null 2>&1; then
        herdr plugin uninstall "${PLUGIN_ID}" >/dev/null 2>&1 || true
    fi
    pkill -f 'herdr-voice daemon' >/dev/null 2>&1 || true
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Step 1: no toolchain
#
# The premise of the whole check, and the easiest thing to lose: an image that
# happens to carry cargo would turn this into a slower copy of linux-check.sh
# while still printing green.
# ---------------------------------------------------------------------------

printf '\n== step: no-toolchain ==\n'
for tool in cargo rustup rustc; do
    if command -v "${tool}" >/dev/null 2>&1; then
        fail "no-toolchain" "-" "${tool} is on PATH, so this image cannot prove anything" \
            "run this in an image with no Rust toolchain; that absence is what the check is about"
    fi
done
record "no-toolchain" "0" "no cargo, no rustup, no rustc"

# ---------------------------------------------------------------------------
# Step 2: what the install itself needs
# ---------------------------------------------------------------------------

printf '\n== step: prereqs ==\n'
if command -v apt-get >/dev/null 2>&1; then
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq >"${LOGS}/apt.log" 2>&1
    apt-get install -y -qq git curl ca-certificates tar >>"${LOGS}/apt.log" 2>&1 \
        || fail "prereqs" "$?" "could not install git, curl, ca-certificates and tar" \
            "read ${LOGS}/apt.log"
else
    fail "prereqs" "-" "no apt-get on this image" \
        "use a Debian or Ubuntu image, or install git, curl, ca-certificates and tar by hand first"
fi
record "prereqs" "0" "git, curl, ca-certificates, tar"

# ---------------------------------------------------------------------------
# Step 3: herdr
# ---------------------------------------------------------------------------

printf '\n== step: herdr ==\n'
if ! command -v herdr >/dev/null 2>&1; then
    curl -fsSL https://herdr.dev/install.sh | sh >"${LOGS}/herdr-install.log" 2>&1 \
        || fail "herdr" "$?" "the herdr installer failed" "read ${LOGS}/herdr-install.log"
fi
export PATH="${HOME}/.local/bin:${PATH}"
command -v herdr >/dev/null 2>&1 \
    || fail "herdr" "-" "herdr is not on PATH after the installer ran" \
        "read ${LOGS}/herdr-install.log and add the directory it used to PATH"
HERDR_VERSION="$(herdr --version 2>&1 | head -n 1)"
record "herdr" "0" "${HERDR_VERSION}"

# ---------------------------------------------------------------------------
# Step 4: the install under test
# ---------------------------------------------------------------------------

printf '\n== step: install ==\n'
printf 'installing %s at %s\n' "${REPO}" "${REF}"
set +e
herdr plugin install "${REPO}" --ref "${REF}" --yes >"${LOGS}/install.log" 2>&1
INSTALL_CODE=$?
set -e
cat "${LOGS}/install.log"
if [ "${INSTALL_CODE}" -ne 0 ]; then
    fail "install" "${INSTALL_CODE}" "herdr plugin install failed" \
        "read ${LOGS}/install.log; the build entry's own message is in it"
fi
record "install" "0" "installed ${REPO} at ${REF}"

# ---------------------------------------------------------------------------
# Step 5: it fetched rather than compiled
#
# With no toolchain present a compile could not have happened, so this is a
# second, cheaper witness rather than the proof: it catches a build entry that
# quietly fell back and somehow succeeded.
# ---------------------------------------------------------------------------

printf '\n== step: no-compile ==\n'
if grep -qiE '^\s*(Compiling|Downloaded) |cargo build' "${LOGS}/install.log"; then
    fail "no-compile" "-" "the install log shows a source build" \
        "read ${LOGS}/install.log; the fetch path did not run, and the fallback's message says why"
fi
if ! grep -q "verified against its published digest" "${LOGS}/install.log"; then
    fail "no-compile" "-" "the install log does not say an archive was verified" \
        "read ${LOGS}/install.log; the build entry may not have run at all"
fi
record "no-compile" "0" "fetched and verified, nothing compiled"

# ---------------------------------------------------------------------------
# Step 6: the plugin is registered
# ---------------------------------------------------------------------------

printf '\n== step: registered ==\n'
herdr plugin list >"${LOGS}/list.log" 2>&1
cat "${LOGS}/list.log"
grep -q "${PLUGIN_ID}" "${LOGS}/list.log" \
    || fail "registered" "-" "${PLUGIN_ID} is not in herdr plugin list" \
        "read ${LOGS}/list.log"
grep -E "${PLUGIN_ID}.*enabled" "${LOGS}/list.log" >/dev/null \
    || fail "registered" "-" "${PLUGIN_ID} is installed but not enabled" \
        "read ${LOGS}/list.log"
record "registered" "0" "${PLUGIN_ID} installed and enabled"

# ---------------------------------------------------------------------------
# Step 7: the binary it fetched actually runs
# ---------------------------------------------------------------------------

printf '\n== step: runs ==\n'
# herdr stores a GitHub-installed plugin under a directory named for the plugin
# id plus a hash, so the name is found rather than known.
PLUGIN_ROOT="$(find "${HOME}/.config/herdr/plugins/github" \
    -maxdepth 1 -type d -name "${PLUGIN_ID}-*" 2>/dev/null | head -n 1)"
if [ -z "${PLUGIN_ROOT}" ] || [ ! -x "${PLUGIN_ROOT}/target/release/herdr-voice" ]; then
    fail "runs" "-" "no executable at target/release/herdr-voice under the installed plugin" \
        "look under ${HOME}/.config/herdr/plugins/github for the plugin directory"
fi
set +e
"${PLUGIN_ROOT}/target/release/herdr-voice" doctor >"${LOGS}/doctor.log" 2>&1
DOCTOR_CODE=$?
set -e
cat "${LOGS}/doctor.log"
if [ ! -s "${LOGS}/doctor.log" ]; then
    fail "runs" "${DOCTOR_CODE}" "the fetched binary produced no output at all" \
        "a binary for the wrong architecture fails like this; check the target in the install log"
fi
record "runs" "${DOCTOR_CODE}" "the fetched binary runs and answers"

print_report
printf '\nthe install path works with no Rust toolchain present\n'
```

Make it executable: `chmod +x scripts/install-check.sh`.

- [ ] **Step 2: Check it is syntactically sound without running it**

Run: `bash -n scripts/install-check.sh`
Expected: no output, exit 0. It cannot be run here: it installs packages as root
and needs a published release.

- [ ] **Step 3: Run the four gates, then commit**

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && python3 scripts/check_manifest.py
git add scripts/install-check.sh
git commit -m "$(cat <<'EOF'
A container check for the claim this issue actually makes

linux-check.sh installs rustup and then links a local checkout, so it has a
toolchain by construction and never runs a build entry. Neither half of that can
witness an install that compiles nothing.

This asserts first that no cargo, rustup or rustc is present — the premise, and
the easiest thing to lose to an image that happens to carry one — and then
installs from GitHub, which is the path under test. It reads the install log for
the build entry's own words rather than only its exit code, because a fallback
that succeeded would otherwise look identical to a fetch.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## After the last task

These are **not** implementation and are not done by whoever executes this plan.

1. The four gates green on the whole branch, and `superpowers:requesting-code-review`
   on the diff — S4 closes on a review of the diff, not on a pull request.
2. The owner decides whether to cut `v0.0.0`. He is told first what the releases
   page will show.
3. The tag is pushed; `release.yml` publishes five archives and five sidecars as a
   prerelease. While it does, the time between the release appearing and the asset
   URL answering 200 is measured, and the retry budget in `DESIGN_42.md` section 4
   is confirmed or corrected.
4. `scripts/install-check.sh` runs in a Debian container against `--ref v0.0.0`.
   Its report becomes a section of `docs/evidence.md` with the date, the image and
   the architecture.
5. The `v0.0.0` release is not deleted before that section exists.
6. A new issue is opened for the first real release: the version decision, the
   tag, and a check that its archives install.

## Self-review

**Spec coverage.** Section 1 of the design — the tag from the manifest — is task 2.
Section 2, where the logic lives, is tasks 2 and 5. Section 3's step list is tasks
2 to 4. Section 4's five outcomes are task 4, with the retry in task 3. Section 5,
the digest, is task 1. Section 6, the prerelease rule, is task 1. Section 7, the
container check, is task 7. Section 8's testing is tasks 1 to 4 and 6. Section 9's
documents are task 5. Section 10's ordering is this plan's task order, with its
steps 6 to 8 in "After the last task".

**Criteria coverage.** AC-1 task 4 and task 6; AC-2 tasks 1 and 4; AC-3 task 4;
AC-3a task 4; AC-4 task 4; AC-4a task 3; AC-4b tasks 3 and 4; AC-5 task 4;
AC-5a task 4; AC-6, AC-6a, AC-6b "After the last task", with the script from task
7; AC-7 task 5, whose comment states that `link` runs neither entry; AC-8 task 5;
AC-9 every task's gate step; AC-10 tasks 1 to 4 and 6.

**Known gap, stated rather than hidden.** AC-1 asks for the fetch to work on four
platforms; only the two this machine and CI can reach are exercised by a real
fetch, and only one of those — `aarch64-unknown-linux-gnu`, in the container —
performs a real install. The macOS paths are exercised as functions, not as
installs. Windows is parsed and its mapping tested, nothing more, which the issue
permits by putting Windows verification out of bounds.
