.PHONY: clippy smoke-db-lifecycle smoke-db-migrate test-db-postgres

clippy:
	cargo clippy --all-targets --all-features

smoke-db-lifecycle:
	./scripts/smoke/db-lifecycle.sh

smoke-db-migrate: smoke-db-lifecycle

test-db-postgres:
	./scripts/test/db-postgres.sh
