# TSM — TypeScript Metrics

**Fast, tree-sitter-powered static analysis for TypeScript and TSX codebases.**

[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

---

TSM parses TypeScript and TSX files using [tree-sitter](https://tree-sitter.github.io/tree-sitter/) and computes a rich set of software quality metrics — cyclomatic complexity, Halstead volume, maintainability index, object-oriented coupling and cohesion metrics, React component metrics, and more. It can surface threshold violations inline, detect well-known design smell patterns, and output results as a colour-coded table, JSON, or CSV for downstream tooling.

## Features

- **Parallel analysis** via Rayon — processes large codebases in seconds
- **Function-level metrics**: LOC, SLOC, cyclomatic complexity, nesting depth, parameter count, Halstead volume, Maintainability Index
- **Class-level metrics**: WMC, DIT, NOI, NOM / NOAM / NOOM, TCC, CBO, RFC, WOC
- **File-level metrics**: total LOC / SLOC, function count, class count, import count, Technical Debt score
- **React / TSX metrics**: JSX nesting depth, Number of Used Components (NUC)
- **Design smell detection**: God Class, Brain Method, Feature Envy, Refused Bequest
- **Configurable thresholds** via `tsm.yaml` with warning and error severity levels
- **Multiple output formats**: coloured table, JSON, CSV
- **CLI filters**: `--min-complexity`, `--min-loc` to focus on hotspots

---

## Installation

### From source

Requires [Rust 1.78+](https://rustup.rs/).

```bash
git clone https://github.com/your-org/tsm.git
cd tsm
cargo build --release
# Binary is at ./target/release/ts-static-analyzer
```

### Cargo install (once published)

```bash
cargo install ts-static-analyzer
```

---

## Usage

```
ts-static-analyzer analyze [OPTIONS] <PATHS>...
```

### Arguments

| Argument | Description |
|---|---|
| `<PATHS>...` | One or more files or directories to analyze |

### Options

| Flag | Description |
|---|---|
| `-f, --format <FORMAT>` | Output format: `table` (default), `json`, `csv` |
| `-v, --verbose` | Print skipped files and parse warnings to stderr |
| `--min-complexity <N>` | Show only functions with cyclomatic complexity ≥ N |
| `--min-loc <N>` | Show only functions with LOC ≥ N |
| `--timing` | Print elapsed time and thread count after analysis |

### Examples

Analyze an entire project directory:

```bash
ts-static-analyzer analyze ./src
```

Find complex hotspots quickly:

```bash
ts-static-analyzer analyze ./src --min-complexity 10
```

Export results for CI or a dashboard:

```bash
ts-static-analyzer analyze ./src --format json > metrics.json
ts-static-analyzer analyze ./src --format csv  > metrics.csv
```

Check timing on a large codebase:

```bash
ts-static-analyzer analyze ./src --timing
# Analysis completed in 0.842s across 312 file(s) using 8 thread(s)
```

### Sample table output

```
Analyzed 3 file(s) — 12 function(s) — 487 LOC total

╔════════════════════════════╦══════════════════════╦══════╦═════╦══════╦════════════╦═════════╦════════╗
║ File                       ║ Function             ║ Line ║ LOC ║ SLOC ║ Complexity ║ Nesting ║ Params ║
╠════════════════════════════╬══════════════════════╬══════╬═════╬══════╬════════════╬═════════╬════════╣
║ src/order/service.ts       ║ processOrder         ║   42 ║  87 ║   71 ║         14 ║       4 ║      3 ║
║ src/order/service.ts       ║ validatePayload      ║  130 ║  22 ║   18 ║          3 ║       2 ║      1 ║
║ src/user/repository.ts     ║ findByEmail          ║   18 ║   9 ║    8 ║          2 ║       1 ║      1 ║
╚════════════════════════════╩══════════════════════╩══════╩═════╩══════╩════════════╩═════════╩════════╝

Violations (1 total):

╔════════════════════════╦══════════════════╦══════╦════════════════════════╦═══════╦═══════════╦══════════╗
║ File                   ║ Entity           ║ Line ║ Metric                 ║ Value ║ Threshold ║ Severity ║
╠════════════════════════╬══════════════════╬══════╬════════════════════════╬═══════╬═══════════╬══════════╣
║ src/order/service.ts   ║ processOrder     ║   42 ║ cyclomatic_complexity  ║    14 ║        10 ║ warning  ║
╚════════════════════════╩══════════════════╩══════╩════════════════════════╩═══════╩═══════════╩══════════╝
```

Complexity cells are colour-coded: green (< 5), yellow (5–9), red (≥ 10).

---

## Metrics Reference

### Function-level metrics

These metrics are computed for every function declaration, function expression, arrow function, generator function, and class method.

| Metric | Column | Description |
|---|---|---|
| **LOC** | `LOC` | Total lines of code spanned by the function, including blank lines and comments |
| **SLOC** | `SLOC` | Source lines of code — blank and comment-only lines are excluded |
| **Cyclomatic Complexity** | `Complexity` | McCabe's CC. Starts at 1 and adds 1 per decision point: `if`, `while`, `for`, `for..in`, `switch case`, `catch`, ternary, and logical operators `&&`, `||`, `??`. Nested functions are measured independently and do not inflate the enclosing function's CC. |
| **Max Nesting Depth** | `Nesting` | Maximum depth of control-flow nesting within the function body |
| **Parameter Count** | `Params` | Number of formal parameters in the function signature |
| **Halstead Volume** | — | `V = N × log₂(η)` where N = total operators + operands, η = vocabulary (distinct operators + distinct operands). Computed per function; nested functions are excluded from the enclosing scope. |
| **Maintainability Index** | — | SEI / Visual Studio normalized variant: `MI = max(0, (171 − 5.2·ln(HV) − 0.23·CC − 16.2·ln(LOC)) / 171 × 100)`. Score is in [0, 100]; higher is better. |

### Class-level metrics

These metrics are computed for every class declaration, abstract class declaration, and named class expression.

| Metric | Abbreviation | Description |
|---|---|---|
| **Weighted Methods per Class** | WMC | Sum of cyclomatic complexities of all methods in the class. A high WMC indicates a class that is difficult to understand and change. |
| **Depth of Inheritance Tree** | DIT | Number of edges from the class to the root of its inheritance hierarchy. Deeper hierarchies increase the risk of inheriting unexpected behaviour. External / built-in parents count as root (DIT = 1 for their direct children). |
| **Number of Implemented Interfaces** | NOI | Count of interfaces listed in the `implements` clause. |
| **Number of Methods** | NOM | Total number of methods (including constructors, getters, setters, abstract methods, and overrides). Also broken down as NOAM (added methods) and NOOM (overriding methods). |
| **Tight Class Cohesion** | TCC | `connected_pairs / total_pairs` where two methods are connected if they share at least one `this.field` access. Range [0, 1]; higher means more cohesive. Classes with 0 or 1 method are vacuously cohesive (TCC = 1.0). Static methods are excluded. |
| **Coupling Between Objects** | CBO | Number of distinct external types a class is structurally coupled to through: `extends`, `implements`, property type annotations, method parameter types, and return type annotations. Method body runtime references (e.g. `new Foo()`) are intentionally excluded. Primitive types (`string`, `number`, `boolean`, `void`, `any`, etc.) are excluded naturally. |
| **Response For Class** | RFC | `NOM + |RS|` where the Response Set (RS) is the set of unique callees invoked across all method bodies. Callees include both function calls (`this.foo()`, `bar()`) and constructor calls (`new Service()`). |
| **Weight of Class** | WOC | `public_attributes / (public_attributes + public_methods)`. Approaches 1.0 for data-heavy DTO-like classes, 0.0 for behaviour-heavy service classes. Members with no accessibility modifier are treated as implicitly public. |

### File-level metrics

| Metric | Description |
|---|---|
| **Total LOC** | Sum of all lines in the file |
| **Total SLOC** | Source lines of code (blank and comment lines excluded) |
| **Function Count** | Number of functions discovered in the file |
| **Class Count** | Number of classes discovered in the file |
| **Import Count** | Number of import statements |
| **Technical Debt** | `total = Σ max(0, 1 − MI_f/100) × √(HV_f + 1)` across all functions. Also reported as `per_100_sloc = total / SLOC × 100` for cross-file comparison. Files without functions score zero. |

### React / TSX metrics

These metrics require TSX parsing and operate on React components (PascalCase function components and class components with a `render()` method).

| Metric | Abbreviation | Description |
|---|---|---|
| **JSX Nesting Depth** | — | Maximum depth of nested `jsx_element` or `jsx_fragment` nodes in the file. Self-closing elements do not open a new level. High nesting (typically > 4–5) signals a component that should be decomposed. |
| **Number of Used Components** | NUC | Count of distinct PascalCase JSX tag references (React components) within a component's render output. HTML intrinsic tags (lowercase) are excluded. Inline render-callback arrow functions are traversed; nested component definitions are scoped separately. |

---

## Detection Strategies

TSM implements four design smell detection strategies from *Object-Oriented Metrics in Practice* (Lanza & Marinescu, 2006).

### God Class

A class that has grown too large and too central, accumulating data and behaviour that belongs elsewhere. Flagged when **all three** conditions hold simultaneously:

- **WMC > 47** — complex enough to be a brain
- **TCC < 0.33** — low internal cohesion; the class is not a cohesive unit
- **ATFD > 5** — excessively dependent on other classes' data (Access To Foreign Data)

ATFD counts `obj.property` member expressions inside method bodies where the base object is a plain identifier that is neither `this` nor `super`. Method calls are not counted, and only the outermost access in a chain is counted (e.g. `a.b.c` contributes 1).

### Brain Method

A function that has become too long and too complex, effectively acting as the brain of the class. Flagged when **all three** conditions hold simultaneously:

- **SLOC > 65** — long enough to be hard to read (uses SLOC, not LOC, so comment-heavy functions are not falsely flagged)
- **CC > 5** — complex enough to be branchy
- **Nesting > 3** — deeply nested, indicating complicated control flow

### Feature Envy

A method that is more interested in the data of other classes than in the data of its own class. Flagged when **both** conditions hold:

- **ATFD > 5** — accesses more than five foreign attributes
- **ATFD > local_accesses** — foreign attribute accesses outnumber `this.x` accesses

ATFD counts data accesses (`obj.property`) only — method calls (`obj.method()`) and `this.method()` calls are excluded from both counts.

### Refused Bequest

A subclass that inherits from a parent but overrides very few — or none — of its methods, signalling the inheritance relationship may be inappropriate. Flagged when:

- **DIT > 0** — the class is a subclass
- **override_ratio < 0.33** — fewer than 33 % of the class's own methods override a parent method

NOOM counts both methods marked with the TypeScript `override` keyword **and** concrete methods that implement abstract methods from the direct parent without writing `override` (implicit overrides), preventing false positives on common TypeScript patterns.

---

## Configuration

TSM looks for a `tsm.yaml` file in the current working directory and in each analyzed path directory. If no file is found, built-in defaults apply. All fields are optional — unspecified metrics keep their defaults.

### Full example

```yaml
thresholds:
  cyclomatic_complexity:
    warning: 10   # default: 10
    error:   25   # default: 25

  loc:
    warning: 50   # default: 50
    error:   100  # default: 100

  nesting:
    warning: 3    # default: 3
    error:   5    # default: 5

  params:
    warning: 4    # default: 4
    error:   7    # default: 7

  wmc:
    warning: 20   # default: 20
    error:   50   # default: 50

  noi:
    warning: 3    # default: 3
    error:   5    # default: 5
```

Each metric accepts a `warning` and an `error` level. The error threshold must be greater than or equal to the warning threshold; TSM will reject the configuration otherwise. Violations at or above the error threshold are coloured red; violations at or above the warning threshold are coloured yellow.

### Configurable thresholds

| Key | Entity | Default warning | Default error |
|---|---|---|---|
| `cyclomatic_complexity` | Function | 10 | 25 |
| `loc` | Function | 50 | 100 |
| `nesting` | Function | 3 | 5 |
| `params` | Function | 4 | 7 |
| `wmc` | Class | 20 | 50 |
| `noi` | Class | 3 | 5 |

---

## Output Formats

### Table (default)

Coloured terminal table using Unicode box-drawing characters. Complexity cells are green, yellow, or red based on thresholds. A separate violations table is printed below if any threshold is exceeded.

```bash
ts-static-analyzer analyze ./src
ts-static-analyzer analyze ./src --format table
```

### JSON

Machine-readable output. Includes all file, function, and class metrics as well as a `violations` array. Suitable for CI pipelines, dashboards, or further processing.

```bash
ts-static-analyzer analyze ./src --format json
```

```json
{
  "files": [
    {
      "path": "src/order/service.ts",
      "total_loc": 342,
      "total_sloc": 271,
      "function_count": 8,
      "class_count": 1,
      "import_count": 5,
      "functions": [
        {
          "name": "processOrder",
          "file": "src/order/service.ts",
          "line": 42,
          "loc": 87,
          "sloc": 71,
          "cyclomatic_complexity": 14,
          "max_nesting": 4,
          "param_count": 3
        }
      ],
      "classes": [
        {
          "name": "OrderService",
          "file": "src/order/service.ts",
          "line": 10,
          "method_count": 8,
          "wmc": 32,
          "noi": 1
        }
      ]
    }
  ],
  "total_files": 1,
  "total_functions": 8,
  "total_loc": 342,
  "violations": [
    {
      "file": "src/order/service.ts",
      "line": 42,
      "entity": "processOrder",
      "metric": "cyclomatic_complexity",
      "value": 14,
      "threshold": 10,
      "severity": "warning"
    }
  ]
}
```

### CSV

RFC 4180 compliant CSV output. One row per function. Fields containing commas, quotes, or newlines are properly escaped. Suitable for import into spreadsheets or data analysis tools.

```bash
ts-static-analyzer analyze ./src --format csv
```

```
file,function,line,loc,sloc,complexity,nesting,params
src/order/service.ts,processOrder,42,87,71,14,4,3
src/order/service.ts,validatePayload,130,22,18,3,2,1
src/user/repository.ts,findByEmail,18,9,8,2,1,1
```

---

## Architecture

### Parsing

TSM uses [tree-sitter](https://tree-sitter.github.io/tree-sitter/) with the [`tree-sitter-typescript`](https://github.com/tree-sitter/tree-sitter-typescript) grammar, which provides two language configurations:

- **`LANGUAGE_TYPESCRIPT`** — used for `.ts` files
- **`LANGUAGE_TSX`** — used for `.tsx` files (enables JSX element nodes)

Tree-sitter produces a concrete syntax tree (CST) that is traversed by metric calculators. Because tree-sitter is error-recovering, partially invalid files are still partially analyzable.

### Parallel processing

File analysis is parallelized using [Rayon](https://github.com/rayon-rs/rayon). Each file is parsed and measured independently in a parallel iterator (`par_iter`). The number of OS threads that actually participate in the work is tracked and reported when `--timing` is enabled, using `thread::current().id()` rather than Rayon's logical thread index to account for the calling thread.

### Metric isolation

Nested functions are treated as separate units throughout. A nested function's operators, operands, and decision points do not contribute to the enclosing function's Halstead Volume, cyclomatic complexity, or Maintainability Index. This ensures each function is measured in isolation.

### Design decisions

- **Structural coupling only (CBO)**: method body runtime references like `new Foo()` are excluded from CBO because they are implementation details that change frequently; structural (type-level) coupling is more stable and actionable.
- **SLOC for Brain Method**: the Brain Method detector uses SLOC (not LOC) so that functions with many comment lines are not falsely flagged as overly long.
- **Implicit abstract overrides (Refused Bequest)**: a concrete method that implements an abstract method from the direct parent is counted as an override even without the `override` keyword, preventing a common false-positive pattern in TypeScript codebases.

---

## Contributing

Contributions are welcome. Please open an issue before starting significant work so we can discuss the approach.

```bash
# Run the test suite
cargo test

# Run with output
cargo test -- --nocapture

# Check formatting and lints
cargo fmt --check
cargo clippy
```

Tests are co-located with implementations in `#[cfg(test)]` modules. New metrics and strategies should have comprehensive unit tests covering edge cases, threshold boundaries, and the empty-input case.

---

## License

MIT. See [LICENSE](LICENSE).

---

## References

- Lanza, M. & Marinescu, R. (2006). *Object-Oriented Metrics in Practice*. Springer.
- McCabe, T. J. (1976). A Complexity Measure. *IEEE Transactions on Software Engineering*, 2(4), 308–320.
- Halstead, M. H. (1977). *Elements of Software Science*. Elsevier.
- Bieman, J. M. & Kang, B.-K. (1995). Cohesion and Reuse in an Object-Oriented System. *ACM SIGSOFT Software Engineering Notes*, 20(SI), 259–262.
- Coleman, D. et al. (1994). Using Metrics to Evaluate Software System Maintainability. *IEEE Computer*, 27(8), 44–49.
