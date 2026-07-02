.PHONY: fmt check clippy test test-db-postgres smoke-db-lifecycle smoke-db-migrate smoke-beta-auth test-all

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt --all -- --check

check:
	cargo check --all-targets --all-features

test:
	cargo test --all-targets --all-features --locked

smoke-db-lifecycle:
	./scripts/smoke/db-lifecycle.sh

smoke-beta-auth:
	./scripts/smoke/beta-auth.sh

smoke-db-migrate: smoke-db-lifecycle

test-db-postgres:
	./scripts/test/db-postgres.sh

test-all:
	$(MAKE) fmt
	$(MAKE) clippy
	$(MAKE) check
	$(MAKE) test
	$(MAKE) test-db-postgres
	$(MAKE) smoke-db-lifecycle
	$(MAKE) smoke-beta-auth
