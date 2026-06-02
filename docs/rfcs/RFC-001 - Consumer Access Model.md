# RFC-001 — Consumer Access Model

---
status: draft
owner: iron-burrow
last_reviewed: 2026-06-01
agent_edit_policy: update_when_relevant
---

Status: Draft

Author: José Maria Sosa

Created: 2026-06-01

---

# Summary

The Mother API is the public contract boundary of Iron Burrow.

Its purpose is not to expose every piece of blockchain data available within the Iron Burrow ecosystem.

Its purpose is to expose useful, documented, stable, customer-driven capabilities through a predictable and trustworthy interface.

The Mother API serves humans, applications, AI agents, and automated systems that require access to curated blockchain intelligence.

This RFC defines who may consume the Mother API, how access is granted, how usage is measured, and the principles that guide future API development.

---

# Motivation

Iron Burrow operates multiple internal services:

* Indexers
* Read models
* Infrastructure gateways
* Blockchain nodes
* Analytics services
* Protocol-specific intelligence systems

These systems exist to gather, normalize, and process information.

The Mother API exists to transform that internal complexity into externally consumable products.

Without a clear access model, the Mother API risks becoming:

* An undocumented data dump
* A free infrastructure subsidy
* A scraping target
* An unbounded maintenance burden

This RFC establishes a sustainable path that protects Iron Burrow while encouraging collaboration and experimentation.

---

# Vision

Iron Burrow is not attempting to become a generic blockchain data provider.

Iron Burrow aims to become a trusted source of deterministic blockchain intelligence.

Consumers should be able to ask:

* What is the historical yield of this protocol?
* What changed between these blocks?
* How concentrated are token holders?
* Which validators are performing best?
* What deterministic evidence supports this conclusion?

And receive documented, reproducible answers.

---

# Consumer Categories

## Friends

Early users.

Developers.

Hackathon teams.

Community members.

Characteristics:

* Free access
* Generous limits
* Feedback encouraged

Goal:

Help shape the platform.

---

## Partners

Organizations actively collaborating with Iron Burrow.

Examples:

* Protocol teams
* Validator operators
* Research groups
* MCP developers
* Strategic integrations

Characteristics:

* Higher limits
* Dedicated support
* Custom capabilities may be considered

Goal:

Build mutually beneficial relationships.

---

## Public Consumers

Unknown internet users.

Characteristics:

* Self-service onboarding
* Conservative limits
* Standardized capabilities only

Goal:

Protect platform resources while allowing exploration.

---

## Internal Services

Iron Burrow-operated systems.

Examples:

* Sentinel
* Read Models
* Internal workers
* Operational tooling

Characteristics:

* Authenticated service-to-service communication
* Separate operational controls

Goal:

Reliable platform operation.

---

# Customer-Shaped Development

The Mother API follows customer-shaped development.

Capabilities are added when there is a demonstrated consumer need.

The default answer to speculative functionality is:

"Who needs this?"

Examples:

Good reason:

A partner requires historical Aave yield calculations.

Bad reason:

The endpoint might be useful someday.

This policy intentionally limits API surface area.

Every public capability carries maintenance cost.

---

# Access Model

Every request belongs to a consumer.

Every consumer receives credentials.

Every credential maps to a plan.

Every plan defines limits.

Conceptually:

Consumer
→ Credential
→ Plan
→ Usage Policy

The exact implementation is intentionally left outside the scope of this RFC.

---

# Metering Philosophy

Iron Burrow measures before it bills.

The platform must first understand:

* Which endpoints are valuable
* Which endpoints are expensive
* Which endpoints are abused
* Which consumers generate meaningful usage

Billing decisions should emerge from real usage patterns.

Not speculation.

---

# Rate Limiting

Rate limits exist to preserve platform health.

Rate limits are not punishment.

Rate limits protect:

* Infrastructure
* Data providers
* Shared resources
* Other consumers

Limits may vary by:

* Consumer category
* Endpoint family
* Computational cost
* Historical behavior

---

# Partner Program

Partnerships are encouraged.

Partners may request:

* Higher limits
* New endpoints
* New protocols
* Additional datasets
* Specialized integrations

Partnerships are based on mutual value creation.

Not merely consumption.

---

# Product Development Principle

The Mother API should grow through real demand.

Preferred sequence:

1. Consumer problem identified
2. Internal capability developed
3. Contract documented
4. Usage measured
5. Capability stabilized

Avoid:

1. Build giant surface area
2. Hope someone uses it

---

# Future Billing

Billing is explicitly out of scope for this RFC.

Future billing may consider:

* Request volume
* Compute consumption
* Historical data depth
* Premium datasets
* Dedicated infrastructure

Any billing model must preserve accessibility for experimentation and learning.

---

# Non-Goals

The Mother API is not:

* A generic RPC provider
* A blockchain explorer clone
* An unlimited archive node service
* A free public data warehouse

Those services already exist elsewhere.

Iron Burrow focuses on deterministic intelligence and curated capabilities.

---

# Guiding Principle

The burrow grows because somebody needs another tunnel.

Not because tunnels are easy to dig.
