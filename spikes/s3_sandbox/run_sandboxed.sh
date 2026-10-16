#!/usr/bin/env bash
# S3 spike: run a command inside an unprivileged bubblewrap sandbox.
# Usage: run_sandboxed.sh INPUT_DIR OUTPUT_DIR FONT_DIR -- CMD ARGS...
# Inside: /in (ro), /out (rw), /fonts (ro), /usr,/lib,/lib64,/etc/fonts (ro), fresh /tmp, no network, no home.
set -euo pipefail
IN="$1"; OUT="$2"; FONTS="$3"; shift 3; [ "$1" = "--" ] && shift
args=(--unshare-all --die-with-parent --new-session --clearenv
  --setenv PATH /usr/bin:/bin --setenv HOME /nonexistent --setenv LANG C.UTF-8
  --ro-bind /usr /usr --symlink usr/bin /bin --symlink usr/lib /lib --symlink usr/lib64 /lib64 --symlink usr/bin /sbin
  --ro-bind-try /etc/fonts /etc/fonts --ro-bind-try /etc/ld.so.cache /etc/ld.so.cache
  --proc /proc --dev /dev --tmpfs /tmp
  --ro-bind "$IN" /in --bind "$OUT" /out --ro-bind "$FONTS" /fonts
  --chdir /tmp)
exec bwrap "${args[@]}" -- "$@"

