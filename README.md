# ttying

[中文](README.zh-CN.md) | English

ttying is a small, security-focused web terminal server written in Rust. It runs a real PTY on the host and exposes it through xterm.js in a browser.

```text
Windows x86_64  ->  ttying-windows-x86_64.exe
Linux x86_64    ->  ttying-linux-x86_64
Android arm64   ->  ttying-android-arm64.apk
```

The Android app only starts, stops, and reports the status of the local server. Terminal interaction happens in the system browser. The application id is `cn.z.ttying`.

Current release: **v1.8.0** · [GitHub repository](https://github.com/ying-zhang/ttying) · [Releases](https://github.com/ying-zhang/ttying/releases) · [Website](https://ying-zhang.github.io/ttying/)

## Development disclosure

ttying was developed with extensive AI assistance. The AI assistant contributed architecture and protocol design, Rust and frontend implementation, security review, regression tests, build/release automation, Android UI work, and documentation. Human review, requirements, testing direction, and product decisions were provided by the project owner.

According to the project owner's records, development consumed approximately **290 million tokens**, at an approximate cost of **CNY 10**. This is a usage record, not a claim about the cost of every future build or deployment.

The code should still be independently reviewed before deployment in a security-sensitive environment.

## Features

- Persistent PTY sessions with a five-minute detach grace period.
- Refresh, bfcache restore, and network reconnects reattach to the same shell.
- Bounded 512 KiB raw output replay with sequence numbers and explicit truncation notices.
- Token authentication with a URL-fragment bootstrap and HttpOnly session cookie.
- Read-only by default unless `-W` / `--writable` is passed.
- Origin/Host validation, login rate limiting, WebSocket size limits, and connection timeouts.
- xterm.js search, safe web links, copy/paste shortcuts, persisted font size, and automatic/dark/light themes.
- Mobile special-key toolbar and viewport handling for the software keyboard.
- A large circular start/stop control and natural status UI on Android.
- Chinese/English UI: native Android strings follow the system language; the browser UI follows the browser language.
- INI configuration via `--config`, with command-line arguments taking precedence.
- No built-in TLS: deploy behind a trusted HTTPS reverse proxy or VPN.

## References and design inspiration

ttying is an independent Rust implementation. The following projects and their source code were studied for protocol behavior and user experience:

- [ttyd](https://github.com/tsl0922/ttyd) — original C web terminal; WebSocket framing, resize/flow-control ideas, and the relationship between a WebSocket and a PTY.
- [ttyd-rs](https://github.com/tsl0922/ttyd-rs) — xterm.js integration, terminal preferences, search/export, clipboard, and theme handling.
- [ttyd-rs-wolegca](https://github.com/wolegca/ttyd-rs) — session IDs, reattach behavior, reconnect backoff, heartbeats, visibility probes, status panels, mobile keys, and user preferences.
- [krust](https://github.com/bitshift/krust) — bounded scrollback replay, output-gap handling, focus reporting, clipboard error reporting, and touch/selection behavior.
- [termpair](https://github.com/cs01/termpair) — terminal sharing status, viewer count, read-only presentation, and end-to-end encrypted transport concepts.
- [sshx](https://github.com/ekzhang/sshx) — persisted settings, theme/scrollback configuration, latency reporting, and reconnect behavior.
- [webterm](https://github.com/nasa42/webterm) — persistent sessions, notifications, and session-scoped browser state.

No source code from those projects is copied into this repository. Their licenses and attribution requirements remain their own.

## Security model

- Default bind: `127.0.0.1:7755`.
- Android prefers port `7755`; if it is occupied, the server falls back to an ephemeral port.
- A random 256-bit token is generated when neither `--token` nor `--token-file` is provided.
- `--token-file` is preferred over `--token`: command-line arguments can be visible in process lists and shell history.
- The token is bootstrapped through `#auth=<token>`, exchanged with `POST /api/session`, and replaced by an HttpOnly cookie.
- The token is never placed in a query string. The URL fragment is not sent to the HTTP server or to a reverse proxy.
- WebSocket handshakes require authentication and an Origin matching the request Host, unless an explicit `--allowed-origin` is configured.
- Terminal input is disabled by default. `-W` / `--writable` enables it.
- This is not a sandbox. The spawned shell runs with the permissions of the ttying process.

### HTTPS reverse proxies

When an HTTPS reverse proxy terminates TLS, start ttying with:

```powershell
--secure-cookie
```

This enables the Secure cookie attribute and makes Origin validation expect an `https://` origin. Without it, a browser page loaded over HTTPS sends `Origin: https://...` and the WebSocket/API may be rejected even when Host is correct.

`--public-url https://tty.example.cn` enables Secure cookies automatically for foreground starts and makes the printed URL use the public hostname.

## Quick start

### Windows

```powershell
.\\dist\\ttying-windows-x86_64.exe --token-file .\\token.txt -W -- cmd.exe
```

Open the printed URL. The default port is `7755`.

### Linux

```bash
./ttying-linux-x86_64 --token-file ./token.txt -W -- bash
```

### Android

```text
adb install -r ttying-android-arm64.apk
```

Open the app, press the circular **启动** button, then **在浏览器中打开终端**. The server listens on loopback only. Closing the UI does not stop the foreground service; use the circular **停止** button to stop it.

## INI configuration

Copy `ttying.ini.example` to a private file and start with:

```powershell
ttying.exe --config .\ttying.ini
```

Command-line arguments override INI values. Relative `token_file` paths are resolved relative to the INI file.

```ini
[server]
bind = 127.0.0.1
port = 7755
public_url = https://tty.example.cn

[security]
token_file = token.txt
secure_cookie = true
max_clients = 10
allowed_origins = https://tty.example.cn

[terminal]
writable = true
command = cmd.exe /k
```

Supported sections and keys:

| Section | Keys |
| --- | --- |
| `[server]` | `bind`, `port`, `public_url` |
| `[security]` | `token`, `token_file`, `secure_cookie`, `max_clients`, `allowed_origins` |
| `[terminal]` | `writable`, `command` |

Unknown sections or keys are rejected. Use quotes around command arguments containing spaces. The INI file may contain a token, so restrict its permissions, or use `token_file` and protect that file instead.

### Cloudflare Tunnel / HTTPS reverse proxy

```powershell
.\\dist\\ttying-windows-x86_64.exe `
  --bind 127.0.0.1 --port 7755 `
  --token-file .\\token.txt -W `
  --secure-cookie `
  --public-url https://tty.example.cn `
  -- cmd.exe
```

Example Cloudflare Tunnel ingress:

```yaml
ingress:
  - hostname: tty.example.cn
    service: http://127.0.0.1:7755
  - service: http_status:404
```

Print the public bootstrap URL without starting the server:

```powershell
.\\dist\\ttying-windows-x86_64.exe `
  --token-file .\\token.txt `
  --public-url https://tty.example.cn `
  --print-url
```

Open the resulting URL:

```text
https://tty.example.cn/#auth=<url-encoded-token>
```

Cloudflare sees only the HTTPS request, not the fragment. Do not use `?token=` or a path segment for the token. If only `https://tty.example.cn/` is opened, ttying displays the token login form; the token is then sent in the Authorization header over HTTPS.

## Build

### Local requirements

- Rust with targets `x86_64-pc-windows-gnu`, `x86_64-unknown-linux-musl`, and `aarch64-linux-android`
- PowerShell 7 for `scripts/build-all.ps1`
- Node.js 22 / npm
- Python 3
- JDK 17+
- Android SDK, NDK, build-tools, and `cargo-ndk`
- Zig and `cargo-zigbuild` for the static Linux build
- MinGW-w64 for the Windows GNU build

Copy `scripts/build-config.example.psd1` to `scripts/build.config.psd1` and fill in the local toolchain paths. The local file is ignored by Git. Environment variables can be used instead:

```text
CARGO_HOME, RUSTUP_HOME
ANDROID_HOME or ANDROID_SDK_ROOT, ANDROID_NDK_HOME
TTYING_JDK_BIN, TTYING_PYTHON
TTYING_ZIG_BIN, TTYING_MINGW_BIN
TTYING_NODE, TTYING_NPM
ANDROID_BUILD_TOOLS, ANDROID_PLATFORM_JAR
```

Build all artifacts:

```powershell
.\\scripts\\build-all.ps1
```

Preflight the resolved toolchain without building:

```powershell
.\\scripts\\build-all.ps1 -ValidateOnly
```

The Android APK is release-signed. Configure `AndroidKeystore`, `AndroidKeyAlias`, `AndroidKeyStorePassword`, and `AndroidKeyPassword` in `scripts/build.config.psd1`, or provide:

```text
TTYING_ANDROID_KEYSTORE
TTYING_ANDROID_KEY_ALIAS
TTYING_ANDROID_KEYSTORE_PASSWORD
TTYING_ANDROID_KEY_PASSWORD
```

Back up the release keystore and passwords securely. Losing them prevents future APK updates signed with the same identity.

### Tests

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
node --test web/tests/reconnect.test.cjs
python scripts/security-regression.py
python scripts/smoke-test.py
```

The smoke and security tests are cross-platform. They accept:

```text
TTYING_TEST_EXE
TTYING_TEST_COMMAND
TTYING_TEST_SHELL
TTYING_TEST_TOKEN
TTYING_TEST_PORT
```

See `AUDIT.zh-CN.md` for the Chinese security review, reproduction notes, and remaining limitations.

## Design

```text
Browser (xterm.js)
  |  HTTPS / WebSocket
  v
Axum HTTP + WebSocket server
  |  output broadcast + bounded history
  v
Persistent PTY session (5 minute detach grace)
  |  shell / command
  v
Operating system
```

### Session model

A WebSocket is only a view of a PTY session. A disconnect does not immediately kill the shell. The session is retained for up to five minutes and can be reattached by an unguessable session id stored in `sessionStorage`. If the PTY exits, the client receives `session_ended` and shows a Restart action.

Each session keeps up to 512 KiB of raw output history. New connections receive a sequence-numbered replay. The replay is intentionally not a VT screen snapshot: a full-screen TUI may need to redraw after history truncation.

### WebSocket protocol

```text
Client -> Server
  0x30 + bytes      input
  0x31 + JSON       resize {cols, rows}
  0x32 + JSON       attach {session, cols, rows}
  0x33              heartbeat ping

Server -> Client
  0x30 + bytes      output
  0x31 + JSON       session info
  0x32              heartbeat pong
  0x33 + JSON       error
```

The server enforces a 1 MiB WebSocket message/frame limit, a 64 KiB input limit, a 10-second attach deadline, a 60-second receive idle timeout, and a 10-second send timeout.

### Authentication flow

```text
#auth=<token>  ->  POST /api/session  ->  HttpOnly cookie  ->  /ws
```

The fragment is cleared with `history.replaceState`. The cookie is `SameSite=Strict`, `HttpOnly`, and optionally `Secure`.

### Repository layout

```text
crates/ttying-core   PTY, sessions, HTTP/WebSocket server, auth
crates/ttying-cli    Windows/Linux CLI, token handling, Windows service
crates/ttying-jni    Android JNI bridge
web/                 xterm.js frontend
android/             Android Activity, foreground service, resources
scripts/             build, APK packaging, smoke/security tests
packaging/           systemd unit
.github/workflows/   CI and tagged release builds
```

## Limitations

- No built-in TLS. Use a trusted HTTPS reverse proxy or VPN.
- Raw replay is not full terminal-state restoration.
- The shell is not sandboxed beyond the privileges of the ttying process.
- Deliberately daemonized descendants that create a new process session can escape process-group cleanup; use OS-level containment if required.
- Android lifecycle testing on physical devices is still recommended for each release.
- The repository does not claim complete CVE coverage for third-party dependencies.

## GitHub Actions and releases

- `.github/workflows/ci.yml` runs Rust tests, Clippy, frontend tests, protocol regressions, and Android JNI compilation on pushes and pull requests.
- `.github/workflows/release.yml` runs on tags matching `v*`.
- The release workflow builds:
  - `ttying-windows-x86_64.exe`
  - `ttying-linux-x86_64`
  - `ttying-android-arm64.apk`
  - `SHA256SUMS.txt`
- Create a release with:
  ```powershell
  git tag v1.8.0
  git push origin v1.8.0
  ```
- Required repository secrets for Android signing:
  - `ANDROID_KEYSTORE_BASE64`
  - `ANDROID_KEY_ALIAS`
  - `ANDROID_KEYSTORE_PASSWORD`
  - `ANDROID_KEY_PASSWORD`

## License

BSD 3-Clause. See [LICENSE](LICENSE).
