-- Capability IDs are release-owned by the compiled CapabilityRegistry. Grants
-- remain mutable PostgreSQL authorization state, but they no longer reference
-- a PostgreSQL capability catalogue.

do $$
declare
  unknown_ids text;
begin
  select string_agg(capability_id, ', ' order by capability_id)
  into unknown_ids
  from (
    select distinct capability_id
    from (
      select capability_id from mother_api.api_consumer_capability_grant
      union
      select capability_id from mother_api.api_key_capability_grant
      union
      select capability_id from mother_api.ib_account_capability_grant
      union
      select capability_id from mother_api.ib_client_capability_grant
      union
      select capability_id from mother_api.product_usage_event
    ) persisted
    where capability_id not in (
      'balances.read',
      'transfers.read',
      'workspace.activity.read',
      'catalog.read',
      'prices.read',
      'scan.read',
      'lab.read',
      'treasury.read',
      'treasury.snapshot.write',
      'reports.read',
      'reports.write'
    )
  ) unknown;

  if unknown_ids is not null then
    raise exception 'cannot remove mother_api.capability: unknown persisted capability IDs: %', unknown_ids;
  end if;
end;
$$;

-- Preserve a previous revocation or a narrower grant by only adding the
-- broad legacy baseline when a principal has no grant for that capability.
with baseline(capability_id) as (
  values ('balances.read'::text), ('transfers.read'::text)
)
insert into mother_api.api_consumer_capability_grant (
  consumer_id, capability_id, network_scope
)
select consumer.id, baseline.capability_id, '*'
from mother_api.api_consumer consumer
cross join baseline
where not exists (
  select 1
  from mother_api.api_consumer_capability_grant grant_row
  where grant_row.consumer_id = consumer.id
    and grant_row.capability_id = baseline.capability_id
)
on conflict (consumer_id, capability_id, network_scope) do nothing;

with baseline(capability_id) as (
  values ('balances.read'::text), ('transfers.read'::text)
)
insert into mother_api.api_key_capability_grant (
  api_key_id, capability_id, network_scope
)
select api_key.id, baseline.capability_id, '*'
from mother_api.api_key
cross join baseline
where api_key.kind = 'legacy'
  and not exists (
    select 1
    from mother_api.api_key_capability_grant grant_row
    where grant_row.api_key_id = api_key.id
      and grant_row.capability_id = baseline.capability_id
  )
on conflict (api_key_id, capability_id, network_scope) do nothing;

alter table mother_api.api_consumer_capability_grant
  drop constraint if exists api_consumer_capability_grant_capability_id_fkey;
alter table mother_api.api_key_capability_grant
  drop constraint if exists api_key_capability_grant_capability_id_fkey;
alter table mother_api.ib_account_capability_grant
  drop constraint if exists ib_account_capability_grant_capability_id_fkey;
alter table mother_api.ib_client_capability_grant
  drop constraint if exists ib_client_capability_grant_capability_id_fkey;
alter table mother_api.product_usage_event
  drop constraint if exists product_usage_event_capability_id_fkey;

drop table mother_api.capability;
