# 安全策略

中文 | [English](SECURITY.md)

本文件仅作为 GitHub 安全策略的入口。权威的安全模型、部署与认证边界、systemd 首次部署和已知限制见 [README](README.md#安全模型)。

## 漏洞报告

请使用 GitHub 的私有漏洞报告功能或本仓库的安全公告（Security Advisory）。在修复版本发布前，不要在公开 issue 中披露可利用的问题。

报告中请包含：

- 受影响的版本或 commit；
- 操作系统与架构；
- 服务器绑定的是回环地址还是非回环地址；
- 最小复现步骤；
- 预期行为与实际行为。

## 适用范围

ttying 不提供 TLS，也不是沙箱：启动的 shell 使用 ttying 进程的权限运行。应在可信反向代理终止 TLS 并以 `--secure-cookie` 启动，将 token 与签名材料排除在版本控制之外，并使用最小权限账户运行服务。不受支持的部署方式和设计限制见[限制](README.md#限制)。
