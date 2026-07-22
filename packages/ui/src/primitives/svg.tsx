// The SVG element set, native (Apple TV / Android TV): react-native-svg.
//
// Everything in the kit that draws vectors (icons, the progress ring, the brand
// wheel and lockup) goes through this ONE module, so those components stay
// single-source instead of each needing its own web/native pair. See svg.web.tsx
// for the browser half, which maps the same prop names onto DOM svg elements.

export {
  Circle,
  default as Svg,
  Ellipse,
  G,
  Line,
  Path,
  Polygon,
  Polyline,
  Rect,
  SvgXml,
} from 'react-native-svg';
