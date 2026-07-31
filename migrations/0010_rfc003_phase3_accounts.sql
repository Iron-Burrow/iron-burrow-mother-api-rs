-- RFC-003 Phase 3 account, session, and API-key ownership foundation.
-- This migration is additive: all pre-existing API keys remain legacy keys.

create table mother_api.ib_account (
  id uuid primary key default gen_random_uuid(),
  public_id text not null unique,
  status text not null default 'pending_verification',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  closed_at timestamptz,
  constraint ib_account_public_id_valid check (public_id ~ '^iba_[0-9a-f]{32}$'),
  constraint ib_account_status_valid check (status in ('pending_verification', 'active', 'suspended', 'closed')),
  constraint ib_account_timestamps_sane check (updated_at >= created_at and (closed_at is null or closed_at >= created_at))
);

create table mother_api.account_identity (
  id uuid primary key default gen_random_uuid(),
  ib_account_id uuid not null references mother_api.ib_account(id) on delete restrict,
  email_normalized text not null,
  email_lookup_hash bytea not null,
  status text not null default 'pending_verification',
  verified_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint account_identity_email_non_empty check (btrim(email_normalized) <> ''),
  constraint account_identity_hash_length check (length(email_lookup_hash) = 32),
  constraint account_identity_status_valid check (status in ('pending_verification', 'verified', 'disabled')),
  constraint account_identity_verification_matches_status check ((status = 'verified' and verified_at is not null) or (status <> 'verified' and verified_at is null)),
  constraint account_identity_timestamps_sane check (updated_at >= created_at)
);
create unique index account_identity_email_lookup_hash_unique on mother_api.account_identity(email_lookup_hash);
create index idx_account_identity_account on mother_api.account_identity(ib_account_id);

create table mother_api.email_verification (
  id uuid primary key default gen_random_uuid(),
  account_identity_id uuid not null references mother_api.account_identity(id) on delete cascade,
  purpose text not null,
  secret_hash bytea not null,
  expires_at timestamptz not null,
  consumed_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  constraint email_verification_purpose_valid check (purpose in ('signup', 'login')),
  constraint email_verification_secret_hash_length check (length(secret_hash) = 32),
  constraint email_verification_expiry_valid check (expires_at > created_at),
  constraint email_verification_single_terminal_state check (num_nonnulls(consumed_at, revoked_at) <= 1)
);
create unique index email_verification_secret_hash_unique on mother_api.email_verification(secret_hash);
create index idx_email_verification_identity_active on mother_api.email_verification(account_identity_id, purpose, expires_at) where consumed_at is null and revoked_at is null;

create table mother_api.browser_session (
  id uuid primary key default gen_random_uuid(),
  ib_account_id uuid not null references mother_api.ib_account(id) on delete cascade,
  account_identity_id uuid not null references mother_api.account_identity(id) on delete restrict,
  session_hash bytea not null,
  csrf_hash bytea not null,
  expires_at timestamptz not null,
  idle_expires_at timestamptz not null,
  last_seen_at timestamptz not null default now(),
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  constraint browser_session_hash_length check (length(session_hash) = 32 and length(csrf_hash) = 32),
  constraint browser_session_expiry_valid check (expires_at > created_at and idle_expires_at > created_at and last_seen_at >= created_at),
  constraint browser_session_revocation_valid check (revoked_at is null or revoked_at >= created_at)
);
create unique index browser_session_hash_unique on mother_api.browser_session(session_hash);
create index idx_browser_session_account_active on mother_api.browser_session(ib_account_id, expires_at, idle_expires_at) where revoked_at is null;

create table mother_api.ib_account_capability_grant (
  ib_account_id uuid not null references mother_api.ib_account(id) on delete cascade,
  capability_id text not null references mother_api.capability(id) on delete restrict,
  network_scope text not null default '*',
  status text not null default 'active',
  expires_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (ib_account_id, capability_id, network_scope),
  constraint ib_account_capability_grant_network_scope_valid check (network_scope = '*' or network_scope ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
  constraint ib_account_capability_grant_status_valid check (status in ('active', 'revoked')),
  constraint ib_account_capability_grant_revocation_valid check ((status = 'revoked' and revoked_at is not null) or (status = 'active' and revoked_at is null)),
  constraint ib_account_capability_grant_times_valid check (updated_at >= created_at and (expires_at is null or expires_at > created_at))
);
create index idx_ib_account_capability_grant_active on mother_api.ib_account_capability_grant(ib_account_id, capability_id, network_scope) where status = 'active';

alter table mother_api.api_key add column kind text;
alter table mother_api.api_key add column ib_account_id uuid references mother_api.ib_account(id) on delete restrict;
update mother_api.api_key set kind = 'legacy' where kind is null;
alter table mother_api.api_key alter column kind set not null;
alter table mother_api.api_key alter column consumer_id drop not null;
alter table mother_api.api_key add constraint api_key_kind_valid check (kind in ('legacy', 'account', 'anonymous_demo'));
alter table mother_api.api_key add constraint api_key_owner_matches_kind check (
  (kind = 'legacy' and consumer_id is not null and ib_account_id is null)
  or (kind = 'account' and consumer_id is null and ib_account_id is not null)
  or (kind = 'anonymous_demo' and consumer_id is null and ib_account_id is null)
);
create index idx_api_key_ib_account_id on mother_api.api_key(ib_account_id) where ib_account_id is not null;

create table mother_api.anonymous_demo_issuance_intent (
  id uuid primary key default gen_random_uuid(),
  secret_hash bytea not null unique,
  api_key_id uuid references mother_api.api_key(id) on delete restrict,
  expires_at timestamptz not null,
  consumed_at timestamptz,
  created_at timestamptz not null default now(),
  constraint anonymous_demo_issuance_intent_hash_length check (length(secret_hash) = 32),
  constraint anonymous_demo_issuance_intent_expiry_valid check (expires_at > created_at),
  constraint anonymous_demo_issuance_intent_consumption_valid check (consumed_at is null or consumed_at >= created_at)
);
create index idx_anonymous_demo_issuance_intent_active on mother_api.anonymous_demo_issuance_intent(expires_at) where consumed_at is null;

comment on table mother_api.ib_account is 'RFC-003 product account boundary. It is distinct from an on-chain address and legacy API consumer.';
comment on table mother_api.anonymous_demo_issuance_intent is 'One-time anonymous demo form intents. No visitor IP or raw credential is stored.';
