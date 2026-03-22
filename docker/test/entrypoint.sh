#!/bin/sh
set -e

# Start Prosody in the background.
prosody &
PROSODY_PID=$!

# Wait until it responds to status checks.
echo "[test-server] starting Prosody..."
until prosodyctl status 2>/dev/null; do
    sleep 0.5
done

# Create fixed test accounts.
prosodyctl register alice localhost alice123 2>/dev/null || true
prosodyctl register bob   localhost bob123   2>/dev/null || true

echo "[test-server] ready — alice@localhost / bob@localhost"

# Keep the container alive by waiting on the Prosody process.
wait $PROSODY_PID
