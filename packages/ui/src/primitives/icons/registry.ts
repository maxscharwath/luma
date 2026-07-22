// The icon set, by Tabler slug. THIS LIST IS THE SOURCE: `bun run icons:gen`
// reads it, pulls each icon's node data out of @tabler/icons and writes
// `icons.generated.ts`.
//
// Why generate instead of importing @tabler/icons-react directly: that package
// renders DOM <svg>, which does not exist on Apple TV or Android TV. Extracting
// the raw path data lets ONE <Icon name="play" /> render through DOM svg on the
// browser targets and through react-native-svg on the native ones, from the same
// design source. It also ships only the 48 icons we use instead of 5093.
//
// To add an icon: add its Tabler slug here (https://tabler.io/icons), rerun the
// generator, and use it as <Icon name="the-slug" />.

export const ICON_SLUGS = [
  'adjustments-horizontal',
  'backspace',
  'badge-4k',
  'badge-cc',
  'chart-bar',
  'check',
  'chevron-down',
  'chevron-left',
  'chevron-right',
  'chevron-up',
  'chevrons-left',
  'chevrons-right',
  'clock',
  'cpu',
  'gauge',
  'keyboard',
  'language',
  'list',
  'lock',
  'logout',
  'maximize',
  'minimize',
  'movie',
  'picture-in-picture',
  'player-pause-filled',
  'player-play-filled',
  'player-stop-filled',
  'player-track-next-filled',
  'plus',
  'power',
  'repeat',
  'rotate-clockwise-2',
  'search',
  'server-2',
  'settings',
  'space',
  'sparkles',
  'trash',
  'typography',
  'users-group',
  'volume',
  'volume-2',
  'volume-3',
  'volume-off',
  'wave-sine',
  'wifi-off',
  'world-search',
  'x',
] as const;
