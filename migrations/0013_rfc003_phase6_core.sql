-- RFC-003 Phase 6: delegated Clients, product usage, and immutable treasury snapshots.

create table mother_api.ib_client (
  id uuid primary key default gen_random_uuid(),
  public_id text not null unique,
  ib_account_id uuid not null references mother_api.ib_account(id) on delete restrict,
  label text not null,
  status text not null default 'active',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  revoked_at timestamptz,
  constraint ib_client_public_id_valid check (public_id ~ '^ibc_[0-9a-f]{32}$'),
  constraint ib_client_label_valid check (char_length(btrim(label)) between 1 and 120),
  constraint ib_client_status_valid check (status in ('active', 'revoked')),
  constraint ib_client_revocation_matches_status check ((status = 'active' and revoked_at is null) or (status = 'revoked' and revoked_at is not null))
);
create index idx_ib_client_account_active on mother_api.ib_client(ib_account_id, status, created_at desc);

create table mother_api.ib_client_capability_grant (
  ib_client_id uuid not null references mother_api.ib_client(id) on delete cascade,
  capability_id text not null references mother_api.capability(id) on delete restrict,
  network_scope text not null default '*',
  status text not null default 'active',
  expires_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (ib_client_id, capability_id, network_scope),
  constraint ib_client_capability_grant_scope_valid check (network_scope = '*' or network_scope ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
  constraint ib_client_capability_grant_status_valid check (status in ('active', 'revoked')),
  constraint ib_client_capability_grant_revocation_valid check ((status = 'revoked' and revoked_at is not null) or (status = 'active' and revoked_at is null))
);

alter table mother_api.api_key add column client_id uuid references mother_api.ib_client(id) on delete restrict;
alter table mother_api.api_key drop constraint api_key_kind_valid;
alter table mother_api.api_key add constraint api_key_kind_valid check (kind in ('legacy', 'account', 'anonymous_demo', 'agent'));
alter table mother_api.api_key drop constraint api_key_owner_matches_kind;
alter table mother_api.api_key add constraint api_key_owner_matches_kind check (
  (kind = 'legacy' and consumer_id is not null and ib_account_id is null and client_id is null)
  or (kind = 'account' and consumer_id is null and ib_account_id is not null and client_id is null)
  or (kind = 'anonymous_demo' and consumer_id is null and ib_account_id is null and client_id is null)
  or (kind = 'agent' and consumer_id is null and ib_account_id is null and client_id is not null)
);
create index idx_api_key_client_id on mother_api.api_key(client_id) where client_id is not null;

create table mother_api.product_quota_policy (
  id uuid primary key default gen_random_uuid(),
  ib_account_id uuid references mother_api.ib_account(id) on delete cascade,
  ib_client_id uuid references mother_api.ib_client(id) on delete cascade,
  requests_per_day integer not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint product_quota_policy_owner check (num_nonnulls(ib_account_id, ib_client_id) = 1),
  constraint product_quota_policy_limit_valid check (requests_per_day > 0),
  unique nulls not distinct (ib_account_id, ib_client_id)
);

create table mother_api.product_usage_event (
  id uuid primary key default gen_random_uuid(),
  api_key_id uuid references mother_api.api_key(id) on delete set null,
  ib_account_id uuid references mother_api.ib_account(id) on delete set null,
  ib_client_id uuid references mother_api.ib_client(id) on delete set null,
  capability_id text not null references mother_api.capability(id) on delete restrict,
  network_slug text,
  outcome text not null,
  response_class text,
  occurred_at timestamptz not null default now(),
  constraint product_usage_event_outcome_valid check (outcome in ('accepted', 'denied', 'rate_limited', 'completed')),
  constraint product_usage_event_response_class_valid check (response_class is null or response_class in ('successful', 'client_error', 'server_error'))
);
create index idx_product_usage_event_key_time on mother_api.product_usage_event(api_key_id, occurred_at desc);
create index idx_product_usage_event_account_time on mother_api.product_usage_event(ib_account_id, occurred_at desc);

create table mother_api.workspace_treasury_snapshot (
  id uuid primary key default gen_random_uuid(),
  public_id text not null unique,
  workspace_id uuid not null references mother_api.workspace(id) on delete restrict,
  requested_as_of jsonb not null,
  quote_currency text not null,
  asset_slugs jsonb not null,
  payload jsonb not null,
  captured_at timestamptz not null default now(),
  constraint workspace_treasury_snapshot_public_id_valid check (public_id ~ '^wts_[0-9a-f]{32}$'),
  constraint workspace_treasury_snapshot_payload_valid check (jsonb_typeof(payload) = 'object'),
  constraint workspace_treasury_snapshot_assets_valid check (jsonb_typeof(asset_slugs) = 'array')
);
create index idx_workspace_treasury_snapshot_timeline on mother_api.workspace_treasury_snapshot(workspace_id, captured_at desc, public_id desc);

create function mother_api.reject_workspace_treasury_snapshot_mutation()
returns trigger language plpgsql as $$ begin raise exception 'workspace treasury snapshots are append-only'; end; $$;
create trigger workspace_treasury_snapshot_no_update before update on mother_api.workspace_treasury_snapshot for each row execute function mother_api.reject_workspace_treasury_snapshot_mutation();
create trigger workspace_treasury_snapshot_no_delete before delete on mother_api.workspace_treasury_snapshot for each row execute function mother_api.reject_workspace_treasury_snapshot_mutation();

insert into mother_api.capability (id, description) values
  ('catalog.read', 'Read authenticated Data Lab asset and network catalog views.'),
  ('prices.read', 'Read authenticated Data Lab price views.'),
  ('scan.read', 'Read authenticated Workspace-member Scan views.'),
  ('lab.read', 'Run authenticated curated Data Lab research.'),
  ('treasury.read', 'Read account-owned Workspace treasury snapshots.'),
  ('treasury.snapshot.write', 'Capture account-owned Workspace treasury snapshots.')
on conflict (id) do nothing;

insert into mother_api.ib_account_capability_grant (ib_account_id, capability_id, network_scope)
select account.id, capability.id, '*'
from mother_api.ib_account account
cross join mother_api.capability capability
where account.status = 'active'
  and capability.id in ('catalog.read', 'prices.read', 'scan.read', 'lab.read', 'treasury.read', 'treasury.snapshot.write')
on conflict do nothing;
