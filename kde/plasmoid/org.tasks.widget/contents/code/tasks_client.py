#!/usr/bin/env python3
"""
tasks_client.py — CLI wrapper for the org.tasks.TasksMcp D-Bus service.

Usage:
  tasks_client.py list-lists
  tasks_client.py list-tasks [filter_json]
  tasks_client.py set-status <task_id> <new_status>

All output is JSON on stdout.
"""

import ast
import json
import re
import subprocess
import sys
from typing import Any

SERVICE = "org.tasks.TasksMcp"
OBJECT_PATH = "/org/tasks/TasksMcp"
IFACE = "org.tasks.TasksMcp"


def _parse_gdbus_output(text: str) -> Any:
    """Convert GVariant tuple output from gdbus to a Python value."""
    normalized = text.strip()
    # Strip GVariant type annotations like @as, @a{sv}, etc.
    normalized = re.sub(r"@[A-Za-z0-9_(){}\[\],]+\s+", "", normalized)
    # Strip numeric type prefixes: uint32 1 -> 1, int64 -1 -> -1
    normalized = re.sub(r"\b(?:u?int(?:16|32|64)|byte)\s+(-?\d+)", r"\1", normalized)
    # GVariant booleans to Python
    normalized = re.sub(r"\btrue\b", "True", normalized)
    normalized = re.sub(r"\bfalse\b", "False", normalized)
    parsed = ast.literal_eval(normalized)
    # gdbus wraps the return value in a tuple; unwrap single-element tuples
    if isinstance(parsed, tuple):
        return parsed[0] if len(parsed) == 1 else parsed
    return parsed


def gdbus_call(method: str, *args: str) -> str:
    """Call a D-Bus method and return its JSON string result."""
    cmd = [
        "gdbus", "call",
        "--session",
        "--dest", SERVICE,
        "--object-path", OBJECT_PATH,
        "--method", f"{IFACE}.{method}",
    ] + list(args)
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
    if result.returncode != 0:
        err = result.stderr.strip() or f"gdbus exited with code {result.returncode}"
        raise RuntimeError(err)
    return str(_parse_gdbus_output(result.stdout))


def main() -> int:
    if len(sys.argv) < 2:
        print(json.dumps({"error": "no command given"}))
        return 1

    cmd = sys.argv[1]

    try:
        if cmd == "list-lists":
            raw = gdbus_call("ListLists")
            print(json.dumps(json.loads(raw)))

        elif cmd == "list-tasks":
            filter_arg = sys.argv[2] if len(sys.argv) > 2 else "{}"
            raw = gdbus_call("ListTasks", filter_arg)
            print(json.dumps(json.loads(raw)))

        elif cmd == "set-status":
            if len(sys.argv) < 4:
                print(json.dumps({"error": "set-status requires <task_id> <status>"}))
                return 1
            payload = json.dumps({"id": sys.argv[2], "status": sys.argv[3]})
            raw = gdbus_call("SetStatus", payload)
            print(json.dumps(json.loads(raw)))

        else:
            print(json.dumps({"error": f"unknown command: {cmd}"}))
            return 1

    except Exception as exc:  # noqa: BLE001
        print(json.dumps({"error": str(exc)}))
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
