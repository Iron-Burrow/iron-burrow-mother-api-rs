.PHONY: clippy smoke-beta-auth smoke-db-lifecycle smoke-db-migrate test-db-postgres

clippy:
	cargo clippy --all-targets --all-features

smoke-db-lifecycle:
	./scripts/smoke/db-lifecycle.sh

smoke-beta-auth:
	./scripts/smoke/beta-auth.sh

smoke-db-migrate: smoke-db-lifecycle

test-db-postgres:
	./scripts/test/db-postgres.sh

test-all:
	cargo test --all-targets --all-features --locked
	make test-db-postgres
	make smoke-db-lifecycle
	make smoke-beta-auth
