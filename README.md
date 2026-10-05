# dbt-oracle 🔮

A lightning-fast, dialect-aware Language Server Protocol (LSP) engine for dbt, written in Rust.

`dbt-oracle` gives developers and AI agents an omniscient view over their entire data DAG. By dynamically parsing your `manifest.json`, it provides real-time, mathematically guaranteed context without locking you into proprietary platforms.

## Features
- **Dynamic IntelliSense**: Autocompletes `ref()` and `source()` by parsing your actual DAG.
- **Lineage Tracing**: Hover over any model to instantly see its upstream dependencies and downstream dependents.
- **Schema Parsing**: Hover over a column name to see its data type and description pulled directly from your YAML configs.
- **Dialect-Aware Snippets**: Contextual autocompletes tailored for Snowflake, DuckDB, and Apache Iceberg (e.g., `QUALIFY`, `PIVOT`, `read_parquet`).
- **Agent-Ready**: Perfect for Model Context Protocol (MCP) servers, allowing AI coding assistants to confidently navigate your data warehouse.

## Getting Started

1. Ensure you have Rust installed (`cargo`).
2. Clone this repo and build the binary:
   ```bash
   cargo build --release
   ```
3. Connect your editor (VSCode, Cursor, Neovim) to the resulting `./target/release/dbt-oracle` binary using any standard LSP client extension.

## License
MIT License
