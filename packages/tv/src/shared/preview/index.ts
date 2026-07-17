// Samsung Tizen "Smart Hub Preview" integration.
//
// When the KROMA tile is focused on the TV home screen even with the app NOT
// running Samsung can expand it into a carousel of content tiles. We surface
// the newest movies there and deep-link straight into playback when a tile is
// selected.
//
// The pieces, by concern:
//   • tizen.ts the minimal Tizen typings + the `tizen` feature-detect;
//   • cards.ts building the carousel tile JSON from the live catalog;
//   • service.ts persisting that JSON + nudging the background service;
//   • deeplink.ts decoding the tile selection that launched/targeted the app.
//
// Everything is feature-detected against the `tizen` global, so it is a no-op on
// webOS and in the browser dev server.

export { onDeepLink, readDeepLink } from '#tv/shared/preview/deeplink';
export { publishPreview } from '#tv/shared/preview/service';
export type { DeepLink } from '#tv/shared/preview/types';
