import {
  type CSSProperties,
  type ImgHTMLAttributes,
  type ReactNode,
  type SyntheticEvent,
  useEffect,
  useRef,
  useState,
} from 'react';

type ImgAttrs = ImgHTMLAttributes<HTMLImageElement>;

export interface ImageProps {
  /** Image URL. Pass an already-sized URL this component never rewrites it. */
  src?: string | null;
  /** Alt text. Empty (the default) marks the artwork decorative. */
  alt?: string;
  /** Fade duration (ms) for both the load-in and the cross-fade on `src` change. */
  duration?: number;
  /** object-fit for the artwork. Default `'cover'`. */
  fit?: CSSProperties['objectFit'];
  /** object-position for the artwork. Default `'50% 50%'`. */
  position?: CSSProperties['objectPosition'];
  /** CSS background painted behind the image the instant-visible fallback fill
   *  (gradient or solid colour) shown while loading and if the load fails. */
  background?: string;
  /** Rich loading state rendered above the background until the image loads
   *  (e.g. a `<Skeleton/>`). Most callers just use `background` instead. */
  placeholder?: ReactNode;
  /** Rendered above the background when there is no `src` or the load fails.
   *  Omit to simply reveal the `background`. */
  fallback?: ReactNode;
  /** Container border-radius (the container clips the image to it). */
  radius?: CSSProperties['borderRadius'];
  /** Stretch the container to fill a positioned parent (`position:absolute`,
   *  inset 0). Use this instead of an `absolute inset-0` class an inline style
   *  would otherwise lose to (see below). Default self-sizes via `className`. */
  fill?: boolean;
  /** Sizing classes for the container when NOT `fill` (e.g. `aspect-2/3`,
   *  `h-14 w-14 rounded-full`). NB: a `position`/`inset` utility here cannot win
   *  over the container's inline styles reach for `fill` to stretch instead. */
  className?: string;
  /** Extra styles merged onto the container. */
  style?: CSSProperties;
  loading?: ImgAttrs['loading'];
  /** Fetch priority hint. Set `'high'` on the one above-the-fold hero/backdrop
   *  that is the LCP element; leave unset (browser default) everywhere else. */
  fetchPriority?: ImgAttrs['fetchPriority'];
  decoding?: ImgAttrs['decoding'];
  draggable?: boolean;
  sizes?: string;
  srcSet?: string;
  crossOrigin?: ImgAttrs['crossOrigin'];
  referrerPolicy?: ImgAttrs['referrerPolicy'];
  onLoad?: (e: SyntheticEvent<HTMLImageElement>) => void;
  onError?: (e: SyntheticEvent<HTMLImageElement>) => void;
}

/* Fill the container. Uses the four longhands (not the `inset` shorthand, which
   old webOS Chromium 53 does not know and would drop from an inline style). */
const FILL: CSSProperties = {
  position: 'absolute',
  top: 0,
  right: 0,
  bottom: 0,
  left: 0,
  width: '100%',
  height: '100%',
};

/**
 * Generic image surface with a built-in fade a shadcn-style drop-in wherever
 * KROMA renders artwork (posters, backdrops, avatars, stills, module icons).
 *
 * - **Fade-in on load** the artwork starts transparent and eases to full
 *   opacity once decoded, so tiles/heroes never pop in.
 * - **Cross-fade on `src` change** the previously loaded image is held
 *   underneath while the new one loads, then the new one fades in over it (great
 *   for a hero/backdrop that swaps as you browse).
 * - Reveals `background` (gradient/colour) while loading and on error, so the
 *   surface is never blank the KROMA "instant gradient" look.
 *
 * Inline styles + an opacity transition only, so it is safe on every client tier
 * including the legacy-TV browsers (no grid, no colour-mix, no util down-levelling).
 *
 * Sizing is the caller's job: give the container a size via `className`/`style`.
 * The image fills it with `object-fit: cover` by default.
 */
export function Image({
  src = null,
  alt = '',
  duration = 400,
  fit = 'cover',
  position = '50% 50%',
  background,
  placeholder,
  fallback,
  radius,
  fill = false,
  className,
  style,
  loading = 'lazy',
  fetchPriority,
  decoding = 'async',
  draggable = false,
  sizes,
  srcSet,
  crossOrigin,
  referrerPolicy,
  onLoad,
  onError,
}: Readonly<ImageProps>) {
  const [shown, setShown] = useState<string | null>(src);
  const [loaded, setLoaded] = useState(false);
  const [errored, setErrored] = useState(false);
  const [under, setUnder] = useState<string | null>(null);
  const loadedSrcRef = useRef<string | null>(null);
  const imgRef = useRef<HTMLImageElement | null>(null);

  // Adjust state during render when `src` changes (avoids a one-frame flicker a
  // post-commit effect would cause): promote the last fully-loaded image to the
  // underlay and start the incoming one from opacity 0. Clearing to null (or the
  // same url) drops the underlay so we never cross-fade from a stale image.
  if (shown !== src) {
    const prev = loadedSrcRef.current;
    setUnder(src && prev && prev !== src ? prev : null);
    setShown(src);
    setLoaded(false);
    setErrored(false);
  }

  // Cached images can already be `complete` before React attaches `onLoad`, so
  // the load event never fires mark them loaded on mount to reveal them.
  useEffect(() => {
    const el = imgRef.current;
    if (el?.complete && el.naturalWidth > 0) {
      loadedSrcRef.current = src;
      setLoaded(true);
    }
  }, [src]);

  // Drop the underlay once the incoming image has finished fading in over it.
  useEffect(() => {
    if (!loaded || under == null) return;
    const id = setTimeout(() => setUnder(null), duration);
    return () => clearTimeout(id);
  }, [loaded, under, duration]);

  const handleLoad = (e: SyntheticEvent<HTMLImageElement>) => {
    loadedSrcRef.current = src;
    setLoaded(true);
    onLoad?.(e);
  };

  const handleError = (e: SyntheticEvent<HTMLImageElement>) => {
    setErrored(true);
    setUnder(null);
    onError?.(e);
  };

  const showImg = Boolean(src) && !errored;
  const showFallback = fallback != null && (!src || errored);

  return (
    <div
      className={className}
      style={{
        // `fill` stretches to a positioned parent; otherwise the container is a
        // relative box the caller sizes (via className) and the positioning
        // context for the layered images.
        position: fill ? 'absolute' : 'relative',
        ...(fill ? { top: 0, right: 0, bottom: 0, left: 0 } : null),
        overflow: 'hidden',
        borderRadius: radius,
        background,
        ...style,
      }}
    >
      {under && under !== src ? (
        <img
          key="under"
          src={under}
          alt=""
          aria-hidden
          draggable={false}
          style={{ ...FILL, objectFit: fit, objectPosition: position }}
        />
      ) : null}

      {placeholder != null && !loaded && !errored ? (
        <div key="placeholder" style={FILL}>
          {placeholder}
        </div>
      ) : null}

      {showImg ? (
        <img
          key={src ?? ''}
          ref={imgRef}
          src={src ?? undefined}
          alt={alt}
          loading={loading}
          fetchPriority={fetchPriority}
          decoding={decoding}
          draggable={draggable}
          sizes={sizes}
          srcSet={srcSet}
          crossOrigin={crossOrigin}
          referrerPolicy={referrerPolicy}
          onLoad={handleLoad}
          onError={handleError}
          style={{
            ...FILL,
            objectFit: fit,
            objectPosition: position,
            opacity: loaded ? 1 : 0,
            transition: `opacity ${duration}ms ease`,
          }}
        />
      ) : null}

      {showFallback ? (
        <div key="fallback" style={FILL}>
          {fallback}
        </div>
      ) : null}
    </div>
  );
}
