// The SVG element set, web (Tizen / webOS / desktop / browser): plain DOM svg.
//
// react-native-svg does run under react-native-web, but it drags a large runtime
// through the bundler to reproduce something the browser already does natively,
// and every byte counts on a TV's slow connection. Its prop names are the SVG
// attribute names in camelCase, which is exactly what React DOM wants, so these
// wrappers are pass-throughs rather than translations.

import type { SVGProps } from 'react';

type El<T> = (props: SVGProps<T>) => React.ReactElement;

export const Svg: El<SVGSVGElement> = (props) => (
  <svg aria-hidden="true" focusable="false" {...props} />
);
export const Path: El<SVGPathElement> = (props) => <path {...props} />;
export const Circle: El<SVGCircleElement> = (props) => <circle {...props} />;
export const Rect: El<SVGRectElement> = (props) => <rect {...props} />;
export const Line: El<SVGLineElement> = (props) => <line {...props} />;
export const Polyline: El<SVGPolylineElement> = (props) => <polyline {...props} />;
export const Polygon: El<SVGPolygonElement> = (props) => <polygon {...props} />;
export const Ellipse: El<SVGEllipseElement> = (props) => <ellipse {...props} />;
export const G: El<SVGGElement> = (props) => <g {...props} />;

/** Render a raw SVG document string.
 *
 * The QR code is generated as SVG markup by qrcode-generator, and both worlds
 * need a way to display a document they did not build element by element:
 * react-native-svg parses it, and the browser simply is an SVG parser. The
 * markup is app-generated from a trusted server URL plus a server-issued code,
 * never user input. */
export function SvgXml({
  xml,
  width,
  height,
}: Readonly<{ xml: string | null; width?: number | string; height?: number | string }>) {
  if (!xml) return null;
  return (
    <div
      style={{ width, height, display: 'flex' }}
      // biome-ignore lint/security/noDangerouslySetInnerHtml: app-generated QR SVG built by qrcode-generator from a trusted server URL + server-issued code, never user input.
      dangerouslySetInnerHTML={{ __html: xml }}
    />
  );
}
