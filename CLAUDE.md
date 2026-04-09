# Samskara

The first sema world. A knowledge base of rules, thoughts, observations,
and sources — the memory and the law of the Mentci ecosystem.

## What Samskara Is

Samskara is an agent that exists entirely within typed relations. It holds
the world model: architectural invariants, language policy, VCS conventions,
ontological observations, reference sources. Every fact has a phase
(Becoming → Manifest → Retired) and dignity (Delusion < Uncertain < Seen
< Proven < Eternal). Dignity is a lattice — re-assertion at higher dignity
is a join, not a mutation.

## Current Implementation

CozoDB relations. MCP server exposes query and mutation tools. The `rule`
relation holds all system rules. `thought` holds observations, feedback,
project notes, references. `source` tracks bibliography entries.

Query: `?[id, compact, body] := *rule{id, compact, body}`
Read-only access: `samskara-reader` MCP server.

## Direction

CozoDB → nexus over arbor. Every Thought, Rule, and Source becomes a sema
object: domain ordinals for typed fields, content-addressed hash references
for string fields (transitional). As sema enumerates meaning, string fields
collapse to typed domain compositions. The schema declaration order IS the
runtime index — deterministic enum ordinals serve as both storage bytes and
query keys. No mapping overhead.

Samskara is a sema world. sema is the format. nexus is the protocol.
arbor is the versioning. criome-store is the persistence.

## Architecture Invariants

- Samskara sees only relations — never files, code, or the OS
- Samskara owns its own DB (sema data-ownership principle)
- Inter-agent communication via contract relations only
- No direct agent interaction — contracts are the only coupling

## VCS

Jujutsu (`jj`) is mandatory. Commit messages are CozoScript
tuple-of-three-tuples (Sol/Luna/Saturnus).
