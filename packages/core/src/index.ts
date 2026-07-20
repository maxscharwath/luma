// @kroma/core shared domain logic (media/HEVC direct-play, player, i18n, remote,
// formatting, permissions). It re-exports @kroma/client so app code can keep
// importing the API client, wire types and schemas from `@kroma/core` unchanged.
export * from '@kroma/client';
export * from './browse';
export * from './discover';
export * from './format';
export * from './genre-art';
export * from './hevc';
export * from './i18n';
export * from './match';
export * from './people';
export * from './permissions';
export * from './platform';
export * from './player';
export * from './remote';
export * from './subtitles';
