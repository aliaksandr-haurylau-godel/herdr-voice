# DESIGN_42

How `herdr plugin install` stops compiling the plugin. Written from `AC_42.md`,
which the designer gate passed on 2026-09-16.

## What changes

The manifest's two `build` entries stop running `cargo build --release` and start
running a script this repository ships, one per platform family. The script works
out which release archive belongs to the checkout it is sitting in, fetches it
along with a digest published beside it, refuses anything whose bytes disagree,
and unpacks the binary to the path every other manifest entry already names.
Where it can establish that no archive exists for this platform, it builds from
source and says why; where it cannot establish anything, it stops rather than
guessing.

Three other things change because that one does. `.github/workflows/release.yml`
starts publishing a digest beside each archive, and learns which tags are
prereleases. A new container check proves an install with no Rust toolchain. The
comment in the manifest and the sentence in `docs/design.md` stop describing a
mechanism that did not exist.

## 1. How the script knows what to fetch

### Context

A build command runs inside the checkout herdr made. That checkout is a shallow
git clone with no tags: `git describe --tags` in an installed plugin on this
machine answers "fatal: No names found, cannot describe anything", and `git tag`
prints nothing. Build commands also receive none of herdr's environment, so there
is no variable naming the version, the source or the reference.

### Problem

The archive is named after the tag — `release.yml` builds
`herdr-voice-${GITHUB_REF_NAME}-${target}.tar.gz` — and the script has no way to
read the tag. It cannot construct the URL from anything it can see, unless
something inside the checkout states which release the checkout belongs to.

### Decision

The script reads `version` from `herdr-plugin.toml` and asks for the tag
`v<version>`. The archive is `herdr-voice-v<version>-<target>.tar.gz` and the
base URL is
`https://github.com/aliaksandr-haurylau-godel/herdr-voice/releases/download/v<version>/`.

This creates a rule the release process must keep from here on: **a tag `vX.Y.Z`
is cut from a commit whose manifest says `X.Y.Z`.** Nothing enforces it and
nothing ever did — `scripts/check_manifest.py` compares the manifest's version
with the crate's and never looks at a tag, and `release.yml` takes the tag name
for the archive name and the release name and checks it against nothing, so a
disagreeing tag passes every gate today. The rule is created by this mechanism,
not inherited from one.

### Why

It is the only statement of the release inside the checkout. The alternative —
asking GitHub for the latest release — was rejected because it installs code that
is not the code that was cloned: a person pinning `--ref` to an old revision
would silently get the newest binary.

## 2. Where the fetch logic lives

### Context

herdr runs `command` as argv with no shell, and offers `platforms` on each
`[[build]]` entry so a platform difference is declared rather than branched.
Two plugins installed on this machine solve this problem in the two available
shapes: `ranolp.handsfree` puts a `curl … | tar xz` pipeline in a single manifest
line, and `persiyanov.reviewr` calls a script kept in its own repository.

### Problem

This issue needs an architecture map, a version read out of a file, a digest
comparison, a bounded retry, a rule separating a 404 from a network failure, a
fallback, and four distinct named failure messages. A single `sh -c` line cannot
carry that. `ranolp.handsfree` fits in one line only because it hardcodes its
version and its one platform and verifies nothing.

### Decision

Two scripts in the repository, called by the manifest:

```toml
[[build]]
platforms = ["macos", "linux"]
command = ["sh", "scripts/install.sh"]

[[build]]
platforms = ["windows"]
command = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/install.ps1"]
```

Each script resolves the repository root from its own location rather than from
the environment, because a build command receives no `HERDR_PLUGIN_ROOT`. Neither
script writes to `herdr-plugin.toml`, because changing the manifest during a
build aborts the install.

### Why

Two implementations of one rule drift apart, which is the cost of this decision.
What keeps it small is that the matrix has exactly one Windows target: the
PowerShell script needs a single constant, `x86_64-pc-windows-msvc`, where the
shell script needs a four-way map. There is no shared table to keep in step, so
the duplication is the fetch-verify-unpack sequence and not the knowledge.

## 3. What the shell script does, step by step

`scripts/install.sh` is POSIX shell. Its decision logic is functions and its
entry point is guarded, so a test can source it without running it.

1. **Resolve the root** from `dirname "$0"/..`.
2. **Read the version** — the first `version = "…"` in `herdr-plugin.toml` — and
   form `TAG="v$VERSION"`.
3. **Map the platform** from `uname -s` and `uname -m`:

   | `uname -s` | `uname -m` | target |
   |---|---|---|
   | `Darwin` | `arm64` | `aarch64-apple-darwin` |
   | `Darwin` | `x86_64` | `x86_64-apple-darwin` |
   | `Linux` | `x86_64` | `x86_64-unknown-linux-gnu` |
   | `Linux` | `aarch64` or `arm64` | `aarch64-unknown-linux-gnu` |

   Anything else has no archive in the matrix and goes to the fallback.
4. **Fetch the archive, then the digest sidecar**, each through the retry in
   section 4. A 404 on the archive goes to the fallback; a 404 on the sidecar,
   once the archive is already in hand, stops.
5. **Verify** — compute the digest with `sha256sum` where it exists and
   `shasum -a 256` otherwise, because macOS has the second and not the first, and
   compare the hexadecimal field.
6. **Unpack** into a temporary directory and copy the binary to
   `target/release/herdr-voice` with mode 0755, creating the directory first.
7. **Exit 0.**

The binary lands at `target/release/herdr-voice` because that is where the
existing `startup`, `actions` and `panes` entries look for it —
`herdr-plugin.toml:21`, `:25`, `:32`, `:39`, `:46`, `:53`, `:61`, `:69`, `:77`.
Putting it anywhere else would mean editing ten entries this issue has no reason
to touch.

`scripts/install.ps1` does the same with `Invoke-WebRequest`, `Get-FileHash
-Algorithm SHA256` and the `tar.exe` that ships with Windows, landing the binary
at `target\release\herdr-voice.exe`. Its target is the constant
`x86_64-pc-windows-msvc`; on an ARM64 Windows machine there is no archive in the
matrix, so it goes to the fallback.

## 4. What happens when the fetch does not succeed

### Context

The build entry runs once, before herdr registers anything. A build command that
exits non-zero aborts the install and leaves no plugin behind — herdr reports the
plugin id, the build index, the working directory, the command, the exit status
and the captured output, and stops. There is no retry and no partial install.

### Problem

Five different things can go wrong and they do not want the same answer. Treating
them alike either strands a person who could have compiled, or starts an hour of
compiling nobody asked for, or turns the digest check into decoration.

### Decision

Five outcomes, and a retry underneath all of them.

| What happened | What the script does |
|---|---|
| The archive URL answers 404 through all the retries | Build from source, printing one line with the URL, the tag and the target, and saying that either no release carries that tag or that release has no build for this platform |
| A timeout, a name-resolution failure, a 5xx | Stop. Say the archive could not be reached and to run the install again |
| The archive arrived and the digest sidecar does not | Stop. Say the release is missing the digest for that archive and cannot be verified |
| The archive arrived and its digest disagrees | Stop. Name the archive, the expected digest and the computed one |
| The fallback ran and `cargo` is not on the machine | Stop. Name the missing toolchain, and that the alternative is a machine where a release archive exists for this platform |

The retry sits underneath: **five attempts, three seconds apart**, for the 404
case as much as for the error case. Only a 404 that survives all five counts as a
missing target.

**The first row's message names both possibilities and chooses between neither.**
A 404 on the archive URL is what a missing release and a missing target both look
like, and the script fetches nothing else that would tell them apart. Separating
them needs a release-level request — `GET /repos/{owner}/{repo}/releases/tags/v<version>`
answers 404 for the first case and 200 with an asset list for the second — and
that request brings a second network dependency, its own retry, its own failure
mode, and an unauthenticated rate limit of sixty requests an hour per address
that a shared address in a container or a runner can exhaust. It would buy one
clause in one message, in exchange for a fifth way for the install to fail in a
script whose whole purpose is predictable failure. So the message says which URL
did not answer and names both readings, and the person reading it can see a
release page for themselves.

**A missing sidecar stops rather than falling back.** The archive is in hand, so
the release exists and this platform is built — what is missing is the means to
check the bytes, and unpacking unverified bytes is the one thing the digest was
added to prevent. Falling back would answer a broken release with an hour of
compiling and say nothing about the release being broken. Stopping says exactly
what is wrong, and it is wrong on the publishing side, where it can be fixed.

Every message that ends in a stop also says that nothing was installed, because
the person is reading it out of an aborted install and the machine is in a state
they did not choose.

### Why

The retry and the 404 rule are two rules, not one, and collapsing them
reintroduces the hazard. GitHub's content delivery network answers 404 for some
minutes after a release publishes — which is why `persiyanov.reviewr` retries with
`--retry-all-errors` — so a bare 404 is not reliable evidence that a target is
missing. **The retry decides whether we know; the 404 rule decides what we do
once we know.**

A network failure stops rather than falling back because falling back starts
compiling the `candle` family, which is the heaviest thing in the tree and the
exact cost this issue exists to remove. A person whose network hiccupped did not
ask for an hour of compiling and would rather run the command again.

A digest mismatch stops rather than falling back because an archive that arrived
with the wrong bytes is not the same event as an archive that is not there, and
answering it with a source build would make the verification ornamental.

Five attempts three seconds apart is `persiyanov.reviewr`'s budget, taken as a
starting point. **S5 replaces it with a measurement**: when the `v0.0.0` archives
publish, the asset URL is polled until it answers 200 and the real window is
recorded in `docs/evidence.md` with the date and the platform. If the window is
longer than fifteen seconds the budget changes and the design is corrected.

## 5. The digest, and what the release workflow publishes

### Context

`.github/workflows/release.yml` builds five targets, packages one `.tar.gz` each
and publishes them with `gh release create "$GITHUB_REF_NAME" dist/*.tar.gz`.
Nothing else is published — no checksum file of any kind.

### Problem

"Verify what they fetched against a digest published with it" is not satisfiable
against that workflow. The digest has to start existing before anything can check
it.

### Decision

**One `.sha256` sidecar per archive**, named `<archive>.sha256`, written in
`sha256sum` format with the archive's bare name, produced in the packaging step
that already runs under `shell: bash`. That step branches between `sha256sum` and
`shasum -a 256`, because macOS runners have the second and not the first — the
same branch the install script makes when it verifies. Both the archives and the
sidecars are uploaded as artifacts and passed to `gh release create`.

### Why

One sidecar per archive means the script fetches exactly the two files it needs
and parses one line, and a missing sidecar is distinguishable from a missing
archive. A single `SHA256SUMS` for the whole release would be one asset, but the
script would download every target's digest to check one, and a half-published
release becomes harder to tell apart from a complete one.

## 6. Which tags publish as a prerelease

### Context

This issue publishes a disposable release tagged `v0.0.0` so that the container
check has something to install from. No version number moves: `herdr-plugin.toml`
and `Cargo.toml` both stay at `0.0.0`, and the first real release of the plugin
is a separate issue.

### Problem

`gh release create` at `release.yml:61` passes neither `--prerelease` nor
`--latest=false`. As the workflow stands, tagging `v0.0.0` would publish it as
the repository's **latest** release — so a stranger running `herdr plugin install`
would find it and install it as though it were a version of the plugin. That is
the opposite of what a disposable tag is for.

### Decision

The rule goes in the workflow, keyed on the tag:

- A tag of `v0.0.0`, or any tag containing a hyphen, publishes with
  `--prerelease --latest=false`, and with `--notes` carrying fixed text saying the
  release exists to verify the install path and is not a version of the plugin.
- Every other tag publishes as it does today, with `--generate-notes`.

### Why

A hand edit after the workflow run is a step that gets forgotten exactly once,
and the once is the run where `v0.0.0` sits at the top of the releases page.
`0.0.0` is already this repository's placeholder for "unreleased" in both the
manifest and the crate, so keying on it states something true rather than
inventing a convention. A hyphen is semver's own prerelease marker and costs
nothing to honour.

Fixed notes rather than generated ones for those tags, because the point of the
text is to say what the tag is for, and because it avoids depending on how `gh`
combines `--notes` with `--generate-notes`. Generated notes were checked and do
work on a repository with no previous release: the API behind the flag, asked for
`tag_name=v0.0.0`, returned notes covering every merged pull request. Real
releases keep them.

## 7. Proving it in a container

### Context

`scripts/linux-check.sh` verifies this plugin against a live herdr on Linux in a
throwaway container. Its shape is: install system packages, install rustup and
cargo, build with `cargo build --release`, link with `herdr plugin link .`. It has
a Rust toolchain by construction, and it exercises `link`, which never runs build
entries at all.

### Problem

The claim this issue makes is that an install works **without** a toolchain. That
cannot be checked by a script whose second step installs one, and it cannot be
checked through `link`, which skips the code under test.

### Decision

A new `scripts/install-check.sh`, beside the existing one rather than inside it.
It runs in a glibc container as root with network access and:

1. Asserts `cargo` and `rustup` are **absent**, and fails loudly if either is
   present — the premise of the check is the thing most easily lost.
2. Installs herdr from `https://herdr.dev/install.sh`.
3. Runs `herdr plugin install aliaksandr-haurylau-godel/herdr-voice --ref v0.0.0 --yes`.
4. Asserts the command succeeded, that `herdr plugin list` shows `haurylau.voice`
   enabled from a GitHub source, that the installed binary runs, and that nothing
   in the captured output compiled anything.

It records each step's exit code and a one-line outcome and prints them as a
table at the end whether it passed or failed, the way `linux-check.sh` does.

The reference is the tag, not the branch. A tag pins one tree, and the archives
were built from exactly that tree, so the manifest the script reads and the
archives it fetches cannot disagree. A branch head moves, and evidence describing
a tree the published archives did not come from is evidence of nothing.

A glibc image because the Linux archives are `x86_64-unknown-linux-gnu` and
`aarch64-unknown-linux-gnu` and will not run on musl. On this machine Apple's
`container` runtime is arm64, so the target actually exercised is
`aarch64-unknown-linux-gnu`. No musl target is added to the matrix; this issue
does not ask for one.

### Why

Everything else in this design can be reasoned about and unit-tested. Whether
herdr runs the build entry from the checkout, whether its output reaches the
person, and whether an install with no toolchain ends with a working plugin, can
only be established by doing it.

## 8. Testing without a network, a herdr or a microphone

### Context

The repository's rule is that the pipeline stages are separated by interfaces so
they can be tested without a microphone, a model or a live herdr, and that
anything which cannot be is verified by hand and written into `docs/evidence.md`.
The new logic is shell, not Rust, so `cargo test` does not reach it.

### Decision

`scripts/install.sh` is written as functions plus a thin entry point, guarded so
that sourcing it defines the functions without running them.
`scripts/test-install.sh` sources it, replaces the fetching function with a stub
whose answers it controls, and asserts:

- each of the four `uname` pairs maps to the right target, and an unknown pair
  goes to the fallback;
- the tag comes from a fixture manifest, so `0.4.2` asks for `v0.4.2`;
- a digest that disagrees stops, and the message names both digests;
- an archive that arrives whose sidecar 404s stops, and does not unpack and does
  not fall back;
- a 404 on every attempt falls back, and the message names the URL, the tag and
  the target, and both readings of the 404;
- a 404 that succeeds on a later attempt does **not** fall back;
- a 5xx or a transport failure stops and does not fall back;
- the fallback with no `cargo` stops with the named message.

This becomes a job in `.github/workflows/check.yml` beside `manifest` and
`debris`, run on Linux and macOS so both branches of the `sha256sum` /
`shasum` split are exercised.

For `scripts/install.ps1`, CI parses the file on a Windows runner and tests its
target detection, and nothing further. Windows behaviour beyond that is not
verified here: the issue puts Windows verification out of bounds and leaves it
to #1, and this design makes the Windows archive fetchable without claiming it
then runs.

### Why

Every rule in section 4 is a decision the script makes about which of five things
happened, and each is exactly the kind of thing that is written once, read as
obviously correct, and wrong. A stub for the fetch is the only seam needed to
drive all of them, and it needs no network and no release.

## 9. The sentences that were not true

`herdr-plugin.toml:7-10` and `docs/design.md:346-349` both describe build entries
that fetch an archive. They become descriptions of what the entries now do. The
manifest comment also states the rule from section 1 — that the tag matches the
manifest's version — because the manifest is where someone changing the version
will be looking.

`README.md:38`, "Not yet installable. When the first release is tagged", stays as
it is. This issue does tag and publish a release, so the sentence is no longer
literally true — but `v0.0.0` is a disposable prerelease marked not-latest
precisely so that nobody installs it, and rewriting the README around it would
advertise the test tag as a way to install. It becomes true to change when the
first real release is cut.

## 10. Order, and what depends on what

The digest has to exist before anything can verify it, and the archives have to
exist before the container check can install them. That fixes the order:

1. `release.yml` publishes sidecars and learns the prerelease rule.
2. `scripts/install.sh`, `scripts/install.ps1` and `scripts/test-install.sh`, with
   the manifest's `build` entries repointed at them. The test suite depends on the
   script; nothing here depends on a release existing.
3. `check.yml` gains the job that runs the shell tests on Linux and macOS, and the
   job that parses `scripts/install.ps1` on a Windows runner and tests its target
   detection. Whether those PowerShell assertions live inline in the workflow or
   in a `scripts/test-install.ps1` is the plan's choice.
4. The documents and the manifest comment.
5. `scripts/install-check.sh` is **written** here, not run. It is code in the tree,
   so it is part of what the four gates pass and what the tag pins; running it
   needs a published release and therefore comes last.
6. **The four gates green**, then the `v0.0.0` tag is cut from the branch head —
   after the code is final, because the tag pins the tree the archives are built
   from. Publishing a release is outward-facing and is the owner's call; it is
   not taken as part of implementation.
7. `release.yml` runs on the tag. The window before the assets answer 200 is
   measured here, and the retry budget in section 4 is confirmed or corrected.
8. `scripts/install-check.sh` runs in a container against `--ref v0.0.0`, and its
   output goes into `docs/evidence.md` with the platform and the image.

The `v0.0.0` release must still exist when that evidence is written; it is not
deleted before then, because evidence pointing at a release that has been removed
is worse than no evidence.

`herdr-plugin.toml` is also being edited by #41, which adds a `[[panes]]` entry
with `id = "setup"`. Different section from the `[[build]]` entries, so whichever
pull request lands second rebases.

## 11. What this design does not do

- **It does not cut the first real release.** No version moves. A separate issue
  decides the version, cuts the tag and checks that its archives install.
- **It does not sign or notarise anything**, and publishes nowhere but GitHub
  releases. Both are out of bounds in the issue.
- **It does not claim the Windows archive runs.** It makes it fetchable; #1 owns
  whether it works.
- **It does not make an arbitrary revision installable without compiling.** After
  this lands, `main` still says `0.0.0`, so an install from `main` finds the
  `v0.0.0` archives. A revision whose manifest names a version nobody released
  gets the fallback, correctly.
- **It does not add a musl target**, so the archives stay unusable on Alpine.
