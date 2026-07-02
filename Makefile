.PHONY: fmt check clippy test test-db-postgres smoke-db-lifecycle smoke-db-migrate smoke-beta-auth test-all clean

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt --all -- --check

check:
	cargo check --all-targets --all-features

test:
	cargo test --all-targets --all-features

smoke-db-lifecycle:
	./scripts/smoke/db-lifecycle.sh

smoke-beta-auth:
	./scripts/smoke/beta-auth.sh

smoke-db-migrate: smoke-db-lifecycle

test-db-postgres:
	./scripts/test/db-postgres.sh

test-all:
	make fmt
	make clippy
	make check
	make test
	make test-db-postgres
	make smoke-db-lifecycle
	make smoke-beta-auth
