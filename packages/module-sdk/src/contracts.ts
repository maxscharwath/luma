// The two augmentation points that give the module system end-to-end types.
//
// A module publishes its cross-module API and its events by *merging* into
// these interfaces from its own package, e.g. in `@kroma/module-torrents`:
//
//   declare module '@kroma/module-sdk' {
//     interface ModuleApiRegistry { torrents: TorrentsApi }
//     interface KromaEvents { 'torrents:done': { infoHash: string } }
//   }
//
// After that, `host.getModuleApi('torrents')` is typed as `TorrentsApi` and
// `bus.emit('torrents:done', ...)` is checked. They start empty on purpose.

/** Maps a module id to the API it exports for other modules to consume. */
// biome-ignore lint/suspicious/noEmptyInterface: augmentation target.
export interface ModuleApiRegistry {}

/** Maps an event name to its payload type. */
// biome-ignore lint/suspicious/noEmptyInterface: augmentation target.
export interface KromaEvents {}
