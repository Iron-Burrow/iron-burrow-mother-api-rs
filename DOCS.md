---
status: active
owner: iron-burrow
last_reviewed: 2026-06-01
agent_edit_policy: update_when_relevant
---

# DOCS.md

This file defines the documentation policy for the `iron-burrow-mother-api-rs`
(Mother API) repository. It is the authoritative reference for how documents
in this repo are organized, labeled, and maintained. The policy is shared
across Iron Burrow repositories so contributors and agents see the same
rules everywhere.

## Document categories

Mother API recognizes the following kinds of documents:

- **Living docs**: continuously updated descriptions of current truth, such as
  [README.md](README.md), [DOCS.md](DOCS.md), and [AGENTS.md](AGENTS.md).
- **Change log**: [HISTORY.md](HISTORY.md) is an append-style narrative log
  of notable changes. New entries are added at the bottom under a dated
  heading; existing entries are not rewritten.
- **Contracts**: the promised public and internal surface of Mother API,
  captured in `CONTRACTS.md`. Contracts are binding promises, not
  aspirations.
- **Decisions (ADRs)**: accepted architectural decisions stored under
  [docs/adr/](docs/adr/). Decisions are durable and only superseded by newer
  ADRs.
- **Specs**: accepted or draft implementation specs under
  [docs/specs/](docs/specs/). Specs follow accepted RFCs and describe how a
  feature is built.
- **RFCs (proposals)**: design discussions under [docs/rfcs/](docs/rfcs/). RFCs
  are not truth unless their status is `accepted`.
- **Rejected RFCs**: kept under [docs/rfcs/](docs/rfcs/) with `status: rejected`.
  They remain valuable as decision records.
- **Superseded documents**: previously authoritative documents replaced by
  newer ones. Marked `status: superseded` and, when applicable, link the
  replacement via `superseded_by`.
- **Archives**: historical memory under [docs/archive/](docs/archive/).
  Archives are not modernized.

## Status vocabulary

The `status` field in front matter is authoritative. Allowed values:

- `active`: current truth; should be kept up to date.
- `contract`: a binding promise (used for `CONTRACTS.md`).
- `draft`: a proposal under discussion; not authoritative.
- `accepted`: an accepted RFC, spec, or ADR.
- `rejected`: a proposal that was considered and not adopted.
- `superseded`: replaced by a newer document.
- `archived`: historical memory; not current truth.

## Required front matter

Every project markdown document in this repo must begin with YAML front matter
of this form:

```yaml
---
status: active | contract | draft | accepted | rejected | superseded | archived
owner: iron-burrow
last_reviewed: YYYY-MM-DD
agent_edit_policy: update_when_relevant | update_only_if_contract_changes | do_not_update | ask_before_editing | append_only
---
```

For `superseded` documents, include `superseded_by: <relative path>` only when
a concrete replacement exists.

`agent_edit_policy` describes how agents should treat the file:

- `update_when_relevant`: edit freely when the change is on-topic.
- `update_only_if_contract_changes`: edit only when an actual contract change
  motivates it (typical for `CONTRACTS.md`).
- `do_not_update`: do not edit; usually for `archived` documents.
- `ask_before_editing`: do not edit without explicit human approval.
- `append_only`: add new entries; do not rewrite existing ones (typical for
  ADR logs).

## Documentation map

```
README.md                         Repo entrance and short navigation guide
DOCS.md                           Documentation policy
AGENTS.md                         Agent instructions
HISTORY.md                        Append-style project change log
CONTRACTS.md                      Public and internal contract promises
docs/rfcs/                        Proposals and design discussions
docs/specs/                       Accepted/draft implementation specs
docs/adr/                         Accepted architectural decisions
docs/archive/                     Historical memory
```

Documentation subdirectories are created when their first child arrives.
Mother API is greenfield Rust and carries no legacy v0 documentation.

## Rules

- Location is useful, but front matter `status` is authoritative.
- [README.md](README.md) must stay brief and navigational. It is not a place
  to dump design discussion.
- RFCs are proposals, not current truth, unless their status is `accepted`.
- Rejected RFCs remain useful as decision records and must not be deleted.
- Archived documents are memory, not current truth, and must not be
  modernized.
- If an endpoint is added or changed in the future, `CONTRACTS.md` must be
  created or updated in the same change.
- Do not allow code behavior to imply promises that documentation does not
  explain.
- Prefer small, focused doc PRs over large rewrites.

## CONTRACTS.md

`CONTRACTS.md` captures the reliable `/v1/*` endpoints Mother API exposes
plus `/health`. Per the rules above, those promises must remain captured in
`CONTRACTS.md` with `status: contract` and
`agent_edit_policy: update_only_if_contract_changes`.

The [README.md](README.md) endpoint section should stay brief and
navigational. `CONTRACTS.md` is the authoritative reference for implemented
Mother API endpoint promises.

The first Mother API spec under [docs/specs/](docs/specs/) is
`SPEC-001-dis-aave-v3-realized-yield.md`, which describes how Mother API
will consume the DIS Aave V3 realized yield internal endpoint. It does not
relieve the need for `CONTRACTS.md` covering Mother API's own public
surface.
