import { type CSSProperties, useCallback, useEffect, useState } from 'react';

export type SubSize = 'sm' | 'md' | 'lg' | 'xl';
export type SubEdge = 'shadow' | 'outline' | 'box' | 'none';

export interface SubtitleStyle {
  size: SubSize;
  color: string;
  /** Background box opacity 0–100 (only used when edge = 'box'). */
  bgOpacity: number;
  edge: SubEdge;
}

export const DEFAULT_SUB_STYLE: SubtitleStyle = {
  size: 'md',
  color: '#FFFFFF',
  bgOpacity: 75,
  edge: 'shadow',
};

export const SUB_COLORS = ['#FFFFFF', '#F5E050', '#6FE0E0', '#7CE08A', '#F4B642'];

const SIZE_PX: Record<SubSize, number> = { sm: 26, md: 36, lg: 48, xl: 62 };
const KEY = 'kroma.subtitleStyle';

/** Persisted subtitle appearance. SSR-safe: starts from defaults (matching the
 * server render), then hydrates from localStorage on the client. */
export function useSubtitleStyle(): [SubtitleStyle, (next: Partial<SubtitleStyle>) => void] {
  const [style, setStyle] = useState<SubtitleStyle>(DEFAULT_SUB_STYLE);

  useEffect(() => {
    try {
      const raw = localStorage.getItem(KEY);
      if (raw) setStyle({ ...DEFAULT_SUB_STYLE, ...JSON.parse(raw) });
    } catch {
      /* ignore */
    }
  }, []);

  const update = useCallback((next: Partial<SubtitleStyle>) => {
    setStyle((prev) => {
      const merged = { ...prev, ...next };
      try {
        localStorage.setItem(KEY, JSON.stringify(merged));
      } catch {
        /* ignore */
      }
      return merged;
    });
  }, []);

  return [style, update];
}

/** Compute the inline CSS for the subtitle text span from the settings. */
export function subtitleCss(style: SubtitleStyle): CSSProperties {
  const css: CSSProperties = {
    color: style.color,
    fontSize: SIZE_PX[style.size],
    fontWeight: 600,
    lineHeight: 1.3,
    fontFamily: "'Hanken Grotesk', system-ui, sans-serif",
    whiteSpace: 'pre-line',
    display: 'inline-block',
    borderRadius: 10,
    padding: style.edge === 'box' ? '4px 16px' : undefined,
  };
  if (style.edge === 'shadow') {
    css.textShadow = '0 2px 10px rgba(0,0,0,.92), 0 0 3px rgba(0,0,0,.95)';
  } else if (style.edge === 'outline') {
    css.textShadow =
      '-1.5px -1.5px 0 #000, 1.5px -1.5px 0 #000, -1.5px 1.5px 0 #000, 1.5px 1.5px 0 #000, 0 2px 6px rgba(0,0,0,.7)';
  } else if (style.edge === 'box') {
    css.background = `rgba(0,0,0,${Math.max(0, Math.min(100, style.bgOpacity)) / 100})`;
  }
  return css;
}
