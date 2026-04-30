# Aether Demo: Clone and Run in 60 Seconds

Complete provenance-first programming demo with trust enforcement.

## Step 1: Build Aether CLI
```bash
cargo build --bin aether-cli
```

## Step 2: Wrap Rust Library
```bash
cargo run --bin aether-cli -- wrap examples/demo/lib.rs --source "ai" --confidence 0.6
```

**Expected output:**
```
✓ Wrapped examples/demo/lib.rs to examples/demo/lib.aeth
```

## Step 3: Static Provenance Audit
```bash
cargo run --bin aether-cli -- report examples/demo/main.aeth
```

**Expected output:**
```
[aether] provenance report: examples/demo/main.aeth

  fn main            source: user   confidence: 1.00  ✓
  fn add_numbers     source: ai     confidence: 0.60  ⚠
  fn optimize_query  source: ai     confidence: 0.60  ⚠
  fn helper_function source: ?      confidence: 0.00  ✗

  flat score:     0.55
  tagged:         3/4 functions
  untagged:       1 (silence = zero trust)
```

## Step 4: Run with High Trust Threshold (Blocked)
```bash
cargo run --bin aether-cli -- run --session-id demo --min-trust 0.8 examples/demo/main.aeth
```

**Expected output:**
```
[aether] execution complete — trust score: 0.55 (weighted) / 0.55 (flat)
[aether] trust report:
  [BLOCKED] add_numbers     confidence: 0.60   source: ai      examples/demo/lib.aeth
  [BLOCKED] optimize_query  confidence: 0.60   source: ai      examples/demo/lib.aeth
  [BLOCKED] helper_function confidence: 0.00   source: ?       examples/demo/main.aeth
  [OK]      main            confidence: 1.00   source: user    examples/demo/main.aeth

[aether] blocked — trust score 0.55 (weighted) is below minimum 0.80
```

## Step 5: Run with Lower Trust Threshold (Success)
```bash
cargo run --bin aether-cli -- run --session-id demo --min-trust 0.7 examples/demo/main.aeth
```

**Expected output:**
```
[aether] extern fn `add_numbers` called — returning zero value (not linked)
[aether] extern fn `optimize_query` called — returning zero value (not linked)
[aether] execution complete — trust score: 0.55 (weighted) / 0.55 (flat)
26
```

## What Happened:

1. **Static Audit**: `aether report` showed mixed trust levels (1.00 user, 0.60 AI, 0.00 untagged)
2. **Trust Enforcement**: High threshold (0.8) blocked execution due to AI/untagged functions
3. **Extern Handling**: Lower threshold (0.7) allowed execution with placeholder values
4. **Trust Scoring**: Weighted scoring gives deeper function calls more influence

The demo shows Aether's core value: structural accountability for AI-generated code.
