# Security Policy

## Supported versions

Security fixes are applied to the latest released version of Cannon. Please
update (`cannon --update`) before reporting.

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Use GitHub's private vulnerability reporting: open the repository's **Security**
tab and choose **Report a vulnerability**. Include:

- the Cannon version (`cannon --version`) and your OS/architecture;
- the command or configuration that triggers the problem;
- the impact you observed and, if possible, a minimal reproduction.

You can expect an acknowledgement within a few days. Fixes are released as soon
as practical and credited to the reporter unless you prefer otherwise.

## Scope notes

- Cannon is a load generator: pointing it at systems you do not own or have
  permission to test is your responsibility.
- `--insecure` disables TLS certificate verification by design; do not use it
  against production endpoints you do not control.
- Release binaries ship with a `SHA256SUMS` file, and `install.sh` verifies it
  before installing. The checksum only detects corruption: it comes from the
  same release as the binary.
- From the next release on, binaries and `SHA256SUMS` carry a signed build
  provenance attestation (Sigstore, produced by the release workflow). To check
  that a binary was built from this repository's workflow:

  ```bash
  gh attestation verify cannon-linux-x64 --repo FelipeFelipeRenan/cannon-project
  ```

  `cannon --update` does not verify this yet; if that matters for you, download
  the binary and verify it manually.
