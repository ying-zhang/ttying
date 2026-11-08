# ttying

中文 | [English](README.md)

ttying 是一个使用 Rust 编写的小型 Web 终端服务器。它在主机上运行真实 PTY，并通过浏览器中的 xterm.js 提供终端交互。

```text
Windows x86_64  ->  ttying-windows-x86_64.exe
Linux x86_64    ->  ttying-linux-x86_64
Android arm64   ->  ttying-android-arm64.apk
```

Android APK 只负责启动、停止和显示本地服务状态，实际终端交互在系统浏览器中完成。Android 包名为 `cn.z.ttying`。

当前版本：**v1.8.0** · [GitHub 仓库](https://github.com/ying-zhang/ttying) · [发布页](https://github.com/ying-zhang/ttying/releases)

## 开发说明

ttying 在开发过程中使用了大量 AI 辅助。AI 助手参与了架构与协议设计、Rust 和前端实现、安全审查、回归测试、构建/发布自动化、Android 界面和文档整理；项目负责人提供了需求、产品决策、人工审查和测试方向。

根据项目负责人的记录，本项目开发累计消耗约 **2.9 亿词元**，成本约 **人民币 10 元**。这是历史用量记录，不代表每次构建或部署的成本。

在安全敏感的生产环境中部署前，仍建议进行独立代码审查。

## 功能

- PTY 会话在 WebSocket 断开后保留 5 分钟，支持刷新、bfcache 恢复和网络重连重新附着。
- 512 KiB 有界原始输出回放，带序号和截断提示。
- Token 认证：URL fragment 引导、POST 换取 HttpOnly cookie。
- 默认只读，只有传入 `-W` / `--writable` 才允许输入。
- Host/Origin 校验、登录限流、WebSocket 大小限制和连接超时。
- xterm.js 搜索、安全链接、复制粘贴快捷键、字体大小和自动/深色/浅色主题。
- 移动端特殊键工具栏和软键盘 viewport 适配。
- Android 使用中央圆形启动/停止大按钮和自然语言状态。
- 支持中英文界面：Android 原生字符串跟随系统语言，浏览器界面跟随浏览器语言。
- 支持 `--config` 读取 INI 配置，命令行参数优先级更高。
- 不内置 TLS，生产环境应放在可信 HTTPS 反向代理或 VPN 后面。

## 参考项目与设计来源

ttying 是独立的 Rust 实现。代码设计时参考了以下项目的交互行为和公开实现：

- [ttyd](https://github.com/tsl0922/ttyd)：原始 C Web 终端，WebSocket 帧、resize/流控，以及 WebSocket 与 PTY 生命周期关系。
- [ttyd-rs](https://github.com/tsl0922/ttyd-rs)：xterm.js 集成、终端偏好、搜索/导出、剪贴板和主题。
- [ttyd-rs-wolegca](https://github.com/wolegca/ttyd-rs)：session id、重新附着、指数退避重连、心跳、可见性探测、状态面板、移动键和用户设置。
- [krust](https://github.com/bitshift/krust)：有界滚动回放、输出缺口处理、焦点上报、剪贴板错误提示和触摸选择。
- [termpair](https://github.com/cs01/termpair)：终端共享状态、查看者数量、只读展示和端到端加密思路。
- [sshx](https://github.com/ekzhang/sshx)：设置持久化、主题/scrollback、延迟统计和重连行为。
- [webterm](https://github.com/nasa42/webterm)：持久会话、通知和浏览器会话状态。

本项目没有复制上述项目的源代码；它们的许可证和署名要求仍归各自项目所有。

## 安全模型

- 默认监听 `127.0.0.1:7755`。
- Android 优先使用 `7755`；如果端口被占用，自动回退到系统临时端口。
- 未指定 `--token` 或 `--token-file` 时生成 256 位随机 token。
- 优先使用 `--token-file`，避免 token 出现在进程命令行和 shell 历史中。
- Token 通过 `#auth=<token>` 引导，经 `POST /api/session` 换取 HttpOnly session cookie。
- Token 不放在 query string 中；URL fragment 不会发送给 HTTP 服务器或反向代理。
- WebSocket 需要认证，并校验 Origin 与 Host；也可以显式配置 `--allowed-origin`。
- 默认禁止终端输入，传入 `-W` / `--writable` 才允许输入。
- ttying 不是沙箱；shell 使用 ttying 进程的权限运行。

### HTTPS 反向代理

HTTPS 反向代理终止 TLS 时，必须使用：

```powershell
--secure-cookie
```

它会让 cookie 带 `Secure`，并让 Origin 校验按 `https://` 处理。否则浏览器页面通过 HTTPS 加载时发送 `Origin: https://...`，WebSocket/API 可能因为 scheme 不匹配被拒绝。

`--public-url https://tty.example.cn` 会在前台启动时自动启用 Secure cookie，并让程序输出公网 URL。

## 快速使用

### Windows

```powershell
.\dist\ttying-windows-x86_64.exe --token-file .\token.txt -W -- cmd.exe
```

默认端口是 `7755`。

### Linux

```bash
./ttying-linux-x86_64 --token-file ./token.txt -W -- bash
```

### Android

```text
adb install -r ttying-android-arm64.apk
```
Android UI starts the foreground service; terminal interaction is in the browser.
打开应用，点击中央圆形启动按钮，再点击在浏览器中打开终端。服务只监听 127.0.0.1；关闭界面不会停止前台服务。

## INI 配置文件

复制 `ttying.ini.example` 为本地配置文件，然后启动：

```powershell
ttying.exe --config .	tying.ini
```

命令行参数会覆盖 INI 中的同名配置。`token_file` 使用相对路径时，相对于 INI 文件所在目录解析。

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

支持的分组和键：

| 分组 | 键 |
| --- | --- |
| `[server]` | `bind`、`port`、`public_url` |
| `[security]` | `token`、`token_file`、`secure_cookie`、`max_clients`、`allowed_origins` |
| `[terminal]` | `writable`、`command` |

未知分组或键会报错。包含空格的命令参数需要使用引号。INI 中可以直接放 token，但应限制文件权限；更推荐使用 `token_file` 并保护 token 文件。

### Cloudflare Tunnel / HTTPS 反向代理

```powershell
.\dist\ttying-windows-x86_64.exe --bind 127.0.0.1 --port 7755 --token-file .\token.txt -W --secure-cookie --public-url https://tty.example.cn -- cmd.exe
```

```yaml
ingress:
  - hostname: tty.example.cn
    service: http://127.0.0.1:7755
  - service: http_status:404
```

只生成公网 URL：

```powershell
.\dist\ttying-windows-x86_64.exe --token-file .\token.txt --public-url https://tty.example.cn --print-url
```

```text
https://tty.example.cn/#auth=<url-encoded-token>
```

Cloudflare 看不到 URL fragment。不要把 token 放在 query string 或路径中。只打开 `https://tty.example.cn/` 时，ttying 会显示 token 输入框。

## 构建

复制 `scripts/build-config.example.psd1` 为 `scripts/build.config.psd1`，填写本机工具链路径。该文件已被 Git 忽略。也可以使用环境变量：

```text
CARGO_HOME, RUSTUP_HOME
ANDROID_HOME 或 ANDROID_SDK_ROOT, ANDROID_NDK_HOME
TTYING_JDK_BIN, TTYING_PYTHON
TTYING_ZIG_BIN, TTYING_MINGW_BIN
TTYING_NODE, TTYING_NPM
ANDROID_BUILD_TOOLS, ANDROID_PLATFORM_JAR
```

```powershell
.\scripts\build-all.ps1
.\scripts\build-all.ps1 -ValidateOnly
```

Android APK 使用 release keystore 签名。在 `scripts/build.config.psd1` 中配置 `AndroidKeystore`、`AndroidKeyAlias`、`AndroidKeyStorePassword`、`AndroidKeyPassword`，或设置：

```text
TTYING_ANDROID_KEYSTORE
TTYING_ANDROID_KEY_ALIAS
TTYING_ANDROID_KEYSTORE_PASSWORD
TTYING_ANDROID_KEY_PASSWORD
```

必须单独备份 release keystore 和密码；丢失后无法用同一身份发布 APK 更新。

### 测试

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
node --test web/tests/reconnect.test.cjs
python scripts/security-regression.py
python scripts/smoke-test.py
```

测试脚本支持 `TTYING_TEST_EXE`、`TTYING_TEST_COMMAND`、`TTYING_TEST_SHELL`、`TTYING_TEST_TOKEN` 和 `TTYING_TEST_PORT`。安全审计记录见 `AUDIT.zh-CN.md`。

## 设计思路

```text
Browser (xterm.js)
  | HTTPS / WebSocket
  v
Axum HTTP + WebSocket server
  | output broadcast + bounded history
  v
Persistent PTY session (5 minute detach grace)
  | shell / command
  v
Operating system
```

WebSocket 只是 PTY 会话的视图。断开后 shell 保留最多五分钟，可通过 sessionStorage 中保存的随机 session id 重新附着。PTY 退出后客户端收到 `session_ended` 并显示重新启动按钮。

每个会话保存最多 512 KiB 原始输出历史，新连接按序号回放。该回放不是完整 VT 屏幕快照，TUI 应用在历史被截断后可能需要自行重绘。

### WebSocket 协议

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

服务端限制 WebSocket 消息/帧最大 1 MiB，输入最大 64 KiB，attach 超时 10 秒，接收空闲超时 60 秒，发送超时 10 秒。

### 认证流程

```text
#auth=<token> -> POST /api/session -> HttpOnly cookie -> /ws
```

前端使用 `history.replaceState` 清除 fragment。Cookie 为 `SameSite=Strict`、`HttpOnly`，HTTPS 下应为 `Secure`。

### 代码结构

```text
crates/ttying-core   PTY、会话、HTTP/WebSocket 服务、认证
crates/ttying-cli    Windows/Linux CLI、token、Windows 服务
crates/ttying-jni    Android JNI 桥接
web/                 xterm.js 前端
android/             Android Activity、前台服务、资源
scripts/             构建、APK 打包、smoke/security 测试
packaging/           systemd 单元
.github/workflows/   CI 和 tag 发布构建
```

## 限制

- 不内置 TLS，必须使用可信 HTTPS 反向代理或 VPN。
- 原始回放不等同于完整终端状态恢复。
- shell 不受 ttying 沙箱保护，只具有 ttying 进程的权限。
- 主动 daemonize 并创建新 session 的后代进程可能逃逸进程组清理，需要操作系统级 containment。
- 每个 Android release 仍建议做真机生命周期测试。
- 不对第三方依赖的完整 CVE 覆盖作承诺。

## GitHub Actions 与发布

- `.github/workflows/ci.yml` 在 push 和 pull request 时运行 Rust 测试、Clippy、前端测试、协议回归和 Android JNI 编译。
- `.github/workflows/release.yml` 在匹配 `v*` 的 tag 上运行。
- 发布构建产物：
  - `ttying-windows-x86_64.exe`
  - `ttying-linux-x86_64`
  - `ttying-android-arm64.apk`
  - `SHA256SUMS.txt`
- 创建发布：
  ```powershell
  git tag v1.8.0
  git push origin v1.8.0
  ```
- Android 签名需要以下 repository secrets：
  - `ANDROID_KEYSTORE_BASE64`
  - `ANDROID_KEY_ALIAS`
  - `ANDROID_KEYSTORE_PASSWORD`
  - `ANDROID_KEY_PASSWORD`

## 许可证

BSD 3-Clause，详见 [LICENSE](LICENSE)。
