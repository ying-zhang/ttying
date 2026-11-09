# ttying

[中文](README.md) | English

ttying is a small, security-focused web terminal server written in Rust. It runs a real PTY on the host and exposes it through xterm.js in a browser.

```text
Windows x86_64  ->  ttying-windows-x86_64.exe
Linux x86_64    ->  ttying-linux-x86_64
macOS arm64     ->  ttying-macos-arm64.tar.gz
Android arm64   ->  ttying-android-arm64.apk
```

The Android app only starts, stops, and reports the status of the local server. Terminal interaction happens in the system browser. The application id is `cn.z.ttying`.

Current release: **v1.8.3** · [GitHub repository](https://github.com/ying-zhang/ttying) · [Releases](https://github.com/ying-zhang/ttying/releases) · [Website](https://ying-zhang.cn/ttying/)

## Development disclosure

ttying was developed with extensive AI assistance. The AI assistant contributed architecture and protocol design, Rust and frontend implementation, security review, regression tests, build/release automation, Android UI work, and documentation. Human review, requirements, testing direction, and product decisions were provided by the project owner.

According to the project owner's records, development consumed approximately **290 million tokens**, at an approximate cost of **CNY 10**.

The code should still be independently reviewed before deployment in a security-sensitive environment.

## Features

- PTY sessions survive WebSocket disconnects for five minutes.
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

This section describes the default bind, token authentication, cookie and session boundaries, and the prerequisites a deployment must satisfy.

- Default bind: `127.0.0.1:7755`.
- Android prefers port `7755`; if it is occupied, the server falls back to an ephemeral port.
- A random 256-bit token is generated when neither `--token` nor `--token-file` is provided.
- `--token-file` is preferred over `--token`: command-line arguments are visible in process lists and shell history.
- The token is bootstrapped through `#auth=<token>`, exchanged with `POST /api/session`, and replaced by an HttpOnly session cookie.
- The token is never placed in a query string. The URL fragment is not sent to the HTTP server or to a reverse proxy.
- WebSocket handshakes require authentication and an Origin matching the request Host, unless an explicit `--allowed-origin` is configured.
- Terminal input is disabled by default. `-W` / `--writable` enables it.
- This is not a sandbox. The spawned shell runs with the permissions of the ttying process.
- Token files, signing keys, and passwords must stay out of version control and be readable only by the service account; supply passwords through the documented environment variables or a secret manager.
- Run ttying under the least-privileged account that is appropriate for the use case.

### HTTPS reverse proxies

When an HTTPS reverse proxy terminates TLS, start ttying with:

```powershell
--secure-cookie
```

This enables the Secure cookie attribute and makes Origin validation expect an `https://` origin. Without it, a browser page loaded over HTTPS sends `Origin: https://...` and the WebSocket/API may be rejected even when Host is correct.

`--public-url https://tty.example.cn` enables Secure cookies automatically for foreground starts and makes the printed URL use the public hostname.

### Deployment and authentication boundaries

When `--allowed-origin` is set, it is an exclusive browser Origin allowlist: other origins are not accepted just because they match the Host header. When it is unset, ttying validates the HTTP(S) scheme and Host. Non-browser clients may omit Origin but must still authenticate. A reverse proxy should reject unexpected Host values; public deployments should configure an explicit Origin allowlist. Host comparison alone does not defend against DNS rebinding.

Authentication cookies are valid for 24 hours and are renewed by `POST /api/session`. Logout revokes that cookie and closes its WebSockets; it does not revoke the shared master token or terminate the PTY. Bearer-token access is not affected by cookie logout; revoking the master token requires replacing it and restarting the service. The auth table holds at most 4096 entries; when full, the earliest-expiring cookies are evicted and their clients must re-authenticate, while a valid token can still log in. When the failed-login rate-limit table is full, incorrect logins from new sources are rejected; valid credentials are unaffected.

On Android the fragment bootstrap URL is sent only to the default browser package configured in system settings; if no default browser is configured, the app asks the user to set one first. Browsers and extensions can read the fragment, so they must be trusted. Keeping the fragment out of HTTP requests does not keep it secret from the app that receives the URL. An HTTP login performed inside a native app cannot write into an external browser's cookie store.

Normal startup does not print the credential URL when stdout is redirected; `--print-url` explicitly requests the full link. Protect the token file and its parent directory. On Windows, foreground startup preserves existing explicit ACLs, including read access for the service account, and disables inheritance; administrators must still remove explicit read grants for untrusted accounts. Do not place runtime tokens, signing keys, or passwords under version control. Keep signing material outside the working tree and supply passwords through the documented environment variables or a secret manager. This work does not move existing private signing files or modify CI secrets.

### systemd first-time deployment

The sample unit enables a read-only `/etc`, so the user and token must exist beforehand. After installing the binary to `/usr/local/bin/ttying`, run:

```sh
sudo useradd --system --home-dir /var/lib/ttying --create-home --shell /usr/sbin/nologin ttying
sudo install -d -m 0700 -o ttying -g ttying /etc/ttying
sudo -u ttying sh -c 'umask 077; openssl rand -hex 32 > /etc/ttying/token'
sudo install -m 0644 packaging/ttying.service /etc/systemd/system/ttying.service
sudo systemctl daemon-reload
sudo systemctl enable --now ttying
```

Generate the token only on first deployment; never overwrite the token of a running service. To print the login URL explicitly:

```sh
sudo -u ttying /usr/local/bin/ttying --token-file /etc/ttying/token --print-url
```

An HTTPS proxy deployment needs `--secure-cookie --allowed-origin https://your-domain` added to the unit. The sample unit enables input with `-W`; remove it for read-only deployments. A hidden tab may disconnect after 60 seconds without an inbound heartbeat, with a five-minute reattach window afterwards; when capacity is exhausted, new sessions fail instead of prematurely killing sessions that are still inside the grace period.

### Reporting a vulnerability

Use GitHub's private vulnerability reporting or a private security advisory for this repository. Do not disclose an exploitable issue in a public issue before a fix is available.

Include:

- affected version or commit;
- operating system and architecture;
- whether the server was bound to loopback or a non-loopback address;
- a minimal reproduction;
- expected and observed behavior.

Known design limits and unsupported deployment patterns are listed under [Limitations](#limitations). The short [SECURITY.md](SECURITY.md) and [SECURITY.zh-CN.md](SECURITY.zh-CN.md) files at the repository root exist for hosting-platform detection; this section is authoritative.

## Quick start

### Windows

```powershell
.\dist\ttying-windows-x86_64.exe --token-file .\token.txt -W -- cmd.exe
```

Open the printed URL. The default port is `7755`.

### Linux

```bash
./ttying-linux-x86_64 --token-file ./token.txt -W -- bash
```

### macOS (Apple silicon)

```bash
tar -xzf ttying-macos-arm64.tar.gz
./ttying --token-file ./token.txt -W -- zsh
```

The archive contains the `ttying` binary, `ttying.ini.example`, `LICENSE`, `README.md`, and `README.en-US.md`. The binary is ad-hoc signed, not notarized: if Gatekeeper blocks the first launch, clear the quarantine attribute with `xattr -d com.apple.quarantine ./ttying`. The server listens on loopback only, so the macOS application firewall may ask for permission on the first start; inbound connections from other hosts are not required.

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

Command-line arguments override INI values. Relative `token_file` paths are resolved relative to the INI file. Use `--read-only` to override `writable = true` from the INI file, and `--no-secure-cookie` to override `secure_cookie = true`.

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
.\dist\ttying-windows-x86_64.exe `
  --bind 127.0.0.1 --port 7755 `
  --token-file .\token.txt -W `
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
.\dist\ttying-windows-x86_64.exe `
  --token-file .\token.txt `
  --public-url https://tty.example.cn `
  --print-url
```

Open the resulting URL:

```text
https://tty.example.cn/#auth=<url-encoded-token>
```

Cloudflare sees only the HTTPS request, not the fragment. Do not use `?token=` or a path segment for the token. If only `https://tty.example.cn/` is opened, ttying displays the token login form.

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

The macOS arm64 target (`aarch64-apple-darwin`) is built only by GitHub Actions on a macOS runner, so no local Apple toolchain is required.

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
.\scripts\build-all.ps1
```

Preflight the resolved toolchain without building:

```powershell
.\scripts\build-all.ps1 -ValidateOnly
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
crates/ttying-cli    Windows/Linux/macOS CLI, token handling, Windows service
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
- The macOS build is ad-hoc signed and not notarized, so Gatekeeper may require clearing the quarantine attribute on first launch. It targets Apple silicon only; Intel Macs are not covered.
- The repository does not claim complete CVE coverage for third-party dependencies.

## GitHub Actions and releases

- `.github/workflows/ci.yml` runs Rust tests, Clippy, frontend tests, protocol regressions, Android JNI compilation, and a macOS arm64 build check on pushes and pull requests.
- `.github/workflows/release.yml` runs on tags matching `v*`.
- The release workflow builds:
  - `ttying-windows-x86_64.exe`
  - `ttying-linux-x86_64`
  - `ttying-macos-arm64.tar.gz`
  - `ttying-android-arm64.apk`
  - `SHA256SUMS.txt`
- Create a release with:
  ```powershell
  git tag v1.8.3
  git push origin v1.8.3
  ```
- Required repository secrets for Android signing:
  - `ANDROID_KEYSTORE_BASE64`
  - `ANDROID_KEY_ALIAS`
  - `ANDROID_KEYSTORE_PASSWORD`
  - `ANDROID_KEY_PASSWORD`

## License

BSD 3-Clause. See [LICENSE](LICENSE).
