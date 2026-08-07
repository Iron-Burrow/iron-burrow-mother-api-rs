-- Password authentication is additive. Existing verified passwordless
-- accounts remain intact until the bounded operator-led migration described
-- by SPEC-031 gives them an Argon2id password hash.

alter table mother_api.account_identity
  add column password_hash text;

alter table mother_api.account_identity
  add constraint account_identity_password_hash_non_empty
  check (password_hash is null or btrim(password_hash) <> '');

-- The passwordless endpoint is removed in this release, so no pre-existing
-- entry link may remain consumable through a future accidental route restore.
update mother_api.email_verification
  set revoked_at = now()
  where consumed_at is null and revoked_at is null;
