# Aether Provenance Model — v0.1.0

## Purpose

Every node in an Aether program carries an optional but first-class provenance tag.
Provenance answers: who generated this code, under what context, with what confidence,
and derived from what prior artifacts.

The provenance system is not a debugging aid bolted on after the fact.
It is load-bearing infrastructure. The checker enforces it. The runtime records it.
The LSP surfaces it. Humans reviewing AI-generated code rely on it.

---

## ProvenanceTag Schema

```
ProvenanceTag {
  id:         UUID        -- deterministic: SHA-256(author + prompt + timestamp)
  author:     AuthorType  -- see AuthorType below
  model:      String?     -- e.g. "grok-3", "claude-sonnet-4-6" (null for humans)
  timestamp:  ISO8601     -- UTC, second precision minimum
  prompt:     SHA256?     -- hash of the prompt that produced this node
  confidence: Float       -- 0.0 (untrusted) → 1.0 (human-verified)
  parent:     UUID[]      -- IDs this tag was derived from (empty = root)
  version:    SemVer      -- Aether version that produced this tag
}

AuthorType
  = "human"
  | "ai:<model_id>"        -- e.g. "ai:claude-sonnet-4-6"
  | "transform:<pass>"     -- e.g. "transform:inline_constants"
```

---

## Rules

### Rule 1 — Propagation Through Lowering

- Every AIR node inherits the provenance tag of its source AST node.
- Lowering passes that transform a node append themselves as a new parent:
  `author: "transform:<pass_name>"`.
- A pass may never silently drop a provenance tag. Dropping = compile error E3002.

### Rule 2 — Confidence Propagation

| Event                          | Effect on Confidence          |
|-------------------------------|-------------------------------|
| AI-generated, unreviewed       | As declared (typically 0.6–0.8) |
| Human edited                   | + 0.2, capped at 1.0          |
| Human reviewed, no edits       | + 0.1, capped at 1.0          |
| Lowering pass applied          | × 0.95                        |
| Merged from N parents          | min(parent confidences) × 0.95 |
| `extern` without @prov tag     | Compile error E3003           |
| Child confidence > parent      | Compile error E3004           |

### Rule 3 — Acyclicity

The provenance graph is a DAG. A tag may not appear in its own ancestor chain.
Cycles are detected during the checker's provenance validation phase.
Cycle detected = compile error E3001.

### Rule 4 — Extern Rule

Any `extern` declaration must carry a `@prov` tag. Enforced at parse time, not just
the checker phase. An AI agent cannot silently import untracked code. No exceptions.

### Rule 5 — Block Inheritance

If a `fn_decl` carries a `@prov` tag, all statements in its body inherit that tag
unless they declare their own. Child confidence is capped at parent confidence at
the time of inheritance.

### Rule 6 — Root Nodes

A node with `parent: []` is a root. Roots authored by `"ai:*"` must have a
`prompt` hash. Roots authored by `"human"` may omit the prompt hash.
A root node authored by `"transform:*"` is a compile error E3005 —
transforms always have a parent.

---

## Confidence Score Reference

```
1.0   Human-written and verified
0.9   Human-written, unreviewed
0.8   AI-generated, human-reviewed with no edits
0.7   AI-generated, spot-checked
0.6   AI-generated, unreviewed
0.5   AI-generated from a vague or ambiguous prompt
<0.5  Derived from multiple low-confidence parents
0.0   Explicitly untrusted (e.g. flagged by auditor)
```

These are recommended values, not enforced ranges (except the child > parent rule).

---

## ProvenanceStore

### Storage
- **Local:** append-only SQLite. Schema is versioned with the language.
- **Remote:** pluggable via `ProvenanceBackend` trait.
- Records are immutable once written. Updates create new records with a parent reference.
- Every record includes the Aether `version` field for migration safety.

### Query API

```
by_prompt(hash: SHA256)           -> Vec<Node>
by_author(author: AuthorType)     -> Vec<Node>
chain(node_id: UUID)              -> Vec<ProvenanceTag>   // full ancestor chain
confidence_below(t: Float)        -> Vec<Node>
diff(tag_a: UUID, tag_b: UUID)    -> ProvenanceDiff       // structural diff
```

### Export
- SARIF-compatible output for standard code review tooling.
- `aether prov <file>` renders the graph as ASCII or `.dot` for Graphviz.

---

## Error Codes (Provenance Range: E3xxx)

| Code  | Meaning                                              |
|-------|------------------------------------------------------|
| E3001 | Provenance cycle detected                            |
| E3002 | Lowering pass dropped a provenance tag               |
| E3003 | `extern` declaration missing @prov tag               |
| E3004 | Child confidence exceeds parent confidence           |
| E3005 | Root node has author type `transform:*`              |
| E3006 | Provenance ID collision (non-deterministic hash)     |
| E3007 | Malformed ISO8601 timestamp in @prov tag             |
| E3008 | Confidence value outside [0.0, 1.0]                 |
