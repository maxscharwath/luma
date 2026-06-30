// Background-service plumbing for the Tizen Smart Hub preview. The carousel is
// rendered by the TV platform from data a *background service* provides
// (config.xml declares `use.preview = bg_service`). That service can't reach
// LUMA on its own (no shared localStorage / mDNS, and Node's `fs` is unavailable
// to it), so the work is split:
//   • the foreground (this module) builds the tile JSON from the live catalog and
//     writes it to the package-private `wgt-private` dir, then nudges the
//     service to republish;
//   • the service (clients/tizen/public/service/preview-service.js) reads that
//     file and calls webapis.preview.setPreviewData(), which the TV shows.

import { type ContinueItem, type LumaClient, type MediaItem } from '@luma/core';
import { buildPreviewData } from '#tv/shared/preview/cards';
import { type Tizen, type TizenFile, tizen } from '#tv/shared/preview/tizen';

// Must match the <tizen:service id> in clients/tizen/public/config.xml.
const SERVICE_ID = 'LumaTV0001.PreviewSvc';
// Package-private dir shared between the foreground app and its service.
const PRIVATE_DIR = 'wgt-private';
const PREVIEW_FILE = 'preview.json';
// Samsung policy: preview data should not refresh more than ~once per 10 min. We
// keep the on-disk file current on every catalog change but only nudge the
// service to republish past this interval (the TV also polls it on its own).
const REPUBLISH_MS = 10 * 60 * 1000;

function writePrivateFile(t: Tizen, name: string, data: string): Promise<void> {
  return new Promise((resolve, reject) => {
    t.filesystem.resolve(
      PRIVATE_DIR,
      (dir) => {
        let file: TizenFile;
        try {
          file = dir.resolve(name);
        } catch {
          file = dir.createFile(name);
        }
        file.openStream(
          'w',
          (stream) => {
            try {
              stream.write(data);
            } finally {
              stream.close();
            }
            resolve();
          },
          reject,
          'UTF-8',
        );
      },
      reject,
      'rw',
    );
  });
}

/** Ask the background service to (re)publish. Best-effort: the TV also polls the
 *  service on its own schedule, so a failure here just delays the refresh. */
function nudgeService(t: Tizen): void {
  try {
    const ctl = new t.ApplicationControl('http://tizen.org/appcontrol/operation/pick');
    t.application.launchAppControl(ctl, SERVICE_ID, undefined, () => undefined);
  } catch {
    /* ignore */
  }
}

let lastNudge = 0;

/** Persist the carousel (resume + recently-added rows) and (throttled) ask the
 *  service to publish it. No-op off Tizen. */
export async function publishPreview(client: LumaClient, movies: MediaItem[]): Promise<void> {
  const t = tizen();
  if (!t) return;
  // Continue-watching is per-user and needs auth best-effort, empty if absent.
  let continueItems: ContinueItem[] = [];
  try {
    continueItems = await client.continueWatching();
  } catch {
    /* not logged in / no progress yet */
  }
  const data = buildPreviewData(client, movies, continueItems);
  if (!data) return;
  try {
    await writePrivateFile(t, PREVIEW_FILE, data);
    const now = Date.now();
    if (now - lastNudge >= REPUBLISH_MS) {
      lastNudge = now;
      nudgeService(t);
    }
  } catch {
    /* best-effort: the carousel keeps its previous contents */
  }
}
