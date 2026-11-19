# ⚡ Thunderstorm (`thunderstorm-rs`)

[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)]()

**Thunderstorm** is a high-performance, in-memory SNOMED CT UK Clinical Edition ontology engine and clinical exploration suite written in Rust.

It compiles the full UK Clinical Extension hierarchy, defining relationships, active/inactive concept states, national NHS England general practice code usage statistics, and NHS England Primary Care Domain (PCD) reference sets into a compressed in-memory Compressed Sparse Row (CSR) graph snapshot.

---

## Key Features

- **⚡ Sub-Millisecond In-Memory Queries**: Loads the full 640k+ concept graph snapshot from cold storage in **< 350ms** and executes multi-criteria ECL / semantic queries in **micro-to-milliseconds** using SIMD-accelerated Roaring Bitmaps.
- **🏷️ Multi-Criteria ECL-Style Search**:
  - Transitive closure hierarchy filtering (`--include-isa` and `--exclude-isa`).
  - Semantic tag bitmasking (`disorder`, `finding`, `procedure`, `body structure`, etc.).
  - Defining attribute and role-group relationships (`--include-attr` / `--exclude-attr`).
  - Expressive text matching with phrases, globs/wildcards (`*`, `?`), and disjunctions (`|`).
- **📊 Real-World National Clinical Usage**: Directly embeds NHS England primary care event frequencies (over 100,000+ active concepts calibrated against real UK GP consultations), allowing instant frequency-based ranking and activity thresholds (`--with-usage`, `--min-usage`).
- **📚 Primary Care Domain (PCD) Clusters**: Native support for 800+ UK national reference sets (QOF disease registers, clinical risk groups, seasonal vaccinations, audit clusters) with union (`--include-refset`) and exclusion (`--exclude-refset`) logic.
- **📦 Clean Separation of Concerns**:
  - `thunderstorm-build`: Administrative ingestion of official TRUD RF2 ZIP releases, code usage files, and PCD refsets into `thunderstorm_graph.bin`.
  - `thunderstorm-query`: Interactive clinical exploration CLI (`search`, `node`, `refsets`).
  - `thunderstorm` library: Reusable, zero-infrastructure Rust crate for downstream clinical software, MCP agents, and data pipelines.

---

## Architecture Overview

```
                      Official TRUD Releases
                 ┌───────────────────────────────┐
                 │  • SNOMED UK Clinical RF2 ZIP │
                 │  • NHS England GP Usage TXT   │
                 │  • PCD Refsets ZIP / TXT      │
                 └───────────────┬───────────────┘
                                 │
                                 ▼
                     [thunderstorm-build]
                 Stream & parse in ~4.5s
                                 │
                                 ▼
                    [thunderstorm_graph.bin]
                 Compact CSR Snapshot (~85 MB)
                                 │
                 ┌───────────────┴───────────────┐
                 ▼                               ▼
       [thunderstorm-query]              Downstream Services
     - Interactive Clinical CLI        - External MCP Servers
     - Sub-millisecond ECL queries     - Research Pipelines
     - Real-world frequency ranks      - Clinical AI Agents
```

---

## Installation

### Option A: Cargo (Linux, macOS, Windows)

Requires [Rust & Cargo](https://rustup.rs/) installed:

```bash
# Clone the repository
git clone https://github.com/your-org/thunderstorm-rs.git
cd thunderstorm-rs

# Compile and install binaries directly into ~/.cargo/bin (or %USERPROFILE%\.cargo\bin)
cargo install --path crates/thunderstorm
```

Verify the installation:
```bash
thunderstorm-build --help
thunderstorm-query --help
```

### Option B: Nix Flake (NixOS / Linux / macOS)

Hermetic, reproducible build without manual toolchain management:

```bash
# Build the package (binaries in result/bin/)
nix build

# Install to your user profile
nix profile install .

# Or run directly without installing:
nix run .#thunderstorm-query -- search asthma
```

---

## Building the Graph Snapshot

Because official SNOMED CT UK Clinical Edition releases are licensed by NHS England / SNOMED International, raw archives are not bundled with git releases.

To compile a graph snapshot:

1. Download the latest **SNOMED CT UK Clinical Edition Snapshot ZIP** from [NHS England TRUD](https://isd.digital.nhs.uk/trud/).
2. *(Optional)* Download the annual **NHS England Code Usage** text file.
3. *(Optional)* Download the **Primary Care Domain (PCD) Refsets** archive.

Run `thunderstorm-build` with the `--install` flag:

```bash
thunderstorm-build build \
  --zip /path/to/uk_sct2cl_43.0.1_20260923000001Z.zip \
  --with-usage /path/to/SNOMED_code_usage_2024-25.txt \
  --with-refsets /path/to/PCD_Refset_Content_text_files.zip \
  --install
```

> **Note on `--install`**: Passing `--install` places the compiled `thunderstorm_graph.bin` into your platform's user directory (`~/.thunderstorm/thunderstorm_graph.bin` on Linux/macOS, `%USERPROFILE%\.thunderstorm\thunderstorm_graph.bin` on Windows). `thunderstorm-query` will automatically find and load it from any directory without requiring the `-g` flag!

---

## CLI Usage Guide (`thunderstorm-query`)

### 1. Multi-Criteria Clinical Search (`search`)

```bash
thunderstorm-query search [OPTIONS] [QUERY]
```

#### Textual Search Syntax:
- **Positional Search**: Provide terms directly:
  ```bash
  thunderstorm-query search asthma --with-usage
  ```
- **Multiple Words / Phrases**: Wrap multi-word queries in quotes:
  ```bash
  thunderstorm-query search "acute exacerbation of asthma" --with-usage
  ```
- **Repeatable Inclusions (`AND` logic)**:
  ```bash
  thunderstorm-query search --include-text "asthma" --include-text "severe" --with-usage
  ```
- **Repeatable Exclusions (`NOT` logic)**:
  ```bash
  thunderstorm-query search "asthma" --exclude-text "childhood" --exclude-text "mild"
  ```
- **Wildcards (`*` and `?`)**:
  ```bash
  thunderstorm-query search "decompensat*"       # matches decompensated, decompensation
  thunderstorm-query search "infarct*"          # matches infarction, infarcts
  ```
- **Pipe Disjunctions (`OR` logic)**:
  ```bash
  thunderstorm-query search "infarct*|ischemi*" --include-tag disorder --with-usage
  ```

#### ECL Hierarchy & Semantic Filters:
- **Is-A Subsumption (`<< root`)**:
  ```bash
  # All disorders under Heart failure (84114007)
  thunderstorm-query search --include-isa 84114007 --include-tag disorder --with-usage
  ```
- **Surgical Hierarchy Exclusion (`MINUS << root`)**:
  ```bash
  thunderstorm-query search --include-isa 84114007 --exclude-isa 42343007
  ```
- **Attribute Filtering (`TYPE:VALUE[:INCLUDE_DESCENDANTS]`)**:
  ```bash
  # Disorders where Finding site is Airway structure (89187006)
  thunderstorm-query search --include-attr 363698007:89187006:true --include-tag disorder
  ```

#### Refset Exploration:
- **Union of Refsets (`Cluster1 OR Cluster2`)**:
  ```bash
  # Combines Coronary Heart Disease (CHD_COD) and Heart Failure (HF_COD)
  thunderstorm-query search --include-refset CHD_COD --include-refset HF_COD --with-usage
  ```
- **Exclude Refset (`MINUS Cluster`)**:
  ```bash
  thunderstorm-query search --include-refset AST_COD --exclude-refset SEVAST_COD
  ```

---

### 2. Inspecting a Concept (`node`)

Deep inspection of a single clinical concept:

```bash
thunderstorm-query node 195967001
```

Outputs:
- Official Concept ID (SCTID) and Fully Specified Name (FSN).
- Active/Inactive status and semantic tag.
- National GP primary care usage frequency.
- Associated Primary Care Domain (PCD) clusters.
- Immediate parents with individual usage counts.
- Immediate children with individual usage counts.
- Defining attributes grouped by relationship role group.

---

### 3. Listing PCD Reference Sets (`refsets`)

Explore all 800+ embedded Primary Care Domain clusters:

```bash
# List all clusters sorted by code count
thunderstorm-query refsets --sort-by count

# Filter clusters by keyword (e.g. Asthma)
thunderstorm-query refsets --filter "AST" --sort-by codes

# Show annual aggregate national usage across each cluster
thunderstorm-query refsets --filter "HF_" --with-usage
```

---

## Environment Variables

| Variable | Description |
| :--- | :--- |
| `THUNDERSTORM_GRAPH_PATH` | Overrides the default graph snapshot path for all commands. |

Resolution order for graph snapshot:
1. Explicit CLI argument: `-g <PATH>` / `--graph <PATH>`.
2. Environment variable: `THUNDERSTORM_GRAPH_PATH`.
3. Current directory: `./thunderstorm_graph.bin`.
4. Parent directories: `../thunderstorm_graph.bin`, `../../thunderstorm_graph.bin`.
5. Global user directory: `~/.thunderstorm/thunderstorm_graph.bin` (or `%USERPROFILE%\.thunderstorm\thunderstorm_graph.bin`).

---

## Library Usage (`thunderstorm` crate)

To use Thunderstorm as a library in another Rust project or MCP server:

Add to your `Cargo.toml`:
```toml
[dependencies]
thunderstorm = { path = "../thunderstorm-rs/crates/thunderstorm" }
```

Example Rust code:
```rust
use thunderstorm::{SnomedGraph, SearchQuery, SemanticTag};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Fast mmap / bincode load in < 350ms
    let graph = SnomedGraph::load_from_file("thunderstorm_graph.bin")?;

    // Build composable clinical search query
    let query = SearchQuery::new()
        .include_text("asthma")
        .include_tag(SemanticTag::Disorder)
        .include_refset("AST_COD")
        .sort_by_usage(true)
        .with_limit(10);

    let results = graph.search(&query);
    for r in results {
        println!("{:<18} | {:>10?} uses | {}", r.sctid, r.usage, r.term);
    }

    Ok(())
}
```

---

## License

Dual-licensed under either:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
