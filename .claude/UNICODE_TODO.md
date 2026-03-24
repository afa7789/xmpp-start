# Unicode / Emoji Shaping TODO for conversation.rs

Agent B is working on `src/ui/conversation.rs`. The following text widgets in that file
need `.shaping(iced::widget::text::Shaping::Advanced)` added to render emojis and
complex Unicode characters correctly.

## Required changes

### Message body text
All `text(...)` calls that render **message body content** (from other users or yourself)
must have `.shaping(Shaping::Advanced)`. This includes:
- The main message body `text` widget (wherever `msg.body` / `DisplayMessage.body` is rendered)
- Any rich-text `span(...)` components that display user text
- Action/emote message text

### Sender names
Any `text(...)` widget that renders a **sender JID or display name** should use
`.shaping(Shaping::Advanced)` — e.g. the "alice@example.com" label above a message bubble.

### Reaction emoji
The reaction counts and emoji labels (e.g. `"👍 2"`) displayed below messages must use
`.shaping(Shaping::Advanced)` — these are explicitly emoji characters.

### How to apply
Import at the top of the file (already present via `use iced::widget::text::Shaping` or
accessible as `iced::widget::text::Shaping`):

```rust
use iced::widget::text::Shaping;
```

Then chain `.shaping(Shaping::Advanced)` on every `text(...)` widget displaying user content:

```rust
text(message_body).size(14).shaping(Shaping::Advanced)
```

## Context
- iced uses cosmic-text internally, which loads system fonts (including Apple Color Emoji
  on macOS, Noto Color Emoji on Linux).
- `Shaping::Basic` (the default) skips Unicode BiDi and emoji fallback — hence emojis
  show as boxes.
- `Shaping::Advanced` enables HarfBuzz shaping + font fallback for complex scripts and emoji.
- System emoji fonts are already loaded by cosmic-text at startup — no extra font loading
  needed, just the shaping mode change.
