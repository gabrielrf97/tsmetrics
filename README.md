# tsmetrics — TypeScript Static Analyzer

**Fast, tree-sitter-powered static analysis for TypeScript and TSX.**

[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

→ **[Full documentation](https://gabrielrf97.github.io/tsmetrics/)**

---

## What it does

tsmetrics parses your TypeScript/TSX files and computes 25+ quality metrics in parallel:

- **Function metrics** — cyclomatic complexity, LOC, nesting depth, Halstead volume, Maintainability Index, closure depth
- **Class metrics** — WMC, DIT, TCC, CBO, RFC, NOI, NOM, WOC
- **React / FP metrics** — hook complexity, effect density, component responsibility, prop drilling, render complexity, module cohesion, pure function ratio
- **Design smell detection** — God Class, Brain Method, Feature Envy, Refused Bequest

## Quick start

```bash
# Install
git clone https://github.com/gabrielrf97/tsmetrics.git && cd tsmetrics
cargo install --path .

# Analyze a directory
tsmetrics analyze ./src

# HTML report
tsmetrics analyze ./src --format html > report.html

# JSON for CI
tsmetrics analyze ./src --format json > metrics.json

# Exclude test files
tsmetrics analyze ./src --exclude "**/*.test.ts"
```

## Sample output

```
Analyzed 3 file(s) — 12 function(s) — 487 LOC total

╒════════════════════════════╤══════════════════════╤══════╤═════╤══════╤════════════╤═════════╤════════╕
│ File                       │ Function             │ Line │ LOC │ SLOC │ Complexity │ Nesting │ Params │
╞════════════════════════════╪══════════════════════╪══════╪═════╪══════╪════════════╪═════════╪════════╡
│ src/order/service.ts       │ processOrder         │   42 │  87 │   71 │         14 │       4 │      3 │
│ src/order/service.ts       │ validatePayload       │  130 │  22 │   18 │          3 │       2 │      1 │
│ src/user/repository.ts     │ findByEmail           │   18 │   9 │    8 │          2 │       1 │      1 │
╘════════════════════════════╧══════════════════════╧══════╧═════╧══════╧════════════╧═════════╧════════╛

Violations (1): processOrder — cyclomatic_complexity 14 ≥ 10 [warning]
```

Complexity is color-coded: green (&lt;5), yellow (5–9), red (≥10).

## Configuration

Drop a `tsmetrics.yaml` in your project root:

```yaml
exclude:
  - "**/*.test.ts"
  - "src/generated/**"

thresholds:
  cyclomatic_complexity:
    warning: 10
    error: 25
  loc:
    warning: 50
    error: 100
```

## Documentation

Full docs at **[gabrielrf97.github.io/tsmetrics](https://gabrielrf97.github.io/tsmetrics/)** — metrics reference, configuration, CLI flags, output formats, and design smell detection rules.

## Contributing

```bash
cargo test        # run tests
cargo fmt         # format
cargo clippy      # lint
```

Open an issue before starting significant work. PRs welcome.

## License

MIT — see [LICENSE](LICENSE).
