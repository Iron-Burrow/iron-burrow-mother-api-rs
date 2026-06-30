.PHONY: clippy smoke-db-migrate

clippy:
	cargo clippy --all-targets --all-features

smoke-db-migrate:
	./scripts/smoke/db-migrate.sh