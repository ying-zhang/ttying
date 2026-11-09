# ttying

中文 | [English](README.en-US.md)

ttying 是一个使用 Rust 编写的小型、以安全为重的 Web 终端服务器。它在主机上运行真实 PTY，并通过浏览器中的 xterm.js 提供终端交互。

```text
Windows x86_64  ->  ttying-windows-x86_64.exe
Linux x86_64    ->  ttying-linux-x86_64
macOS arm64     ->  ttying-macos-arm64.tar.gz
Android arm64   ->  ttying-android-arm64.apk
```

Android APK 只负责启动、停止和显示本地服务状态，实际终端交互在系统浏览器中完成。Android 包名为 `cn.z.ttying`。

当前版本：**v1.8.3** · [GitHub 仓库](https://github.com/ying-zhang/ttying) · [发布页](https://github.com/ying-zhang/ttying/releases) · [项目主页](https://ying-zhang.cn/ttying/)

## 开发说明

ttying 在开发过程中使用了大量 AI 辅助。AI 助手参与了架构与协议设计、Rust 和前端实现、安全审查、回归测试、构建/发布自动化、Android 界面和文档整理；项目负责人提供了需求、产品决策、人工审查和测试方向。

根据项目负责人的记录，本项目开发累计消耗约 **2.9 亿词元**，成本约 **人民币 10 元**。

在安全敏感的生产环境中部署前，仍建议进行独立代码审查。

## 功能

- PTY 会话在 WebSocket 断开后保留 5 分钟。
- 刷新、bfcache 恢复和网络重连都会重新附着到同一个 shell。
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

本节说明默认绑定、Token 认证、Cookie 与会话边界，以及部署时必须满足的前提。

- 默认监听 `127.0.0.1:7755`。
- Android 优先使用 `7755`；如果端口被占用，自动回退到系统临时端口。
- 未指定 `--token` 或 `--token-file` 时生成 256 位随机 token。
- 优先使用 `--token-file`：token 出现在命令行参数中会残留于进程列表和 shell 历史。
- Token 通过 `#auth=<token>` 引导，经 `POST /api/session` 换取 HttpOnly session cookie。
- Token 不放在 query string 中；URL fragment 不会发送给 HTTP 服务器或反向代理。
- WebSocket 需要认证，并校验 Origin 与 Host；未显式配置 `--allowed-origin` 时，Origin 必须与请求 Host 匹配。
- 默认禁止终端输入，传入 `-W` / `--writable` 才允许输入。
- ttying 不是沙箱；shell 使用 ttying 进程的权限运行。
- token 文件、签名密钥和密码不得进入版本控制，应仅服务账户可读；密码通过已文档化的环境变量或秘密管理器提供。
- 以符合用途的最小权限账户运行 ttying。

### HTTPS 反向代理

HTTPS 反向代理终止 TLS 时，必须使用：

```powershell
--secure-cookie
```

它会让 cookie 带 `Secure`，并让 Origin 校验按 `https://` 处理。否则浏览器页面通过 HTTPS 加载时发送 `Origin: https://...`，WebSocket/API 可能因为 scheme 不匹配被拒绝。

`--public-url https://tty.example.cn` 会在前台启动时自动启用 Secure cookie，并让程序输出公网 URL。

### 部署与认证边界

`--allowed-origin` 非空时是排他的浏览器 Origin 白名单，不再同时放行与 Host 相同的其他 Origin。未配置时按 HTTP(S) scheme 和 Host 校验。非浏览器客户端可以不带 Origin，但仍需认证。反向代理应拒绝非预期 Host；公网部署应显式配置 Origin 白名单。Host 比较本身不提供 DNS rebinding 防护。

认证 cookie 默认有效期 24 小时，`POST /api/session` 续期。注销撤销该 cookie 及其 WebSocket，不撤销共享主 token，也不终止 PTY。Bearer token 直连不受 cookie 注销控制；撤销主 token 需要更换 token 并重启服务。认证表最多 4096 条，满时淘汰最早到期 cookie，被淘汰客户端需要重新认证；正确 token 仍可登录。失败限流表满时拒绝新来源的错误登录，正确凭据不受影响。

Android 仅把带 fragment 的引导 URL 发给设置中的默认浏览器包；没有明确默认浏览器时提示用户先配置。浏览器和扩展能看到 fragment，必须信任它们。fragment 不进入 HTTP 请求不等于对接收 URL 的应用保密。原生 App 内先进行 HTTP 登录不能写入外部浏览器的 cookie 仓库。

正常启动在 stdout 被重定向时不输出凭据链接；`--print-url` 明确请求输出完整链接。保护 token 文件及其父目录。Windows 前台启动保留已有显式 ACL，包括服务账户的读取权限，并关闭继承；管理员仍需移除不可信账户的显式读取授权。不要将运行 token、签名密钥或密码放入版本控制。签名材料宜存放在工作树外，通过已文档化的环境变量或秘密管理器提供密码。本次修复不移动已有私人签名文件、不修改 CI secrets。

### systemd 首次部署

示例单元启用了只读 `/etc`，必须事先创建用户和 token。将可执行文件安装到 `/usr/local/bin/ttying` 后执行：

```sh
sudo useradd --system --home-dir /var/lib/ttying --create-home --shell /usr/sbin/nologin ttying
sudo install -d -m 0700 -o ttying -g ttying /etc/ttying
sudo -u ttying sh -c 'umask 077; openssl rand -hex 32 > /etc/ttying/token'
sudo install -m 0644 packaging/ttying.service /etc/systemd/system/ttying.service
sudo systemctl daemon-reload
sudo systemctl enable --now ttying
```

仅在首次部署生成 token，不要覆盖运行中服务的 token。可显式运行以下命令取登录链接：

```sh
sudo -u ttying /usr/local/bin/ttying --token-file /etc/ttying/token --print-url
```

HTTPS 代理部署需要在单元中添加 `--secure-cookie --allowed-origin https://你的域名`。示例单元的 `-W` 开启输入，只读部署请移除。隐藏标签页可能因 60 秒没有入站心跳断开，之后有 5 分钟重新附着窗口；容量不足时新建会话失败，不再提前杀死宽限期内的旧会话。

### 漏洞报告

请使用 GitHub 的私有漏洞报告功能或本仓库的安全公告（Security Advisory）。在修复版本发布前，不要在公开 issue 中披露可利用的问题。

报告中请包含：

- 受影响的版本或 commit；
- 操作系统与架构；
- 服务器绑定的是回环地址还是非回环地址；
- 最小复现步骤；
- 预期行为与实际行为。

已知设计限制与不受支持的部署方式见[限制](#限制)。仓库根的 [SECURITY.md](SECURITY.md) 与 [SECURITY.zh-CN.md](SECURITY.zh-CN.md) 是供代码托管平台识别的简版入口，本节内容为准。

## 快速使用

### Windows

```powershell
.\dist\ttying-windows-x86_64.exe --token-file .\token.txt -W -- cmd.exe
```

打开终端输出的 URL。默认端口是 `7755`。

### Linux

```bash
./ttying-linux-x86_64 --token-file ./token.txt -W -- bash
```

### macOS（Apple 芯片）

```bash
tar -xzf ttying-macos-arm64.tar.gz
./ttying --token-file ./token.txt -W -- zsh
```

压缩包内包含 `ttying` 可执行文件、`ttying.ini.example`、`LICENSE`、`README.md` 和 `README.en-US.md`。该二进制仅做 ad-hoc 签名，未做公证；若首次启动被 Gatekeeper 拦截，可执行 `xattr -d com.apple.quarantine ./ttying` 清除隔离属性。服务只监听回环地址，因此 macOS 应用防火墙可能只在首次启动时询问权限，无需允许来自其他主机的外部连接。

### Android

```text
adb install -r ttying-android-arm64.apk
```

打开应用，点击中央圆形**启动**按钮，再点击**在浏览器中打开终端**。服务只监听 127.0.0.1；关闭界面不会停止前台服务，停止服务需点击圆形**停止**按钮。

## INI 配置文件

复制 `ttying.ini.example` 为本地配置文件，然后启动：

```powershell
ttying.exe --config .\ttying.ini
```

命令行参数会覆盖 INI 中的同名配置。`token_file` 使用相对路径时，相对于 INI 文件所在目录解析。使用 `--read-only` 可以覆盖 INI 中的 `writable = true`，使用 `--no-secure-cookie` 可以覆盖 `secure_cookie = true`。

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
.\dist\ttying-windows-x86_64.exe `
  --bind 127.0.0.1 --port 7755 `
  --token-file .\token.txt -W `
  --secure-cookie `
  --public-url https://tty.example.cn `
  -- cmd.exe
```

Cloudflare Tunnel 的 ingress 示例：

```yaml
ingress:
  - hostname: tty.example.cn
    service: http://127.0.0.1:7755
  - service: http_status:404
```

只生成公网 URL、不启动服务：

```powershell
.\dist\ttying-windows-x86_64.exe `
  --token-file .\token.txt `
  --public-url https://tty.example.cn `
  --print-url
```

打开生成的地址：

```text
https://tty.example.cn/#auth=<url-encoded-token>
```

Cloudflare 只能看到 HTTPS 请求，看不到 fragment。不要把 token 放在 query string 或路径中。只打开 `https://tty.example.cn/` 时，ttying 会显示 token 输入框。

## 构建

### 本地环境要求

- Rust，含 `x86_64-pc-windows-gnu`、`x86_64-unknown-linux-musl`、`aarch64-linux-android` 目标
- PowerShell 7（运行 `scripts/build-all.ps1`）
- Node.js 22 / npm
- Python 3
- JDK 17+
- Android SDK、NDK、build-tools 与 `cargo-ndk`
- Zig 与 `cargo-zigbuild`（静态 Linux 构建）
- MinGW-w64（Windows GNU 构建）

macOS arm64 目标（`aarch64-apple-darwin`）只在 GitHub Actions 的 macOS runner 上构建，本地不需要 Apple 工具链。

复制 `scripts/build-config.example.psd1` 为 `scripts/build.config.psd1`，填写本机工具链路径。该文件已被 Git 忽略。也可以使用环境变量：

```text
CARGO_HOME, RUSTUP_HOME
ANDROID_HOME 或 ANDROID_SDK_ROOT, ANDROID_NDK_HOME
TTYING_JDK_BIN, TTYING_PYTHON
TTYING_ZIG_BIN, TTYING_MINGW_BIN
TTYING_NODE, TTYING_NPM
ANDROID_BUILD_TOOLS, ANDROID_PLATFORM_JAR
```

构建全部产物：

```powershell
.\scripts\build-all.ps1
```

只预检工具链、不执行构建：

```powershell
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

smoke 与 security 测试跨平台，支持以下环境变量：

```text
TTYING_TEST_EXE
TTYING_TEST_COMMAND
TTYING_TEST_SHELL
TTYING_TEST_TOKEN
TTYING_TEST_PORT
```

## 设计思路

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

### 会话模型

WebSocket 只是 PTY 会话的视图。断开不会立即终止 shell：会话保留最多五分钟，可通过 `sessionStorage` 中的不可猜测 session id 重新附着。PTY 退出后客户端收到 `session_ended` 并显示重新启动按钮。

每个会话保存最多 512 KiB 原始输出历史，新连接按序号回放。该回放不是完整的 VT 屏幕快照，全屏 TUI 应用在历史被截断后可能需要自行重绘。

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
#auth=<token>  ->  POST /api/session  ->  HttpOnly cookie  ->  /ws
```

前端使用 `history.replaceState` 清除 fragment。Cookie 为 `SameSite=Strict`、`HttpOnly`，HTTPS 下应为 `Secure`。

### 代码结构

```text
crates/ttying-core   PTY、会话、HTTP/WebSocket 服务、认证
crates/ttying-cli    Windows/Linux/macOS CLI、token、Windows 服务
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
- macOS 构建只做 ad-hoc 签名、未做公证，首次启动可能需要清除隔离属性；目前只覆盖 Apple 芯片，不支持 Intel Mac。
- 不对第三方依赖的完整 CVE 覆盖作承诺。

## GitHub Actions 与发布

- `.github/workflows/ci.yml` 在 push 和 pull request 时运行 Rust 测试、Clippy、前端测试、协议回归、Android JNI 编译和 macOS arm64 构建验证。
- `.github/workflows/release.yml` 在匹配 `v*` 的 tag 上运行。
- 发布构建产物：
  - `ttying-windows-x86_64.exe`
  - `ttying-linux-x86_64`
  - `ttying-macos-arm64.tar.gz`
  - `ttying-android-arm64.apk`
  - `SHA256SUMS.txt`
- 创建发布：
  ```powershell
  git tag v1.8.3
  git push origin v1.8.3
  ```
- Android 签名需要以下 repository secrets：
  - `ANDROID_KEYSTORE_BASE64`
  - `ANDROID_KEY_ALIAS`
  - `ANDROID_KEYSTORE_PASSWORD`
  - `ANDROID_KEY_PASSWORD`

## 许可证

BSD 3-Clause，详见 [LICENSE](LICENSE)。
