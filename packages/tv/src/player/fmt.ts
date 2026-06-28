// Player time + language formatting live in @luma/core (shared with the web
// player). Re-exported here so the TV player modules keep their local import path.
export { formatTimecode as fmtTime, langCode, langName } from '@luma/core';
