# Samskara

Samskara is the **pure datalog agent**. It exists entirely within CozoDB
relations — never files, code, or the OS. Its sole interface is relational data.

## Source of Truth

All rules, patterns, and architecture decisions are in the `rule` relation.
Query with: `?[id, compact, body] := *rule{id, compact, body}`

For read-only access from other agents, use the `samskara-reader` MCP server.

## VCS

Jujutsu (`jj`) is mandatory. Commit messages are CozoScript tuples.

## Architecture invariants

- Samskara ONLY interacts through CozoDB relations.
- Samskara owns its own DB (Sema data-ownership principle).
- Inter-agent communication via `samskara-lojix-contract` relations only.
