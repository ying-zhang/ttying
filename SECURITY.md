# Security Policy

[中文](SECURITY.zh-CN.md) | English

This file is the short entry point for GitHub's security policy. The authoritative security model, deployment and authentication boundaries, systemd setup, and known limitations live in the [README](README.en-US.md#security-model).

## Reporting a vulnerability

Use GitHub's private vulnerability reporting or a private security advisory for this repository. Do not disclose an exploitable issue in a public issue before a fix is available.

Please include:

- affected version or commit;
- operating system and architecture;
- whether the server was bound to loopback or a non-loopback address;
- a minimal reproduction;
- expected and observed behavior.

## Scope

ttying provides no TLS and is not a sandbox: the spawned shell runs with the privileges of the ttying process. Terminate TLS at a trusted reverse proxy and start ttying with `--secure-cookie`, keep token and signing material out of version control, and run the service under a least-privileged account. Unsupported deployment patterns and design limits are listed under [Limitations](README.en-US.md#limitations).
