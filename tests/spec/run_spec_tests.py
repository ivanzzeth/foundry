#!/usr/bin/env python3
"""Run eth_simulateV1 spec tests against a running anvil instance."""

import json
import os
import sys
import urllib.request

ANVIL_URL = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:18546"
SPEC_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "eth_simulateV1")

passed = 0
failed = 0
skipped = 0
failures = []

for fname in sorted(os.listdir(SPEC_DIR)):
    if not fname.endswith(".io"):
        continue
    test_name = fname[:-3]
    filepath = os.path.join(SPEC_DIR, fname)

    request_line = None
    expected_line = None
    with open(filepath) as f:
        for line in f:
            line = line.strip()
            if line.startswith(">> "):
                request_line = line[3:]
            elif line.startswith("<< "):
                expected_line = line[3:]

    if not request_line or not expected_line:
        print(f"SKIP  {test_name} (no request/response)")
        skipped += 1
        continue

    try:
        req = urllib.request.Request(
            ANVIL_URL,
            data=request_line.encode(),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=10) as resp:
            actual_raw = resp.read().decode()
    except Exception as e:
        print(f"FAIL  {test_name} (request error: {e})")
        failed += 1
        failures.append(f"  {test_name}: request error: {e}")
        continue

    try:
        expected = json.loads(expected_line)
        actual = json.loads(actual_raw)
    except json.JSONDecodeError as e:
        print(f"FAIL  {test_name} (json parse error: {e})")
        failed += 1
        failures.append(f"  {test_name}: json parse error")
        continue

    exp_type = "result" if "result" in expected else "error"
    act_type = "result" if "result" in actual else "error"

    if exp_type != act_type:
        print(f"FAIL  {test_name} (expected={exp_type}, got={act_type})")
        failed += 1
        failures.append(f"  {test_name}: expected {exp_type}, got {act_type}: {actual_raw[:200]}")
        continue

    if exp_type == "result":
        er = expected.get("result", [])
        ar = actual.get("result", [])
        if len(er) != len(ar):
            print(f"FAIL  {test_name} (block count: expected={len(er)} got={len(ar)})")
            failed += 1
            failures.append(f"  {test_name}: block count mismatch")
            continue

        mismatch = None
        for i, (eb, ab) in enumerate(zip(er, ar)):
            ec = eb.get("calls", [])
            ac = ab.get("calls", [])
            if len(ec) != len(ac):
                mismatch = f"block[{i}] call count: expected={len(ec)} got={len(ac)}"
                break
            for j, (ecall, acall) in enumerate(zip(ec, ac)):
                es = ecall.get("status", "")
                as_ = acall.get("status", "")
                if es != as_:
                    mismatch = f"block[{i}].call[{j}] status: expected={es} got={as_}"
                    break
            if mismatch:
                break

        if mismatch:
            print(f"FAIL  {test_name} ({mismatch})")
            failed += 1
            failures.append(f"  {test_name}: {mismatch}")
        else:
            print(f"PASS  {test_name}")
            passed += 1
    else:
        ec = expected.get("error", {}).get("code", 0)
        ac = actual.get("error", {}).get("code", 0)
        if ec == ac:
            print(f"PASS  {test_name}")
            passed += 1
        else:
            print(f"FAIL  {test_name} (error code: expected={ec} got={ac})")
            failed += 1
            failures.append(f"  {test_name}: error code expected={ec} got={ac}")

print()
print(f"==========================================")
print(f"Results: {passed} passed, {failed} failed, {skipped} skipped (total: {passed + failed + skipped})")
print(f"==========================================")

if failures:
    print()
    print("Failures:")
    for f in failures:
        print(f)
    sys.exit(1)
