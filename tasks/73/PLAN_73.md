# PLAN_73 — the plugin id becomes `herdr-voice`

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans`
> with `superpowers:test-driven-development` to implement this plan task by task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the plugin's id stops carrying a surname — `haurylau.voice` becomes
`herdr-voice` — and a person who already has the plugin installed is told what the
rename left behind and offered the repair, rather than finding three keys that do
nothing.

**Architecture:** one constant changes value and a second is added beside it.
`setup` learns to recognise a `[[keys.command]]` block naming the previous id as
its own predecessor, to rewrite such blocks by editing only the quoted command
value on their `command` lines, and to report the two other leftovers the rename
produces. The safe-write half of `append` is extracted so the rewrite and the
append share one path to disk.

**Tech Stack:** Rust 2021, `serde`, `serde_json`, `toml`, `interprocess`. No new
dependency.

**Spec:** `tasks/73/DESIGN_73.md`, which the planner gate passed on round 2.
Acceptance criteria: `tasks/73/AC_73.md`. Read both.

## Global Constraints

- Everything inside the repository is written in English: code, comments, output
  strings, tests, commit messages.
- The repository is public. No employer, client, internal system or private
  machine may be named, and no absolute home path may appear. Paths in comments
  and documents are relative to the repository root.
- Four gates before every commit, run in this worktree, all green:
  `cargo test`; `cargo clippy --all-targets -- -D warnings`; `cargo fmt --check`;
  `python3 scripts/check_manifest.py`.
- Every commit message ends with
  `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- Before committing, grep every file written for `<new_string>`, `</new_string>`,
  `<old_string>`, `</old_string>` and for conflict markers at the start of a line.
- Device selection by name, never by index; configuration keys have defaults; no
  panic paths in the daemon. None of these is touched by this plan, and none may
  be broken by it.
- `PLUGIN_ID` is `"herdr-voice"`. `LEGACY_PLUGIN_ID` is `"haurylau.voice"`. The
  three action ids — `ptt`, `dictate`, `cancel` — and the keys they sit on —
  `ctrl+g`, `prefix+i`, `ctrl+shift+g` — do not change.

---

## File structure

| file | what changes |
|---|---|
| `herdr-plugin.toml` | `id` becomes `herdr-voice` (line 1) |
| `src/transport.rs` | `PLUGIN_ID`'s value; new `LEGACY_PLUGIN_ID`; new `legacy_sibling`; two test assertions |
| `src/setup.rs` | `Decision::superseded`; `decide`; `rewrite_commands`; `commit` extracted out of `append`; `Legacy`; `run`'s signature, report and question; `main` builds `Legacy`; test assertions |
| `src/config.rs` | module comment; two test assertions |
| `src/indicator.rs` | two test assertions |
| `src/client.rs` | the timeout message |
| `src/daemon.rs` | one doc comment |
| `tests/setup_process.rs` | one assertion, and `Legacy` reaching the process |
| `scripts/install-check.sh`, `scripts/linux-check.sh`, `scripts/test-install.sh`, `scripts/test-install.ps1` | the assigned id |
| `README.md`, `CLAUDE.md` | the command each prints |
| `docs/decisions.md` | one row for the rewrite decision |
| `docs/evidence.md` | a new section, written in S5 |

Tasks 1 to 3 are independent of each other. Task 4 consumes 1 and 2. Task 5
consumes 4. Task 6 consumes 1 and 2 and nothing else, so it can be done beside 4
and 5. Task 7 consumes 1. Task 8 consumes everything before it.

---

### Task 1: the two constants, and the id everywhere it is spelled out

**Files:**
- Modify: `herdr-plugin.toml:1`
- Modify: `src/transport.rs:18`
- Modify: `src/config.rs:6`, `src/daemon.rs:995`, `src/client.rs:78`
- Modify: `scripts/install-check.sh:30`, `scripts/linux-check.sh:30`,
  `scripts/test-install.sh:55`, `scripts/test-install.ps1:49`
- Modify: `README.md:46`, `CLAUDE.md:116`
- Test: the existing assertions in `src/setup.rs`, `src/transport.rs`,
  `src/config.rs`, `src/indicator.rs`, `tests/setup_process.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `transport::PLUGIN_ID == "herdr-voice"` and
  `transport::LEGACY_PLUGIN_ID == "haurylau.voice"`, both `pub const &str`.

- [ ] **Step 1: run the suite and watch the manifest test fail on purpose**

Change one side only, to see the guard work:

```bash
sed -i '' 's/^id = "haurylau.voice"$/id = "herdr-voice"/' herdr-plugin.toml
cargo test every_action_the_snippet_names_is_declared_in_the_manifest 2>&1 | tail -20
```

Expected: FAIL, `assertion \`left == right\` failed` on
`manifest["id"]` against `PLUGIN_ID` — the assertion at `src/setup.rs:1683`.
This is the guard the design leans on; see it fire before relying on it.

- [ ] **Step 2: change the constant and add the legacy one**

In `src/transport.rs`, replace line 18's declaration with:

```rust
/// The plugin id, which is also the last component of every derived path.
pub const PLUGIN_ID: &str = "herdr-voice";

/// The id this plugin had until issue #73. It survives for one purpose: telling
/// a person who installed the plugin under it what the rename left behind. See
/// `tasks/73/DESIGN_73.md`, section 1.
pub const LEGACY_PLUGIN_ID: &str = "haurylau.voice";
```

- [ ] **Step 3: run the suite and read every failure**

```bash
cargo test 2>&1 | grep -E "^(test |failures:|---- )" | head -40
```

Expected: the manifest assertion now passes, and the assertions that spell the
old id out fail. They are in `src/setup.rs` (lines 741, 922, 1218, 1376, 1443,
1481, 1547, 1592, 1601), `src/transport.rs` (269, 279), `src/config.rs` (576,
586), `src/indicator.rs` (947, 1264) and `tests/setup_process.rs` (122).

- [ ] **Step 4: update each failing assertion**

Replace `haurylau.voice` with `herdr-voice` in each. Keep them literal: an
assertion that builds its expectation from the same constant the code uses
asserts nothing. The two in `src/config.rs` and the two in `src/transport.rs` are
whole paths — `/tmp/xdg/herdr/plugins/herdr-voice/voice.sock` and the rest.

Then add the other half of AC-3 to `a_rendered_block_carries_all_four_fields`
(`src/setup.rs:735`), which today asserts what the snippet contains and never
what it does not:

```rust
        assert!(
            !rendered.contains("haurylau"),
            "the snippet is what a person pastes into their configuration: {rendered}"
        );
```

- [ ] **Step 5: update the four scripts, the two documents and the three comments**

```bash
grep -rn "haurylau\.voice" scripts README.md CLAUDE.md src/config.rs src/daemon.rs src/client.rs
```

Change each to `herdr-voice`. `src/client.rs:78` is a string a person reads when
the daemon does not answer; `src/config.rs:6` and `src/daemon.rs:995` are module
comments.

- [ ] **Step 6: the only places left are the constant and the tests that assert on it**

```bash
grep -rn "haurylau\.voice" . --exclude-dir=target --exclude-dir=tasks \
  --exclude-dir=.git --exclude=evidence.md
```

Expected: one line, `src/transport.rs`'s `LEGACY_PLUGIN_ID`. Tests naming it
arrive in tasks 2 to 5.

- [ ] **Step 7: all four gates**

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check \
  && python3 scripts/check_manifest.py
```

Expected: all four pass; the suite's count is unchanged from the 491 + 2 it was
before this task.

- [ ] **Step 8: commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
Give the plugin an id that carries no surname

haurylau.voice becomes herdr-voice. The id is not internal: it is what a
person types, what a keybinding names and what herdr calls the plugin's
configuration directory.

The previous id stays in one constant, LEGACY_PLUGIN_ID, which the next
commits use to recognise what an earlier install left behind.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `legacy_sibling`

**Files:**
- Modify: `src/transport.rs` — add the function after `state_directory`
- Test: `src/transport.rs`, in its `mod tests`

**Interfaces:**
- Consumes: `PLUGIN_ID`, `LEGACY_PLUGIN_ID` from task 1.
- Produces: `pub fn legacy_sibling(current: &std::path::Path) -> Option<PathBuf>`.

- [ ] **Step 1: write the failing tests**

Add to `mod tests` in `src/transport.rs`:

```rust
#[test]
fn the_legacy_sibling_is_the_same_directory_under_the_old_id() {
    assert_eq!(
        legacy_sibling(Path::new("/tmp/x/herdr/plugins/herdr-voice")),
        Some(PathBuf::from("/tmp/x/herdr/plugins/haurylau.voice"))
    );
}

/// herdr hands the plugin its directory in HERDR_PLUGIN_CONFIG_DIR and
/// HERDR_PLUGIN_STATE_DIR, and both end in the id herdr knows the plugin by.
/// Deriving the old location by running the same derivation with the old
/// constant would have returned that very directory, so `setup` would have
/// called the live configuration orphaned and told the person to kill the
/// daemon they are running.
#[test]
fn the_legacy_sibling_is_never_the_directory_it_was_given() {
    for given in [
        "/tmp/x/herdr/plugins/herdr-voice",
        "/some/place/herdr-voice",
        "herdr-voice",
    ] {
        let current = Path::new(given);
        assert_ne!(legacy_sibling(current).as_deref(), Some(current), "{given}");
    }
}

#[test]
fn a_directory_that_is_not_this_plugin_s_own_has_no_legacy_sibling() {
    for given in [
        "/tmp/x/herdr/plugins/somebody-else",
        "/tmp/x/herdr/plugins/herdr-voice/models",
        "/",
    ] {
        assert_eq!(legacy_sibling(Path::new(given)), None, "{given}");
    }
}
```

`mod tests` in `src/transport.rs` begins at line 240; check whether `Path` and
`PathBuf` are already in its `use` list and add what is missing.

- [ ] **Step 2: run them and watch them fail**

```bash
cargo test --lib transport::tests::the_legacy_sibling 2>&1 | tail -20
```

Expected: FAIL, `cannot find function \`legacy_sibling\` in this scope`.

- [ ] **Step 3: write the function**

In `src/transport.rs`, after `state_directory`:

```rust
/// The same directory under the id this plugin had before issue #73, or `None`
/// when `current` is not this plugin's own directory.
///
/// Derived from the current directory rather than computed a second time.
/// `state_directory` and `config::directory` both return
/// `HERDR_PLUGIN_STATE_DIR` / `HERDR_PLUGIN_CONFIG_DIR` verbatim when herdr sets
/// them, and herdr sets both to a path ending in the id it knows the plugin by —
/// so running either derivation with `LEGACY_PLUGIN_ID` would hand back the
/// directory the plugin is using now.
pub fn legacy_sibling(current: &std::path::Path) -> Option<PathBuf> {
    if current.file_name().and_then(|name| name.to_str()) != Some(PLUGIN_ID) {
        return None;
    }
    Some(current.parent()?.join(LEGACY_PLUGIN_ID))
}
```

- [ ] **Step 4: run them and watch them pass**

```bash
cargo test --lib transport::tests::the_legacy_sibling 2>&1 | tail -10
cargo test --lib transport::tests::a_directory_that_is_not 2>&1 | tail -10
```

Expected: PASS, four tests in total across the two runs.

- [ ] **Step 5: all four gates, then commit**

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check \
  && python3 scripts/check_manifest.py
git add -A
git commit -m "$(cat <<'EOF'
Find the old id's directory beside the one in use

herdr hands a plugin its directories in HERDR_PLUGIN_CONFIG_DIR and
HERDR_PLUGIN_STATE_DIR, and both end in the id herdr knows the plugin by.
Running the existing derivation with the old constant would therefore have
returned the directory the plugin reads now, and the socket the running
daemon owns.

legacy_sibling derives the old location from the current one instead, by
replacing its last component, and answers None when the last component is
not this plugin's id. It can never answer with the directory it was given.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `commit`, extracted out of `append`

**Files:**
- Modify: `src/setup.rs:417-487`
- Test: `src/setup.rs`, in its `mod tests`

**Interfaces:**
- Consumes: nothing.
- Produces:
  `fn commit(herdr: &dyn Herdr, path: &Path, make: impl FnOnce(&str) -> String) -> Result<(), WriteError>`,
  private to the module. `append` keeps its signature
  `pub fn append(herdr: &dyn Herdr, path: &Path, addition: &str) -> Result<(), WriteError>`.

- [ ] **Step 1: write the failing test**

`commit` must be able to produce text that is not the original plus something.
Add to `mod tests` in `src/setup.rs`:

```rust
#[test]
fn commit_writes_what_the_maker_returns_and_not_the_original() {
    let path = scratch("commit-replaces");
    std::fs::write(&path, "[theme]\nname = \"old\"\n").unwrap();
    commit(&FakeHerdr::clean(), &path, |original| {
        assert_eq!(original, "[theme]\nname = \"old\"\n");
        "[theme]\nname = \"new\"\n".to_string()
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[theme]\nname = \"new\"\n"
    );
}

/// The whole reason the write path is shared: herdr judges the candidate, and a
/// candidate it refuses never reaches the real file.
#[test]
fn commit_leaves_the_file_alone_when_herdr_refuses_the_candidate() {
    let path = scratch("commit-refused");
    std::fs::write(&path, "[theme]\n").unwrap();
    let herdr = FakeHerdr::answering(vec![
        Check::from_status(0, "config: ok".into()),
        Check::from_status(1, "config: issues found".into()),
    ]);
    let err = commit(&herdr, &path, |_| "[nonsense]\n".to_string()).unwrap_err();
    assert!(matches!(err, WriteError::CandidateRejected { .. }), "{err:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[theme]\n");
}
```

- [ ] **Step 2: run them and watch them fail**

```bash
cargo test --lib setup::tests::commit_ 2>&1 | tail -20
```

Expected: FAIL, `cannot find function \`commit\` in this scope`.

- [ ] **Step 3: extract the body**

Rename the existing `pub fn append` body to `fn commit`, changing its signature
and the two lines that build the new text. Everything else in it — the
`create_dir_all`, the `canonicalize`, the check of the original, the candidate
path, the permissions, the check of the candidate, the rename, the cleanup on
every failure — stays exactly as it is:

```rust
/// Replaces the file at `path` with whatever `make` returns for its current
/// text, with herdr's approval and nobody else's bytes lost.
///
/// [keep the whole doc comment that is on `append` today; it describes this]
fn commit(
    herdr: &dyn Herdr,
    path: &Path,
    make: impl FnOnce(&str) -> String,
) -> Result<(), WriteError> {
    // ... unchanged down to the point where the candidate's text is built ...
    let candidate_text = make(original.as_deref().unwrap_or_default());
    // ... unchanged from the candidate file onwards ...
}

/// Appends `addition` to the file at `path`. The blank line before it is what
/// keeps an appended block off the end of whatever was there.
pub fn append(herdr: &dyn Herdr, path: &Path, addition: &str) -> Result<(), WriteError> {
    commit(herdr, path, |original| {
        let mut text = original.to_string();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push('\n');
        text.push_str(addition);
        text
    })
}
```

- [ ] **Step 4: run the whole suite**

```bash
cargo test 2>&1 | tail -15
```

Expected: PASS. `append`'s own tests are the measuring device here — the symbolic
link test, the permissions test, the inode test, the trailing-newline test and
`a_commented_hand_written_file_keeps_every_byte_it_had` (`src/setup.rs:1047`) all
still pass, unchanged, because `append`'s behaviour did not change.

- [ ] **Step 5: re-establish the properties by mutation, not by trusting the move**

The four properties this extraction carries are the ones somebody's herdr
configuration depends on, and they were established by a mutation review when the
code was written. A move is exactly the kind of change that can drop one silently,
so break each one on purpose and watch a test go red. After each, undo it.

| mutation | expected |
|---|---|
| delete the `if original.is_some()` check of the original, so herdr never judges what is already there | a test fails on `OriginalRejected` |
| replace `std::fs::rename(&candidate, target)` with `std::fs::copy` plus a remove | the inode test fails — `src/setup.rs:1034` |
| delete the `canonicalize` that resolves a symbolic link | the symbolic-link test fails |
| delete the `set_permissions` that carries the original's mode | the permissions test fails |

```bash
cargo test --lib setup:: 2>&1 | tail -5
```

Expected, for each mutation in turn: at least one named failure. A mutation that
leaves the suite green is a property that is no longer tested — stop and say so
rather than continuing.

- [ ] **Step 6: all four gates, then commit**

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check \
  && python3 scripts/check_manifest.py
git add -A
git commit -m "$(cat <<'EOF'
Give append's safe write a name of its own

The sequence that makes a write to somebody's herdr configuration safe —
herdr judges the original, a candidate is written beside the real file with
the original's permissions, herdr judges that, one rename puts it in place,
and a symbolic link is resolved first — is now `commit`, which takes the
text it writes from a function of the original.

append is one call to it and behaves exactly as before, which its own tests
still measure. The rewrite that follows is a second call.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: recognising and rewriting a predecessor's blocks

**Files:**
- Modify: `src/setup.rs` — `Decision` (line 156), `decide` (line 167), and a new
  `rewrite_commands` beside them
- Test: `src/setup.rs`, in its `mod tests`

**Interfaces:**
- Consumes: `LEGACY_PLUGIN_ID` (task 1), `commit` (task 3).
- Produces:
  - `Decision` gains `pub superseded: Vec<(&'static Binding, String)>` — the
    binding, and the key its predecessor's block sits on.
  - `pub fn rewrite_commands(original: &str) -> String`.

- [ ] **Step 1: write the failing tests for `decide`**

```rust
#[test]
fn a_block_naming_the_previous_id_is_this_plugin_s_own_predecessor() {
    let existing = inspect(
        "[[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"haurylau.voice.ptt\"\n",
    )
    .unwrap();
    let decision = decide(&existing);
    assert_eq!(
        decision
            .superseded
            .iter()
            .map(|(b, key)| (b.action, key.as_str()))
            .collect::<Vec<_>>(),
        vec![("ptt", "ctrl+g")]
    );
    assert!(
        decision.blocked.is_empty(),
        "its own past is not a stranger holding the key: {:?}",
        decision.blocked
    );
    assert!(
        !decision.to_add.iter().any(|b| b.action == "ptt"),
        "ptt is repaired by the rewrite, not by an append"
    );
}

/// A key held by something that is not this plugin, under either id, is still
/// somebody else's.
#[test]
fn a_block_naming_another_plugin_is_still_a_stranger() {
    let existing = inspect(
        "[[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"somebody.else.ptt\"\n",
    )
    .unwrap();
    let decision = decide(&existing);
    assert!(decision.superseded.is_empty(), "{:?}", decision.superseded);
    assert_eq!(decision.blocked.len(), 1);
}
```

- [ ] **Step 2: run them and watch them fail**

```bash
cargo test --lib setup::tests::a_block_naming 2>&1 | tail -20
```

Expected: FAIL, `no field \`superseded\` on type \`Decision\``.

- [ ] **Step 3: add the field and the branch**

In `src/setup.rs`, add to `Decision`:

```rust
    /// A block naming the id this plugin had before issue #73, with the key it
    /// sits on. Its key does nothing until the block is rewritten.
    pub superseded: Vec<(&'static Binding, String)>,
```

and in `decide`, as the first test inside the loop, before the `already` test:

```rust
        let superseded = format!("{LEGACY_PLUGIN_ID}.{}", binding.action);
        if let Some((key, _)) = existing.commands.iter().find(|(_, c)| *c == superseded) {
            decision.superseded.push((binding, key.clone()));
            continue;
        }
```

`LEGACY_PLUGIN_ID` needs re-exporting beside `PLUGIN_ID` at `src/setup.rs:9`:

```rust
pub use crate::transport::{LEGACY_PLUGIN_ID, PLUGIN_ID};
```

- [ ] **Step 4: run them and watch them pass**

```bash
cargo test --lib setup::tests::a_block_naming 2>&1 | tail -10
```

Expected: PASS, 2 tests.

- [ ] **Step 5: write the failing tests for `rewrite_commands`**

```rust
/// The file this edits is hand-written, and the reason a key was chosen sits
/// above the block that uses it. The key does not change, so the comment stays
/// true — as long as the block stays where it is and only the command's value
/// moves.
#[test]
fn the_rewrite_changes_the_command_value_and_nothing_else() {
    let original = "# my herdr configuration\n\
                    \n\
                    [keys]\n\
                    prefix = \"ctrl+b\"\n\
                    \n\
                    # ctrl+g, because alt+v did nothing and alt+g typed ©\n\
                    [[keys.command]]\n\
                    \tkey = \"ctrl+g\"\n\
                    \ttype = \"plugin_action\"\n\
                    \tcommand   =   \"haurylau.voice.ptt\"  # hold to talk\n\
                    \n\
                    [[keys.command]]\n\
                    key = \"prefix+i\"\n\
                    command = \"haurylau.voice.dictate\"\n";
    let after = rewrite_commands(original);
    assert!(after.contains("\tcommand   =   \"herdr-voice.ptt\"  # hold to talk"), "{after}");
    assert!(after.contains("command = \"herdr-voice.dictate\""), "{after}");
    assert!(
        after.contains("# ctrl+g, because alt+v did nothing and alt+g typed ©\n[[keys.command]]"),
        "the comment stays directly above the block it explains: {after}"
    );
    assert_eq!(
        after.replace("herdr-voice.", "haurylau.voice."),
        original,
        "nothing but the two command values moved"
    );
}

/// A comment is not an assignment. One that mentions the old command says what
/// it said; rewriting it would edit a person's prose.
#[test]
fn a_comment_that_mentions_the_old_command_is_left_alone() {
    let original = "[[keys.command]]\n\
                    # this used to be haurylau.voice.ptt before the rename\n\
                    key = \"ctrl+g\"\n\
                    command = \"haurylau.voice.ptt\"\n";
    let after = rewrite_commands(original);
    assert!(
        after.contains("# this used to be haurylau.voice.ptt before the rename"),
        "{after}"
    );
    assert!(after.contains("command = \"herdr-voice.ptt\""), "{after}");
}

/// Outside a [[keys.command]] block nothing is a binding, whatever it looks
/// like.
#[test]
fn a_command_assignment_outside_a_binding_block_is_left_alone() {
    let original = "[some.other.section]\n\
                    command = \"haurylau.voice.ptt\"\n";
    assert_eq!(rewrite_commands(original), original);
}

#[test]
fn an_action_this_plugin_does_not_have_is_left_alone() {
    let original = "[[keys.command]]\n\
                    key = \"ctrl+j\"\n\
                    command = \"haurylau.voice.whatever\"\n";
    assert_eq!(rewrite_commands(original), original);
}
```

- [ ] **Step 6: run them and watch them fail**

```bash
cargo test --lib setup::tests::the_rewrite 2>&1 | tail -20
```

Expected: FAIL, `cannot find function \`rewrite_commands\` in this scope`.

- [ ] **Step 7: write `rewrite_commands`**

```rust
/// Every `command` value naming this plugin's previous id, inside a
/// `[[keys.command]]` block, rewritten to name the current one. Every other byte
/// of the file is copied through.
///
/// A line edit rather than a pass through a TOML document: the file is
/// hand-written and commented, the reason a key was chosen sits above the block
/// that uses it, and a value tree keeps neither comments nor layout. The key
/// does not change, so the comment goes on describing the binding under it.
pub fn rewrite_commands(original: &str) -> String {
    let mut out = String::with_capacity(original.len());
    let mut in_block = false;
    for line in original.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed == "[[keys.command]]" {
            in_block = true;
        } else if trimmed.starts_with('[') {
            in_block = false;
        }
        match in_block
            .then(|| BINDINGS.iter().find_map(|binding| replaced(line, binding)))
            .flatten()
        {
            Some(rewritten) => out.push_str(&rewritten),
            None => out.push_str(line),
        }
    }
    out
}

/// `line` with the quoted value of a `command` assignment naming `binding`'s
/// predecessor replaced, or `None` when this line is not that assignment.
fn replaced(line: &str, binding: &Binding) -> Option<String> {
    let (key, rest) = line.split_once('=')?;
    if key.trim() != "command" {
        return None;
    }
    let old = format!("\"{LEGACY_PLUGIN_ID}.{}\"", binding.action);
    let at = rest.find(&old)?;
    // Only what precedes the value may be blank: `command = x "old"` is not an
    // assignment of that value.
    if !rest[..at].trim().is_empty() {
        return None;
    }
    let new = format!("\"{PLUGIN_ID}.{}\"", binding.action);
    Some(format!("{key}={}{new}{}", &rest[..at], &rest[at + old.len()..]))
}
```

- [ ] **Step 8: run them and watch them pass**

```bash
cargo test --lib setup::tests:: 2>&1 | tail -15
```

Expected: PASS, the whole `setup` module.

- [ ] **Step 9: all four gates, then commit**

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check \
  && python3 scripts/check_manifest.py
git add -A
git commit -m "$(cat <<'EOF'
Recognise a binding left by the id this plugin used to have

A [[keys.command]] block naming haurylau.voice.ptt was, to decide(), a
stranger holding ctrl+g: the person was told to pick another key for a
binding they had added on this plugin's own instruction.

Such a block is now its own list, superseded, and rewrite_commands rewrites
the quoted command value on its command line and nothing else. The block
does not move and the key does not change, so a comment above it goes on
describing the binding it explains.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: what `setup` reports and asks

**Files:**
- Modify: `src/setup.rs` — `run` (line 503), its early return (576-590), its
  question (592-598), `main` (642)
- Modify: `tests/setup_process.rs` — the call into `run`
- Test: `src/setup.rs`, in its `mod tests`

**Interfaces:**
- Consumes: `Decision::superseded`, `rewrite_commands` (task 4), `commit`
  (task 3), `legacy_sibling` (task 2).
- Produces:
  - `pub struct Legacy { pub config_file: Option<PathBuf>, pub current_config_dir: Option<PathBuf>, pub daemon_socket: Option<String> }`,
    deriving `Debug, Default, Clone`.
  - `run` takes it: `pub fn run(herdr: &dyn Herdr, path: Option<PathBuf>, legacy: &Legacy, interactive: bool, answer: &mut dyn FnMut() -> Option<String>, out: &mut dyn std::io::Write) -> u8`.
  - `pub fn legacy_daemon_socket(state_dir: Option<PathBuf>) -> Option<String>`,
    `#[cfg(unix)]`.

- [ ] **Step 1: write the failing test for the superseded report and the rewrite**

```rust
#[test]
fn y_rewrites_the_predecessor_s_blocks_and_says_which_keys_they_were_on() {
    let path = scratch("rewrite-yes");
    std::fs::write(
        &path,
        "# why ctrl+g: alt+g typed ©\n\
         [[keys.command]]\n\
         key = \"ctrl+g\"\n\
         type = \"plugin_action\"\n\
         command = \"haurylau.voice.ptt\"\n",
    )
    .unwrap();
    let mut said = String::new();
    let mut answer = || Some("y\n".to_string());
    let code = run(
        &FakeHerdr::clean(),
        Some(path.clone()),
        &Legacy::default(),
        true,
        &mut answer,
        &mut said,
    );
    assert_eq!(code, 0, "{said}");
    assert!(said.contains("ctrl+g"), "the key it sits on: {said}");
    assert!(
        said.contains(&path.display().to_string()),
        "the file it sits in: {said}"
    );
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("command = \"herdr-voice.ptt\""), "{after}");
    assert!(after.contains("# why ctrl+g: alt+g typed ©\n[[keys.command]]"), "{after}");
    assert!(!after.contains("haurylau.voice"), "{after}");
}

#[test]
fn anything_but_y_leaves_the_predecessor_s_blocks_where_they_are() {
    let path = scratch("rewrite-no");
    let original = "[[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"haurylau.voice.ptt\"\n";
    std::fs::write(&path, original).unwrap();
    let mut said = String::new();
    let mut answer = || Some("n\n".to_string());
    run(
        &FakeHerdr::clean(),
        Some(path.clone()),
        &Legacy::default(),
        true,
        &mut answer,
        &mut said,
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}
```

Every existing call of `run` in `mod tests` and in `tests/setup_process.rs` gains
`&Legacy::default()` in the same position.

- [ ] **Step 2: run them and watch them fail**

```bash
cargo test --lib setup::tests::y_rewrites 2>&1 | tail -20
```

Expected: FAIL, `cannot find struct \`Legacy\``.

- [ ] **Step 3: add `Legacy` and take it in `run`**

```rust
/// What an install under the id this plugin had before issue #73 left behind.
/// Gathered by `main` and handed in, so `run` has no branch that depends on the
/// environment and the tests need not mutate one they share — the reason
/// `config_path_from` takes its three values rather than reading them.
#[derive(Debug, Default, Clone)]
pub struct Legacy {
    /// The configuration file under the old id, when one is there.
    pub config_file: Option<PathBuf>,
    /// The directory the plugin reads now, which the same sentence names.
    pub current_config_dir: Option<PathBuf>,
    /// The old socket, when a daemon still answers on it. Unix only: on Windows
    /// the address is a name in the pipe namespace with no path.
    pub daemon_socket: Option<String>,
}
```

- [ ] **Step 4: report the superseded blocks and fold them into the question**

In `run`, after the `already` loop and before the `blocked` loop:

```rust
    for (binding, key) in &decision.superseded {
        let _ = writeln!(
            out,
            "superseded: {key} in {} carries this plugin's previous id, {}.{}. \
             The id is now {PLUGIN_ID}, so that key does nothing when it is pressed.",
            path.display(),
            LEGACY_PLUGIN_ID,
            binding.action
        );
    }
```

The early return at what is now the `to_add.is_empty()` test becomes
`if decision.to_add.is_empty() && decision.superseded.is_empty() {`, so a file
with nothing to add but something to rewrite reaches the question.

The question and the write become, in place of the current `append` call:

```rust
    let snippet = render(&decision.to_add);
    if !decision.to_add.is_empty() {
        let _ = writeln!(out, "\nthese go into {}:\n\n{snippet}", path.display());
    }
    let _ = write!(out, "\n{} in the file named above? [y/N] then Enter: ", ask(&decision));
    let _ = out.flush();

    let said = answer().unwrap_or_default();
    if !matches!(said.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        let _ = writeln!(out, "\nnothing was changed.");
        return 0;
    }

    let rewriting = !decision.superseded.is_empty();
    let outcome = commit(herdr, &path, |original| {
        let mut text = if rewriting {
            rewrite_commands(original)
        } else {
            original.to_string()
        };
        if !snippet.is_empty() {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push('\n');
            text.push_str(&snippet);
        }
        text
    });
```

with the existing `match` on the result kept, its success arm listing the
rewritten keys as well as the added ones. `ask` is:

```rust
/// "rewrite 2 bindings", "append 1 binding", or both — the question sits under a
/// wall of TOML and has to say what answering it does.
fn ask(decision: &Decision) -> String {
    let mut parts = Vec::new();
    if !decision.superseded.is_empty() {
        parts.push(format!(
            "rewrite {} to name {PLUGIN_ID}",
            plural_bindings(decision.superseded.len())
        ));
    }
    if !decision.to_add.is_empty() {
        parts.push(format!("append {}", plural_bindings(decision.to_add.len())));
    }
    parts.join(" and ")
}
```

- [ ] **Step 5: run them and watch them pass**

```bash
cargo test --lib setup::tests:: 2>&1 | tail -15
```

Expected: PASS.

- [ ] **Step 6: write the failing tests for the two leftovers**

```rust
#[test]
fn the_report_names_the_old_configuration_file_and_the_one_read_now() {
    let path = scratch("legacy-config");
    std::fs::write(&path, "[theme]\n").unwrap();
    let legacy = Legacy {
        config_file: Some(PathBuf::from("/tmp/c/haurylau.voice/config.toml")),
        current_config_dir: Some(PathBuf::from("/tmp/c/herdr-voice")),
        daemon_socket: None,
    };
    let mut said = String::new();
    let mut answer = || Some("n\n".to_string());
    run(&FakeHerdr::clean(), Some(path), &legacy, true, &mut answer, &mut said);
    assert!(said.contains("/tmp/c/haurylau.voice/config.toml"), "{said}");
    assert!(said.contains("/tmp/c/herdr-voice"), "{said}");
    assert!(said.contains("mv "), "it gives the one command that moves it: {said}");
}

#[test]
fn the_report_names_a_daemon_that_still_answers_on_the_old_socket() {
    let path = scratch("legacy-daemon");
    std::fs::write(&path, "[theme]\n").unwrap();
    let legacy = Legacy {
        daemon_socket: Some("/tmp/s/haurylau.voice/voice.sock".to_string()),
        ..Legacy::default()
    };
    let mut said = String::new();
    let mut answer = || Some("n\n".to_string());
    run(&FakeHerdr::clean(), Some(path), &legacy, true, &mut answer, &mut said);
    assert!(said.contains("/tmp/s/haurylau.voice/voice.sock"), "{said}");
    assert!(said.contains("kill"), "and the one way to end it: {said}");
}

#[test]
fn nothing_is_said_about_leftovers_that_are_not_there() {
    let path = scratch("legacy-none");
    std::fs::write(&path, "[theme]\n").unwrap();
    let mut said = String::new();
    let mut answer = || Some("n\n".to_string());
    run(
        &FakeHerdr::clean(),
        Some(path),
        &Legacy::default(),
        true,
        &mut answer,
        &mut said,
    );
    assert!(!said.contains("mv "), "{said}");
    assert!(!said.contains("kill"), "{said}");
}
```

- [ ] **Step 7: run them and watch them fail, then report the leftovers**

```bash
cargo test --lib setup::tests::the_report_names 2>&1 | tail -20
```

Expected: FAIL on the assertions, not on compilation.

Print them at the end of `run`'s interactive half, on every path that reaches the
report — before each `return`, or in a small function called from each:

```rust
/// What the rename left behind besides the keybindings. Printed whether or not
/// anything was rewritten: a person who repairs the keys and stops there runs on
/// defaults, with a daemon from the old build still holding the old socket.
fn report_legacy(legacy: &Legacy, out: &mut dyn std::io::Write) {
    if let (Some(file), Some(now)) = (&legacy.config_file, &legacy.current_config_dir) {
        let _ = writeln!(
            out,
            "\nyour configuration from before the rename is still at {}, and this \
             plugin now reads {}. Move it with:\n  mv {} {}/",
            file.display(),
            now.display(),
            file.display(),
            now.display()
        );
    }
    if let Some(socket) = &legacy.daemon_socket {
        let _ = writeln!(
            out,
            "\na daemon from before the rename is still running and holding {socket}. \
             No herdr restart ends it — it is not herdr's child. End it with:\n  \
             kill $(lsof -t {socket})"
        );
    }
}
```

- [ ] **Step 8: run them and watch them pass**

```bash
cargo test --lib setup::tests:: 2>&1 | tail -15
```

Expected: PASS.

- [ ] **Step 9: build `Legacy` in `main`**

```rust
/// A daemon from before the rename still answers on the old socket, or nothing
/// does. Nothing is sent on the connection; a connection that opens is the whole
/// answer. Unix only: on Windows the address is a name in the pipe namespace,
/// with no path for this to look at and no `lsof` to name a process with.
#[cfg(unix)]
pub fn legacy_daemon_socket(state_dir: Option<PathBuf>) -> Option<String> {
    let socket = crate::transport::legacy_sibling(&state_dir?)?.join("voice.sock");
    let address = crate::transport::Address::path(socket.to_string_lossy().into_owned());
    crate::transport::connect(&address).ok()?;
    Some(address.display().to_string())
}
```

and in `main`:

```rust
    let config_vars = crate::config::Vars::from_env();
    let current_config_dir = crate::config::directory(&config_vars);
    let config_file = current_config_dir
        .as_deref()
        .and_then(crate::transport::legacy_sibling)
        .map(|dir| dir.join(crate::config::FILE_NAME))
        .filter(|file| file.is_file());
    #[cfg(unix)]
    let daemon_socket = legacy_daemon_socket(crate::transport::state_directory(
        &crate::transport::Vars::from_env(),
    ));
    #[cfg(not(unix))]
    let daemon_socket = None;
    let legacy = Legacy {
        config_file,
        current_config_dir,
        daemon_socket,
    };
```

passing `&legacy` to `run`.

- [ ] **Step 10: a test that the probe answers for a live socket and not a dead one**

```rust
#[cfg(unix)]
#[test]
fn the_probe_answers_only_while_something_is_listening() {
    let dir = scratch("probe").parent().unwrap().join("herdr-voice");
    std::fs::create_dir_all(&dir).unwrap();
    let legacy = crate::transport::legacy_sibling(&dir).unwrap();
    std::fs::create_dir_all(&legacy).unwrap();
    assert_eq!(legacy_daemon_socket(Some(dir.clone())), None);

    let address = crate::transport::Address::path(
        legacy.join("voice.sock").to_string_lossy().into_owned(),
    );
    let listener = crate::transport::listen(&address).unwrap();
    let answered = legacy_daemon_socket(Some(dir));
    drop(listener);
    assert_eq!(answered.as_deref(), Some(address.display()));
}
```

- [ ] **Step 11: run it, then all four gates, then commit**

```bash
cargo test 2>&1 | tail -15
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check \
  && python3 scripts/check_manifest.py
git add -A
git commit -m "$(cat <<'EOF'
Tell a person what the rename left on their machine

setup now reports each key still bound to the previous id, and offers to
rewrite those blocks and append the missing ones in one question and one
write.

It also names the two leftovers the keybindings do not cover: a
configuration file under the old id, which the plugin no longer reads, and
a daemon from before the rename, which holds the old socket and which no
herdr restart ends because it is not herdr's child.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: the notice at daemon start

**Files:**
- Modify: `src/setup.rs` — a new `superseded_keys` beside `decide`
- Modify: `src/daemon.rs` — a new `rename_notice_line` and `announce_rename`, and
  one call between the `Runtime` at line 1180 and `serve` at line 1204
- Test: `src/setup.rs` and `src/daemon.rs`, in their `mod tests`

**Interfaces:**
- Consumes: `LEGACY_PLUGIN_ID` (task 1). Independent of task 4's `superseded`.
- Produces:
  - `pub fn superseded_keys(text: &str) -> Vec<String>` in `src/setup.rs`;
  - `pub fn rename_notice(keys: &[String]) -> Option<(String, String)>` in
    `src/daemon.rs`, the title and the body;
  - `pub fn rename_notice_line(keys: &[String]) -> String`.

- [ ] **Step 1: write the failing test for `superseded_keys`**

```rust
#[test]
fn the_superseded_keys_are_the_ones_found_in_the_order_the_bindings_declare() {
    let text = "[[keys.command]]\nkey = \"ctrl+shift+g\"\ncommand = \"haurylau.voice.cancel\"\n\n\
                [[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"haurylau.voice.ptt\"\n\n\
                [[keys.command]]\nkey = \"ctrl+t\"\ncommand = \"somebody.else.thing\"\n";
    assert_eq!(superseded_keys(text), vec!["ctrl+g", "ctrl+shift+g"]);
}

#[test]
fn a_configuration_with_no_predecessor_block_has_no_superseded_keys() {
    let text = "[[keys.command]]\nkey = \"ctrl+g\"\ncommand = \"herdr-voice.ptt\"\n";
    assert!(superseded_keys(text).is_empty());
}

/// A file the daemon cannot parse is not evidence that a key is dead, and the
/// daemon has no terminal to report it on.
#[test]
fn a_configuration_that_does_not_parse_has_no_superseded_keys() {
    assert!(superseded_keys("[keys\nbroken =").is_empty());
}
```

- [ ] **Step 2: run them and watch them fail**

```bash
cargo test --lib setup::tests::the_superseded_keys 2>&1 | tail -20
```

Expected: FAIL, `cannot find function \`superseded_keys\``.

- [ ] **Step 3: write it**

```rust
/// The keys of every `[[keys.command]]` block naming this plugin's previous id,
/// in the order `BINDINGS` declares the actions — so the sentence built from
/// them does not depend on the order somebody's file happens to be in.
///
/// A file that does not parse has none: the daemon reads this at start and has
/// no terminal to report a broken configuration on. `setup` has one, and does.
pub fn superseded_keys(text: &str) -> Vec<String> {
    let Ok(existing) = inspect(text) else {
        return Vec::new();
    };
    BINDINGS
        .iter()
        .filter_map(|binding| {
            let command = format!("{LEGACY_PLUGIN_ID}.{}", binding.action);
            existing
                .commands
                .iter()
                .find(|(_, c)| *c == command)
                .map(|(key, _)| key.clone())
        })
        .collect()
}
```

- [ ] **Step 4: run them and watch them pass**

```bash
cargo test --lib setup::tests::the_superseded_keys 2>&1 | tail -10
cargo test --lib setup::tests::a_configuration 2>&1 | tail -10
```

Expected: PASS, three tests across the two runs.

- [ ] **Step 5: write the failing tests for the notice's text**

In `src/daemon.rs`'s `mod tests`:

```rust
#[test]
fn the_rename_notice_names_the_count_and_the_keys() {
    let keys = ["ctrl+g".to_string(), "prefix+i".to_string(), "ctrl+shift+g".to_string()];
    let (title, body) = rename_notice(&keys).expect("a notice");
    assert_eq!(title, "Dictation: the plugin id changed");
    assert_eq!(
        body,
        "3 keys still name haurylau.voice, which no longer exists: ctrl+g, \
         prefix+i and ctrl+shift+g. Run the setup action to repair them."
    );
}

#[test]
fn one_key_gives_the_notice_in_the_singular_and_names_only_that_key() {
    let (_, body) = rename_notice(&["prefix+i".to_string()]).expect("a notice");
    assert_eq!(
        body,
        "1 key still names haurylau.voice, which no longer exists: prefix+i. \
         Run the setup action to repair it."
    );
}

#[test]
fn no_superseded_key_is_no_notice() {
    assert!(rename_notice(&[]).is_none());
}

#[test]
fn the_journal_line_carries_the_same_count_and_keys() {
    let keys = ["ctrl+g".to_string(), "prefix+i".to_string()];
    assert_eq!(
        rename_notice_line(&keys),
        "rename: 2 keys still name haurylau.voice: ctrl+g and prefix+i"
    );
}
```

Check the exact strings against `tasks/73/DESIGN_73.md` section 5a before running:
the design pins both forms in full, and these assertions are that pin.

- [ ] **Step 6: run them and watch them fail**

```bash
cargo test --lib daemon::tests::the_rename_notice 2>&1 | tail -20
```

Expected: FAIL, `cannot find function \`rename_notice\``.

- [ ] **Step 7: write the two functions**

```rust
/// `ctrl+g, prefix+i and ctrl+shift+g` — an Oxford-less list, because it is read
/// in a toast rather than parsed.
fn key_list(keys: &[String]) -> String {
    match keys.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// The title and body of the one notice a daemon raises when the rename left
/// keys behind, or `None` when it left none. See `tasks/73/DESIGN_73.md`,
/// section 5a, which pins both forms.
pub fn rename_notice(keys: &[String]) -> Option<(String, String)> {
    if keys.is_empty() {
        return None;
    }
    let (noun, verb, object) = if keys.len() == 1 {
        ("key", "names", "it")
    } else {
        ("keys", "name", "them")
    };
    Some((
        "Dictation: the plugin id changed".to_string(),
        format!(
            "{} {noun} still {verb} {}, which no longer exists: {}. Run the setup \
             action to repair {object}.",
            keys.len(),
            crate::setup::LEGACY_PLUGIN_ID,
            key_list(keys)
        ),
    ))
}

/// Written whether the toast is raised, refused or switched off: `[ui] toasts`
/// decides whether the person is interrupted, never whether something is
/// recorded.
pub fn rename_notice_line(keys: &[String]) -> String {
    format!(
        "rename: {} keys still name {}: {}",
        keys.len(),
        crate::setup::LEGACY_PLUGIN_ID,
        key_list(keys)
    )
}
```

- [ ] **Step 8: run them and watch them pass**

```bash
cargo test --lib daemon::tests::the_rename 2>&1 | tail -10
cargo test --lib daemon::tests::one_key_gives 2>&1 | tail -10
cargo test --lib daemon::tests::no_superseded 2>&1 | tail -10
```

Expected: PASS, four tests.

- [ ] **Step 9: write the failing tests for the raising path**

`announce_rename` is not on a thread and needs no clock, so these tests build a
runtime and call it directly. The helpers are the ones the module already has:
`runtime_with` (`src/daemon.rs:1563`), `RecordingJournal` and `TestJournal`
(`src/daemon.rs:1310-1320`), and `FakeDeliverer` from
`crate::delivery::tests_support`. `a_tap_with_toasts_off_still_writes_the_journal_line`
(`src/daemon.rs:3539`) is the same shape and shows how a journal handle is kept.

```rust
/// A runtime whose journal the test can read back, with `[ui] toasts` as given.
fn runtime_reading_back(
    fake: crate::delivery::tests_support::FakeDeliverer,
    toasts: bool,
) -> (Runtime, std::sync::Arc<RecordingJournal>) {
    let mut runtime = runtime_with(fake, false);
    runtime.delivery_settings.toasts = toasts;
    let journal = std::sync::Arc::new(RecordingJournal::default());
    runtime.journal = Box::new(TestJournal(std::sync::Arc::clone(&journal)));
    (runtime, journal)
}

fn lines(journal: &std::sync::Arc<RecordingJournal>) -> Vec<String> {
    journal.0.lock().unwrap().clone()
}

#[test]
fn the_rename_notice_is_journalled_and_raised_once() {
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let (runtime, journal) = runtime_reading_back(fake.clone(), true);
    announce_rename(&runtime, &["ctrl+g".to_string()]);
    assert_eq!(
        lines(&journal),
        vec!["rename: 1 keys still name haurylau.voice: ctrl+g"]
    );
    assert_eq!(
        fake.calls(),
        vec![crate::delivery::tests_support::Call::Notify(
            "Dictation: the plugin id changed".into(),
            "1 key still names haurylau.voice, which no longer exists: ctrl+g. \
             Run the setup action to repair it."
                .into()
        )],
        "once per daemon start, and exactly the body the design pins"
    );
}

#[test]
fn no_superseded_key_raises_nothing_and_records_nothing() {
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let (runtime, journal) = runtime_reading_back(fake.clone(), true);
    announce_rename(&runtime, &[]);
    assert!(lines(&journal).is_empty(), "{:?}", lines(&journal));
    assert!(fake.calls().is_empty(), "{:?}", fake.calls());
}

/// The rule the file already states at `src/daemon.rs:688-690`: `[ui] toasts`
/// decides whether the person is interrupted, never whether something is
/// recorded.
#[test]
fn with_toasts_off_the_rename_notice_is_still_journalled() {
    let fake = crate::delivery::tests_support::FakeDeliverer::ok();
    let (runtime, journal) = runtime_reading_back(fake.clone(), false);
    announce_rename(&runtime, &["ctrl+g".to_string()]);
    assert_eq!(
        lines(&journal),
        vec!["rename: 1 keys still name haurylau.voice: ctrl+g"]
    );
    assert!(fake.calls().is_empty(), "{:?}", fake.calls());
}

/// A notice herdr refuses is recorded rather than lost, and the start goes on.
#[test]
fn a_refused_rename_notice_is_recorded_and_the_start_continues() {
    let fake = crate::delivery::tests_support::FakeDeliverer::ok().and_notify_fails(
        crate::delivery::DeliveryError::Rejected("ui_busy".to_string()),
    );
    let (runtime, journal) = runtime_reading_back(fake, true);
    announce_rename(&runtime, &["ctrl+g".to_string()]);
    let written = lines(&journal);
    assert_eq!(written.len(), 2, "{written:?}");
    assert_eq!(written[0], "rename: 1 keys still name haurylau.voice: ctrl+g");
    assert!(written[1].starts_with("toast failed:"), "{written:?}");
}
```

Check `RecordingJournal`'s field and `FakeDeliverer::and_notify_fails`'s signature
before running: if either differs from what is written here, follow the code
rather than this plan, and keep the assertions.

- [ ] **Step 10: run them, watch them fail, then write `announce_rename`**

```rust
/// One notice per daemon start, when the rename left keys behind. The journal
/// line goes first and unconditionally; `toast` decides only whether the person
/// is interrupted, and records its own failure when herdr refuses.
fn announce_rename(runtime: &Runtime, keys: &[String]) {
    let Some((title, body)) = rename_notice(keys) else {
        return;
    };
    runtime.journal.write(&rename_notice_line(keys));
    toast(runtime, &title, &body);
}
```

- [ ] **Step 11: call it once, at start**

In `start`, between the `Runtime` at line 1180 and `serve` at line 1204:

```rust
    // What the rename of issue #73 left in somebody's herdr configuration. Read
    // here, once, for the same reason the plugin's own configuration is read
    // here: nothing that runs while a person is speaking touches a file.
    let superseded = crate::setup::config_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| crate::setup::superseded_keys(&text))
        .unwrap_or_default();
    announce_rename(&runtime, &superseded);
```

- [ ] **Step 12: run the whole suite**

```bash
cargo test 2>&1 | tail -15
```

Expected: PASS. If a test elsewhere now sees an extra journal line, it is because
the machine running the tests has legacy bindings in its own herdr configuration
— which means `start` is reading the real environment from a test. It is not:
nothing in the suite calls `start`. Check that before changing any other test.

- [ ] **Step 13: all four gates, then commit**

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check \
  && python3 scripts/check_manifest.py
git add -A
git commit -m "$(cat <<'EOF'
Say at start that the rename left keys behind

setup explains everything the rename left on a machine, once somebody runs
it. Nothing told them to. A person who relinks and restarts herdr presses
ctrl+g, nothing happens, and nothing anywhere says why — the silent failure
this repository weighs the same as a wrong transcript.

The daemon now reads the herdr configuration once at start, and when a
binding still names the previous id it raises one notice saying how many
keys there are, which ones, and that setup repairs them. The journal line
is written whether the toast is raised, refused or switched off.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: the documents

**Files:**
- Modify: `docs/decisions.md` — one row
- Modify: `README.md` — the sentence about what `setup` does, if it promises only
  an append

**Interfaces:**
- Consumes: task 1's constants.
- Produces: nothing code depends on.

- [ ] **Step 1: add the decision row**

`docs/decisions.md` is a table of what was decided without the repository's owner,
one line each. Add, in date order:

```
| `setup` rewrites a `[[keys.command]]` block naming the plugin's previous id, in place and on an explicit answer, rather than only appending | After the rename an append cannot put the three bindings on their keys: `decide` reports each key as held by a stranger, and two blocks on one key make herdr answer `config: issues found`. The alternative leaves the person editing TOML by hand to recover keys the rename took from them | 2026-09-18, #73 |
```

- [ ] **Step 2: check what `README.md` promises**

```bash
grep -n "setup" README.md
```

If it says `setup` prints the snippet and offers to append it, add that it also
repairs bindings left by the previous id. If it already says something wider,
leave it.

- [ ] **Step 3: all four gates, then commit**

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check \
  && python3 scripts/check_manifest.py
git add -A
git commit -m "$(cat <<'EOF'
Record the decision to rewrite a predecessor's bindings

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: the diff is reviewed, then S5 verifies against a live herdr

**Files:**
- Modify: `docs/evidence.md` — a new section
- Modify: `tasks/73/RUN_73.md` — the S4 and S5 records

**Interfaces:**
- Consumes: tasks 1 to 7.
- Produces: the evidence S5 rests on.

- [ ] **Step 1: the whole diff is reviewed before a pull request exists**

Per `CLAUDE.md`: S4 closes on a review of the diff, through
`superpowers:requesting-code-review`, not on a pull request. Record the verdict in
`tasks/73/RUN_73.md`.

- [ ] **Step 2: check for the two kinds of leftover this repository has committed before**

```bash
git diff main...HEAD | grep -nE "^\+.*(</?new_string>|</?old_string>)" || echo clean
git diff main...HEAD | grep -nE "^\+(<<<<<<<|=======|>>>>>>>)" || echo clean
```

Expected: `clean` twice.

- [ ] **Step 3: verify against a live herdr, on a checkout that is not the owner's**

Link this worktree under the new id only if the owner's plugin is unlinked first,
or — better — verify with a copy of the checkout, and unlink it afterwards. Record
in `docs/evidence.md`, with the platform:

- `herdr plugin link .` registers `herdr-voice`; `herdr plugin list` names it and
  `herdr plugin config-dir herdr-voice` prints a path ending in `herdr-voice`
  (AC-2);
- a daemon started under the new id strips a tab decoration left under the old
  one, and a token written under the old source is gone within 1.8 seconds
  (AC-10);
- what `setup` printed, verbatim, run against a copy of a configuration that
  carries the three old blocks — never against the owner's own file (AC-6, AC-7).

- [ ] **Step 4: record the S5 verdict in `tasks/73/RUN_73.md` and open the pull request**

A negative result, recorded, passes S5. A claim with no command and no output
beside it does not.
