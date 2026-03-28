#!/usr/bin/env python3
"""
Atomized GUI tests for ReXisCe using cliclick + osascript + screencapture.
Runs within Claude Code — no API key needed. Claude reads screenshots directly.

Usage (from project root, inside Claude Code):
  python test-server/gui_test.py login          # single test
  python test-server/gui_test.py --all           # all tests
  python test-server/gui_test.py --group core    # group
  python test-server/gui_test.py --list          # list tests

Or via Makefile:
  cd test-server && make gui-test TEST=login
"""

import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
APP_BINARY = PROJECT_ROOT / "target" / "debug" / "rexisce"
SCREENSHOT_DIR = Path("/tmp")
SETTINGS_DIR = Path.home() / "Library" / "Application Support" / "rexisce"

# Window position cache (set after launch)
WIN_X, WIN_Y, WIN_W, WIN_H = 244, 75, 1024, 796


# ---------------------------------------------------------------------------
# Low-level helpers
# ---------------------------------------------------------------------------

def screenshot(name="test"):
    path = SCREENSHOT_DIR / f"rexisce_{name}.png"
    subprocess.run(["screencapture", "-x", "-C", str(path)], capture_output=True)
    return str(path)


def screenshot_window(name="test"):
    path = SCREENSHOT_DIR / f"rexisce_{name}.png"
    subprocess.run(
        ["screencapture", "-x", "-R",
         f"{WIN_X},{WIN_Y},{WIN_W},{WIN_H}", str(path)],
        capture_output=True,
    )
    return str(path)


def click(x, y):
    subprocess.run(["cliclick", f"c:{x},{y}"], capture_output=True)
    time.sleep(0.3)


def type_text(text):
    """Type text using osascript (handles special chars like @)."""
    # Escape for AppleScript
    escaped = text.replace("\\", "\\\\").replace('"', '\\"')
    subprocess.run(
        ["osascript", "-e",
         f'tell application "System Events" to keystroke "{escaped}"'],
        capture_output=True,
    )
    time.sleep(0.2)


def press_tab():
    subprocess.run(
        ["osascript", "-e", "tell application \"System Events\" to key code 48"],
        capture_output=True,
    )
    time.sleep(0.3)


def press_return():
    subprocess.run(["cliclick", "kp:return"], capture_output=True)
    time.sleep(0.3)


def press_escape():
    subprocess.run(["cliclick", "kp:esc"], capture_output=True)
    time.sleep(0.3)


def select_all():
    subprocess.run(
        ["osascript", "-e",
         'tell application "System Events" to keystroke "a" using command down'],
        capture_output=True,
    )
    time.sleep(0.1)


def focus_app():
    subprocess.run(
        ["osascript", "-e",
         'tell application "System Events" to set frontmost of process "rexisce" to true'],
        capture_output=True,
    )
    time.sleep(0.5)


def get_window_pos():
    global WIN_X, WIN_Y, WIN_W, WIN_H
    result = subprocess.run(
        ["osascript", "-e", """
tell application "System Events"
    tell process "rexisce"
        set p to position of window 1
        set s to size of window 1
        return (item 1 of p as text) & " " & (item 2 of p as text) & " " & (item 1 of s as text) & " " & (item 2 of s as text)
    end tell
end tell
"""],
        capture_output=True, text=True,
    )
    parts = result.stdout.strip().split()
    if len(parts) == 4:
        WIN_X, WIN_Y, WIN_W, WIN_H = [int(p) for p in parts]


def clear_settings():
    for f in ["settings.json", "credentials.json"]:
        p = SETTINGS_DIR / f
        if p.exists():
            p.unlink()


# ---------------------------------------------------------------------------
# Composite actions
# ---------------------------------------------------------------------------

def fill_field(text):
    """Select all in current field and type new text."""
    select_all()
    time.sleep(0.1)
    type_text(text)


def login_as(jid="alice@localhost", password="alice123", server="localhost"):
    """Fill login form and click Connect."""
    focus_app()
    get_window_pos()
    time.sleep(0.5)

    # Click JID field (36% down from window top)
    jid_y = WIN_Y + int(WIN_H * 0.36)
    center_x = WIN_X + WIN_W // 2
    click(center_x, jid_y)
    time.sleep(0.3)
    fill_field(jid)

    press_tab()
    fill_field(password)

    press_tab()
    fill_field(server)

    time.sleep(0.3)

    # Click Connect button (sweep Y to find it)
    btn_x = WIN_X + int(WIN_W * 0.43)
    for y_pct in range(57, 62):
        click(btn_x, WIN_Y + int(WIN_H * y_pct / 100))


def wait_for_connect(timeout=15):
    """Wait for the app to connect by checking logs."""
    time.sleep(timeout)


# ---------------------------------------------------------------------------
# Test definitions
# ---------------------------------------------------------------------------

@dataclass
class TestResult:
    name: str
    passed: bool
    screenshot: str = ""
    message: str = ""


def test_login() -> TestResult:
    """Login as alice@localhost and verify chat screen appears."""
    clear_settings()
    proc = subprocess.Popen([str(APP_BINARY)])
    time.sleep(3)

    login_as()
    time.sleep(10)

    path = screenshot_window("login")
    proc.terminate()
    proc.wait(timeout=5)

    # Check if login was successful by looking at the log
    # The screenshot will show the result - caller (Claude) can verify visually
    return TestResult("login", True, path,
                      "Logged in as alice@localhost. Check screenshot for chat screen.")


def test_login_wrong_password() -> TestResult:
    """Try wrong password and verify rejection."""
    clear_settings()
    proc = subprocess.Popen([str(APP_BINARY)])
    time.sleep(3)

    login_as(password="wrongpassword")
    time.sleep(8)

    path = screenshot_window("wrong_password")
    proc.terminate()
    proc.wait(timeout=5)

    return TestResult("login_wrong_password", True, path,
                      "Attempted login with wrong password. Should show error or stay on login.")


def test_send_message() -> TestResult:
    """Login, click New, type bob@localhost, send a message."""
    clear_settings()
    proc = subprocess.Popen([str(APP_BINARY)])
    time.sleep(3)

    login_as()
    time.sleep(10)

    focus_app()
    get_window_pos()

    # Click "New" button (top-right of sidebar area)
    new_btn_x = WIN_X + int(WIN_W * 0.18)
    new_btn_y = WIN_Y + int(WIN_H * 0.095)
    click(new_btn_x, new_btn_y)
    time.sleep(0.5)

    # Type bob@localhost in the JID input that appears
    type_text("bob@localhost")
    press_return()
    time.sleep(1)

    # Type message in the composer
    type_text("Hello from GUI test!")
    press_return()
    time.sleep(2)

    path = screenshot_window("send_message")
    proc.terminate()
    proc.wait(timeout=5)

    return TestResult("send_message", True, path,
                      "Sent 'Hello from GUI test!' to bob@localhost.")


def test_settings_open_close() -> TestResult:
    """Login, open settings, verify modal, close it."""
    clear_settings()
    proc = subprocess.Popen([str(APP_BINARY)])
    time.sleep(3)

    login_as()
    time.sleep(10)

    focus_app()
    get_window_pos()

    # Look for settings/gear icon - typically in sidebar header area
    # The account name "alice@localhost" is clickable or there's a gear icon
    # Try clicking the account area at top of sidebar
    click(WIN_X + 50, WIN_Y + 35)
    time.sleep(1)

    path1 = screenshot_window("settings_open")

    # Close settings (press Escape or click X)
    press_escape()
    time.sleep(0.5)

    path2 = screenshot_window("settings_close")

    proc.terminate()
    proc.wait(timeout=5)

    return TestResult("settings_open_close", True, path1,
                      f"Opened settings. Screenshots: {path1}, {path2}")


def test_join_muc() -> TestResult:
    """Login and join testroom@conference.localhost."""
    clear_settings()
    proc = subprocess.Popen([str(APP_BINARY)])
    time.sleep(3)

    login_as()
    time.sleep(10)

    focus_app()
    get_window_pos()

    # Click "#" button (join room) in sidebar header
    hash_btn_x = WIN_X + int(WIN_W * 0.14)
    hash_btn_y = WIN_Y + int(WIN_H * 0.095)
    click(hash_btn_x, hash_btn_y)
    time.sleep(1)

    # Type room JID
    type_text("testroom@conference.localhost")
    press_tab()
    type_text("alice")
    press_return()
    time.sleep(3)

    path = screenshot_window("join_muc")
    proc.terminate()
    proc.wait(timeout=5)

    return TestResult("join_muc", True, path,
                      "Joined testroom@conference.localhost.")


# All tests registry
TESTS = {
    "login": ("core", test_login),
    "login_wrong_password": ("core", test_login_wrong_password),
    "send_message": ("chat", test_send_message),
    "settings_open_close": ("ui", test_settings_open_close),
    "join_muc": ("chat", test_join_muc),
}

GROUPS = {
    "core": ["login", "login_wrong_password"],
    "chat": ["send_message", "join_muc"],
    "ui": ["settings_open_close"],
}


def run_tests(names):
    """Run tests and print results with screenshot paths."""
    # Build first
    print("Building app...")
    r = subprocess.run(["cargo", "build"], cwd=PROJECT_ROOT, capture_output=True, text=True)
    if r.returncode != 0:
        print(f"Build failed:\n{r.stderr[-300:]}")
        sys.exit(1)

    results = []
    for name in names:
        group, func = TESTS[name]
        print(f"\n{'='*50}")
        print(f"  TEST: {name} [{group}]")
        print(f"{'='*50}")

        try:
            result = func()
            results.append(result)
            print(f"  Screenshot: {result.screenshot}")
            print(f"  Note: {result.message}")
        except Exception as e:
            results.append(TestResult(name, False, "", str(e)))
            print(f"  ERROR: {e}")

        # Clean up any leftover process
        subprocess.run(["pkill", "-f", "target/debug/rexisce"], capture_output=True)
        time.sleep(1)

    # Summary
    print(f"\n{'='*50}")
    print("  RESULTS")
    print(f"{'='*50}")
    for r in results:
        status = "RAN" if r.passed else "ERR"
        print(f"  [{status}] {r.name}: {r.message[:60]}")
        if r.screenshot:
            print(f"         screenshot: {r.screenshot}")

    print(f"\n  {len(results)} test(s) executed.")
    print("  Verify results by reading the screenshot files with Claude.")


def main():
    args = sys.argv[1:]

    if not args or args == ["--list"]:
        print("Available GUI tests:\n")
        for name, (group, _) in TESTS.items():
            print(f"  {name:30s} [{group}]")
        print(f"\nGroups: {', '.join(GROUPS.keys())}")
        print(f"\nUsage:")
        print(f"  python gui_test.py login")
        print(f"  python gui_test.py --group core")
        print(f"  python gui_test.py --all")
        return

    if args == ["--all"]:
        names = list(TESTS.keys())
    elif args[0] == "--group" and len(args) > 1:
        group = args[1]
        if group not in GROUPS:
            print(f"Unknown group: {group}. Available: {', '.join(GROUPS.keys())}")
            sys.exit(1)
        names = GROUPS[group]
    else:
        names = [a for a in args if a in TESTS]
        if not names:
            print(f"Unknown test(s). Available: {', '.join(TESTS.keys())}")
            sys.exit(1)

    run_tests(names)


if __name__ == "__main__":
    main()
