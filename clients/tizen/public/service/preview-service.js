/**
 * LUMA Smart Hub preview background service.
 *
 * The Samsung TV runs this (separately from the UI, even while LUMA is closed)
 * to publish the "new movies" carousel shown on the home screen when the LUMA
 * tile is focused. The foreground app writes the tile JSON to the package's
 * private `wgt-private/preview.json`; this service reads it and hands it to
 * webapis.preview.setPreviewData().
 *
 * Runtime notes: this is a Tizen JS service (a Node-like context). Node's `fs`
 * is NOT available file access goes through tizen.filesystem. `tizen` and
 * `webapis` are globals here, with no <script> include. Keep this file plain
 * CommonJS: it ships verbatim (Vite copies public/ as-is) and is loaded by the
 * platform, not bundled.
 */

var PRIVATE_DIR = 'wgt-private';
var PREVIEW_FILE = 'preview.json';

function log(msg) {
  // `console` may be absent in the Tizen service runtime, so guard it.
  try {
    if (typeof console !== 'undefined' && console.log) {
      console.log(`[luma preview] ${msg}`);
    }
  } catch (_e) {
    /* logging must never break the service */
  }
}

function exit() {
  try {
    tizen.application.getCurrentApplication().exit();
  } catch (_e) {
    /* nothing else to do */
  }
}

function readPreviewData() {
  return new Promise((resolve, reject) => {
    tizen.filesystem.resolve(
      PRIVATE_DIR,
      (dir) => {
        var file;
        try {
          file = dir.resolve(PREVIEW_FILE);
        } catch (e) {
          reject(e);
          return;
        }
        file.openStream(
          'r',
          (stream) => {
            var data = '';
            try {
              if (stream.bytesAvailable > 0) {
                data = stream.read(stream.bytesAvailable);
              }
            } catch (e) {
              stream.close();
              reject(e);
              return;
            }
            stream.close();
            resolve(data);
          },
          reject,
          'UTF-8',
        );
      },
      reject,
      'r',
    );
  });
}

function publish(data) {
  return new Promise((resolve, reject) => {
    if (!data) {
      reject(new Error('no preview data on disk yet'));
      return;
    }
    log(`read ${data.length} bytes from disk`);
    var hasApi =
      typeof webapis !== 'undefined' && webapis.preview && webapis.preview.setPreviewData;
    log(`webapis.preview.setPreviewData present: ${!!hasApi}`);
    if (!hasApi) {
      reject(new Error('webapis.preview.setPreviewData unavailable'));
      return;
    }
    try {
      webapis.preview.setPreviewData(
        data,
        () => {
          resolve();
        },
        (err) => {
          reject(err);
        },
      );
    } catch (e) {
      reject(e);
    }
  });
}

function refresh() {
  readPreviewData()
    .then(publish)
    .then(() => {
      log('preview published OK');
    })
    .catch((err) => {
      log(`skip: ${err?.message ? err.message : JSON.stringify(err)}`);
    })
    .then(exit, exit);
}

function safe(fn) {
  return () => {
    try {
      fn();
    } catch (e) {
      log(`lifecycle error: ${e?.message ? e.message : e}`);
      exit();
    }
  };
}

module.exports = {
  onStart: safe(() => {
    log('service start');
  }),
  onRequest: safe(() => {
    log('onRequest');
    refresh();
  }),
  onExit: safe(() => {
    log('service exit');
  }),
};
