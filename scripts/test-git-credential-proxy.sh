#!/usr/bin/env bash
set -euo pipefail

proxy="$(pwd)/target/debug/rehow-git-credential"
test -x "$proxy"

bwrap_probe_output=""
bwrap_probe_status=0
set +e
bwrap_probe_output="$(/usr/bin/bwrap \
  --die-with-parent \
  --new-session \
  --unshare-all \
  --ro-bind /usr /usr \
  --symlink usr/bin /bin \
  --symlink usr/lib /lib \
  --symlink usr/lib /lib64 \
  --proc /proc \
  --dev /dev \
  --tmpfs /tmp \
  -- /usr/bin/true 2>&1)"
bwrap_probe_status=$?
set -e

if [ "$bwrap_probe_status" -ne 0 ]; then
  case "$bwrap_probe_output" in
    *"bwrap: loopback: Failed RTM_NEWADDR: Operation not permitted"*)
      if [ "${REHOW_ALLOW_BWRAP_NETWORK_NAMESPACE_SKIP:-0}" = "1" ]; then
        printf '%s\n' \
          'SKIP: Bubblewrap network namespaces are unavailable; credential-proxy sandbox smoke was not run.' \
          >&2
        exit 0
      fi
      ;;
  esac

  printf '%s\n' "$bwrap_probe_output" >&2
  exit "$bwrap_probe_status"
fi

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

output="$({
  printf 'protocol=https\nhost=rehow.invalid\n\n'
} | /usr/bin/bwrap \
  --die-with-parent \
  --new-session \
  --unshare-all \
  --ro-bind /usr /usr \
  --symlink usr/bin /bin \
  --symlink usr/lib /lib \
  --symlink usr/lib /lib64 \
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
  --symlink usr/lib /lib64 \
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
