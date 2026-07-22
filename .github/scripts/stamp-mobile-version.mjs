// Stamp the release version and a monotonic build number into
// clients/mobile/app.json, in the runner only (never committed).
//
// This has to happen BEFORE `expo prebuild`: the generated projects bake
// `versionCode` / `versionName` (Android) and `CFBundleVersion` /
// `CFBundleShortVersionString` (iOS) from app.json at generation time, so
// patching afterwards would mean editing generated Gradle and pbxproj files.
//
// The build number MUST be unique and increasing forever: App Store Connect
// rejects a (version, build) pair it has already seen, and Play refuses a
// versionCode that is not higher than the last one. Minutes since 2020-01-01 UTC
// gives both properties without any state to carry between runs, and matches
// what the Android TV job already does. It stays inside Android's 2100000000
// ceiling until the year 5900.
//
// Usage: node .github/scripts/stamp-mobile-version.mjs <x.y.z>

import { readFileSync, writeFileSync } from 'node:fs';

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
  console.error(`usage: stamp-mobile-version.mjs <x.y.z> (got ${JSON.stringify(process.argv[2])})`);
  process.exit(1);
}

const EPOCH_2020 = 1577836800; // 2020-01-01T00:00:00Z, same base as the Android TV job
const build = Math.floor((Math.floor(Date.now() / 1000) - EPOCH_2020) / 60);

const path = 'clients/mobile/app.json';
const config = JSON.parse(readFileSync(path, 'utf8'));
config.expo.version = version;
config.expo.ios = { ...config.expo.ios, buildNumber: String(build) };
config.expo.android = { ...config.expo.android, versionCode: build };
writeFileSync(path, `${JSON.stringify(config, null, 2)}\n`);

console.log(`app.json stamped: version ${version}, build ${build}`);
