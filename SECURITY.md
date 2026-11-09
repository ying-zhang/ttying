# Security Policy

See [deployment and authentication boundaries](DEPLOYMENT-SECURITY.zh-CN.md) for
Origin allowlists, cookie revocation, Android browser trust and systemd preparation.

## Reporting a vulnerability

Please use GitHub's private vulnerability reporting or a private security advisory for this repository. Do not disclose an exploitable issue in a public issue before a fix is available.

Include:

- affected version or commit;
- operating system and architecture;
- whether the server was bound to loopback or a non-loopback address;
- a minimal reproduction;
- expected and observed behavior.

## Deployment assumptions

- ttying does not provide TLS. Terminate TLS at a trusted reverse proxy or use a VPN.
- HTTPS deployments must use `--secure-cookie`; this also makes Origin validation expect an `https://` origin.
- Keep the token file and release signing materials outside version control and readable only by the service account.
- The spawned shell is not a sandbox. Run ttying under the least-privileged account that is appropriate for the use case.

## Known design limits

- Reconnect output replay is bounded raw output history, not a complete VT screen snapshot.
- Deliberately daemonized descendants that create a new process session may escape process-group cleanup.
- The project does not claim complete CVE coverage for third-party dependencies.
