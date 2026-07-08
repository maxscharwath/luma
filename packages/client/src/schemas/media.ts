// Runtime schemas for the media / catalogue domain mirrors of the ts-rs
// generated wire types (see `../generated`, the single source of truth).
//
// Same conventions as `./accounts.ts`: each schema mirrors a Rust struct, adds
// runtime validation + branded ids, and carries a compile-time drift guard.
// `true satisfies SameKeys<…>` breaks the build if a field is added/removed in
// Rust and `gen:types` is rerun without updating the schema. Tagged unions use
// `z.discriminatedUnion` and skip the drift line (it yields false failures).
//
// Schemas are defined in dependency order: a type used as a nested field appears
// above the schema that references it.

import { z } from 'zod';
import { ItemId, LibraryId, ShowId } from './ids';

/** What sort of thing a media item is. */
export const MediaKind = z.enum(['movie', 'episode', 'video']);

/** What a [`Marker`] segment is (serialized lowercase). */
export const MarkerKind = z.enum(['intro', 'credits']);

/** Library classification, derived from the kinds of items it holds. */
export const LibraryKind = z.enum(['movies', 'shows', 'mixed']);

/** UI grouping bucket for a job (serializes lowercase). */
export const Category = z.enum([
  'maintenance',
  'library',
  'recommendations',
  'pipeline',
  'acquisition',
]);

/** Video stream description (fields may be null when unknown). */
export const VideoTrack = z.object({
  codec: z.string(),
  width: z.number().nullable(),
  height: z.number().nullable(),
  hdr: z.boolean(),
  bitDepth: z.number().nullable(),
});
export type VideoTrack = z.infer<typeof VideoTrack>;

/** One audio stream/track. */
export const AudioTrack = z.object({
  index: z.number(),
  codec: z.string(),
  channels: z.number().nullable(),
  language: z.string().nullable(),
  title: z.string().nullish(),
  default: z.boolean(),
});
export type AudioTrack = z.infer<typeof AudioTrack>;

/** A subtitle track. */
export const SubtitleTrack = z.object({
  language: z.string().nullable(),
  codec: z.string(),
});
export type SubtitleTrack = z.infer<typeof SubtitleTrack>;

/** One physical file backing a logical [`MediaItem`]. `id` is a `short_hash` of
 * the absolute path (opaque, per-file) not a media-item id. */
export const MediaFile = z.object({
  id: z.string(),
  relPath: z.string().nullable(),
  container: z.string(),
  durationMs: z.number().nullable(),
  video: VideoTrack.nullable(),
  audio: AudioTrack.nullable(),
  audioTracks: z.array(AudioTrack),
  subtitles: z.array(SubtitleTrack),
  size: z.number().nullable(),
  edition: z.string().nullish(),
  probed: z.boolean(),
});
export type MediaFile = z.infer<typeof MediaFile>;

/** One timed segment of an episode (intro / credits), in milliseconds. */
export const Marker = z.object({
  kind: MarkerKind,
  startMs: z.number(),
  endMs: z.number(),
});
export type Marker = z.infer<typeof Marker>;

/** One top-billed cast member. */
export const CastMember = z.object({
  name: z.string(),
  character: z.string().nullable(),
  profileUrl: z.string().nullish(),
});
export type CastMember = z.infer<typeof CastMember>;

/** One key crew member (director, writer, creator). */
export const CrewMember = z.object({
  name: z.string(),
  job: z.string(),
  profileUrl: z.string().nullish(),
});
export type CrewMember = z.infer<typeof CrewMember>;

/** Resolved provider metadata for one movie or show. `imdbId` is an opaque
 * external id (plain string); `tmdbId` is numeric. */
export const Metadata = z.object({
  provider: z.string(),
  tmdbId: z.number(),
  imdbId: z.string().nullable(),
  title: z.string().nullable(),
  tagline: z.string().nullable(),
  overview: z.string().nullable(),
  releaseDate: z.string().nullable(),
  genres: z.array(z.string()),
  rating: z.number().nullable(),
  posterUrl: z.string().nullable(),
  backdropUrl: z.string().nullable(),
  logoUrl: z.string().nullable(),
  themeUrl: z.string().nullish(),
  cast: z.array(CastMember).nullish(),
  crew: z.array(CrewMember).nullish(),
  tmdbUrl: z.string(),
});
export type Metadata = z.infer<typeof Metadata>;

/** A single playable media item. `defaultFileId` is an opaque file id (plain
 * string); `id`/`showId` are branded. */
export const MediaItem = z.object({
  id: ItemId,
  title: z.string(),
  kind: MediaKind,
  year: z.number().nullable(),
  durationMs: z.number().nullable(),
  container: z.string(),
  video: VideoTrack.nullable(),
  audio: AudioTrack.nullable(),
  audioTracks: z.array(AudioTrack),
  subtitles: z.array(SubtitleTrack),
  library: z.string(),
  showId: ShowId.nullable(),
  showTitle: z.string().nullable(),
  season: z.number().nullable(),
  episode: z.number().nullable(),
  episodeEnd: z.number().nullable(),
  episodeTitle: z.string().nullable(),
  relPath: z.string().nullable(),
  addedAt: z.string(),
  metadata: Metadata.nullish(),
  files: z.array(MediaFile),
  defaultFileId: z.string().nullish(),
  markers: z.array(Marker).nullish(),
});
export type MediaItem = z.infer<typeof MediaItem>;

/** One season's worth of episodes, sorted by episode number. */
export const Season = z.object({
  number: z.number(),
  episodes: z.array(MediaItem),
  cast: z.array(CastMember).nullish(),
});
export type Season = z.infer<typeof Season>;

/** A TV show aggregate (not a file), built by grouping episodes during a scan. */
export const Show = z.object({
  id: ShowId,
  title: z.string(),
  year: z.number().nullable(),
  library: z.string(),
  seasonCount: z.number(),
  episodeCount: z.number(),
  video: VideoTrack.nullable(),
  addedAt: z.string(),
  metadata: Metadata.nullish(),
  progress: z.number().nullish(),
});
export type Show = z.infer<typeof Show>;

/** `GET /api/shows/:id` payload: a show plus its seasons. */
export const ShowDetail = z.object({
  show: Show,
  seasons: z.array(Season),
});
export type ShowDetail = z.infer<typeof ShowDetail>;

/** A scanned library root. `id` is an opaque library id (plain string). */
export const Library = z.object({
  id: LibraryId,
  name: z.string(),
  kind: LibraryKind,
  path: z.string(),
  itemCount: z.number(),
});
export type Library = z.infer<typeof Library>;

/** One row of a user's playback progress. */
export const ProgressEntry = z.object({
  itemId: ItemId,
  positionMs: z.number(),
  durationMs: z.number().nullable(),
  updatedAt: z.string(),
});
export type ProgressEntry = z.infer<typeof ProgressEntry>;

/** A "continue watching" entry: the resumable item plus where to resume from. */
export const ContinueItem = z.object({
  item: MediaItem,
  positionMs: z.number(),
  durationMs: z.number().nullable(),
  updatedAt: z.string(),
});
export type ContinueItem = z.infer<typeof ContinueItem>;

/** The episode to play to continue a show (`GET /api/shows/:id/up-next`). */
export const UpNext = z.object({
  item: MediaItem,
  resume: z.boolean(),
});
export type UpNext = z.infer<typeof UpNext>;

/** One rail entry: a movie/video (a [`MediaItem`]) or a whole show (a [`Show`]),
 * distinguished by the `type` tag. */
// drift: runtime-checked (tagged union)
export const SectionItem = z.discriminatedUnion('type', [
  z.object({ type: z.literal('movie'), item: MediaItem }),
  z.object({ type: z.literal('show'), show: Show }),
]);
export type SectionItem = z.infer<typeof SectionItem>;

/** One catalogue rail: a titled row of [`SectionItem`]s. */
export const Section = z.object({
  id: z.string(),
  title: z.string(),
  reason: z.string().nullable(),
  items: z.array(SectionItem),
});
export type Section = z.infer<typeof Section>;

/** The status of one treatment (stage) applied to a single catalog element. */
export const Treatment = z.object({
  key: z.string(),
  status: z.string(),
  error: z.string().nullable(),
});
export type Treatment = z.infer<typeof Treatment>;

/** `GET /api/health`. */
export const Health = z.object({
  status: z.string(),
  version: z.string(),
  ffprobe: z.boolean(),
  libraries: z.number(),
  items: z.number(),
  shows: z.number(),
});
export type Health = z.infer<typeof Health>;

/** Server identity + uptime for the admin sidebar status card. */
export const ServerInfo = z.object({
  name: z.string(),
  hostname: z.string(),
  version: z.string(),
  uptimeSec: z.number(),
  online: z.boolean(),
  sessions: z.number(),
});
export type ServerInfo = z.infer<typeof ServerInfo>;
export type Category = z.infer<typeof Category>;
export type LibraryKind = z.infer<typeof LibraryKind>;
export type MarkerKind = z.infer<typeof MarkerKind>;
export type MediaKind = z.infer<typeof MediaKind>;
