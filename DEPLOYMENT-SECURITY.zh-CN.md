# 部署与认证边界

`--allowed-origin` 非空时是排他的浏览器 Origin 白名单，不再同时放行与 Host 相同的其他 Origin。未配置时按 HTTP(S) scheme 和 Host 校验。非浏览器客户端可以不带 Origin，但仍需认证。反向代理应拒绝非预期 Host；公网部署应显式配置 Origin 白名单。Host 比较本身不提供 DNS rebinding 防护。

认证 cookie 默认有效期 24 小时，`POST /api/session` 续期。注销撤销该 cookie 及其 WebSocket，不撤销共享主 token，也不终止 PTY。Bearer token 直连不受 cookie 注销控制；撤销主 token 需要更换 token 并重启服务。认证表最多 4096 条，满时淘汰最早到期 cookie，被淘汰客户端需要重新认证；正确 token 仍可登录。失败限流表满时拒绝新来源的错误登录，正确凭据不受影响。

Android 仅把带 fragment 的引导 URL 发给设置中的默认浏览器包；没有明确默认浏览器时提示用户先配置。浏览器和扩展能看到 fragment，必须信任它们。fragment 不进入 HTTP 请求不等于对接收 URL 的应用保密。原生 App 内先进行 HTTP 登录不能写入外部浏览器的 cookie 仓库。

正常启动在 stdout 被重定向时不输出凭据链接；`--print-url` 明确请求输出完整链接。保护 token 文件及其父目录。Windows 前台启动保留已有显式 ACL，包括服务账户的读取权限，并关闭继承；管理员仍需移除不可信账户的显式读取授权。不要将运行 token、签名密钥或密码放入版本控制。签名材料宜存放在工作树外，通过已文档化的环境变量或秘密管理器提供密码。本次修复不移动已有私人签名文件、不修改 CI secrets。

## systemd 首次部署

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
