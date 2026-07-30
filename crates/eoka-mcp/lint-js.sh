#!/bin/bash
# Lint JS files in src/js/ — requires biome installed in CI
set -euo pipefail
cd "$(dirname "$0")"
npx --yes @biomejs/biome check src/js/
