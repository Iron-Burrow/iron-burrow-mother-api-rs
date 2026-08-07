create table mother_api.portfolio_simulation_run (
  id uuid primary key,
  public_id text not null unique,
  ib_account_id uuid not null references mother_api.ib_account(id) on delete restrict,
  outcome text not null,
  request_schema_version integer not null,
  strategy_slug text not null,
  strategy_version text not null,
  engine_version text not null,
  evidence_digest text not null,
  input jsonb not null,
  evidence jsonb not null,
  result jsonb not null,
  created_at timestamptz not null default now(),
  constraint portfolio_simulation_run_public_id_valid check (public_id ~ '^psr_[0-9a-f]{32}$'),
  constraint portfolio_simulation_run_outcome_valid check (outcome in ('complete', 'partial', 'unsupported', 'failed')),
  constraint portfolio_simulation_run_schema_valid check (request_schema_version > 0),
  constraint portfolio_simulation_run_nonempty_identity check (
    btrim(strategy_slug) <> '' and btrim(strategy_version) <> '' and btrim(engine_version) <> ''
  ),
  constraint portfolio_simulation_run_digest_valid check (evidence_digest ~ '^[0-9a-f]{64}$'),
  constraint portfolio_simulation_run_payloads_valid check (
    jsonb_typeof(input) = 'object' and jsonb_typeof(evidence) = 'object' and jsonb_typeof(result) = 'object'
  )
);
create index idx_portfolio_simulation_run_account_timeline
  on mother_api.portfolio_simulation_run (ib_account_id, created_at desc, public_id desc);

create function mother_api.reject_portfolio_simulation_run_mutation()
returns trigger language plpgsql as $$ begin raise exception 'portfolio simulation runs are append-only'; end; $$;
create trigger portfolio_simulation_run_no_update
before update on mother_api.portfolio_simulation_run
for each row execute function mother_api.reject_portfolio_simulation_run_mutation();
create trigger portfolio_simulation_run_no_delete
before delete on mother_api.portfolio_simulation_run
for each row execute function mother_api.reject_portfolio_simulation_run_mutation();
