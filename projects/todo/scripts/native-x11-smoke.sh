#!/usr/bin/env bash
set -Eeuo pipefail

# This test runs the actual built Release Todo binary under a virtual X11
# server. It uses original XTEST mouse/keyboard events, not RunenUI ingress
# injection or an Engine-private fixture.
binary="$PWD/projects/todo/target/release/todo"
if [[ ! -x "$binary" ]]; then
  echo "Expected Release Todo binary missing: $binary" >&2
  exit 1
fi
if [[ -z "$DISPLAY" ]]; then
  echo "A real X11 DISPLAY is required for this qualification" >&2
  exit 1
fi

scratch="$(mktemp -d)"
todo_pid=""
wm_pid=""
run=0
window=""
export RUNEN_LAB_TODO_DATA_FILE="$scratch/tasks.json"

cleanup() {
  if [[ -n "$todo_pid" ]] && kill -0 "$todo_pid" 2>/dev/null; then
    kill "$todo_pid" 2>/dev/null || true
    wait "$todo_pid" 2>/dev/null || true
  fi
  if [[ -n "$wm_pid" ]] && kill -0 "$wm_pid" 2>/dev/null; then
    kill "$wm_pid" 2>/dev/null || true
    wait "$wm_pid" 2>/dev/null || true
  fi
  rm -rf -- "$scratch"
}
trap cleanup EXIT

fail() {
  echo "todo_native_x11=fail stage=$1" >&2
  for log in "$scratch"/todo-*.log; do
    if [[ -f "$log" ]]; then
      echo "=== Todo process log: $(basename "$log") ===" >&2
      cat -- "$log" >&2
    fi
  done
  if [[ -f "$scratch/openbox.log" ]]; then
    echo "=== X11 window-manager log ===" >&2
    tail -n 30 "$scratch/openbox.log" >&2
  fi
  exit 1
}

start_todo() {
  run=$((run + 1))
  "$binary" >"$scratch/todo-$run.log" 2>&1 &
  todo_pid=$!
  window=""
  for _ in $(seq 1 100); do
    if ! kill -0 "$todo_pid" 2>/dev/null; then
      wait "$todo_pid" || true
      fail "startup-exit-$run"
    fi
    window="$(xdotool search --onlyvisible --name '^Runen Lab - Todo$' 2>/dev/null | head -n 1 || true)"
    if [[ -n "$window" ]]; then
      xdotool windowfocus --sync "$window" || fail "focus-$run"
      xdotool getwindowgeometry "$window"
      # Window realization does not prove the first UI Present; give the
      # native renderer a bounded stabilization interval before XTEST input.
      sleep 2
      return 0
    fi
    sleep 0.2
  done
  fail "window-not-realized-$run"
}

assert_alive() {
  if ! kill -0 "$todo_pid" 2>/dev/null; then
    wait "$todo_pid" || true
    fail "$1"
  fi
}

state_matches() {
  python3 - "$RUNEN_LAB_TODO_DATA_FILE" "$1" <<'PY'
import json
import sys
from pathlib import Path

try:
    data = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    tasks = data["tasks"]
    expected = sys.argv[2]
    ok = (
        data["version"] == 1
        and len(tasks) == 1
        and tasks[0]["id"] == 1
        and tasks[0]["title"] == "x11"
        and tasks[0]["completed"] is (expected == "complete")
    )
except (OSError, ValueError, KeyError, TypeError):
    ok = False
sys.exit(0 if ok else 1)
PY
}

await_state() {
  for _ in $(seq 1 80); do
    assert_alive "$2-exited"
    if state_matches "$1"; then
      return 0
    fi
    sleep 0.2
  done
  fail "$2-no-persisted-state"
}

close_todo() {
  xdotool windowclose "$window" || fail "wm-close-request-$run"
  for _ in $(seq 1 80); do
    if ! kill -0 "$todo_pid" 2>/dev/null; then
      local status=0
      wait "$todo_pid" || status=$?
      todo_pid=""
      if [[ "$status" -ne 0 ]]; then
        fail "native-close-exit-$status"
      fi
      return 0
    fi
    sleep 0.2
  done
  fail "native-close-timeout-$run"
}

openbox --sm-disable >"$scratch/openbox.log" 2>&1 &
wm_pid=$!
sleep 1
kill -0 "$wm_pid" 2>/dev/null || fail "window-manager-startup"

start_todo
# Coordinates are relative to the client window: the real published Todo
# layout reserves a 240px-wide editable on the second logical row.
xdotool mousemove --window "$window" 40 32 click 1
sleep 0.5
assert_alive "text-field-press"
xdotool type --clearmodifiers --delay 80 "x11"
sleep 1
assert_alive "text-input"
# The real Add button is immediately to the right of the 240px editable.
xdotool mousemove --window "$window" 255 32 click 1
await_state "open" "add-button"
echo "todo_native_x11=pass phase=create-input-and-persist"
close_todo

# Same persisted path, new physical process and X11 window.
start_todo
assert_alive "restart"
state_matches "open" || fail "restart-lost-task"
# Task row follows the input row. Click its first Complete/Reopen button.
xdotool mousemove --window "$window" 38 54 click 1
await_state "complete" "complete-button"
echo "todo_native_x11=pass phase=restart-and-complete"
close_todo
echo "todo_native_x11=pass physical_x11=true mode=release persistence=true"
