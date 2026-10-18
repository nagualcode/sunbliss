PY := .venv/bin/python

.PHONY: build test install validate lab parity superiority report check bench

build:
	cargo build --release

test:
	cargo test --release

install: build
	cargo truce install --au2 -p sunbliss

validate:
	auval -v aufx Snbl JSon | tail -3

# lab targets always rebuild first: a stale CLI binary silently skews results
parity: build
	cd lab && ../$(PY) parity.py

superiority: build
	cd lab && ../$(PY) superiority.py

report:
	cd lab && ../$(PY) report.py > /dev/null

check: install
	cd lab && ../$(PY) plugin_check.py au

lab: parity superiority report

bench: build
	@for q in high pristine extra; do printf "$$q: "; target/release/sunbliss-cli --bench 20 --rate 48000 --set quality=$$q; done
