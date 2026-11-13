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
    if expected == "empty":
        ok = data["version"] == 1 and data["next_id"] == 2 and tasks == []
    elif expected in ("open", "complete"):
        ok = (
            data["version"] == 1
            and len(tasks) == 1
            and tasks[0]["id"] == 1
            and tasks[0]["title"] == "x11"
            and tasks[0]["completed"] is (expected == "complete")
        )
    else:
        ok = False
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

# Preserve each failed close as a failure; a test-owned forced stop is
# only used to isolate the next original native gesture and restart trial.
stop_test_process() {
  if [[ -n "$todo_pid" ]] && kill -0 "$todo_pid" 2>/dev/null; then
    kill "$todo_pid" 2>/dev/null || true
    wait "$todo_pid" 2>/dev/null || true
  fi
  todo_pid=""
}

close_todo() {
  local method="$1"
  # Ubuntu 24.04's xdotool windowclose destroys an X11 window; it is NOT
  # a genuine WM_DELETE_WINDOW/CloseRequested qualification. Openbox's
  # Alt+F4 sends a window-manager close intent through the X11 OS path.
  if ! xdotool windowfocus --sync "$window" ||
     ! xdotool key --clearmodifiers Alt+F4; then
    echo "todo_native_x11=fail stage=$method-wm-close-request" >&2
    stop_test_process
    return 1
  fi
  for _ in $(seq 1 80); do
    if ! kill -0 "$todo_pid" 2>/dev/null; then
      local status=0
      wait "$todo_pid" || status=$?
      todo_pid=""
      if [[ "$status" -ne 0 ]]; then
        echo "todo_native_x11=fail stage=$method-close-exit status=$status" >&2
        return 1
      fi
      echo "todo_native_x11=pass phase=$method-close exit=0"
      return 0
    fi
    sleep 0.2
  done
  local exists=false
  local visible=false
  if xdotool getwindowname "$window" >/dev/null 2>&1; then
    exists=true
  fi
  if xdotool search --onlyvisible --name '^Runen Lab - Todo$' 2>/dev/null |
       grep -qx "$window"; then
    visible=true
  fi
  echo "todo_native_x11=fail stage=$method-close-timeout window_exists=$exists window_visible=$visible" >&2
  stop_test_process
  return 1
}
# The geometry fixture uses the accepted public RunenUI surface publication;
# it is not a replacement for the XTEST event sequence below.
[[ -f "${TODO_NATIVE_TARGETS_FILE:-}" ]] || fail "native-geometry-missing"
declare -A target_x target_y
while read -r label name x y; do
  [[ "$label" == "TODO_NATIVE_TARGET" && "$x" =~ ^[0-9]+$ && "$y" =~ ^[0-9]+$ ]] || fail "native-geometry-invalid"
  target_x["$name"]="$x"
  target_y["$name"]="$y"
done < "$TODO_NATIVE_TARGETS_FILE"
for name in field add toggle remove reopen; do
  [[ -n "${target_x[$name]:-}" && -n "${target_y[$name]:-}" ]] || fail "native-geometry-incomplete"
  echo "todo_native_x11=target name=$name x=${target_x[$name]} y=${target_y[$name]}"
done

openbox --sm-disable >"$scratch/openbox.log" 2>&1 &
wm_pid=$!
sleep 1
kill -0 "$wm_pid" 2>/dev/null || fail "window-manager-startup"

start_todo
# Coordinates are measured through the accepted RunenUI public publication
# and matched against original native OS input on the exact same feature head.
xdotool mousemove --window "$window" "${target_x[field]}" "${target_y[field]}" click 1
sleep 0.5
assert_alive "text-field-press"
xdotool type --clearmodifiers --delay 80 "x11"
sleep 1
assert_alive "text-input"
xdotool mousemove --window "$window" "${target_x[add]}" "${target_y[add]}" click 1
await_state "open" "add-button"
echo "todo_native_x11=pass phase=create-input-and-persist"
close_failures=0
if ! close_todo after-edit; then
  close_failures=$((close_failures + 1))
fi

# First restart: isolate the first actual pointer event on a restored UI
# before touching its task row. This distinguishes an initial M13 recipient
# failure from a Complete-action failure; it does not bypass native ingress.
start_todo
assert_alive "restart"
state_matches "open" || fail "restart-lost-task"
echo "todo_native_x11=pass phase=restart-read-persisted"
xdotool mousemove --window "$window" "${target_x[field]}" "${target_y[field]}" click 1
sleep 0.5
assert_alive "restored-field-click"
echo "todo_native_x11=pass phase=restored-field-click"
# Require the actual published native-font target to work as the first
# original XTEST click. No coordinate search, retry, or UI-path bypass.
xdotool mousemove --window "$window" "${target_x[toggle]}" "${target_y[toggle]}" click 1
await_state "complete" "complete-button"
echo "todo_native_x11=pass phase=restored-task-completed"
if ! close_todo after-complete; then
  close_failures=$((close_failures + 1))
fi

# Second restart: verify the completed state survives another native process.
# An idle WM close is independently tested after reloading the completed data.
start_todo
assert_alive "completed-restart"
state_matches "complete" || fail "completed-restart-lost-task"
echo "todo_native_x11=pass phase=completed-state-reloaded"
if ! close_todo after-restart; then
  close_failures=$((close_failures + 1))
fi

# A fresh persisted completed task must support the genuine Reopen and
# Remove controls through one original native XTEST click each.
start_todo
state_matches "complete" || fail "reopen-restart-lost-completed-state"
xdotool mousemove --window "$window" "${target_x[reopen]}" "${target_y[reopen]}" click 1
await_state "open" "reopen-button"
echo "todo_native_x11=pass phase=native-reopen-persisted"
xdotool mousemove --window "$window" "${target_x[remove]}" "${target_y[remove]}" click 1
await_state "empty" "remove-button"
echo "todo_native_x11=pass phase=native-remove-persisted"
if ! close_todo after-remove; then
  close_failures=$((close_failures + 1))
fi

start_todo
state_matches "empty" || fail "empty-restart-task-reappeared"
echo "todo_native_x11=pass phase=removed-state-reloaded"
if ! close_todo after-empty; then
  close_failures=$((close_failures + 1))
fi

if [[ "$close_failures" -ne 0 ]]; then
  echo "todo_native_x11=fail close_methods=$close_failures physical_x11=true mode=release" >&2
  exit 1
fi
echo "todo_native_x11=pass physical_x11=true mode=release persistence=true reopen=true remove=true close=true"
