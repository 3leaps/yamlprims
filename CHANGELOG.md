# Changelog

All notable changes to the `yamlprims` code and package releases are documented in this file.
Specification-tag release notes are maintained in [RELEASE_NOTES.md](RELEASE_NOTES.md), and per-version code release notes live under `docs/releases/v<VERSION>.md`.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and package releases adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Gold-standard release, signing, verification, and crate-packaging scripts and tests (`scripts/release-*.sh`, `scripts/release-*.py`, `scripts/check-packages.sh`, `scripts/deps-check.sh`, `scripts/doctor.py`) supporting dual `v<VERSION>` and `spec-v<SPEC_VERSION>` tag families.
- Multi-platform release and crates verification workflows (`.github/workflows/release.yml`, `.github/workflows/crates-verify.yml`), `VERSION` (`0.1.0`), `NOTICE`, `deny.toml`, `.fulmen/repo.yaml`, and `RELEASE_CHECKLIST.md`.
