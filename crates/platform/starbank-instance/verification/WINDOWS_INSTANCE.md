# Windows running-instance qualification

WP-16 and REQ-DIST-01. The component provides a headless Windows-only transport for launch paths. The desktop does not activate the transport. Qualification of a signed installed Explorer open remains a separate step.

## Boundary

- A validated ASCII application namespace and the current token's logon SID name one local pipe. The server claims the first instance, limits the pipe to one server handle, and rejects remote clients. A protected DACL grants that logon SID only the explicit client rights. The Owner Rights ACE suppresses implicit owner access to change the DACL. The client opens with individual rights and identification-level security QOS. `FILE_CREATE_PIPE_INSTANCE` shares the `FILE_APPEND_DATA` bit, so both the DACL's client grant and the requested client handle omit it. [Named pipe access rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights) · [Windows security identifiers](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/understand-security-identifiers)
- The connected endpoints obtain peer PIDs from the kernel and compare the live peers' primary-token logon SIDs. The client verifies the peer before transmitting paths. The server verifies the peer before decoding application frames. The peer process handle remains owned through the exchange. This authenticates logon membership. Code running in the same logon is within the endpoint's trust boundary. [Query the server PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid) · [Query the client PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid) · [Token information classes](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-token_information_class)
- A connection carries one versioned request of at most 32 paths and 65,536 bytes including the header. A fixed header is read before sizing the body. The codec preserves original UTF-16 code units and rejects malformed lengths and NULs. The transport accepts lexical local absolute `.SC2Bank` paths. The caller resolves relative arguments before claiming the endpoint, and its admission callback owns any later file-open authority. An empty request carries activation intent alone.
- A single absolute connection deadline covers reading, admission and acknowledgement. The transport checks the deadline before polling pipe I/O and after each poll. The receiver checks it before calling admission. The callback uses nonblocking queue access, such as `try_lock`, and checks the same deadline under the lock before changing the queue. Synchronous callback work cannot be preempted. Each admitted request is accounted for by the acknowledgement. A callback error ends the connection before acknowledgement. The receiving side normally waits for the client to consume the acknowledgement and close, within the same deadline. A missing or invalid acknowledgement after transmission is reported as delivery uncertain. [Disconnect semantics](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-disconnectnamedpipe#remarks)

## Unsafe review

The exception is restricted to [`src/windows/security.rs`](../src/windows/security.rs). The transport and codec forbid unsafe code. This file contains synchronous Windows process, token, security descriptor and pipe queries, plus native assertions in its qualification helpers. `OpenProcess` and `OpenProcessToken` transfer each successful native handle to a sole Rust owner. The aligned, initialized `TOKEN_GROUPS` buffer is fixed in size. The SID offset and fixed read range are checked within the returned bytes before reading from that allocation. The returned SID must match the Windows logon SID layout. The SDDL descriptor has one owner that calls `LocalFree`, and lives through pipe creation. The overlapped client handle is checked for pipe type and transferred exactly once to Tokio.

The Windows transport uses locked Tokio 1.54.1 (MIT), Mio 1.2.5 (MIT), and `windows-sys` 0.61.2 (MIT or Apache-2.0). Tokio and Mio own all pending I/O and its buffers. The application supplies no Win32 callbacks, pending operation structures or pointers to transient frame buffers. Mio bounds its pipe read buffer and retains ownership of in-flight operations through completion. Disconnect cancels and drops per-client buffered state before reuse. Dropping the listener releases the application owner while native completion resources may remain until the pending operation finishes. [Pinned Tokio pipe implementation](https://github.com/tokio-rs/tokio/blob/tokio-1.54.1/tokio/src/net/windows/named_pipe.rs) · [Pinned Mio pipe implementation](https://github.com/tokio-rs/mio/blob/v1.2.5/src/sys/windows/named_pipe.rs)

## Native evidence required

Run the crate's ignored headless qualification tests on native Windows. Use only fresh synthetic endpoint names and child processes. Record the Windows build, executable architecture, Rust pin and exact outcomes. Require the connected client handle to resolve the expected server PID before path transmission and expose the granted rights without server-instance creation or DACL mutation. Require the first server to acquire the protected pipe, a second launch to forward only, and the actual DACL and queue acknowledgement to match. Exercise malformed and overdue connections, lost acknowledgement, cancellation and rebinding.

The hosted same-logon parent and child can establish the connected-handle API and the installed rights. Denial from a distinct logon requires a separately provisioned Windows session and remains an independent test. This transport qualification does not establish packaged Explorer activation, foreground focus, or interaction with the active editor.

## Local verification

On macOS 14.7.2 arm64 with Rust 1.100.0, all nine codec tests and warnings-denied Clippy passed. The codec tests cover every nonzero UTF-16 code unit and 66,646 truncated payload prefixes. Windows x64 MSVC cross-Clippy passed for all targets, and cross-target rustdoc passed with warnings denied. These checks compiled the Windows code and its tests; they did not link or execute a Windows binary. The root lockfile adds only this workspace package and retains every existing registry version.

```sh
cargo test --locked -p starbank-instance
cargo clippy --locked -p starbank-instance --all-targets -- -D warnings
cargo clippy --locked -p starbank-instance --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```

The Windows CI gate lists and requires exactly five ignored qualification cases before running them serially with a nonempty-test guard. The private child helper runs only when an owning test launches it.

```powershell
cargo nextest run --locked -p starbank-instance --lib --run-ignored only --no-tests fail --test-threads 1 -E 'test(/^windows::tests::qualify_/)'
```
