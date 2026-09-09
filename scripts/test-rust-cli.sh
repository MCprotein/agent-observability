#!/usr/bin/env bash
# Scoped resource profile for parallel CLI tests, not a product runtime requirement.
set -euo pipefail

(
  if (( $# != 0 )); then
    echo "CLI test harness accepts no arguments; it runs the complete CLI package." >&2
    exit 64
  fi
  required_fds=1024
  hard_fds=$(ulimit -Hn)
  soft_fds=$(ulimit -Sn)
  if [[ "$hard_fds" != unlimited ]] && (( hard_fds < required_fds )); then
    echo "CLI test harness requires a hard file-descriptor limit of at least 1024 for 32 test threads." >&2
    exit 1
  fi
  if [[ "$soft_fds" != unlimited ]] && (( soft_fds < required_fds )); then
    if ! ulimit -Sn "$required_fds"; then
      echo "CLI test harness could not establish its 1024-descriptor prerequisite." >&2
      exit 1
    fi
  fi
  exec cargo +1.97.0 test --locked -p agent-observability-cli --no-fail-fast -- --test-threads=32
)
