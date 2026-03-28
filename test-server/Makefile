COMPOSE = docker compose

.PHONY: up down reset logs status shell admin

## Start the XMPP test server (detached).
up:
	$(COMPOSE) up -d
	@echo ""
	@echo "Waiting for Prosody to become healthy..."
	@for i in $$(seq 1 30); do \
		STATUS=$$(docker inspect --format='{{.State.Health.Status}}' rexisce-prosody 2>/dev/null); \
		if [ "$$STATUS" = "healthy" ]; then \
			echo "Prosody is ready."; \
			break; \
		fi; \
		sleep 1; \
	done
	@echo ""
	@echo "Connect with: alice@localhost / alice123 on localhost:5222"

## Stop the XMPP test server (preserves data).
down:
	$(COMPOSE) down

## Full reset: stop, remove volumes, rebuild.
reset:
	$(COMPOSE) down -v
	$(COMPOSE) up -d
	@echo "Server reset complete. All data wiped, accounts re-created."

## Tail Prosody logs.
logs:
	$(COMPOSE) logs -f prosody

## Show container status and health.
status:
	@docker inspect --format='Status: {{.State.Health.Status}} | Running: {{.State.Running}} | Uptime: {{.State.StartedAt}}' rexisce-prosody 2>/dev/null || echo "Container not running."

## Open a shell inside the Prosody container.
shell:
	docker exec -it rexisce-prosody bash

## Open Prosody's interactive admin shell.
admin:
	docker exec -it rexisce-prosody prosodyctl shell
