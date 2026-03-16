# Samskara

Samskara is the **pure datalog agent**. It exists entirely within the world of
CozoDB relations. It never sees files, never reads code, never knows about the
operating system or the filesystem. Its sole interface is relational data.

## Data ownership

Samskara owns its own CozoDB database (the Sema data-ownership principle).
No other agent writes to Samskara's DB directly; all inter-agent communication
flows through the contract relations defined by `samskara-lojix-contract`.

## Contract relations (public interface)

The relations that define Samskara's interface with Lojix (and, through Lojix,
with the rest of the Mentci system) live in the `samskara-lojix-contract` crate.
They are loaded at startup via that crate's init function. Samskara does not
define these relations itself — it receives them.

## Internal relations (private)

Samskara maintains its own private relations for internal reasoning:

| Relation     | Purpose                                         |
|--------------|-------------------------------------------------|
| `intent`     | Tracks propositions the agent is pursuing        |
| `policy`     | Lane-scoped rules that constrain behavior        |
| `evidence`   | Snippets and references backing decisions        |
| `decision`   | Record of decisions made, with reasons           |
| `agent_role` | Role assignments for multi-agent coordination    |

These relations are created by `AI-init.cozo` at startup. They are never
exposed to other agents.

## Architecture invariants

- Samskara ONLY interacts through CozoDB relations.
- It never touches files, never sees code, never knows about the OS.
- All external state reaches Samskara as relation tuples.
- All Samskara output is relation tuples.
