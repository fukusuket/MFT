# tool (working name)

Fast forensic triage of Windows NTFS using only `$MFT` and `$UsnJrnl:$J`. It subtracts a Windows baseline and focuses on a time window to surface the files and file activity worth investigating, even when event logs were wiped. Early development.

- Small collection (tens to hundreds of MB); one self-contained HTML report
- Detection by Sigma rules with fixed levels; no computed scores

See [docs/product.md](docs/product.md). Contributing: [AGENTS.md](AGENTS.md).

## License
- Code: [AGPL-3.0-only](LICENSE)
- Detection rules: [DRL 1.1](https://github.com/SigmaHQ/Detection-Rule-License)
