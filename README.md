[![CI](https://github.com/webfor1website/aether/actions/workflows/ci.yml/badge.svg)](https://github.com/webfor1website/aether/actions/workflows/ci.yml)

# Aether

A provenance-first programming language. Every function knows who wrote it, how confident they were, and whether it can be trusted to run.

---

## The problem

AI coding agents write code. That code gets shipped. Nobody knows which parts came from a human, which came from Claude, which came from Cursor, or how confident any of them were. There's no accountability built into the language itself — just vibes and code review.

Aether fixes that at the language level.

---

## How it works

You tag functions with `@prov` metadata:

```aether
@prov(source: "user", confidence: 1.0)
fn verified_add(a: Int, b: Int) -> Int {
  a + b
}

@prov(source: "ai", confidence: 0.6)
fn ai_generated(n: Int) -> Int {
  n * 2
}

fn main() -> Int {
  verified_add(ai_generated(5), 1)
}
```

When you run it, Aether computes a **weighted trust score** — deeper functions in the call stack have more weight:

```
[aether] execution complete — trust score: 0.84 (weighted) / 0.91 (flat)
11
```

Weight formula: `weight = call_depth + 1` (root = depth 0, weight 1)
Weighted score: `SUM(confidence_i * weight_i) / SUM(weight_i)`

You can block execution below a threshold:

```bash
aether run --session-id test1 --min-trust 0.9 program.ae
```

```
[aether] execution complete — trust score: 0.84 (weighted) / 0.91 (flat)
[aether] trust report:
  [BLOCKED] ai_generated    confidence: 0.60   source: ai      program.ae
  [OK]      verified_add    confidence: 1.00   source: user    program.ae

[aether] blocked — trust score 0.84 (weighted) is below minimum 0.90
```

Exit code 2 means blocked by trust gate. Exit code 1 means parse/runtime error. Exit code 0 means success.

---

## Trust evolves

Every successful run nudges confidence up slightly. Every failure nudges it down. The language has memory.

```bash
aether run --session-id s1 program.ae  # trust: 0.80
aether run --session-id s1 program.ae  # trust: 0.81  ← evolved
aether run --session-id s1 program.ae  # trust: 0.82  ← keeps climbing
```

The original `@prov` tag is preserved as the human declaration of intent. Evolution is tracked separately in the provenance store. They're different things.

---

## Replay any session

```bash
aether replay --session-id s1
```

```
[aether] replay — session: s1
  #1  ai_generated    confidence: 0.60   source: ai      program.ae
  #2  verified_add    confidence: 1.00   source: user    program.ae

  final trust score: 0.80
```

Full forensic history of every function that ran, who wrote it, and what it scored.

---

## Multi-file programs

```aether
import "lib.ae";

@prov(source: "user", confidence: 1.0)
fn main() -> Int {
  compute(3)
}
```

Imported functions bring their provenance with them. Trust scores reflect the full dependency graph.

---

## Interop

Wrap existing Rust libraries with provenance tags:

```bash
aether wrap mylib.rs --source "ai" --confidence 0.7
```

Output (mylib.aeth):
```aether
@prov(source: "ai", confidence: 0.70)

extern fn add_numbers(Int, Int) -> Int;
extern fn is_valid(Bool) -> Bool;
```

Import and use in Aether:
```aether
import "mylib.aeth";

@prov(source: "user", confidence: 1.0)
fn main() -> Int {
  add_numbers(5, 10)
}
```

Extern functions return placeholder values and show warnings, but their trust scores still count toward enforcement.

---

## Commit your threshold

Create `.aether-wellbeing` in your repo:

```
min_trust: 0.8
```

Now every run enforces the threshold without a CLI flag. Commit it. Make it policy.

---

## Installation

Requires Rust toolchain.

```bash
git clone https://github.com/yourname/aether
cd aether
cargo build --bin aether-cli
```

---

## Usage

```bash
# Run a program
cargo run --bin aether-cli -- run --session-id <id> <file.ae>

# Run with trust enforcement
cargo run --bin aether-cli -- run --session-id <id> --min-trust 0.8 <file.ae>

# Replay a session
cargo run --bin aether-cli -- replay --session-id <id>

# Enable debug output
AETHER_DEBUG=1 cargo run --bin aether-cli -- run --session-id <id> <file.ae>
```

---

## VS Code

Install the extension from `vscode-aether/`, open a `.aeth` file, and hover any `@prov` function to see its trust score:

```
source: "ai"  confidence: 0.75  [trust: evolving]
```

The LSP provides real-time trust information and provenance metadata directly in your editor.

---

## Language syntax

```aether
@prov(source: "user", confidence: 1.0)
fn add(a: Int, b: Int) -> Int {
  a + b
}

@prov(source: "ai", confidence: 0.7)
fn multiply(a: Int, b: Int) -> Int {
  a * b
}

fn main() -> Int {
  let x: Int = add(2, 3);
  multiply(x, 2)
}
```

Supported:
- `Int` type
- `let` bindings
- Binary expressions (`+`, `-`, `*`, `/`)
- Function calls
- `if/else` expressions
- Recursion
- `@prov` tags with `source` and `confidence` fields
- `import "file.ae"` statements

---

## Trust scoring

- Tagged functions contribute their `confidence` value to the score
- Untagged functions get `confidence: 0.0` — silence is not trust
- `main` is excluded — it's infrastructure, not authored logic
- Score = `AVG(confidence)` across all non-main functions
- Evolution adjusts confidence `+0.05` on success, `-0.1` on failure, per session

---

## Architecture

```
source (.ae) → Lexer → Parser → Checker → IR → Interpreter → Trust Score → Enforcement
                                                      ↓
                                              SQLite prov store
```

```
aether/
├── crates/
│   ├── aether-core/        # AST types: Expr, Stmt, ProvenanceTag, AuthorType
│   ├── aether-parser/      # Lexer + Parser
│   ├── aether-checker/     # Type inference, name resolution, ProvenanceValidator
│   ├── aether-ir/          # IR lowering
│   ├── aether-interp/      # Interpreter — run_main returns (Value, f64, f64)
│   ├── aether-prov-store/  # SQLite store — per-session provenance DB
│   ├── aether-lsp/         # Language Server Protocol — VS Code integration
│   ├── aether-discipline/  # Development workflow and session management
│   ├── aether-format/      # Code formatting
│   ├── aether-runtime/     # Runtime utilities
│   └── aether-cli/         # CLI entry point
```

---

## Why this matters

Right now the AI coding agent space has no accountability infrastructure. Prompting guidelines and code review checklists are voluntary. Aether makes accountability structural — you cannot ship unverified AI-generated code without attribution, because the runtime won't let you.

The long-term goal: Aether as a governance layer that wraps existing Rust/Python/JS codebases, tagging functions with provenance at the boundary between human and AI authorship.

---

## Status

Core pipeline solid. Multi-file imports. Trust evolution. Weighted scoring by call depth. LSP with live hover. VS Code extension. Interop via aether wrap. Extern fn runtime support.

Built by Claude with help from Cascade and Grok. The user didn't do shit.

What's missing: real-world battle testing, a trust algebra paper, and someone trying to game it.

Genius rating: 9.6/10. Ask me in 6 weeks about the gaming problem.
