---
name: Tracking issue
about: Unstable kernel api features
title: Tracking issue for `...`
labels: tracking
assignees: ''

---

Feature gate: `#![feature(...)]`

This is a tracking issue for ...

<!--
Include a short description of the feature.
-->

### Public API

<!--
For most library features, it'd be useful to include a summarized version of the public API.
(E.g. just the public function signatures without their doc comments or implementation.)
-->

```rust
// core::magic

pub struct Magic;

impl Magic {
    pub fn magic(self);
}
```

### Steps / History

<!--
For larger features, more steps might be involved.
If the feature is changed later, please add those PRs here as well.
-->

- [ ] Implementation: #...
- [ ] Stabilization PR

### Unresolved Questions

<!--
Include any open questions that need to be answered before the feature can be
stabilised. If multiple (unrelated) big questions come up, it can be a good idea
to open a separate issue for each, to make it easier to keep track of the
discussions.
-->

- None yet.
