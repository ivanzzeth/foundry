#!/bin/bash
# Run eth_simulateV1 spec tests against a running anvil instance.
# Usage: ./run_spec_tests.sh [anvil_url]
# Default: http://localhost:18545

set -euo pipefail

ANVIL_URL="${1:-http://localhost:18545}"
SPEC_DIR="$(dirname "$0")/eth_simulateV1"
PASS=0
FAIL=0
SKIP=0
FAILURES=""

for test_file in "$SPEC_DIR"/*.io; do
    test_name="$(basename "$test_file" .io)"

    # Extract request (>> line) and expected response (<< line)
    request=$(grep '^>> ' "$test_file" | head -1 | sed 's/^>> //')
    expected=$(grep '^<< ' "$test_file" | head -1 | sed 's/^<< //')

    if [ -z "$request" ] || [ -z "$expected" ]; then
        echo "SKIP  $test_name (no request/response)"
        SKIP=$((SKIP + 1))
        continue
    fi

    # Send request to anvil
    actual=$(curl -s -X POST "$ANVIL_URL" -H "Content-Type: application/json" -d "$request" 2>/dev/null)

    if [ -z "$actual" ]; then
        echo "FAIL  $test_name (no response from anvil)"
        FAIL=$((FAIL + 1))
        FAILURES="$FAILURES\n  $test_name: no response"
        continue
    fi

    # Compare: check if the response has the same structure (result vs error)
    # We do a semantic comparison: both should have result or both should have error
    expected_has_result=$(echo "$expected" | python3 -c "import sys,json; d=json.load(sys.stdin); print('result' if 'result' in d else 'error')" 2>/dev/null)
    actual_has_result=$(echo "$actual" | python3 -c "import sys,json; d=json.load(sys.stdin); print('result' if 'result' in d else 'error')" 2>/dev/null)

    if [ "$expected_has_result" != "$actual_has_result" ]; then
        echo "FAIL  $test_name (expected=$expected_has_result, got=$actual_has_result)"
        FAIL=$((FAIL + 1))
        # Show first 200 chars of actual response for debugging
        short_actual=$(echo "$actual" | head -c 200)
        FAILURES="$FAILURES\n  $test_name: expected $expected_has_result, got $actual_has_result\n    actual: $short_actual"
        continue
    fi

    # For result responses: compare call statuses and key fields
    if [ "$expected_has_result" = "result" ]; then
        # Compare number of blocks and call statuses
        match=$(python3 -c "
import sys, json

expected = json.loads('''$expected''')
actual = json.loads('''$actual''')

er = expected.get('result', [])
ar = actual.get('result', [])

if len(er) != len(ar):
    print(f'block_count_mismatch: expected={len(er)} got={len(ar)}')
    sys.exit(0)

for i, (eb, ab) in enumerate(zip(er, ar)):
    ec = eb.get('calls', [])
    ac = ab.get('calls', [])
    if len(ec) != len(ac):
        print(f'block[{i}] call_count_mismatch: expected={len(ec)} got={len(ac)}')
        sys.exit(0)
    for j, (ecall, acall) in enumerate(zip(ec, ac)):
        es = ecall.get('status', '')
        as_ = acall.get('status', '')
        if es != as_:
            print(f'block[{i}].call[{j}] status_mismatch: expected={es} got={as_}')
            sys.exit(0)

print('ok')
" 2>/dev/null)

        if [ "$match" = "ok" ]; then
            echo "PASS  $test_name"
            PASS=$((PASS + 1))
        else
            echo "FAIL  $test_name ($match)"
            FAIL=$((FAIL + 1))
            FAILURES="$FAILURES\n  $test_name: $match"
        fi
    else
        # For error responses: compare error codes
        match=$(python3 -c "
import sys, json
expected = json.loads('''$expected''')
actual = json.loads('''$actual''')
ee = expected.get('error', {})
ae = actual.get('error', {})
ec = ee.get('code', 0)
ac = ae.get('code', 0)
if ec == ac:
    print('ok')
else:
    print(f'error_code_mismatch: expected={ec} got={ac}')
" 2>/dev/null)

        if [ "$match" = "ok" ]; then
            echo "PASS  $test_name"
            PASS=$((PASS + 1))
        else
            echo "FAIL  $test_name ($match)"
            FAIL=$((FAIL + 1))
            FAILURES="$FAILURES\n  $test_name: $match"
        fi
    fi
done

echo ""
echo "=========================================="
echo "Results: $PASS passed, $FAIL failed, $SKIP skipped (total: $((PASS + FAIL + SKIP)))"
echo "=========================================="

if [ $FAIL -gt 0 ]; then
    echo ""
    echo "Failures:"
    echo -e "$FAILURES"
    exit 1
fi
