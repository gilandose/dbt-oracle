# Agent Instructions (AGENT.md)

If you are an AI Agent operating within this repository, this file serves as your system instructions.

## The Goal
`dbt-oracle` is designed to be the ultimate semantic layer between a dbt project and an AI agent. When editing dbt SQL models, you should heavily rely on querying the LSP to guarantee correctness instead of guessing column names or upstream models.

## How to use the LSP
The LSP accepts standard JSON-RPC 2.0 messages over `stdio`.
You can see an example of how to query the LSP in `agent_query.py`. 

### Common Queries
- **Hover**: Send a `textDocument/hover` request over a model name to fetch its upstream/downstream lineage.
- **Schema Info**: Send a `textDocument/hover` request over a column to fetch its data type.

Always verify your SQL modifications against the LSP before finalizing a task.
