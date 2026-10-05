# DESIGN_19 evidence

Facts the design rests on, each with where it was read. Paths are relative to the
repository root, at `13733c5`.

| Claim in DESIGN_19.md | Where |
|---|---|
| The writer emits every line; the reader reads one | `src/proto.rs:160-161`, `src/proto.rs:167-169` |
| The request frame is a header line plus an exact-length body | `src/proto.rs:103-110`, `src/proto.rs:134-148` |
| The client picks its exit code from the parsed reply and prints the message as is | `src/client.rs:53-65`, `src/main.rs:163-173` |
| The daemon writes the reply once and then the connection is dropped | `src/daemon.rs:1483` and the end of `serve_one` (`src/daemon.rs:1463-1490`) |
| One place flattens newlines already, and a test pins it | `src/daemon.rs:1017-1020`, test at `src/daemon.rs:2410-2420` |
| Texts with newlines that reach the wire unflattened | `src/stt.rs:70-74`, `src/stt/command.rs:151-162` and `:101`, used at `src/daemon.rs:901-904` and `:913-916` |
| Replies carry no protocol token; requests carry `voice/1` | `src/proto.rs:12`, `src/proto.rs:103-107` |
| How a Windows pipe reports a closed peer to a reader differs from Unix | `src/proto.rs:77-87` and the comment at `src/proto.rs:115-119` |

Not verified: the behaviour of the Windows pipe when the server's handle is
dropped after a reply has been written. The design avoids depending on it.
