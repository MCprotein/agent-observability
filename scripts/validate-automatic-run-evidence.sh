#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: validate-automatic-run-evidence.sh <manifest>" >&2
  exit 2
fi

awk '
  function unsigned_integer(value) {
    return value ~ /^[0-9]+$/
  }
  $1 == "report_generation:" {
    generations += 1
    if (NF != 2 || !unsigned_integer($2) || $2 != 20001) bad = 1
  }
  $1 == "report_records:" {
    records += 1
    if (NF != 2 || !unsigned_integer($2) || $2 != 20001) bad = 1
  }
  $1 == "rss_samples:" {
    samples += 1
    if (NF != 2 || !unsigned_integer($2) || $2 < 100) bad = 1
  }
  $1 == "rss_observed_max_gap_ms:" {
    gaps += 1
    if (NF != 2 || !unsigned_integer($2) || $2 > 100) bad = 1
  }
  END {
    if (generations != 5 || records != 5 || samples != 5 || gaps != 5 || bad) exit 1
  }
' "$1"
