#!/usr/bin/env bash
set -euo pipefail

proxy="$(pwd)/target/debug/rehow-git-credential"
test -x "$proxy"

output="$({
  printf 'protocol=https\nhost=rehow.invalid\n\n'
} | /usr/bin/bwrap \
  --die-with-parent \
  --new-session \
  --unshare-all \
  --ro-bind /usr /usr \
  --symlink usr/lib /lib \
  --symlink usr/lib /lib64 \
  --proc /proc \
  --dev /dev \
  --tmpfs /tmp \
  --dir /home \
  --dir /home/rehow \
  --dir /home/rehow/bin \
  --ro-bind "$proxy" /home/rehow/bin/rehow-git-credential \
  --ro-bind /usr/bin/true /home/rehow/bin/credential-helper \
  -- /usr/bin/env -i \
  REHOW_CREDENTIAL_HOST=rehow.invalid \
  REHOW_CREDENTIAL_HELPER_KIND=direct \
  /home/rehow/bin/rehow-git-credential get)"
test -z "$output"

set +e
output="$(/usr/bin/env -i "$proxy" unexpected 2>&1)"
status=$?
set -e
test "$status" -eq 255
test -z "$output"
