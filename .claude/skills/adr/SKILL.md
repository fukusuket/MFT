---
name: adr
description: Record an architecture decision as a new ADR in docs/adr/. Use when a cross-crate, format, dependency or licensing decision is made.
---

1. Find the next number: `ls docs/adr/`.
2. Create `docs/adr/NNNN-<kebab-title>.md`:

```markdown
# ADR NNNN: <Title>

Status: accepted (YYYY-MM-DD)  <!-- or: proposed / superseded by ADR XXXX -->

## Context
<1-3 sentences: the problem and constraints>

## Decision
<the decision, as a table or bullets>

## Consequences
<what changes in code, docs, AGENTS.md, deny.toml, NOTICE>
```

3. If it supersedes an ADR, set that ADR's status to `superseded by ADR NNNN` (do not rewrite its decision).
4. Apply the consequences in the same change and link the ADR from `docs/plan.md` if it closes a gate.
