# DESIGN_52 — evidence

Every statement in `DESIGN_52.md` about the code as it is now, with where it was
read. Line numbers are at `13733c5`.

| Statement | Where |
|---|---|
| `ureq::Error` is `Status(code, response)` or `Transport(t)`; the rewrite engine maps both to `HttpError::Failed` and drops the response | `src/rewrite/http.rs:163-172` |
| The one sentence "cannot reach … check the server is running and the address is correct" | `src/rewrite/http.rs:127-130`, `src/stt/http.rs:68-72` |
| The bound is a 30-second constant in each engine, set on the agent at construction | `src/rewrite/http.rs:90,105-108`, `src/stt/http.rs:17,36-39` |
| ureq builds a timeout as an `io::Error` of kind `TimedOut` with the text "timed out reading response", and turns `WouldBlock` into it | ureq 2.12.1, `src/stream.rs:85-100`; `Cargo.lock` pins 2.12.1 |
| `Transport` exposes `kind()` and a `source()` chain | ureq 2.12.1, `src/error.rs:157,244-256` |
| The transcriber reports a failure to read the audio file through `Refused` | `src/stt/http.rs:132-137` |
| `Refused` is also used for every transport error; `Failed.detail` is fixed text | `src/stt/http.rs:149-159` |
| `HerdrDeliverer::run` maps every `Err` from `output()` to `NotFound` | `src/delivery.rs:182-189` |
| The `PATH` sentence | `src/delivery.rs:19-24` |
| One daemon test constructs `DeliveryError::NotFound { binary, path }` | `src/daemon.rs:2619` |
| Callers of both `HttpError` enums only print them | `src/rewrite.rs:22,29`, `src/stt.rs:42,78` |
| Errors reach the journal and toasts as single lines; a test feeds a newline through them | `src/daemon.rs:2375` |
| CI stops the test job at ten minutes | `docs/decisions.md`, the row on CI caps |
| CI compiles with `-D warnings`, so an unconstructed variant fails the build | `CLAUDE.md`, "Local development"; run brief, Windows dead-code check |

Not verified at design time: that ureq's chain for a refused connection ends in an
`io::Error` of kind `ConnectionRefused` on this machine. The first test in S4 is
written to show it, and the design falls back to the last row of the message table
for a cause that is not recognised.
