#!/usr/bin/env bash
# 한컴오피스 Linux Beta 로컬 게이트.
#
# 우리 엔진이 만든 HWPX를 실제 한컴으로 열어 보고 통과/실패를 판정한다.
# Windows 한컴 검증을 대체하지 않는다 — 같은 제품 계열의 다른 빌드(2020 Beta)이며
# 사용자의 Windows 한컴오피스 2024와 파서 세대가 다르다. 판정 근거와 한계는
# docs/decisions/engine-output-open-verification-2026-10-17.md 를 본다.
#
# 사용법:
#   scripts/hancom_gate.sh <HNC_BIN_DIR> <file.hwpx>...
#   HNC_BIN 예: /path/to/hoffice11/Bin   (hwp 실행 파일과 그 형제 .so들이 있는 디렉터리)
#
# 판정:
#   PASS  문서 로드 후 프로세스가 살아 있고 SIGSEGV가 없다
#   FAIL  SIGSEGV 등으로 종료 (rc 139)
# 주의:
#   - offscreen 플랫폼만 쓴다. VNC 플랫폼은 클라이언트 접속 시 플러그인 자체가
#     죽으므로(정상 문서도 죽는다) 종료 코드 판정에 쓸 수 없다.
#   - 이 스크립트는 "열렸는가"만 본다. "내용이 올바르게 그려졌는가"는 화면 캡처로
#     사람이 확인해야 한다. 빈 문서로 표시되는 경우가 있기 때문이다.
set -u
BIN="${1:?한컴 Bin 디렉터리 경로}"
shift
[ $# -gt 0 ] || { echo "검사할 .hwpx 파일을 하나 이상 지정하세요" >&2; exit 2; }
[ -x "$BIN/hwp" ] || { echo "실행 파일을 찾을 수 없음: $BIN/hwp" >&2; exit 2; }

WAIT="${HANCOM_GATE_WAIT:-15}"
fail=0
printf "%-46s %s\n" "file" "verdict"
for f in "$@"; do
  # 상대 경로를 그대로 넘기면 아래 cd 때문에 실행 파일이 파일을 찾지 못하고
  # "빈 문서"로 열려 통과로 오판한다. 반드시 절대 경로로 바꾼다.
  if [ -f "$f" ]; then f=$(realpath "$f"); else
    printf "%-46s %s\n" "$(basename "$f")" "SKIP(파일 없음)"; continue
  fi
  W=$(mktemp -d "${TMPDIR:-/tmp}/hancom-gate.XXXXXX"); mkdir -p "$W/home"
  ( cd "$BIN" && exec env HOME="$W/home" LD_LIBRARY_PATH="$BIN" \
      QT_QPA_PLATFORM=offscreen ./hwp "$f" ) >"$W/out" 2>"$W/err" </dev/null &
  pid=$!
  sleep "$WAIT"
  if kill -0 "$pid" 2>/dev/null; then
    printf "%-46s %s\n" "$(basename "$f")" "PASS(생존)"
  else
    wait "$pid" 2>/dev/null
    rc=$?
    printf "%-46s %s\n" "$(basename "$f")" "FAIL(종료 rc=$rc)"
    fail=1
  fi
  kill -9 "$pid" 2>/dev/null
  rm -rf "$W"
done
exit "$fail"
