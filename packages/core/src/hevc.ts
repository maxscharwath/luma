// Public barrel for the HEVC / direct-play module. Split into:
//  - ./hevc/capabilities runtime codec/capability probing + the cached verdict
//  - ./hevc/directplay direct-play / audio-support / per-track delivery plans
// Re-exported here so `@kroma/core` (and `./hevc`) keep the same surface.

export * from './hevc/capabilities';
export * from './hevc/directplay';
