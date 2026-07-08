// @luma/core shared domain logic (media/HEVC direct-play, player, i18n, remote,
// formatting, permissions). It re-exports @luma/client so app code can keep
// importing the API client, wire types and schemas from `@luma/core` unchanged.
export * from '@luma/client';
export * from './discover';
export * from './format';
export * from './hevc';
export * from './i18n';
export * from './match';
export * from './people';
export * from './permissions';
export * from './player';
export * from './remote';
export * from './subtitles';
