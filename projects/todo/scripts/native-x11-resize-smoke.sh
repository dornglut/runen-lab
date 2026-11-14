#!/usr/bin/env bash
set -Eeuo pipefail

# Independent physical-X11 window-resize admission smoke. The test exercises
# the real native app and WM, not a headless RunenUI surface publication.
# Actual UI reflow is checked separately by layout_tests.rs, and pixel-level
# native visual qualification remains distinct.
binary="$PWD/projects/todo/target/release/todo"
[[ -x "$binary" && -n "${DISPLAY:-}" ]] || {
  echo "todo_native_resize=fail stage=precondition" >&2
  exit 1
}
scratch="$(mktemp -d)"
app_pid=""
wm_pid=""
cleanup() {
  if [[ -n "$app_pid" ]] && kill -0 "$app_pid" 2>/dev/null; then
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
  fi
  if [[ -n "$wm_pid" ]] && kill -0 "$wm_pid" 2>/dev/null; then
    kill "$wm_pid" 2>/dev/null || true
    wait "$wm_pid" 2>/dev/null || true
  fi
  rm -rf -- "$scratch"
}
trap cleanup EXIT
fail() {
  echo "todo_native_resize=fail stage=$1" >&2
  [[ ! -f "$scratch/todo.log" ]] || tail -n 80 "$scratch/todo.log" >&2
  exit 1
}
export RUNEN_LAB_TODO_DATA_FILE="$scratch/tasks.json"
openbox --sm-disable >"$scratch/openbox.log" 2>&1 &
wm_pid=$!
sleep 1
kill -0 "$wm_pid" 2>/dev/null || fail window-manager-startup

if [[ -n "${RUNEN_LAB_TODO_PROFILE_ROOT:-}" ]]; then
  RUNENWERK_PROFILE_OUTPUT="${RUNEN_LAB_TODO_PROFILE_ROOT}/resize.json" "$binary" >"$scratch/todo.log" 2>&1 &
else
  "$binary" >"$scratch/todo.log" 2>&1 &
fi
app_pid=$!
window=""
for _ in $(seq 1 80); do
  kill -0 "$app_pid" 2>/dev/null || fail app-startup-exit
  window="$(xdotool search --onlyvisible --name '^Runen Lab - Todo$' 2>/dev/null | head -n 1 || true)"
  [[ -z "$window" ]] || break
  sleep 0.2
done
[[ -n "$window" ]] || fail window-not-realized
xdotool windowfocus --sync "$window" || fail window-focus
sleep 1

# Exercise multiple original native resize requests, including contraction.
# Do not click during resize: native input qualification is a separate test.
for dimensions in 520x480 960x650 800x600; do
  width="${dimensions%x*}"
  height="${dimensions#*x}"
  xdotool windowsize --sync "$window" "$width" "$height" || fail "resize-request-$dimensions"
  matched=false
  for _ in $(seq 1 50); do
    kill -0 "$app_pid" 2>/dev/null || fail "app-exited-after-$dimensions"
    geometry="$(xdotool getwindowgeometry --shell "$window")" || fail "geometry-$dimensions"
    actual_width="$(printf '%s\n' "$geometry" | sed -n 's/^WIDTH=//p')"
    actual_height="$(printf '%s\n' "$geometry" | sed -n 's/^HEIGHT=//p')"
    if [[ "$actual_width" == "$width" && "$actual_height" == "$height" ]]; then
      matched=true
      break
    fi
    sleep 0.1
  done
  if [[ "$matched" != true ]]; then
    echo "todo_native_resize=diagnostic requested=$dimensions observed=${actual_width}x${actual_height}" >&2
    fail "geometry-not-updated-$dimensions"
  fi
  sleep 0.3
  kill -0 "$app_pid" 2>/dev/null || fail "app-exited-after-settle-$dimensions"
  echo "todo_native_resize=pass requested=$dimensions observed=${actual_width}x${actual_height} app_alive=true"
done

# Profiled proof needs an ordinary WM close so the real Engine exporter
# can complete. Preserve the original resize-only behavior without opt-in.
if [[ -n "${RUNEN_LAB_TODO_PROFILE_ROOT:-}" ]]; then
  xdotool windowfocus --sync "$window" || fail "profiled-resize-window-focus"
  xdotool key --clearmodifiers Alt+F4 || fail "profiled-resize-wm-close"
  for _ in $(seq 1 80); do
    if ! kill -0 "$app_pid" 2>/dev/null; then
      status=0
      wait "$app_pid" || status=$?
      app_pid=""
      [[ "$status" -eq 0 ]] || fail "profiled-resize-close-exit-$status"
      break
    fi
    sleep 0.2
  done
  [[ -z "$app_pid" ]] || fail "profiled-resize-close-timeout"
  [[ -s "$RUNEN_LAB_TODO_PROFILE_ROOT/resize.json" ]] || fail "profiled-resize-no-capture"
  echo "todo_native_resize=pass profile_capture=complete"
fi
