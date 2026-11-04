#!/usr/bin/env bash
# Copyright (c) 2025-2026 3 Leaps, LLC. SPDX-License-Identifier: MIT OR Apache-2.0
# Synthetic signed-tag controls for both spec-vX.Y and vX.Y.Z release tags.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
export PYTHONDONTWRITEBYTECODE=1
bash "$ROOT/scripts/release-tag-controls.test.sh"
bash "$ROOT/scripts/verify-pinned-tag.test.sh"
echo 'signed-tag controls passed'
