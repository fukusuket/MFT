---
name: adr
description: Record an architecture decision as a new ADR in docs/adr/. Use when a cross-crate, format, dependency or licensing decision is made.
---

1. Find the next number: `ls docs/adr/`.
2. Create `docs/adr/NNNN-<kebab-title>.md`:

```markdown
# ADR NNNN: <Title>

Status: proposed (YYYY-MM-DD)  <!-- a human changes this to accepted (AGENTS.md H2) -->

## Context
<1-3 sentences: the problem and constraints>

## Decision
<the decision, as a table or bullets>

## Consequences
<what changes in code, docs, AGENTS.md, deny.toml, NOTICE>
```

3. If it supersedes an ADR, set that ADR's status to `superseded by ADR NNNN` (do not rewrite its decision).
4. Stop and ask a human to accept it (H2). Apply the consequences only after acceptance.
