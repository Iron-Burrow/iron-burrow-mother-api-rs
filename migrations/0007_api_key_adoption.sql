create table if not exists mother_api.api_consumer (
  id uuid primary key default gen_random_uuid(),
  slug text not null,
  display_name text not null,
  category text not null,
  status text not null default 'active',
  metadata jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint api_consumer_slug_normalized
    check (
      slug = lower(btrim(slug))
      and slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'
    ),
  constraint api_consumer_display_name_non_empty
    check (btrim(display_name) <> ''),
  constraint api_consumer_category_known
    check (category in ('friend', 'partner', 'public', 'internal')),
  constraint api_consumer_status_known
    check (status in ('active', 'disabled', 'archived')),
  constraint api_consumer_metadata_object
    check (jsonb_typeof(metadata) = 'object'),
  constraint api_consumer_timestamps_sane
    check (updated_at >= created_at)
);

create table if not exists mother_api.api_key (
  id uuid primary key default gen_random_uuid(),
  consumer_id uuid not null references mother_api.api_consumer (id) on delete restrict,
  label text not null,
  key_prefix text not null,
  key_hash bytea not null,
  hash_algorithm text not null default 'sha256',
  status text not null default 'active',
  expires_at timestamptz,
  revoked_at timestamptz,
  last_used_at timestamptz,
  metadata jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint api_key_label_non_empty
    check (btrim(label) <> ''),
  constraint api_key_prefix_normalized
    check (
      key_prefix = lower(btrim(key_prefix))
      and key_prefix ~ '^[a-z0-9]+(_[a-z0-9]+)*$'
    ),
  constraint api_key_hash_algorithm_known
    check (hash_algorithm = 'sha256'),
  constraint api_key_hash_sha256_length
    check (length(key_hash) = 32),
  constraint api_key_status_known
    check (status in ('active', 'disabled', 'revoked')),
  constraint api_key_metadata_object
    check (jsonb_typeof(metadata) = 'object'),
  constraint api_key_timestamps_sane
    check (
      updated_at >= created_at
      and (expires_at is null or expires_at > created_at)
      and (last_used_at is null or last_used_at >= created_at)
      and (revoked_at is null or revoked_at >= created_at)
    ),
  constraint api_key_revoked_at_matches_status
    check (
      (status = 'revoked' and revoked_at is not null)
      or (status <> 'revoked' and revoked_at is null)
    )
);

create unique index if not exists api_consumer_slug_unique
  on mother_api.api_consumer (slug);

create unique index if not exists api_key_key_prefix_unique
  on mother_api.api_key (key_prefix);

create unique index if not exists api_key_key_hash_unique
  on mother_api.api_key (key_hash);

create index if not exists idx_api_key_consumer_id
  on mother_api.api_key (consumer_id);

create index if not exists idx_api_key_active_key_prefix
  on mother_api.api_key (key_prefix)
  where status = 'active';

comment on table mother_api.api_consumer is
  'Future inbound API consumer identities. Migrations and reference data must not create real customer records.';

comment on table mother_api.api_key is
  'Future inbound API key credentials. Migrations and reference data must not create real API keys.';

comment on column mother_api.api_key.key_prefix is
  'Non-secret lookup hint only. Future auth must verify the full presented secret by hashing it and comparing against key_hash.';

comment on column mother_api.api_key.key_hash is
  'SHA-256 digest of a future cryptographically random high-entropy API-key secret. Do not reuse this pattern for passwords or low-entropy user-created tokens.';

comment on column mother_api.api_key.hash_algorithm is
  'Initial API-key digest algorithm. sha256 is acceptable only for cryptographically random high-entropy API-key secrets.';
