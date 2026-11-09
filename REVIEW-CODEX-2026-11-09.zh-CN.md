# 独立评审、hy 意见复核与修复记录

日期：2026-11-09。基线：v1.8.2 当前源码。审查 Rust core/CLI/JNI、浏览器状态机、Android、构建/发布脚本、部署文档。保留 hy 原报告 `REVIEW-2026-11-09.zh-CN.md`，不改写其历史结论。HTML 落地页及正在更新的截图不属于本次修改范围。

## 结论与独立发现

未确认无凭据的远程命令执行或认证绕过。hy 的主要功能和并发问题合理，但严重性、攻击前提和部分修复建议需要纠正；不能将全部建议当作已证实漏洞，也不能将历史发布产物测试等同于本次源码验证。

| 编号 | 严重性 | 修复前位置 | 发现与本次处理 |
|---|---|---|---|
| C01 | P1 | `ttying-cli/src/main.rs` 的 `apply_config`、`service_windows.rs` | 服务 JSON 被当成 INI，服务无法进入 dispatcher。服务模式跳过 INI；新增不依赖管理员的参数路径测试。对应 H-01。 |
| C02 | P1 | `ttying-core/src/server.rs` 的 `api_session` | 校验 cookie 与续期不原子，logout 后旧 sid 可复活。二者共用一次写锁；认证只读路径不再全表写锁清理。对应 M-03/L-06。 |
| C03 | P2 | 同上 | 有效 token 可填满认证表，之后全局拒绝新 cookie。改为淘汰最早到期记录；有界表仍可能使旧客户端重新登录，不承诺抵抗持共享主 token 的恶意用户。对应 H-02。 |
| C04 | P2 | `server.rs` 登录失败计数 | 检查与计数分锁可并发超额，满表不记新来源又使限流失效。合并为一个原子检查/计数；满表对未知来源失败关闭，正确凭据绕过。对应 M-04，并补充其并发变体。 |
| C05 | P2 | `session.rs::attach_or_create` | PTY 创建持全局锁，容量淘汰又不尊重五分钟宽限期。锁内预留容量、锁外创建、锁内发布；shutdown 标志阻止迟到创建复活；只清理结束/过期会话，满时拒绝新建。对应 M-05/M-06。原生 PTY 调用本身无可安全取消的硬超时，见限制。 |
| C06 | P2 | `server.rs::origin_allowed` | 显式白名单仍允许 Host 兜底，不能固定可信域。非空白名单改为排他；文档明确无 Origin 的非浏览器仍要认证、代理须校验 Host。对应 M-01/M-02。 |
| C07 | P2 | `ttying-cli/src/main.rs::apply_config` | **独立发现**：CLI `--token-file` 被 INI `token` 抢占，可能继续使用旧凭据。显式选择任一凭据源即屏蔽 INI 的另一个源，增加回归。 |
| C08 | P2 | `main.rs` 公网 URL 处理 | **独立发现**：HTTPS 自动识别用原始字符串前缀，URL 解析允许的大小写形式可能漏启 Secure。先统一解析/规范化 URL，再决定 HTTPS；配置错误在启动前返回。 |
| C09 | P2 | `main.rs` token 读取/打印 | Unix token 路径多次解析导致 TOCTOU。改为 O_NOFOLLOW 单次打开、同一描述符验类型/权限/读取，限制读取 4096 字节；Windows 打开 reparse point 本体拒绝符号链接。正常重定向 stdout 不输出凭据，仅显式 `--print-url` 输出。对应 M-08/M-11。 |
| C10 | P2 | Windows 凭据与服务配置 | 前台重建 token 文件丢服务 ACL。改为原位保护，保留显式服务授权；安装时将 token 路径绝对化，防止服务 CWD 改变。ACL 工具使用 System32 绝对路径。对应 M-07/L-24/L-25 的成立部分。管理员仍需审计已有显式授权。 |
| C11 | P2 | `MainActivity.openBrowser` / JNI status | 隐式 HTTP handler 可被选为凭据接收方。先解析默认浏览器，再显式限定包；不能确定默认浏览器时提示配置。去掉重复的 status.token 字段及不可达分支。浏览器仍能看到 URL token，不能宣称消除所有本机凭据暴露。对应 H-03/L-27。 |
| C12 | P2 | `.github/workflows/release.yml` | 所有 job 继承 contents:write。改顶层 read，仅 publish write。对应 H-04。 |
| C13 | P2 | `web/src/main.js` | 登录在途期间，旧退避计时器可启动新登录。即时重连入口统一撤销计时器；附着前不发 resize；OSC8 与普通链接共用安全打开函数。对应 L-16/L-20/L-22。 |
| C14 | P3 | Web build / Windows PTY / shell 默认值 | Web build 统一复制 HTML，重建嵌入 JS；Windows PtySession 增加 Drop 兜底 kill；空/相对/不存在的 SHELL 回退 /bin/sh；扩充密钥忽略；纠正 Gatekeeper 注释，补 systemd 预配置和认证边界说明。 |

## 对 hy 全部意见的取舍

| hy 编号 | 评估 |
|---|---|
| H-01 | 成立，功能性 P1；最终建议表列 P0 过高。已修。 |
| H-02 | 成立，但需要持有效共享 token；其用户在可写模式本来就能执行程序，不能解读为未认证 DoS。已修全局拒绝新登录。 |
| H-03 | 信任边界合理，隐式 Intent 不会把 URL 广播给所有 handler；“必进日志”没有证据。App 内换 cookie 不会写入浏览器 cookie 仓库，该建议不可直接使用。已限定默认浏览器并记录剩余边界。 |
| H-04 | 合理的最小权限加固，已修；并非已经发生供应链入侵。 |
| M-01/M-02 | 成立但不能绕过 token。已提供排他配置；保留默认同 Host 兼容模式，文档说明代理边界。 |
| M-03/M-04 | 成立，已修并测试。饱和时一味 LRU 驱逐失败计数仍可轮换 IP 绕过，因此采用失败关闭。 |
| M-05/M-06 | 成立，已修锁粒度和宽限期；不声称原生创建具备可取消硬超时。 |
| M-07/M-08 | 成立，已修主要路径。父目录可写攻击者还可直接替换整个 token，因此目录权限仍是前提。Windows 任意 reparse/ACL 竞争的完整防御需要句柄级平台 API，不能只凭 is_symlink 声称全部解决。 |
| M-09 | 属本机签名材料管理风险，不能仅凭落盘证明密钥已失陷。不读取或输出秘密、不擅自轮换/移动签名身份；文档建议外置存储。 |
| M-10 | 合理供应链加固建议；本次只收紧 token 权限，未固定所有第三方 Action SHA、Rust 工具链或 cargo-ndk 版本，列为后续加固，不能称构建完全可复现。 |
| M-11 | stdout 泄露路径成立，已修。命令行秘密风险原文已说明；不能从 16 字节长度推定熵，随机 token 仍为推荐方案，不用任意复杂度规则冒充安全保证。 |
| L-01 | health 信息可见属低风险设计取舍，无敏感凭据；保留兼容。 |
| L-02 | cookie logout 不撤销 master Bearer 属模型边界，已明确 TTL/重启撤销语义。 |
| L-03 | 合理，permit 移到认证/Origin 之后。 |
| L-04 | 隐藏标签页断线属实际交互边界；保留入站超时以避免僵死连接永久占槽，已说明重附着窗口。 |
| L-05/L-06/L-07 | 分别限制已附着 resize 为 2 KiB、认证读路径用读锁、SHELL 回退。已修。 |
| L-08 | Windows 缺 Drop 成立，补兜底；但“测试用 taskkill 就证明应用正常 stop 无法收敛”证据不足，测试强杀主进程绕过正常 stop。Job Object 进程树约束尚未实现。 |
| L-09 | **不成立部分**：std::process::Command::spawn 的 exec 错误通道在 pre_exec/exec 完成后才返回，不能假设返回时 setsid 尚未执行。不可中断内核等待可能阻塞是一般 OS 限制，不据此加轮询猜测修复。 |
| L-10 | 文档缺失成立，新增部署说明并链接；token 默认有效期和注销语义已补。 |
| L-11/L-12 | 合理的产物与签名身份加固项，不是已证实产物污染或签名被替换。四包发布仍须实际验证清单和签名；不在源码中猜测/硬编码私人证书指纹。 |
| L-13 | **不适用所述修复方向**：App networkSecurityConfig 控制其客户端网络栈，不控制外部浏览器或 Rust TCP listener；不能用它收敛服务端监听。服务继续仅绑定回环地址。 |
| L-14/L-15 | 已扩充密钥忽略并纠正 ad-hoc 注释。 |
| L-16 | 部分成立：成功 connect 会 closeSocket 并清计时器，确定的窗口是登录在途期间。已在登录入口取消并补测试。 |
| L-17 | 缺口提示可增强，但 server 已发错误并断开，重连会显示 retained history 截断状态，不能称静默继续错位输入。本次保留现有错误提示。 |
| L-18 | 测试覆盖不足判断合理，已新增认证饱和/限流饱和/服务参数/容量宽限期/定时器回归；仍不等同浏览器端到端测试。 |
| L-19/L-20 | 已统一 HTML 复制和两种链接打开路径；OSC8 使用当前 xterm 的 linkHandler 选项，而非猜测 provider API。 |
| L-21 | 失败后保留 password 表单便于修改错误 token，不单独构成越权；成功会清空，保留现有交互。 |
| L-22 | attach 前 resize 已禁止；标签复制继承 sessionStorage 属浏览器语义，共享会话可多视图，未引入互斥所有权策略。 |
| L-23 | 静态页面加固建议交由正在修改 HTML 的 hy；本次不修改其文件。 |
| L-24 | 相对 token 路径已修；SCM 错误上报可改善。卸载保留凭据有利重装，不能自动删除用户自定义 token，未照搬删除建议。 |
| L-25 | PATH 依赖已修。**账户包含空格需额外拼引号的说法不成立**：Command.arg 传独立 argv，手拼引号反可能产生错误。 |
| L-26 | 标签保护/审批/并发属于仓库发布策略，未发现未授权触发。本次不替用户配置 environment 审批。 |
| L-27/L-28 | 不可达分支和残留英文已删除。 |
| L-29 | 多版本传递依赖不等于 CVE，结论合理；未做在线漏洞库完整扫描。 |

## 本次验证与剩余限制

- 本地源码 `cargo test --workspace --locked --offline`：14 项通过；`cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过。
- `node --test web/tests/reconnect.test.cjs`：9 项通过；`npm run build` 成功，嵌入资产重新生成。
- 新构建 debug 服务：7 项 security regression 通过，完整 smoke 通过（认证、输入、PING/PONG、重新附着、历史回放）。
- 本地 v1.8.3 Windows GNU release 构建成功，发布版再次通过 7 项协议回归和 smoke；`--service --config <临时 JSON>` 到达 dispatcher，非 SCM 环境按预期返回 1063，已不再报 INI 解析错误。
- Android 资源由 aapt2 编译链接，新 Java 源码使用 JDK21 `--release 11` 与 API37 android.jar 编译通过，Manifest 仍 targetSdk35/minSdk26。未据此宣称默认浏览器解析在真机全通过。
- 没有在本机运行 macOS；Linux/macOS/Android JNI 最终发布构建须由对应 CI 验证。未做管理员 SCM 实启、Android 真机生命周期、模糊测试或长时压力测试。
- 原生 PTY 创建仍可能卡在系统调用，移动出 manager 锁能隔离其他会话操作，但不等于整个运行时支持强制取消阻塞线程。Windows Job Object、Unix leader 被回收后仍存活的后代进程组清理，均未作完整保证；主动脱离控制的 daemon 不属于当前约束模型。
- 本文明确保留的兼容行为/工程加固项不标记为“全部修复”，也不重写前一日 AUDIT 的历史验证。
