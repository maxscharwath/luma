import * as accounts from './client/accounts';
import * as admin from './client/admin';
import {
  type LumaClientOptions,
  preconnect,
  type RequestContext,
  requestJson,
} from './client/base';
import * as library from './client/library';
import * as media from './client/media';
import * as playback from './client/playback';
import type {
  Activity,
  AdminLibrary,
  AdminOverview,
  AdminUsers,
  AuthResult,
  ContinueItem,
  Health,
  HistoryStats,
  Invite,
  InviteCreated,
  Library,
  MediaItem,
  Metadata,
  MetricsSnapshot,
  Permission,
  PlaybackPing,
  PlaybackSession,
  ProgressEntry,
  PublicUser,
  QuickConnectInit,
  QuickConnectStatus,
  ScanResult,
  SearchResponse,
  Section,
  ServerInfo,
  SettingsView,
  Show,
  ShowDetail,
  StorageInfo,
  TopUser,
  User,
} from './types';

export type { LumaClientOptions } from './client/base';
export { LumaApiError } from './client/base';

/** Thin typed client over the LUMA server REST API. Shared by every client shell.
 *
 * The flat method surface is intentional — call sites use `client.listMovies()`.
 * Each method is a thin delegate to a per-domain implementation in `./client/*`
 * (media, accounts, playback, library, admin), wired through a shared
 * {@link RequestContext}. */
export class LumaClient {
  readonly baseUrl: string;
  private readonly fetchFn: typeof globalThis.fetch;
  private authToken?: string;
  private locale?: string;
  /** The request plumbing handed to every domain function. `json` is bound so it
   * always reads the current auth token / locale set on this instance. */
  private readonly ctx: RequestContext;

  constructor(options: LumaClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, '');
    this.fetchFn = options.fetch ?? globalThis.fetch.bind(globalThis);
    this.authToken = options.authToken;
    this.locale = options.locale;
    this.ctx = { baseUrl: this.baseUrl, fetchFn: this.fetchFn, json: this.json.bind(this) };
    // Warm the connection to the media server as early as possible.
    preconnect(this.baseUrl);
  }

  /** Set (or clear, with `undefined`) the bearer token sent on every request. */
  setAuthToken(token?: string): void {
    this.authToken = token;
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

  private json<T>(path: string, init?: RequestInit): Promise<T> {
    return requestJson<T>(this.fetchFn, this.baseUrl, this.authToken, this.locale, path, init);
  }

  // ----- catalogue / media ----------------------------------------------------

  health(): Promise<Health> {
    return media.health(this.ctx);
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
  search(query: string, opts?: { libraryId?: string; limit?: number }): Promise<SearchResponse> {
    return media.search(this.ctx, query, opts);
  }
  scan(): Promise<ScanResult> {
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
  hlsAudioUrl(id: string, audioIndex = 0, copy = false): string {
    return media.hlsAudioUrl(this.ctx, id, audioIndex, copy);
  }
  hlsMasterUrl(id: string, aac = false, startSec = 0): string {
    return media.hlsMasterUrl(this.ctx, id, aac, startSec);
  }
  posterUrl(id: string): string {
    return media.posterUrl(this.ctx, id);
  }
  showPosterUrl(id: string): string {
    return media.showPosterUrl(this.ctx, id);
  }
  resolveArt(url?: string | null): string | null {
    return media.resolveArt(this.ctx, url);
  }
  posterFor(item: Pick<MediaItem, 'id' | 'metadata'>): string {
    return media.posterFor(this.ctx, item);
  }
  showPosterFor(show: Pick<Show, 'id' | 'metadata'>): string {
    return media.showPosterFor(this.ctx, show);
  }
  backdropFor(x: { metadata?: Metadata | null }): string | null {
    return media.backdropFor(this.ctx, x);
  }
  subtitleUrl(id: string, index: number): string {
    return media.subtitleUrl(this.ctx, id, index);
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
  logout(): Promise<void> {
    return accounts.logout(this.ctx);
  }
  me(): Promise<{ user: User }> {
    return accounts.me(this.ctx);
  }
  updateLanguage(language: string | null): Promise<{ user: User }> {
    return accounts.updateLanguage(this.ctx, language);
  }
  users(): Promise<PublicUser[]> {
    return accounts.users(this.ctx);
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
  forYou(): Promise<MediaItem[]> {
    return playback.forYou(this.ctx);
  }
  saveProgress(itemId: string, positionMs: number, durationMs?: number | null): Promise<void> {
    return playback.saveProgress(this.ctx, itemId, positionMs, durationMs);
  }
  deleteProgress(itemId: string): Promise<void> {
    return playback.deleteProgress(this.ctx, itemId);
  }
  pingPlayback(ping: PlaybackPing): Promise<void> {
    return playback.pingPlayback(this.ctx, ping);
  }
  stopPlayback(sessionId: string): Promise<void> {
    return playback.stopPlayback(this.ctx, sessionId);
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
    patch: { name?: string; folders?: string[]; autoScan?: boolean },
  ): Promise<void> {
    return library.updateLibrary(this.ctx, id, patch);
  }
  deleteLibrary(id: string): Promise<void> {
    return library.deleteLibrary(this.ctx, id);
  }
  scanLibrary(id: string): Promise<void> {
    return library.scanLibrary(this.ctx, id);
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
  topUsers(days = 7): Promise<{ users: TopUser[] }> {
    return admin.topUsers(this.ctx, days);
  }
  playHistory(days = 28): Promise<HistoryStats> {
    return admin.playHistory(this.ctx, days);
  }
  adminOverview(): Promise<AdminOverview> {
    return admin.adminOverview(this.ctx);
  }
}
