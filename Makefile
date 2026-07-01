.PHONY: clippy smoke-db-migrate test-db-postgres

clippy:
	cargo clippy --all-targets --all-features

smoke-db-migrate:
	./scripts/smoke/db-migrate.sh

test-db-postgres:
	./scripts/test/db-postgres.sh
