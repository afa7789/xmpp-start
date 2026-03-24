---
name: sqlx API convention — no query! macro
description: Use untyped sqlx::query() not sqlx::query!() macro in this codebase
type: feedback
---

Never use `sqlx::query!()` macro in this project. It requires DATABASE_URL at compile time and will cause build failures.

**Why:** The project does not set DATABASE_URL during normal builds. All existing store code (roster_repo.rs, conversation_repo.rs, etc.) uses the untyped `sqlx::query()` API with `.bind()` + `.fetch_*()` + `row.get("col")`.

**How to apply:** Whenever writing new SQLite query code, use `sqlx::query("SELECT ...")` with `.bind(value)` calls. Use `row.get::<i64, _>("col") as u32` pattern for integer conversions since SQLite stores integers as i64.
