# KROMA Samsung Tizen TV build & deploy system
# ----------------------------------------------------------------------------
# Usage:   make doctor        # check toolchain + config
#          make package       # build web bundle + sign → .wgt
#          make deploy TV_IP=192.168.1.50   # package → install → launch on TV
#          make logs          # tail the app's logs from the TV
#
# Config can be set on the command line (make X TV_IP=...) or in a local
# `.tizen.env` file (copy from .tizen.env.example git-ignored).
# ----------------------------------------------------------------------------
-include .tizen.env

# Tooling auto-detected in PATH, else under a Tizen Studio install.
TIZEN_HOME ?= $(HOME)/tizen-studio
TIZEN      ?= $(shell command -v tizen 2>/dev/null || echo $(TIZEN_HOME)/tools/ide/bin/tizen)
SDB        ?= $(shell command -v sdb   2>/dev/null || echo $(TIZEN_HOME)/tools/sdb)

# App identity must match public/config.xml.
PKG_ID     ?= KromaTV001
APP_ID     ?= KromaTV001.KROMA

# Signing profile name (created via Certificate Manager see SETUP.md).
PROFILE    ?= KROMA

# Build output + TV connection.
DIST       ?= dist
TV_IP      ?=
TV_PORT    ?= 26101

REPO_ROOT  := $(abspath ../..)
# First connected device serial, and the most recently built .wgt.
SERIAL      = $(shell $(SDB) devices 2>/dev/null | awk 'NR>1 && $$1!="" {print $$1; exit}')
WGT         = $(shell ls -t $(DIST)/*.wgt 2>/dev/null | head -1)

# On-device live-dev (Vite HMR straight to the TV). HOST_IP is this machine's LAN
# address the TV loads the dev server from; detected by scripts/lan-ip.sh (the one
# source of truth shared with dev-device.sh). Override if it picks the wrong iface.
HOST_IP    ?= $(shell ./scripts/lan-ip.sh)
DEV_PORT   ?= 5174
DEV_STAGE  := .dev-shell

.DEFAULT_GOAL := help
.PHONY: help doctor build package connect devices install run deploy redeploy logs uninstall clean cert-selfsigned dev-shell

help: ## Show this help
	@awk 'BEGIN{FS=":.*?## "} /^[a-zA-Z_-]+:.*?## /{printf "  \033[33m%-14s\033[0m %s\n",$$1,$$2}' $(MAKEFILE_LIST)

doctor: ## Check toolchain + print current config
	@echo "tizen   : $(TIZEN)";   $(TIZEN) version 2>/dev/null || echo "  ✗ not found see SETUP.md"
	@echo "sdb     : $(SDB)";      $(SDB) version 2>/dev/null | head -1 || echo "  ✗ not found"
	@echo "profile : $(PROFILE)"
	@echo "app id  : $(APP_ID)  (package $(PKG_ID))"
	@echo "TV      : $(if $(TV_IP),$(TV_IP):$(TV_PORT),<unset pass TV_IP=...>)"
	@echo "device  : $(if $(SERIAL),$(SERIAL),<none connected>)"

build: ## Build the web bundle (Vite → dist/). Pass SERVER=http://host:4040 to bake a default address.
	cd $(REPO_ROOT) && $(if $(SERVER),VITE_KROMA_SERVER="$(SERVER)") bun run build:tizen

package: build ## Build + sign the app → dist/*.wgt
	$(TIZEN) package -t wgt -s $(PROFILE) -- $(DIST)
	@echo "✓ packaged: $$(ls -t $(DIST)/*.wgt 2>/dev/null | head -1)"

connect: ## sdb connect to the TV (requires TV_IP + Developer Mode on)
	@test -n "$(TV_IP)" || { echo "Set TV_IP, e.g.  make deploy TV_IP=192.168.1.50"; exit 1; }
	$(SDB) connect $(TV_IP):$(TV_PORT)

devices: ## List connected devices
	$(SDB) devices

install: ## Install the latest .wgt on the connected TV
	@test -n "$(WGT)" || { echo "No .wgt found run 'make package' first"; exit 1; }
	@test -n "$(SERIAL)" || { echo "No device run 'make connect TV_IP=...'"; exit 1; }
	# No -t: the Tizen CLI auto-targets the single connected device (its target
	# lookup doesn't match the ip:port sdb serial for network TVs).
	$(TIZEN) install -n $(notdir $(WGT)) -- $(DIST)

run: ## Launch the app on the connected TV
	@test -n "$(SERIAL)" || { echo "No device connected"; exit 1; }
	$(TIZEN) run -p $(APP_ID) || $(SDB) shell 0 was_execute $(APP_ID)

deploy: package connect install run ## Full pipeline: build → sign → connect → install → launch

redeploy: build connect install run ## Faster re-deploy (reuses signing; skips re-package if unchanged)

dev-shell: connect ## One-time: install a live-dev shell (redirects the TV to the Vite dev server on HOST_IP). Then run `bun run dev:tizen:device`.
	@test -n "$(HOST_IP)" || { echo "Could not detect HOST_IP pass it: make dev-shell HOST_IP=192.168.1.20 TV_IP=..."; exit 1; }
	@echo "→ dev shell redirects to http://$(HOST_IP):$(DEV_PORT)/"
	# A stripped hand-written config.xml is rejected by retail firmware (no error
	# surfaced), so reuse the real build verbatim and only swap index.html for a tiny
	# loader that redirects to the dev server. Same app id as the shipped app: this
	# replaces it while developing; `make deploy` restores the real one.
	# build:fast = vite build without the tsgo typecheck, so WIP type errors elsewhere
	# in the tree never block getting a live-dev shell onto the TV.
	cd $(REPO_ROOT) && bun run --filter '@kroma/tizen' build:fast
	@rm -rf $(DEV_STAGE) && cp -R $(DIST) $(DEV_STAGE)
	@rm -f $(DEV_STAGE)/*.wgt $(DEV_STAGE)/author-signature.xml $(DEV_STAGE)/signature1.xml
	@sed -e 's|__HOST_IP__|$(HOST_IP)|g' -e 's|__DEV_PORT__|$(DEV_PORT)|g' public/index.dev.html > $(DEV_STAGE)/index.html
	$(TIZEN) package -t wgt -s $(PROFILE) -- $(DEV_STAGE)
	@WGT="$$(ls -t "$(DEV_STAGE)"/*.wgt | head -1)"; $(TIZEN) install -n "$$(basename "$$WGT")" -- $(DEV_STAGE)
	$(TIZEN) run -p $(APP_ID) || $(SDB) shell 0 was_execute $(APP_ID)
	@echo ""
	@echo "✓ Dev shell installed & launched. Now start the dev server:"
	@echo "    KROMA_TV_HOST=$(HOST_IP) bun run dev:tizen:device"
	@echo "  Edit code → it hot-reloads on the TV. Run 'make deploy' to restore the shipped app."

logs: ## Tail the app's logs from the TV (Ctrl-C to stop)
	$(SDB) dlog -v time 2>/dev/null | grep -i -E 'kroma|consoleMessage'

uninstall: connect ## Remove the app from the TV
	$(TIZEN) uninstall -p $(PKG_ID)

clean: ## Delete built .wgt artifacts
	rm -f $(DIST)/*.wgt
	rm -rf $(DEV_STAGE)

cert-selfsigned: ## Create a SELF-SIGNED profile (EMULATOR ONLY retail TVs need a Samsung cert, see SETUP.md)
	$(TIZEN) certificate -a KROMA -f kroma-author -p kroma1234 -c FR -n "KROMA Dev"
	$(TIZEN) security-profiles add -n $(PROFILE) \
		-a $(TIZEN_HOME)-data/keystore/author/kroma-author.p12 -p kroma1234
	@echo "⚠  Self-signed: installs on the emulator, NOT a retail Samsung TV."
