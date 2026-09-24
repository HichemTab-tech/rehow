#!/usr/bin/env bash
set -euo pipefail

proxy="$(pwd)/target/debug/rehow-git-credential"
test -x "$proxy"
fixture_dir="$(mktemp -d)"
trap 'rm -rf -- "$fixture_dir"' EXIT
fixture_helper="$fixture_dir/git-credential-fixture"
cat >"$fixture_helper" <<'EOF'
#!/bin/sh
if [ "$1" = get ]; then
  printf 'protocol=https\nhost=rehow.invalid\nusername=rehow-fixture\npassword=credential-canary\n\n'
fi
EOF
chmod 0700 "$fixture_helper"

output="$({
  printf 'protocol=https\nhost=rehow.invalid\n\n'
} | /usr/bin/bwrap \
  --die-with-parent \
  --new-session \
  --unshare-all \
  --ro-bind /usr /usr \
  --symlink usr/bin /bin \
  --symlink usr/lib /lib \
  --symlink usr/lib64 /lib64 \
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

output="$({
  printf 'protocol=https\nhost=rehow.invalid\n\n'
} | /usr/bin/bwrap \
  --die-with-parent \
  --new-session \
  --unshare-all \
  --ro-bind /usr /usr \
  --symlink usr/bin /bin \
  --symlink usr/lib /lib \
  --symlink usr/lib64 /lib64 \
  --proc /proc \
  --dev /dev \
  --tmpfs /tmp \
  --dir /home \
  --dir /home/rehow \
  --dir /home/rehow/bin \
  --ro-bind "$proxy" /home/rehow/bin/rehow-git-credential \
  --ro-bind "$fixture_helper" /home/rehow/bin/credential-helper \
  -- /usr/bin/env -i \
  REHOW_CREDENTIAL_HOST=rehow.invalid \
  REHOW_CREDENTIAL_HELPER_KIND=direct \
  /home/rehow/bin/rehow-git-credential get)"
test "$output" = $'protocol=https\nhost=rehow.invalid\nusername=rehow-fixture\npassword=credential-canary'
unset output

output="$({
  printf 'protocol=https\nhost=rehow.invalid\n\n'
} | /usr/bin/bwrap \
  --die-with-parent \
  --new-session \
  --unshare-all \
  --ro-bind /usr /usr \
  --symlink usr/bin /bin \
  --symlink usr/lib /lib \
  --symlink usr/lib64 /lib64 \
  --proc /proc \
  --dev /dev \
  --tmpfs /tmp \
  --dir /home \
  --dir /home/rehow \
  --dir /home/rehow/bin \
  --ro-bind "$proxy" /home/rehow/bin/rehow-git-credential \
  --ro-bind "$fixture_helper" /home/rehow/bin/credential-helper \
  -- /usr/bin/env -i \
  HOME=/home/rehow \
  GIT_CONFIG_NOSYSTEM=1 \
  GIT_CONFIG_GLOBAL=/dev/null \
  GIT_TERMINAL_PROMPT=0 \
  REHOW_CREDENTIAL_HOST=rehow.invalid \
  REHOW_CREDENTIAL_HELPER_KIND=direct \
  /usr/bin/git \
  -c credential.helper= \
  -c credential.helper=/home/rehow/bin/rehow-git-credential \
  -c credential.interactive=false \
  credential fill)"
test "$output" = $'protocol=https\nhost=rehow.invalid\nusername=rehow-fixture\npassword=credential-canary'
unset output

set +e
output="$(/usr/bin/env -i "$proxy" unexpected 2>&1)"
status=$?
set -e
test "$status" -eq 255
test -z "$output"
