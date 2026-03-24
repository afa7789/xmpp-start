---
name: minidom ElementBuilder API — .append() not .append_child()
description: ElementBuilder uses .append(element) not .append_child(element)
type: feedback
---

When building nested minidom XML elements in a builder chain, use `.append(el)` on `ElementBuilder`, not `.append_child(el)`.

`.append_child()` is a method on a fully-built mutable `Element`, not on `ElementBuilder`.

**Why:** This caused compile errors on the OMEMO device.rs stanza builders. The existing codebase (avatar.rs) consistently uses `.append()` on builder chains.

**How to apply:**
- On a builder chain: `.append(child_element)`
- On a mutable already-built element: `element.append_child(child_element)`
- Pattern for loop-built children: build parent with `Element::builder(...).build()`, then call `.append_child()` in a loop, then use the result with `.append()` in the outer builder chain.
