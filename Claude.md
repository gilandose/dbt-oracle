# Instructions for Claude (Claude.md)

Hi Claude! If you are interacting with this repository, here is what you need to know:
- This is `dbt-oracle`, a Rust-based LSP for dbt.
- It parses `target/manifest.json` to provide dynamic autocompletions and hover information.
- When assisting users, prioritize efficiency and leverage the provided `agent_query.py` script if you need to test LSP responses.
- Follow standard Rust idioms and ensure `cargo fmt` and `cargo clippy` pass before committing changes.
