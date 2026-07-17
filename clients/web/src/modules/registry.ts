// The web app's frontend module registry.
//
// Adding a module is one import + one register() line (the compile-time tier
// that ships today and works on every target, including the Chromium-53 TVs). A
// future runtime-load tier (Module Federation, web + desktop only) would
// register remotely-loaded modules here too, behind this same registry API - so
// nothing downstream changes.

import { acquisitionModule } from '@kroma/module-acquisition';
import { indexerModule } from '@kroma/module-indexer';
import { ModuleRegistry } from '@kroma/module-sdk';
import { remoteModule } from '@kroma/module-remote';
import { torrentsModule } from '@kroma/module-torrents';
import { vpnModule } from '@kroma/module-vpn';
import { generatedModules } from '@kroma/modules-generated';

export const moduleRegistry = new ModuleRegistry();
moduleRegistry.register(indexerModule);
moduleRegistry.register(torrentsModule);
moduleRegistry.register(vpnModule);
moduleRegistry.register(remoteModule);
moduleRegistry.register(acquisitionModule);
// Single-file (codegen) modules register themselves via the generated roster.
for (const m of generatedModules) moduleRegistry.register(m);
