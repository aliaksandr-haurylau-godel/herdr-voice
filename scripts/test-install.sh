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
# The durable witness. scripts/install-check.sh reads this rather than the
# install log, because herdr reports a build command's output when the build
# fails and nothing establishes that it echoes a successful one.
case "$(cat "${checkout}/target/release/.herdr-voice-install" 2>/dev/null)" in
    "fetched herdr-voice-v0.4.2-x86_64-unknown-linux-gnu.tar.gz from v0.4.2, verified sha256 ${good_digest}")
        printf 'ok    it recorded that it fetched and verified\n' ;;
    *)  printf 'FAIL  the install marker is wrong: [%s]\n' \
            "$(cat "${checkout}/target/release/.herdr-voice-install" 2>/dev/null)"
        failures=$((failures + 1)) ;;
esac

# An uppercase digest in the sidecar is the same digest. Nothing publishes one
# today, but a sidecar regenerated by hand would otherwise be reported as a
# mismatch, and that message blames the download for a difference of spelling.
rm -rf "${checkout}/target"
upper_digest="$(printf '%s' "${good_digest}" | tr 'a-f' 'A-F')"
HV_TEST_ARCHIVE=ok HV_TEST_SIDECAR=ok HV_TEST_DIGEST="${upper_digest}"
check "an uppercase digest still matches" "$(run_main)" 0

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
rm -rf "${checkout}/target"
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
check "a fallback records that it built from source" \
    "$(cat "${checkout}/target/release/.herdr-voice-install" 2>/dev/null)" \
    "built from source"

# A source build that fails says so, rather than ending on the compiler's own
# last line with nothing about the state the machine is in.
printf '#!/bin/sh\necho "fake cargo: no" >&2\nexit 1\n' >"${fakebin}/cargo"
chmod +x "${fakebin}/cargo"
check "a failed source build stops and says so" "$(run_main)" 1
case "$(said)" in
    *"nothing was installed"*) printf 'ok    a failed source build says nothing was installed\n' ;;
    *) printf 'FAIL  a failed source build did not say so: %s\n' "$(said)"
       failures=$((failures + 1)) ;;
esac
printf '#!/bin/sh\necho "fake cargo $*"\n' >"${fakebin}/cargo"
chmod +x "${fakebin}/cargo"

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

if [ "${failures}" -ne 0 ]; then
    printf '\n%s assertion(s) failed\n' "${failures}" >&2
    exit 1
fi
printf '\nall install assertions passed\n'
