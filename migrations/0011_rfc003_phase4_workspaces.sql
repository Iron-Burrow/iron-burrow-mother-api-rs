-- RFC-003 Phase 4 durable, account-owned Workspace foundation.
-- Activity/evidence persistence is deliberately deferred to Phase 5.

create table mother_api.workspace (
  id uuid primary key default gen_random_uuid(),
  public_id text not null unique,
  owner_ib_account_id uuid not null references mother_api.ib_account(id) on delete restrict,
  name text not null,
  description text,
  status text not null default 'active',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  archived_at timestamptz,
  constraint workspace_public_id_valid check (public_id ~ '^wsp_[0-9a-f]{32}$'),
  constraint workspace_name_valid check (char_length(btrim(name)) between 1 and 120),
  constraint workspace_description_valid check (description is null or char_length(description) <= 1000),
  constraint workspace_status_valid check (status in ('active', 'archived')),
  constraint workspace_archive_matches_status check (
    (status = 'active' and archived_at is null) or (status = 'archived' and archived_at is not null)
  )
);
create index idx_workspace_owner_status on mother_api.workspace(owner_ib_account_id, status, updated_at desc);

create table mother_api.workspace_member_address (
  id uuid primary key default gen_random_uuid(),
  public_id text not null unique,
  workspace_id uuid not null references mother_api.workspace(id) on delete restrict,
  network_slug text not null references mother_api.network(slug) on delete restrict,
  address text not null,
  client_ref text,
  created_at timestamptz not null default now(),
  constraint workspace_member_address_public_id_valid check (public_id ~ '^wma_[0-9a-f]{32}$'),
  constraint workspace_member_address_evm_valid check (address ~ '^0x[0-9a-f]{40}$'),
  constraint workspace_member_address_client_ref_valid check (client_ref is null or char_length(client_ref) <= 120),
  unique (workspace_id, network_slug, address)
);
create index idx_workspace_member_address_workspace on mother_api.workspace_member_address(workspace_id, created_at);

create table mother_api.workspace_member_address_label (
  member_address_id uuid not null references mother_api.workspace_member_address(id) on delete restrict,
  label text not null,
  created_at timestamptz not null default now(),
  primary key (member_address_id, label),
  constraint workspace_member_address_label_valid check (char_length(btrim(label)) between 1 and 64)
);
create unique index workspace_member_address_label_casefold_unique
  on mother_api.workspace_member_address_label(member_address_id, lower(label));
