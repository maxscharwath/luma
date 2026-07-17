// Post-build pre-compression: emit a `.br` and `.gz` sibling for every
// compressible asset in dist/client. The Rust server's ServeDir is configured
// with `precompressed_br()/precompressed_gzip()`, so it serves these files
// as-is and the NAS never spends CPU compressing static assets at runtime.
// Zero-dependency on purpose (node:zlib ships both codecs).
//
// node:zlib's async codecs run on the libuv threadpool, so files are compressed
// CONCURRENTLY (a sequential loop here cost ~5 min of CI per web build); the
// pool size is raised to match the machine before the first threadpool call.
import os from 'node:os';

process.env.UV_THREADPOOL_SIZE = String(Math.max(4, os.availableParallelism()));

const { promises: fs } = await import('node:fs');
const path = (await import('node:path')).default;
const { promisify } = await import('node:util');
const zlib = (await import('node:zlib')).default;

const brotli = promisify(zlib.brotliCompress);
const gzip = promisify(zlib.gzip);

const ROOT = path.resolve(process.argv[2] ?? 'dist/client');
const EXTENSIONS = new Set(['.js', '.mjs', '.css', '.html', '.svg', '.json', '.txt', '.map', '.webmanifest']);
// Below this, compression overhead beats the savings.
const MIN_BYTES = 1024;
const CONCURRENCY = Math.max(4, os.availableParallelism());

async function* walk(dir) {
  for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
    const abs = path.join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(abs);
    else yield abs;
  }
}

async function compress(file) {
  const source = await fs.readFile(file);
  if (source.length < MIN_BYTES) return 0;
  const [br, gz] = await Promise.all([
    brotli(source, { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: 11 } }),
    gzip(source, { level: 9 }),
  ]);
  await Promise.all([fs.writeFile(`${file}.br`, br), fs.writeFile(`${file}.gz`, gz)]);
  return source.length - br.length;
}

const queue = [];
for await (const file of walk(ROOT)) {
  if (EXTENSIONS.has(path.extname(file))) queue.push(file);
}

let files = 0;
let saved = 0;
let next = 0;
await Promise.all(
  Array.from({ length: Math.min(CONCURRENCY, queue.length) }, async () => {
    while (next < queue.length) {
      const file = queue[next++];
      const gain = await compress(file);
      if (gain > 0) {
        files += 1;
        saved += gain;
      }
    }
  }),
);
console.log(`precompress: ${files} assets, ${(saved / 1024).toFixed(0)} KiB saved (brotli, x${CONCURRENCY})`);
