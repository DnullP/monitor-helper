APP := monitor-helper
CARGO ?= cargo
ARGS ?=

.PHONY: help build release run list get set profile scan switch-dp switch-hdmi2 check fmt lint test clean

help:
	@printf "Targets:\n"
	@printf "  make build           Build debug binary\n"
	@printf "  make release         Build release binary\n"
	@printf "  make run ARGS='...'  Run with custom arguments\n"
	@printf "  make list            List detected monitors\n"
	@printf "  make get ARGS='--display 1 brightness'\n"
	@printf "  make set ARGS='--display 1 input hdmi'\n"
	@printf "  make profile         Show the detected monitor controller\n"
	@printf "  make scan ARGS='--display 1 --start 0x00 --end 0xFF'\n"
	@printf "  make switch-dp ARGS='--display 1 --target dp1 --duration 10'\n"
	@printf "  make switch-hdmi2 ARGS='--display 1 --interval 5'\n"
	@printf "  make check           Run cargo check\n"
	@printf "  make fmt             Format code\n"
	@printf "  make lint            Run clippy with warnings denied\n"
	@printf "  make test            Run tests\n"
	@printf "  make clean           Remove build artifacts\n"

build:
	$(CARGO) build

release:
	$(CARGO) build --release

run:
	$(CARGO) run -- $(ARGS)

list:
	$(CARGO) run -- list

get:
	$(CARGO) run -- get $(ARGS)

set:
	$(CARGO) run -- set $(ARGS)

profile:
	$(CARGO) run -- profile

scan:
	$(CARGO) run -- scan $(ARGS)

switch-dp:
	./scripts/switch_to_dp_and_restore.sh $(ARGS)

switch-hdmi2:
	./scripts/retry_switch_to_hdmi2.sh $(ARGS)

check:
	$(CARGO) check

fmt:
	$(CARGO) fmt

lint:
	$(CARGO) clippy --all-targets -- -D warnings

test:
	$(CARGO) test

clean:
	$(CARGO) clean