#!/usr/bin/env bash
set -euo pipefail

maximum_glibc="GLIBC_2.39"

if [ "$#" -eq 0 ]; then
  echo "usage: scripts/check-linux-abi-floor.sh BINARY..." >&2
  exit 2
fi

for binary_path in "$@"; do
  test -x "$binary_path"
  observed_glibc="$({
    readelf --version-info "$binary_path" |
      sed -n 's/.*Name: \(GLIBC_[0-9.]*\).*/\1/p'
  } | sort -V | tail -n 1)"

  if [ -z "$observed_glibc" ]; then
    echo "$binary_path has no readable GLIBC version requirements" >&2
    exit 1
  fi

  highest_glibc="$(printf '%s\n%s\n' "$maximum_glibc" "$observed_glibc" | sort -V | tail -n 1)"
  if [ "$highest_glibc" != "$maximum_glibc" ]; then
    echo "$binary_path requires $observed_glibc, above supported floor $maximum_glibc" >&2
    exit 1
  fi

  echo "$binary_path: $observed_glibc"
done
