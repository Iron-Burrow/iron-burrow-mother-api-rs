-- The existing api_consumer boundary is a compatibility owner only. RFC-003
-- replaces it with IBAccount ownership in a later migration; no API key may
-- become broader during that move.

create table if not exists mother_api.capability (
  id text primary key,
  description text not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint capability_id_normalized
    check (id ~ '^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$'),
  constraint capability_description_non_empty
    check (btrim(description) <> ''),
  constraint capability_timestamps_sane
    check (updated_at >= created_at)
);

create table if not exists mother_api.api_consumer_capability_grant (
  consumer_id uuid not null
    references mother_api.api_consumer (id) on delete cascade,
  capability_id text not null
    references mother_api.capability (id) on delete restrict,
  network_scope text not null default '*',
  status text not null default 'active',
  expires_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (consumer_id, capability_id, network_scope),
  constraint api_consumer_capability_grant_network_scope_valid
    check (network_scope = '*' or network_scope ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
  constraint api_consumer_capability_grant_status_known
    check (status in ('active', 'revoked')),
  constraint api_consumer_capability_grant_revocation_matches_status
    check (
      (status = 'revoked' and revoked_at is not null)
      or (status = 'active' and revoked_at is null)
    ),
  constraint api_consumer_capability_grant_timestamps_sane
    check (
      updated_at >= created_at
      and (expires_at is null or expires_at > created_at)
      and (revoked_at is null or revoked_at >= created_at)
    )
);

create table if not exists mother_api.api_key_capability_grant (
  api_key_id uuid not null
    references mother_api.api_key (id) on delete cascade,
  capability_id text not null
    references mother_api.capability (id) on delete restrict,
  network_scope text not null default '*',
  status text not null default 'active',
  expires_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (api_key_id, capability_id, network_scope),
  constraint api_key_capability_grant_network_scope_valid
    check (network_scope = '*' or network_scope ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
  constraint api_key_capability_grant_status_known
    check (status in ('active', 'revoked')),
  constraint api_key_capability_grant_revocation_matches_status
    check (
      (status = 'revoked' and revoked_at is not null)
      or (status = 'active' and revoked_at is null)
    ),
  constraint api_key_capability_grant_timestamps_sane
    check (
      updated_at >= created_at
      and (expires_at is null or expires_at > created_at)
      and (revoked_at is null or revoked_at >= created_at)
    )
);

create index if not exists idx_api_consumer_capability_grant_active
  on mother_api.api_consumer_capability_grant (consumer_id, capability_id, network_scope)
  where status = 'active';

create index if not exists idx_api_key_capability_grant_active
  on mother_api.api_key_capability_grant (api_key_id, capability_id, network_scope)
  where status = 'active';

comment on table mother_api.capability is
  'Application-defined capability registry. Required declarations are applied by Mother API reference data.';

comment on table mother_api.api_consumer_capability_grant is
  'Compatibility owner grants for legacy API consumers. They are not IBAccount identity records.';

comment on table mother_api.api_key_capability_grant is
  'Narrowing grants for issued API keys. A key must also satisfy its owner grant.';
