-- RFC-003 Phase 5 append-only Workspace activity and evidence log.

create table mother_api.workspace_activity_event (
  id uuid primary key default gen_random_uuid(),
  public_id text not null unique,
  workspace_id uuid not null references mother_api.workspace(id) on delete cascade,
  event_type text not null,
  actor_kind text not null,
  actor_api_key_id uuid references mother_api.api_key(id) on delete restrict,
  payload_version integer not null default 1,
  payload jsonb not null,
  occurred_at timestamptz not null default now(),
  constraint workspace_activity_event_public_id_valid check (public_id ~ '^wae_[0-9a-f]{32}$'),
  constraint workspace_activity_event_type_valid check (event_type in (
    'workspace.created', 'workspace.renamed', 'workspace.archived', 'workspace.restored',
    'member_address.added', 'member_address.label_added', 'member_address.label_removed',
    'balance.observed', 'transfer.observed'
  )),
  constraint workspace_activity_event_actor_valid check (
    (actor_kind = 'browser_session' and actor_api_key_id is null) or
    (actor_kind = 'api_key' and actor_api_key_id is not null)
  ),
  constraint workspace_activity_event_payload_valid check (
    payload_version = 1 and jsonb_typeof(payload) = 'object'
  )
);
create index idx_workspace_activity_event_timeline
  on mother_api.workspace_activity_event(workspace_id, occurred_at desc, public_id desc);

create function mother_api.reject_workspace_activity_event_mutation()
returns trigger language plpgsql as $$
begin
  if tg_op = 'DELETE' and pg_trigger_depth() > 1 then
    return old;
  end if;
  raise exception 'workspace activity events are append-only';
end;
$$;
create trigger workspace_activity_event_no_update
  before update on mother_api.workspace_activity_event
  for each row execute function mother_api.reject_workspace_activity_event_mutation();
create trigger workspace_activity_event_no_delete
  before delete on mother_api.workspace_activity_event
  for each row execute function mother_api.reject_workspace_activity_event_mutation();

-- Migrations run before reference-data reconciliation. Seed this declaration
-- so the deliberately granted rows below satisfy their capability foreign key;
-- reference-data owns its ongoing description reconciliation.
insert into mother_api.capability (id, description)
values ('workspace.activity.read', 'Read account-owned Workspace activity and evidence.')
on conflict (id) do nothing;

-- Existing account principals and their account-owned keys intentionally gain
-- the Phase 5 read capability. Legacy and anonymous keys are excluded.
insert into mother_api.ib_account_capability_grant (ib_account_id, capability_id, network_scope)
select id, 'workspace.activity.read', '*'
from mother_api.ib_account
where status = 'active'
on conflict do nothing;

insert into mother_api.api_key_capability_grant (api_key_id, capability_id, network_scope)
select id, 'workspace.activity.read', '*'
from mother_api.api_key
where kind = 'account' and status = 'active'
on conflict do nothing;
