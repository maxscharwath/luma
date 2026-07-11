// The web app's frontend module registry.
//
// Adding a module is one import + one register() line (the compile-time tier
// that ships today and works on every target, including the Chromium-53 TVs). A
// future runtime-load tier (Module Federation, web + desktop only) would
// register remotely-loaded modules here too, behind this same registry API - so
// nothing downstream changes.

import { indexerModule } from '@luma/module-indexer';
import { ModuleRegistry } from '@luma/module-sdk';
import { torrentsModule } from '@luma/module-torrents';
import { generatedModules } from '@luma/modules-generated';

export const moduleRegistry = new ModuleRegistry();
moduleRegistry.register(indexerModule);
moduleRegistry.register(torrentsModule);
// Single-file (codegen) modules register themselves via the generated roster.
for (const m of generatedModules) moduleRegistry.register(m);
