---
name: rust_analyzer_auto_fix
description: rust-analyzer (VSCode) auto-reverts changes that cause compilation errors — must make all interdependent changes atomically
type: feedback
---

In xmpp-start, rust-analyzer runs in the background and auto-fixes compilation errors by reverting changes. If you update a function signature in file A but the callers in file B still use the old signature, rust-analyzer will revert file A back to match the callers.

**Why:** VSCode rust-analyzer applies "quick fixes" automatically when it detects mismatches.

**How to apply:** When changing function signatures that affect multiple files:
1. Update ALL callers FIRST before updating the signature
2. Or update the signature and ALL callers in the same session without pausing for builds
3. Always immediately run `cargo build` after changes to lock them in before the linter can undo them
4. If the linter reverts a file, re-read it before trying to edit it again (the Write tool will fail if file was modified)
