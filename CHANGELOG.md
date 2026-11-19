# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-19

### Added
- **Compressed Sparse Row (CSR) Graph Representation**:
  - High-performance, in-memory directed acyclic graph (DAG) representation of SNOMED CT releases.
  - Sub-millisecond transitive closure evaluations for complex clinical hierarchies.
  - Efficient zero-copy bincode serialization and deserialization for release snapshots (`.bin`).
- **Sub-millisecond ECL Engine**:
  - Full support for SNOMED CT Expression Constraint Language (ECL) primitives:
    - Self (`<id>`), descendants (`<<id>`), direct children (`<id`), ancestors (`>>id`), direct parents (`>id`).
    - Logical operators: Conjunction (`AND`), Disjunction (`OR`), Negation (`MINUS`).
    - Refset membership evaluation (`^<refset_id>`).
- **NHS England Code Usage Integration**:
  - Seamless ingestion of NHS England GP Code Usage statistics.
  - Ranking query and search results by actual UK primary care clinical prevalence.
- **Primary Care Domain (PCD) Refset Support**:
  - Native loading and indexing of Primary Care Domain reference sets (clustering by domain/refset).
  - Inspection and querying commands (`refsets`) with regex filtering and sorting options.
- **Multi-Criteria Search Engine**:
  - Flexible text search with case-insensitive token matching, substring inclusion (`--include-text`), exclusion (`--exclude-text`), wildcards (`*`, `?`), and disjunctions (`|`).
  - Positional search syntax (`thunderstorm-query search "Asthma"`).
- **CLI Utilities**:
  - `thunderstorm-build`: Compiles raw RF2 distribution ZIPs with optional NHS code usage into a compact binary graph snapshot. Supports automatic installation to `~/.thunderstorm/thunderstorm_graph.bin`.
  - `thunderstorm-query`: High-speed interactive or scriptable query tool for ECL evaluation, concept lookups, concept paths/ancestries, and refset discovery.
- **Packaging & Deployment**:
  - Flake-based Nix packaging (`flake.nix`) supporting instant global installation via `nix profile add .`.
  - Cargo workspace configuration with release profiles optimized with Fat LTO, codegen-units=1, and symbol stripping.
