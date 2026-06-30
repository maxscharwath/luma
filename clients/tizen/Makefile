# LUMA Samsung Tizen TV build & deploy system
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
PKG_ID     ?= LumaTV0001
APP_ID     ?= LumaTV0001.LUMA

# Signing profile name (created via Certificate Manager see SETUP.md).
PROFILE    ?= LUMA

# Build output + TV connection.
DIST       ?= dist
TV_IP      ?=
TV_PORT    ?= 26101

REPO_ROOT  := $(abspath ../..)
# First connected device serial, and the most recently built .wgt.
SERIAL      = $(shell $(SDB) devices 2>/dev/null | awk 'NR>1 && $$1!="" {print $$1; exit}')
WGT         = $(shell ls -t $(DIST)/*.wgt 2>/dev/null | head -1)

.DEFAULT_GOAL := help
.PHONY: help doctor build package connect devices install run deploy redeploy logs uninstall clean cert-selfsigned

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
	cd $(REPO_ROOT) && $(if $(SERVER),VITE_LUMA_SERVER="$(SERVER)") bun run build:tizen

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

logs: ## Tail the app's logs from the TV (Ctrl-C to stop)
	$(SDB) dlog -v time 2>/dev/null | grep -i -E 'luma|consoleMessage'

uninstall: connect ## Remove the app from the TV
	$(TIZEN) uninstall -p $(PKG_ID)

clean: ## Delete built .wgt artifacts
	rm -f $(DIST)/*.wgt

cert-selfsigned: ## Create a SELF-SIGNED profile (EMULATOR ONLY retail TVs need a Samsung cert, see SETUP.md)
	$(TIZEN) certificate -a LUMA -f luma-author -p luma1234 -c FR -n "LUMA Dev"
	$(TIZEN) security-profiles add -n $(PROFILE) \
		-a $(TIZEN_HOME)-data/keystore/author/luma-author.p12 -p luma1234
	@echo "⚠  Self-signed: installs on the emulator, NOT a retail Samsung TV."
