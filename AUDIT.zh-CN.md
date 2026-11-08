# ttying 审查与修复记录

审查日期：2026-11-08。范围：`next/` 的 Rust、浏览器、Android、构建脚本、部署文件与 README；参考 `research/krust` 的屏幕回放设计及 `research/ttyd-rs-wolegca` 的会话生命周期。审查基于当前工作区，保留原有未提交修改。

## 已确认的问题

| 编号 | 优先级 | 原始位置 | 问题、影响与修复方向 |
|---|---|---|---|
| A01 | P1 | `ttying-jni/src/lib.rs:112,133` | `start/stop` 持有 STATE Mutex 时调用再次加锁的 `status_json()`，必然死锁。Java 桌面 JNI 实测两者均 4 秒超时；Android 主线程调用会 ANR。状态序列化应直接读取 guard，耗时操作移至后台。 |
| A02 | P1 | `session.rs:73,83,117,183` | 读写线程更新局部 AtomicBool，`is_ended()` 却读取另一实例。自然退出未被观察、死会话可重新附着、Restart 不出现。原始 socket 检查看到无结束帧且 reattached=true，但因 A19 的启动握手问题不能单独证明子进程已退出；该根因由源码确认，修复后由自然退出回归测试覆盖。 |
| A03 | P1 | `session.rs:100,141` | seq 先递增后写 history，快照可能读取 seq=N 而只包含 N-1，服务端随后永久跳过 N。须在同一锁中提交序号和数据。 |
| A04 | P2 | `session.rs:152,160,244,290` | 计数与 detached_at 分开修改，attach 与最后一次 detach 可产生“已连接但计时过期”，manager 解锁后才 attach 还会与 cleanup 竞争。使用统一状态锁并在 manager 锁内完成附着。 |
| A05 | P1 | `server.rs:438,445,534` | 1 MiB 限制只在已分配完整 Binary 后检查，Text 完全绕过；底层默认消息 64 MiB/帧 16 MiB。实测 1,048,577 字节 Text 后连接仍正常。升级时限制消息与帧，拒绝 Text。 |
| A06 | P1 | `server.rs:474,527` | 无 attach/空闲/发送超时，认证客户端可永久占满连接槽；输入队列满时同一循环无法接收心跳或处理结束。加入超时、取消信号与明确背压。 |
| A07 | P1 | `server.rs:369,383,431` | 认证 cookie 撤销/过期不作用于已升级连接。实测 logout=204、新请求=401，但旧 socket 仍有 pong。认证会话需绑定连接并持续验证。 |
| A08 | P2 | `server.rs:164,190,372` | Origin 比较遗漏 scheme、IPv6 比较格式不一致，allowed-origin 实际为附加允许而非覆盖；cookie 缺 Secure 选项。实测 HTTPS Origin 可访问同 host/port 的 HTTP WS。明确外部 HTTPS/可信代理配置，严格比较 origin，支持 Secure cookie。 |
| A09 | P2 | `server.rs:362,371` | 登录 map 无硬上限，每次 cookie 续期创建新记录且旧记录保留；失败限流在代理后可让一个客户端阻断所有用户的正确登录。限制记录数量，复用有效 cookie，正确凭据不被失败计数拒绝。 |
| A10 | P1 | `pty.rs:89,95,101,139` | Unix 多线程 fork 后 setenv 不满足异步信号安全要求；master/dup 未 CLOEXEC，可被后续 shell 继承；dup 失败资源回收不全，子进程 wait/kill 也未统一管理。改用带 CLOEXEC 的 PTY 与 std::process::Command，在 pre_exec 只执行必要 syscall。需 Linux/Android 目标验证。 |
| A11 | P2 | `server.rs:489,563`、`main.js:332` | 截断原始输出不等于终端屏幕快照，重连 reset 后 TUI/UTF-8/ANSI 解析状态可能损坏；Lagged 时静默跳过缺口且无客户端 seq/ACK。须明确“有限原始回放”边界并显式通知缺口，不能宣称无损恢复屏幕。完整 VT 镜像是后续独立功能。 |
| A12 | P1 | `main.js:194,288` | 网络异常、429、5xx 全当成 token 过期，第一次离线重试就停止退避并弹认证框。提取原函数模拟 fetch 网络异常已复现。区分认证与瞬时故障。 |
| A13 | P2 | `main.js:286,463,536,569` | 多入口登录缺少代次/取消，旧异步结果可覆盖新连接；连接前无 attach 状态输入保护、发送队列无上限、登录请求无超时。增加连接代次、请求取消和流量保护。 |
| A14 | P1 | CLI `main.rs:52`、`service_windows.rs:56` | token 先普通写入再 chmod/ACL，失败被忽略；Windows 已存在 token 不检查保护，自定义 service account 未授予读取权限。改为受限创建、权限失败中止并保护已有文件；服务账户须对应 ACL。 |
| A15 | P2 | CLI `main.rs:77,96,173` | 空/弱固定 token 仍可启动，URL token 未编码、IPv6 未加方括号，访问 token 进入 info 日志。统一校验、编码 URL，仅控制台显式显示启动链接。 |
| A16 | P2 | `build-all.ps1:35,46,52,83,95` | 无效首选环境路径遮蔽有效 SDK 回退；版本按字符串排序；只检测 esbuild 目录跳过依赖一致性；cargo 不 locked、不显式 Windows target，CARGO_TARGET_DIR 改变时复制错误路径。应校验工具/版本、锁定依赖并显式产物目录。 |
| A17 | P2 | Android Activity/Service | 原生启动/停止和轮询运行在 UI 主线程，重复 Start 重启服务器，失败不显示原因；binder Stop 没停止 foreground/bound service。后台串行执行并统一状态与服务停止流程。 |
| A18 | P2 | README、systemd、smoke-test | 缺工具安装前提、反代信任边界、认证 TTL/注销语义；systemd token 初次创建目录只读且无预配置步骤；smoke-test 无只读/退出/限额/注销/离线回归，原始 recv 不能正确处理短读。补部署与验证说明。 |
| A19 | P1 | Windows `pty.rs`、只读输入边界 | ConPTY 的初始光标位置询问依赖浏览器输入，默认只读会丢弃应答，导致 shell 卡在启动阶段。新增退出测试发现该阻塞；修复为服务端完成一次启动应答，并跨读取边界过滤该初始询问，避免浏览器重复应答。 |

优先级：P1 为需要优先修复的安全/主要功能问题；P2 为可靠性、配置与边界问题。需要认证的资源攻击不等同于未认证远程代码执行。未发现足够证据声称存在后者。

## 包名与产物核对

源码层面一致：manifest application id=`cn.z.ttying`；Java 声明 `package cn.z.ttying`；JNI 导出 `Java_cn_z_ttying_TtyingNative_{start,stop,status}`；库名 `ttying_jni` / `libttying_jni.so`。磁盘目录末尾 `app/` 不改变 Java 包名。不能把目录名当作 JNI 不匹配。

## 原始验证

- `cargo test --workspace --locked --offline`：通过，但所有 crate 和文档测试均为 0。
- `cargo build --workspace --locked --offline`：Windows debug 编译通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过。
- esbuild 重建 JS/CSS 与已嵌入资产 SHA256 一致。
- `../audit-next/reproduce.py`、`JniProbe.java`、`reconnect-probe.cjs` 保存了有界本机复现。
- 独立 stop probe：Windows 有/无附着 socket 均能停止；未把停止必然挂起列为已复现问题。Unix 忽略 HUP 的后代进程、跨平台关闭仍需目标测试。

## 验证限制

未运行 Android 真机/模拟器与 Linux 运行时测试，未把源码编译成功视为 Android APK 可用的证据。在线检索工具不可用，未完成最新 RustSec/npm 漏洞库审计；依赖结论基于 Cargo.lock 和本地依赖源码，不能宣称无已知 CVE。

## 修复状态

已直接修改实现并增加回归，未提交或覆盖原有工作区更名历史。

| 范围 | 修复结果 |
|---|---|
| A01、A17、包名 | JNI 直接用已有 guard 序列化，Start 幂等，失败状态包含 error；Android 单线程 worker 启停，UI 仅读取缓存状态；包名与符号保持一致。 |
| A02～A04、A19 | 共享结束状态并轮询子进程退出；历史和 seq 同锁提交；计数/脱离时钟同锁，manager 锁内 attach；容量不足明确拒绝；ConPTY 握手由服务端完成。 |
| A05～A09 | 握手层 1 MiB 限制、拒绝 Text；64 KiB 输入限额、队列满拒绝；attach/idle/send 超时；cookie 撤销/过期持续验证；Origin 包含 scheme/IPv6/默认端口；Secure cookie 显式配置；认证记录限额4096，续期复用记录；正确凭据不受其他用户失败计数锁定。 |
| A10 | Unix 使用 CLOEXEC/nonblocking master 和受控 slave；Command 在 fork 前配置环境，pre_exec 仅 setsid/ioctl；不再创建独立 waitpid 线程；回收前检查 Child，避免 reaped PID 信号；停止后读写任务可退出。Linux musl 与 Android arm64 交叉构建验证。 |
| A11 | 修复丢字节竞态和恢复失败后继续跳序号；输出缺口显式报错断开；初次回放携带 history_truncated 并显示提示。README 改为有限原始回放，明确无客户端 ACK/完整 VT 屏幕快照保证。完整屏幕恢复仍是设计限制，并未实现新的 VT 模拟器。 |
| A12、A13 | 分开处理401/403与离线/429/5xx，登录超时/AbortController/代次保护；过期异步结果不覆盖新连接或新 token；认证成功清除内存/表单 token；附着前禁止输入并限制浏览器发送队列；pagehide 关闭连接、pageshow 重连、online 恢复重试。 |
| A14、A15 | 临时文件先设置权限再写秘密并同步发布；首次 token 创建禁止覆盖，避免并发启动互相改 token；Unix 已有文件检查无组/其他用户权限，可在只读部署中读取；Windows 临时文件 ACL 使用 SID 并检查退出码，已有 token 重新安全发布，服务账户获得对应读取权限。固定 token 至少16个可打印ASCII字节，无空白；URL 编码/IPv6修正；info 日志去掉完整访问链接。 |
| A16、A18 | 配置优先级/无效路径/SDK回退明确化，按数字版本选工具，JDK/Python/Node/npm 路径验证；npm ci、Cargo --locked、显式 target 和 target-dir；ValidateOnly 预检；构建清理校验路径和重解析点；README 增补反代、权限、systemd预配置、构建依赖、调试签名和会话边界；smoke recv 修复短读。 |

## 修复后验证

- Rust：4 项回归通过，覆盖序号快照并发不变量、子进程退出不重附着、Origin边界、分段ConPTY询问过滤。
- 前端：`node --test web/tests/reconnect.test.cjs` 共8项通过，覆盖离线、429/500/503、401、token清除和两种过期异步结果竞态。
- `scripts/security-regression.py` 共7项协议检查通过：自然退出结束帧、logout关闭旧socket、超大消息拒绝、scheme拒绝、连接限额、cookie续期复用、只读输入拒绝。
- Java 桌面 JNI 探针：`start/stop` 均正常返回，原4秒死锁不再发生。
- token文件创建/重读返回同一token；空token拒绝；Windows实际ACL仅当前账户、Administrators和SYSTEM。服务账户安装需管理员环境单独验证。
- Windows、Linux musl、Android arm64/APK 完整构建成功；最终产物位于 `dist/`，附 SHA256SUMS。发布构建、打包检查的最终结果见下文。

限制：前端测试使用实际函数与模拟网络，不等同于跨浏览器视觉/移动键盘测试；Android Java编译、JNI符号和APK打包不等同于真机生命周期验证；Linux仅交叉编译，未进行运行时测试。程序不承诺清理主动脱离进程组的 daemon。APK最初为本地debug签名；后续验证已改用独立release keystore，并在Android真机完成安装与运行测试，详见下方补充。

发布验证补充：Windows发布版7项security regression与原smoke test均通过，包含输入、心跳、重附着与历史回放。`aapt2 dump badging` 确认APK包名 `cn.z.ttying`、Activity `cn.z.ttying.MainActivity`、minSdk26/targetSdk35/arm64-v8a；`llvm-readelf --dyn-syms` 确认三个 `Java_cn_z_ttying_TtyingNative_*` 导出。Rustfmt、Windows Clippy 和 `git diff --check` 通过（仅已有CRLF提示）。`--print-url` 对已有Windows凭据仅读取，不替换ACL，以保留已安装服务账户的读取权限；正常前台启动才执行凭据权限收紧。

最终构建无编译警告；Linux musl CLI与Android arm64 JNI的Clippy（`-D warnings`）也通过。最终Windows发布产物再次通过7项security regression和完整smoke。APK通过apksigner v2/v3签名验证，包内 `.so` 与本次构建文件逐字节一致，三个发布文件的SHA256均与 `dist/SHA256SUMS.txt` 一致。

## 后续验证与修复补充（2026-11-08）

- Windows x86_64 release：`cargo test --workspace --locked --offline`、Clippy、`scripts/security-regression.py`、`scripts/smoke-test.py` 全部通过。
- WSL Ubuntu 24.04 x86_64：使用 `dist/ttying-linux-x86_64` 运行原生 smoke test 和 7 项 security regression，全部通过。
- Android 真机（Xiaomi 24069RA21C，Android API 36，arm64-v8a）：通过 ADB 安装 release APK；验证 `cn.z.ttying`、前台服务、停止/重启、浏览器页面加载、状态栏与移动键工具栏。
- Android 真机 WebSocket：经 `adb forward` 完成认证、ATTACH、PING/PONG、输入回显、会话重连与历史回放。
- APK 使用 release keystore 签名；`apksigner verify --print-certs` 显示 `CN=ttying, O=ttying, C=CN`，包名为 `cn.z.ttying`，minSdk 26 / targetSdk 35，原生 ABI 为 arm64-v8a。
- 修复移动浏览器软键盘遮挡底部状态栏/移动键栏的问题：使用 `visualViewport` 同步应用高度。
- 修复 WSL/Android/Windows 测试脚本的退出清理：测试默认仅杀死 ttying 主进程会遗留 Windows ConPTY `cmd.exe`/`conhost.exe`；现改为 Windows 进程树 `taskkill /T /F`，并在测试后检查无 `conhost.exe --headless` 残留。
- 仍有限制：未做完整 Android 生命周期压力测试；Android 发布签名材料需独立备份；Linux 只做 WSL 原生运行与交叉编译，未覆盖其他发行版/内核；完整 VT 屏幕状态恢复仍未实现。
