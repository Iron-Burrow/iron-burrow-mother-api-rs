-- Mother-owned, account-scoped terminal reports produced by Bigwig.
create table mother_api.async_report (
  id uuid primary key,
  public_id text not null unique,
  ib_account_id uuid not null references mother_api.ib_account(id) on delete restrict,
  requesting_api_key_id uuid references mother_api.api_key(id) on delete set null,
  requesting_client_id uuid references mother_api.ib_client(id) on delete set null,
  report_type text not null,
  report_version integer not null,
  input jsonb not null,
  idempotency_key_hash bytea not null,
  request_digest text not null,
  status text not null default 'accepted',
  report jsonb,
  report_digest text,
  failure_code text,
  accepted_at timestamptz not null default now(),
  started_at timestamptz,
  completed_at timestamptz,
  failed_at timestamptz,
  constraint async_report_public_id_valid check (public_id ~ '^rpt_[0-9a-f]{32}$'),
  constraint async_report_type_valid check (btrim(report_type) <> ''),
  constraint async_report_version_valid check (report_version > 0),
  constraint async_report_input_valid check (jsonb_typeof(input) = 'object'),
  constraint async_report_request_digest_valid check (request_digest ~ '^[0-9a-f]{64}$'),
  constraint async_report_status_valid check (status in ('accepted', 'running', 'completed', 'failed')),
  constraint async_report_terminal_shape check (
    (status in ('accepted', 'running') and report is null and report_digest is null and failure_code is null and completed_at is null and failed_at is null)
    or (status = 'completed' and report is not null and report_digest ~ '^[0-9a-f]{64}$' and failure_code is null and completed_at is not null and failed_at is null)
    or (status = 'failed' and report is null and report_digest is null and failure_code = 'execution_failed' and completed_at is null and failed_at is not null)
  ),
  unique (ib_account_id, idempotency_key_hash)
);
create index idx_async_report_account_public_id on mother_api.async_report (ib_account_id, public_id);

create function mother_api.reject_async_report_terminal_mutation()
returns trigger language plpgsql as $$
begin
  if old.status in ('completed', 'failed') then
    raise exception 'async reports are immutable after a terminal result';
  end if;
  return new;
end;
$$;
create trigger async_report_no_terminal_mutation
before update on mother_api.async_report
for each row execute function mother_api.reject_async_report_terminal_mutation();

insert into mother_api.capability (id, description) values
  ('reports.read', 'Read account-owned asynchronous reports.'),
  ('reports.write', 'Request account-owned asynchronous reports.'),
  ('reports.delivery.write', 'Deliver Bigwig asynchronous report terminal results.')
on conflict (id) do nothing;
