import * as accounts from './client/accounts';
import * as acquisition from './client/acquisition';
import * as admin from './client/admin';
import {
  LumaApiError,
  type LumaClientOptions,
  preconnect,
  type RequestContext,
  requestBlob,
  requestJson,
} from './client/base';
import type { DiscoverType } from './client/discovery';
import * as discovery from './client/discovery';
import * as library from './client/library';
import * as media from './client/media';
import * as moduleRegistry from './client/modules';
import * as organize from './client/organize';
import * as playback from './client/playback';
import * as requests from './client/requests';
import * as subtitlesClient from './client/subtitles';
import type {
  Activity,
  AdminLibrary,
  AdminOverview,
  AdminUsers,
  AuthConfig,
  AuthResult,
  ClientTestResult,
  ContinueItem,
  CreateRequestBody,
  DiscoverDetail,
  DiscoverResponse,
  DownloadClientsView,
  DownloadClientView,
  DownloadsView,
  ElementProcessing,
  GrabBody,
  Health,
  HistoryStats,
  IndexerDefinitionDetailView,
  IndexerDefinitionsView,
  IndexersView,
  IndexerTestResult,
  IndexerView,
  InteractiveSearchView,
  Invite,
  InviteCreated,
  JobDetail,
  JobLog,
  JobsView,
  Library,
  LlmAdminConfig,
  ManualAddBody,
  ManualSearchView,
  MediaItem,
  MediaRequest,
  Metadata,
  MetricsSnapshot,
  ModuleInfo,
  NamingTemplatesView,
  NamingView,
  OrganizePlan,
  OrganizeResult,
  PasskeyInfo,
  Permission,
  PersonResponse,
  PipelineElements,
  PipelineTaskView,
  PipelineView,
  PlaybackPing,
  PlaybackSession,
  ProgressEntry,
  PublicUser,
  QuickConnectInit,
  QuickConnectStatus,
  RequestsView,
  SampleNames,
  SaveDownloadClientBody,
  SaveIndexerBody,
  SaveVpnBody,
  SearchResponse,
  Section,
  ServerInfo,
  SessionInfo,
  SettingsView,
  Show,
  ShowDetail,
  StorageInfo,
  SyncDefinitionsResult,
  TopUser,
  TorrentAnalysis,
  UpNext,
  User,
  VpnAdminView,
  VpnTestResult,
} from './types';

export type { AccountPatch, WebAuthnCredential, WebAuthnOptions } from './client/accounts';
export type {
  AdminFsEntry,
  AdminFsList,
  RemoteAccessSave,
  RemoteAccessView,
  RemoteConnectorStatus,
} from './client/admin';
export type { LumaClientOptions } from './client/base';
export { apiErrorText, LumaApiError } from './client/base';
export type { DiscoverType } from './client/discovery';
export type { StoryboardManifest } from './client/media';
export type {
  DownloadedSub,
  GenerateReq,
  GenMode,
  GenQuality,
  SubCapabilities,
  SubtitleGeneration,
} from './client/subtitles';
export { GEN_LANGS, GEN_QUALITIES } from './client/subtitles';

/** Endpoints a 401 must NOT trigger a silent refresh for: the token exchange
 * itself (would recurse) and the pre-auth handshake endpoints (no session bearer
 * to refresh). Every other authed route recovers via one refresh + retry. */
const NO_REFRESH = new Set([
  '/auth/token',
  '/auth/login',
  '/auth/register',
  '/auth/config',
  '/auth/relock',
  '/auth/quickconnect/initiate',
  '/auth/quickconnect/poll',
]);

/** Thin typed client over the LUMA server REST API. Shared by every client shell.
 *
 * The flat method surface is intentional call sites use `client.listMovies()`.
 * Each method is a thin delegate to a per-domain implementation in `./client/*`
 * (media, accounts, playback, library, admin), wired through a shared
 * {@link RequestContext}. */
export class LumaClient {
  readonly baseUrl: string;
  private readonly fetchFn: typeof globalThis.fetch;
  private authToken?: string;
  private locale?: string;
  /** Called on a 401 to mint a fresh session token from the stored access token
   * (silent refresh). Returns the new bearer, or undefined when refresh isn't
   * possible (no access token / PIN needed) then the 401 propagates. */
  private refreshHandler?: () => Promise<string | undefined>;
  /** The request plumbing handed to every domain function. `json` is bound so it
   * always reads the current auth token / locale set on this instance. */
  private readonly ctx: RequestContext;

  constructor(options: LumaClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, '');
    this.fetchFn = options.fetch ?? globalThis.fetch.bind(globalThis);
    this.authToken = options.authToken;
    this.locale = options.locale;
    this.ctx = {
      baseUrl: this.baseUrl,
      fetchFn: this.fetchFn,
      json: this.json.bind(this),
      blob: this.blob.bind(this),
    };
    // Warm the connection to the media server as early as possible.
    preconnect(this.baseUrl);
  }

  /** Set (or clear, with `undefined`) the bearer token sent on every request. */
  setAuthToken(token?: string): void {
    this.authToken = token;
  }

  /** Install (or clear) the silent-refresh handler. When set, a 401 on a
   * non-auth endpoint triggers one refresh + retry before the error surfaces. */
  setRefreshHandler(fn?: () => Promise<string | undefined>): void {
    this.refreshHandler = fn;
  }

  /** Set (or clear) the active UI locale sent as `Accept-Language`, so the
   * server localises admin labels and error messages to match the client. */
  setLocale(locale?: string): void {
    this.locale = locale;
  }

  /** Whether a bearer token is currently set (does not validate it). */
  get hasAuth(): boolean {
    return Boolean(this.authToken);
  }

  private async json<T>(path: string, init?: RequestInit, retried = false): Promise<T> {
    try {
      return await requestJson<T>(
        this.fetchFn,
        this.baseUrl,
        this.authToken,
        this.locale,
        path,
        init,
      );
    } catch (e) {
      // A 401 on an authed endpoint means the session lapsed refresh once from
      // the access token and retry. Only the token-exchange endpoint itself and
      // the pre-auth handshake endpoints are excluded (refreshing them would
      // recurse or is pointless they carry no session bearer). Authed `/auth/*`
      // routes like /auth/me, /auth/me/pin and quickconnect/authorize DO refresh.
      if (
        !retried &&
        e instanceof LumaApiError &&
        e.status === 401 &&
        this.refreshHandler &&
        !NO_REFRESH.has(path.split('?')[0] as string)
      ) {
        const token = await this.refreshHandler();
        if (token) {
          this.authToken = token;
          return this.json<T>(path, init, true);
        }
      }
      throw e;
    }
  }

  private blob(path: string, init?: RequestInit): Promise<Blob> {
    return requestBlob(this.fetchFn, this.baseUrl, this.authToken, this.locale, path, init);
  }

  // ----- catalogue / media ----------------------------------------------------

  health(init?: RequestInit): Promise<Health> {
    return media.health(this.ctx, init);
  }
  /** The modules running on this server, each with its enabled flag + provided
   * capabilities (engine add-form schemas). Drives the admin's data-driven ADD
   * flows. */
  modules(): Promise<ModuleInfo[]> {
    return moduleRegistry.listModules(this.ctx);
  }
  libraries(): Promise<Library[]> {
    return media.libraries(this.ctx);
  }
  items(libraryId?: string): Promise<MediaItem[]> {
    return media.items(this.ctx, libraryId);
  }
  movies(libraryId?: string): Promise<MediaItem[]> {
    return media.movies(this.ctx, libraryId);
  }
  shows(libraryId?: string): Promise<Show[]> {
    return media.shows(this.ctx, libraryId);
  }
  show(id: string): Promise<ShowDetail> {
    return media.show(this.ctx, id);
  }
  item(id: string): Promise<MediaItem> {
    return media.item(this.ctx, id);
  }
  similar(id: string): Promise<MediaItem[]> {
    return media.similar(this.ctx, id);
  }
  themed(query: string): Promise<MediaItem[]> {
    return media.themed(this.ctx, query);
  }
  home(): Promise<Section[]> {
    return media.home(this.ctx);
  }
  /** AI suggestions for a title's detail page; `null` while generating (poll). */
  aiSuggest(id: string): Promise<Section | null> {
    return media.aiSuggest(this.ctx, id);
  }
  search(query: string, opts?: { libraryId?: string; limit?: number }): Promise<SearchResponse> {
    return media.search(this.ctx, query, opts);
  }
  /** Every movie + show one person (cast or crew) is credited in. */
  personCredits(name: string, opts?: { libraryId?: string }): Promise<PersonResponse> {
    return media.personCredits(this.ctx, name, opts);
  }
  scan(): Promise<{ runId: string }> {
    return media.scan(this.ctx);
  }
  status(): Promise<Activity> {
    return media.status(this.ctx);
  }
  logsUrl(tail = 200): string {
    return media.logsUrl(this.ctx, tail);
  }
  logs(tail = 200): Promise<string> {
    return media.logs(this.ctx, tail);
  }
  streamUrl(id: string): string {
    return media.streamUrl(this.ctx, id);
  }
  hlsMasterUrl(id: string, aac = false, startSec = 0, audio = 0): string {
    return media.hlsMasterUrl(this.ctx, id, aac, startSec, audio);
  }
  posterUrl(id: string): string {
    return media.posterUrl(this.ctx, id);
  }
  /** The item's REAL poster bytes (cached TMDB art), for the OS "Now Playing" artwork.
   * Prefers `metadata.posterUrl` (a raster the OS can decode) over the generated SVG
   * placeholder, which NSImage can't render. Cached art is a relative `/api/images/…`
   * path fetched WITH the bearer token; TMDB fallbacks are absolute + fetched directly. */
  posterBlob(item: Pick<MediaItem, 'id' | 'metadata'>): Promise<Blob> {
    const raw = item.metadata?.posterUrl;
    // Absolute (TMDB) fallback: fetch directly, no LUMA auth needed.
    if (raw && /^https?:\/\//.test(raw)) {
      return this.fetchFn(raw).then((r) => {
        if (!r.ok) throw new Error(`poster ${r.status}`);
        return r.blob();
      });
    }
    // Cached art paths are stored WITH the `/api` prefix (the resolveArt convention), but
    // `blob()` re-adds `/api`, so strip one. Fall back to the generated poster endpoint.
    const path = raw ? raw.replace(/^\/api\b/, '') : `/items/${encodeURIComponent(item.id)}/poster`;
    return this.blob(path);
  }
  showPosterUrl(id: string): string {
    return media.showPosterUrl(this.ctx, id);
  }
  resolveArt(url?: string | null): string | null {
    return media.resolveArt(this.ctx, url);
  }
  posterFor(item: { id: string; metadata?: Metadata | null }): string {
    return media.posterFor(this.ctx, item);
  }
  showPosterFor(show: Pick<Show, 'id' | 'metadata'>): string {
    return media.showPosterFor(this.ctx, show);
  }
  backdropFor(x: { metadata?: Metadata | null }): string | null {
    return media.backdropFor(this.ctx, x);
  }
  themeFor(x: { metadata?: Metadata | null }): string | null {
    return media.themeFor(this.ctx, x);
  }
  subtitleUrl(id: string, index: number): string {
    return media.subtitleUrl(this.ctx, id, index);
  }
  /** Storyboard manifest endpoint URL (scrub-bar hover-preview sheet). */
  storyboardUrl(id: string): string {
    return media.storyboardUrl(this.ctx, id);
  }
  /** Fetch the storyboard manifest (`'pending'` while generating, `null` if none). */
  storyboard(id: string): Promise<media.StoryboardManifest | 'pending' | null> {
    return media.storyboard(this.ctx, id);
  }
  downloadedSubtitles(id: string): Promise<subtitlesClient.DownloadedSub[]> {
    return subtitlesClient.downloadedSubtitles(this.ctx, id);
  }
  subtitleCapabilities(id: string): Promise<subtitlesClient.SubCapabilities> {
    return subtitlesClient.subtitleCapabilities(this.ctx, id);
  }
  /** Start a Whisper transcription / LLM translation; returns a `genId` to poll. */
  generateSubtitle(id: string, req: subtitlesClient.GenerateReq): Promise<{ genId: string }> {
    return subtitlesClient.generateSubtitle(this.ctx, id, req);
  }
  /** Live + recently-finished generations for an item. */
  subtitleGenerations(id: string): Promise<subtitlesClient.SubtitleGeneration[]> {
    return subtitlesClient.subtitleGenerations(this.ctx, id);
  }
  /** Cancel a running generation. */
  cancelGeneration(id: string, genId: string): Promise<void> {
    return subtitlesClient.cancelGeneration(this.ctx, id, genId);
  }
  /** Delete a generated subtitle track. */
  deleteSubtitle(id: string, dlId: string): Promise<void> {
    return subtitlesClient.deleteSubtitle(this.ctx, id, dlId);
  }

  // ----- accounts / sessions / invites / quick connect ------------------------

  register(
    email: string,
    username: string,
    password: string,
    inviteToken?: string,
  ): Promise<AuthResult> {
    return accounts.register(this.ctx, email, username, password, inviteToken);
  }
  createInvite(opts?: {
    permissions?: Permission[];
    expiresInDays?: number;
  }): Promise<InviteCreated> {
    return accounts.createInvite(this.ctx, opts);
  }
  invites(): Promise<Invite[]> {
    return accounts.invites(this.ctx);
  }
  checkInvite(token: string): Promise<{ valid: boolean; expiresAt?: number }> {
    return accounts.checkInvite(this.ctx, token);
  }
  revokeInvite(token: string): Promise<void> {
    return accounts.revokeInvite(this.ctx, token);
  }
  login(identifier: string, password: string): Promise<AuthResult> {
    return accounts.login(this.ctx, identifier, password);
  }
  exchangeToken(accessToken: string, pin?: string): Promise<{ token: string; user: User }> {
    return accounts.exchangeToken(this.ctx, accessToken, pin);
  }
  relock(accessToken: string): Promise<void> {
    return accounts.relock(this.ctx, accessToken);
  }
  logout(accessToken?: string): Promise<void> {
    return accounts.logout(this.ctx, accessToken);
  }
  me(): Promise<{ user: User }> {
    return accounts.me(this.ctx);
  }
  updateLanguage(language: string | null): Promise<{ user: User }> {
    return accounts.updateLanguage(this.ctx, language);
  }
  updateAccount(patch: accounts.AccountPatch): Promise<{ user: User }> {
    return accounts.updateAccount(this.ctx, patch);
  }
  changePassword(current: string, next: string): Promise<void> {
    return accounts.changePassword(this.ctx, current, next);
  }
  listSessions(): Promise<SessionInfo[]> {
    return accounts.sessions(this.ctx);
  }
  revokeSession(id: string): Promise<void> {
    return accounts.revokeSession(this.ctx, id);
  }
  passkeyRegisterStart(): Promise<{ ceremonyId: string; options: accounts.WebAuthnOptions }> {
    return accounts.passkeyRegisterStart(this.ctx);
  }
  passkeyRegisterFinish(body: {
    ceremonyId: string;
    name: string;
    credential: accounts.WebAuthnCredential;
  }): Promise<PasskeyInfo> {
    return accounts.passkeyRegisterFinish(this.ctx, body);
  }
  listPasskeys(): Promise<PasskeyInfo[]> {
    return accounts.passkeys(this.ctx);
  }
  deletePasskey(id: string): Promise<void> {
    return accounts.deletePasskey(this.ctx, id);
  }
  passkeyAuthStart(): Promise<{ ceremonyId: string; options: accounts.WebAuthnOptions }> {
    return accounts.passkeyAuthStart(this.ctx);
  }
  passkeyAuthFinish(body: {
    ceremonyId: string;
    credential: accounts.WebAuthnCredential;
  }): Promise<AuthResult> {
    return accounts.passkeyAuthFinish(this.ctx, body);
  }
  users(): Promise<PublicUser[]> {
    return accounts.users(this.ctx);
  }
  authConfig(): Promise<AuthConfig> {
    return accounts.authConfig(this.ctx);
  }
  pinVerify(pin: string): Promise<void> {
    return accounts.pinVerify(this.ctx, pin);
  }
  setPin(pin: string, current?: string): Promise<{ user: User }> {
    return accounts.setPin(this.ctx, pin, current);
  }
  clearPin(current: string): Promise<{ user: User }> {
    return accounts.clearPin(this.ctx, current);
  }
  uploadAvatar(file: Blob): Promise<{ avatarUrl: string }> {
    return accounts.uploadAvatar(this.ctx, file);
  }
  quickConnectInitiate(): Promise<QuickConnectInit> {
    return accounts.quickConnectInitiate(this.ctx);
  }
  quickConnectPoll(secret: string): Promise<QuickConnectStatus> {
    return accounts.quickConnectPoll(this.ctx, secret);
  }
  quickConnectAuthorize(code: string): Promise<void> {
    return accounts.quickConnectAuthorize(this.ctx, code);
  }

  // ----- playback progress / resume / heartbeats ------------------------------

  progress(): Promise<ProgressEntry[]> {
    return playback.progress(this.ctx);
  }
  itemProgress(itemId: string): Promise<ProgressEntry | null> {
    return playback.itemProgress(this.ctx, itemId);
  }
  continueWatching(): Promise<ContinueItem[]> {
    return playback.continueWatching(this.ctx);
  }
  /** The episode to play to continue a show (resume / next unwatched / first). */
  upNext(showId: string): Promise<UpNext | null> {
    return playback.upNext(this.ctx, showId);
  }
  /** The next episode after an item (player autoplay), or null. */
  nextEpisode(itemId: string): Promise<MediaItem | null> {
    return playback.nextEpisode(this.ctx, itemId);
  }
  forYou(): Promise<MediaItem[]> {
    return playback.forYou(this.ctx);
  }
  saveProgress(itemId: string, positionMs: number, durationMs?: number | null): Promise<void> {
    return playback.saveProgress(this.ctx, itemId, positionMs, durationMs);
  }
  deleteProgress(itemId: string): Promise<void> {
    return playback.deleteProgress(this.ctx, itemId);
  }
  watched(): Promise<string[]> {
    return playback.watched(this.ctx);
  }
  markWatched(itemId: string): Promise<void> {
    return playback.markWatched(this.ctx, itemId);
  }
  unmarkWatched(itemId: string): Promise<void> {
    return playback.unmarkWatched(this.ctx, itemId);
  }
  myList(): Promise<string[]> {
    return playback.myList(this.ctx);
  }
  addToList(itemId: string): Promise<void> {
    return playback.addToList(this.ctx, itemId);
  }
  removeFromList(itemId: string): Promise<void> {
    return playback.removeFromList(this.ctx, itemId);
  }
  pingPlayback(ping: PlaybackPing): Promise<void> {
    return playback.pingPlayback(this.ctx, ping);
  }
  stopPlayback(sessionId: string): Promise<void> {
    return playback.stopPlayback(this.ctx, sessionId);
  }

  // ----- discovery / requests -------------------------------------------------

  discoverSearch(
    query: string,
    opts?: { type?: DiscoverType; page?: number },
  ): Promise<DiscoverResponse> {
    return discovery.discoverSearch(this.ctx, query, opts);
  }
  discoverTrending(opts?: { type?: DiscoverType; page?: number }): Promise<DiscoverResponse> {
    return discovery.discoverTrending(this.ctx, opts);
  }
  discoverDetail(kind: 'movie' | 'tv', tmdbId: number): Promise<DiscoverDetail> {
    return discovery.discoverDetail(this.ctx, kind, tmdbId);
  }
  listRequests(opts?: { mine?: boolean }): Promise<RequestsView> {
    return requests.listRequests(this.ctx, opts);
  }
  createRequest(body: CreateRequestBody): Promise<MediaRequest> {
    return requests.createRequest(this.ctx, body);
  }
  deleteRequest(id: string): Promise<void> {
    return requests.deleteRequest(this.ctx, id);
  }
  approveRequest(id: string): Promise<MediaRequest> {
    return requests.approveRequest(this.ctx, id);
  }
  denyRequest(id: string, note?: string): Promise<MediaRequest> {
    return requests.denyRequest(this.ctx, id, note);
  }
  searchReleases(id: string): Promise<InteractiveSearchView> {
    return requests.searchReleases(this.ctx, id);
  }
  grabRelease(id: string, body: GrabBody): Promise<void> {
    return requests.grabRelease(this.ctx, id, body);
  }

  // ----- admin: naming / organize -----------------------------------------------

  adminNaming(): Promise<NamingView> {
    return organize.adminNaming(this.ctx);
  }
  namingSample(templates: NamingTemplatesView): Promise<SampleNames> {
    return organize.namingSample(this.ctx, templates);
  }
  saveNaming(templates: NamingTemplatesView): Promise<void> {
    return organize.saveNaming(this.ctx, templates);
  }
  organizePreview(): Promise<OrganizePlan> {
    return organize.organizePreview(this.ctx);
  }
  organizeApply(): Promise<OrganizeResult> {
    return organize.organizeApply(this.ctx);
  }

  // ----- admin: acquisition (indexers / clients / downloads) --------------------

  adminIndexers(): Promise<IndexersView> {
    return acquisition.adminIndexers(this.ctx);
  }
  createIndexer(body: SaveIndexerBody): Promise<IndexerView> {
    return acquisition.createIndexer(this.ctx, body);
  }
  updateIndexer(id: string, body: SaveIndexerBody): Promise<IndexerView> {
    return acquisition.updateIndexer(this.ctx, id, body);
  }
  deleteIndexer(id: string): Promise<void> {
    return acquisition.deleteIndexer(this.ctx, id);
  }
  testIndexer(id: string): Promise<IndexerTestResult> {
    return acquisition.testIndexer(this.ctx, id);
  }
  adminIndexerDefinitions(): Promise<IndexerDefinitionsView> {
    return acquisition.adminIndexerDefinitions(this.ctx);
  }
  indexerDefinitionDetail(id: string): Promise<IndexerDefinitionDetailView> {
    return acquisition.indexerDefinitionDetail(this.ctx, id);
  }
  syncIndexerDefinitions(): Promise<SyncDefinitionsResult> {
    return acquisition.syncIndexerDefinitions(this.ctx);
  }
  adminDownloadClients(): Promise<DownloadClientsView> {
    return acquisition.adminDownloadClients(this.ctx);
  }
  createDownloadClient(body: SaveDownloadClientBody): Promise<DownloadClientView> {
    return acquisition.createDownloadClient(this.ctx, body);
  }
  updateDownloadClient(id: string, body: SaveDownloadClientBody): Promise<DownloadClientView> {
    return acquisition.updateDownloadClient(this.ctx, id, body);
  }
  deleteDownloadClient(id: string): Promise<void> {
    return acquisition.deleteDownloadClient(this.ctx, id);
  }
  testDownloadClient(id: string): Promise<ClientTestResult> {
    return acquisition.testDownloadClient(this.ctx, id);
  }
  adminDownloads(): Promise<DownloadsView> {
    return acquisition.adminDownloads(this.ctx);
  }
  pauseDownload(id: string): Promise<void> {
    return acquisition.pauseDownload(this.ctx, id);
  }
  resumeDownload(id: string): Promise<void> {
    return acquisition.resumeDownload(this.ctx, id);
  }
  retryDownload(id: string): Promise<void> {
    return acquisition.retryDownload(this.ctx, id);
  }
  reannounceDownload(id: string): Promise<void> {
    return acquisition.reannounceDownload(this.ctx, id);
  }
  pauseAllDownloads(): Promise<{ count: number }> {
    return acquisition.pauseAllDownloads(this.ctx);
  }
  resumeAllDownloads(): Promise<{ count: number }> {
    return acquisition.resumeAllDownloads(this.ctx);
  }
  reannounceDownloads(): Promise<{ count: number }> {
    return acquisition.reannounceDownloads(this.ctx);
  }
  removeDownload(id: string, opts?: { deleteData?: boolean }): Promise<void> {
    return acquisition.removeDownload(this.ctx, id, opts);
  }
  manualSearch(query: string): Promise<ManualSearchView> {
    return acquisition.manualSearch(this.ctx, query);
  }
  analyzeTorrent(magnetOrUrl: string): Promise<TorrentAnalysis> {
    return acquisition.analyzeTorrent(this.ctx, magnetOrUrl);
  }
  manualAdd(body: ManualAddBody): Promise<{ id: string }> {
    return acquisition.manualAdd(this.ctx, body);
  }
  adminVpn(): Promise<VpnAdminView> {
    return acquisition.adminVpn(this.ctx);
  }
  saveVpn(body: SaveVpnBody): Promise<{ wgConfigured: boolean }> {
    return acquisition.saveVpn(this.ctx, body);
  }
  testVpn(): Promise<VpnTestResult> {
    return acquisition.testVpn(this.ctx);
  }

  // ----- admin: libraries -----------------------------------------------------

  adminLibraries(): Promise<{ libraries: AdminLibrary[] }> {
    return library.adminLibraries(this.ctx);
  }
  createLibrary(body: { name: string; kind?: string; folders: string[] }): Promise<{ id: string }> {
    return library.createLibrary(this.ctx, body);
  }
  updateLibrary(
    id: string,
    patch: { name?: string; kind?: string; folders?: string[]; autoScan?: boolean },
  ): Promise<void> {
    return library.updateLibrary(this.ctx, id, patch);
  }
  deleteLibrary(id: string): Promise<void> {
    return library.deleteLibrary(this.ctx, id);
  }
  scanLibrary(id: string): Promise<void> {
    return library.scanLibrary(this.ctx, id);
  }
  /** Browse server-side directories for the library folder picker (roots when
   *  `path` is empty/absent). */
  adminBrowseFolders(path?: string): Promise<admin.AdminFsList> {
    return admin.adminBrowseFolders(this.ctx, path);
  }

  // ----- admin: console -------------------------------------------------------

  adminServer(): Promise<ServerInfo> {
    return admin.adminServer(this.ctx);
  }
  adminSessions(): Promise<{ sessions: PlaybackSession[] }> {
    return admin.adminSessions(this.ctx);
  }
  terminateSession(id: string, message?: string): Promise<void> {
    return admin.terminateSession(this.ctx, id, message);
  }
  adminMetrics(): Promise<MetricsSnapshot> {
    return admin.adminMetrics(this.ctx);
  }
  adminStorage(): Promise<StorageInfo> {
    return admin.adminStorage(this.ctx);
  }
  clearCache(): Promise<{ freedBytes: number }> {
    return admin.clearCache(this.ctx);
  }
  resetMetadata(): Promise<{ items: number; shows: number }> {
    return admin.resetMetadata(this.ctx);
  }
  adminUsers(): Promise<AdminUsers> {
    return admin.adminUsers(this.ctx);
  }
  updateUser(id: string, patch: { permissions?: Permission[]; username?: string }): Promise<void> {
    return admin.updateUser(this.ctx, id, patch);
  }
  deleteUser(id: string): Promise<void> {
    return admin.deleteUser(this.ctx, id);
  }
  adminSettings(view: string): Promise<SettingsView> {
    return admin.adminSettings(this.ctx, view);
  }
  updateSettings(patch: Record<string, unknown>): Promise<{ updated: string[] }> {
    return admin.updateSettings(this.ctx, patch);
  }
  /** Download a portable backup as a Blob; `password` encrypts it (`.luma`). */
  exportBackup(password?: string): Promise<Blob> {
    return admin.exportBackup(this.ctx, password);
  }
  /** Restore a backup file, then trigger a re-scan. Returns per-table counts. */
  importBackup(file: Blob, opts?: admin.BackupImportOptions): Promise<admin.BackupImportResult> {
    return admin.importBackup(this.ctx, file, opts);
  }
  topUsers(days = 7): Promise<{ users: TopUser[] }> {
    return admin.topUsers(this.ctx, days);
  }
  playHistory(days = 28): Promise<HistoryStats> {
    return admin.playHistory(this.ctx, days);
  }
  adminOverview(): Promise<AdminOverview> {
    return admin.adminOverview(this.ctx);
  }

  // ----- admin: background jobs / scheduler -----------------------------------

  adminJobs(): Promise<JobsView> {
    return admin.adminJobs(this.ctx);
  }
  adminJob(key: string): Promise<JobDetail> {
    return admin.adminJob(this.ctx, key);
  }
  runJob(key: string): Promise<{ runId: string }> {
    return admin.runJob(this.ctx, key);
  }
  cancelJob(key: string): Promise<{ cancelled: boolean }> {
    return admin.cancelJob(this.ctx, key);
  }
  updateJob(key: string, patch: { schedule?: string | null; enabled?: boolean }): Promise<void> {
    return admin.updateJob(this.ctx, key, patch);
  }
  jobRunLogs(runId: string): Promise<{ logs: JobLog[] }> {
    return admin.jobRunLogs(this.ctx, runId);
  }

  // ----- admin: per-element pipeline ------------------------------------------

  adminPipeline(): Promise<PipelineView> {
    return admin.adminPipeline(this.ctx);
  }
  pipelineFailed(stage: string): Promise<{ tasks: PipelineTaskView[] }> {
    return admin.pipelineFailed(this.ctx, stage);
  }
  runPipelineStage(stage: string): Promise<{ runId: string }> {
    return admin.runPipelineStage(this.ctx, stage);
  }
  cancelPipelineStage(stage: string): Promise<{ cancelled: boolean }> {
    return admin.cancelPipelineStage(this.ctx, stage);
  }
  pausePipeline(paused: boolean): Promise<{ paused: boolean }> {
    return admin.pausePipeline(this.ctx, paused);
  }
  retryPipelineStage(stage: string): Promise<{ requeued: number }> {
    return admin.retryPipelineStage(this.ctx, stage);
  }
  reprocessPipelineStage(stage: string): Promise<{ requeued: number }> {
    return admin.reprocessPipelineStage(this.ctx, stage);
  }
  retryPipelineTask(stage: string, subjectId: string): Promise<{ requeued: number }> {
    return admin.retryPipelineTask(this.ctx, stage, subjectId);
  }
  reprocessSubject(
    kind: 'item' | 'show',
    id: string,
  ): Promise<{ subjects: number; stages: string[] }> {
    return admin.reprocessSubject(this.ctx, kind, id);
  }
  itemProcessing(id: string): Promise<ElementProcessing> {
    return admin.itemProcessing(this.ctx, id);
  }
  pipelineElements(params?: {
    status?: string;
    kind?: string;
    q?: string;
    page?: number;
    limit?: number;
  }): Promise<PipelineElements> {
    return admin.pipelineElements(this.ctx, params);
  }
  retryElementStage(kind: 'item' | 'show', id: string, stage: string): Promise<void> {
    return admin.retryElementStage(this.ctx, kind, id, stage);
  }
  showProcessing(id: string): Promise<ElementProcessing> {
    return admin.showProcessing(this.ctx, id);
  }

  // ----- admin: AI / LLM ------------------------------------------------------

  adminLlm(): Promise<LlmAdminConfig> {
    return admin.adminLlm(this.ctx);
  }
  saveLlm(body: admin.LlmSave): Promise<void> {
    return admin.saveLlm(this.ctx, body);
  }
  llmModels(probe: admin.LlmProbe): Promise<{ models: string[]; error?: string }> {
    return admin.llmModels(this.ctx, probe);
  }
  testLlm(probe: admin.LlmProbe): Promise<{ ok: boolean; message: string }> {
    return admin.testLlm(this.ctx, probe);
  }

  // ----- admin: remote access (Cloudflare Tunnel connector) -------------------

  adminRemote(): Promise<admin.RemoteAccessView> {
    return admin.adminRemote(this.ctx);
  }
  saveRemote(body: admin.RemoteAccessSave): Promise<admin.RemoteAccessView> {
    return admin.saveRemote(this.ctx, body);
  }
}
