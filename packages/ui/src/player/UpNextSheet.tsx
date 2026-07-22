import type { RemoteKey, Translate } from '@kroma/core';
import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from 'react';
import { Animated, Easing, type LayoutChangeEvent, Pressable, ScrollView } from 'react-native';
import { useT } from '../i18n';
import { gradient } from '../primitives/css';
import { Txt } from '../primitives/Text';
import { Box } from '../system/Box';
import { fonts, motion } from '../tokens';
import { IconCollapse } from './icons';
import type { PanelHandle } from './nav';
import { EYEBROW } from './style';
import { cellWidth } from '../media/Grid';
import { UP_NEXT_COLUMNS, UP_NEXT_GAP, UpNextCard, type UpNextItem } from './UpNextCard';
import { useGridFocus } from './useGridFocus';

export type { UpNextItem };

/** The two contextual buckets feeding the sheet (§10). For a film,
 * `nextEpisodes` is empty so only recommendations show. */
export interface UpNextData {
  nextEpisodes: UpNextItem[];
  recommendations: UpNextItem[];
}

export interface UpNextSheetProps {
  data: UpNextData;
  /** overlay === 'sheet': the sheet rises and captures the D-pad. */
  open: boolean;
  /** Chrome visible; the peek shows ONLY when revealed AND there is data. */
  revealed: boolean;
  /** Header press / ▼ from the controls: the shell opens the sheet. */
  onOpen: () => void;
  onClose: () => void;
  onPlay: (item: UpNextItem) => void;
}

/** Pixels of the sheet that peek above the bottom edge while parked (§10). */
const PEEK_HEIGHT = 150;
/** Sheet height as a fraction of the player surface. */
const SHEET_FRACTION = 0.82;

const SCRIM = 'linear-gradient(180deg, rgba(0,0,0,0.1), rgba(0,0,0,0.55) 45%)';
const SHEET_FILL =
  'linear-gradient(180deg, transparent, rgba(10,10,12,0.55) 12%, rgba(10,10,12,0.97) 30%)';

interface Section {
  id: string;
  title: string;
  items: UpNextItem[];
  offset: number;
}

/** Split the data into "Épisodes suivants" then "Recommandations", tracking the
 * flat offset each section starts at so one focus index spans every card. */
function buildSections(data: UpNextData, t: Translate): Section[] {
  const sections: Section[] = [];
  if (data.nextEpisodes.length) {
    sections.push({
      id: 'episodes',
      title: t('player.nextEpisodes'),
      items: data.nextEpisodes,
      offset: 0,
    });
  }
  if (data.recommendations.length) {
    sections.push({
      id: 'recommendations',
      title: t('player.recommendations'),
      items: data.recommendations,
      offset: data.nextEpisodes.length,
    });
  }
  return sections;
}


/**
 * The YouTube-TV-style "À suivre" surface (§10): ONE sliding sheet with two
 * positions. Parked (peek) it sits low so only the header + a clipped card row
 * show, and the cards are not focusable (the shell owns ▼). Open, it rises over
 * a scrim into a scrollable grid grouped into "Épisodes suivants" then
 * "Recommandations". D-pad focus runs across the FLAT list of every card via
 * `useGridFocus` (cols=3); ▲ off the top (or Back) closes, Enter plays.
 */
export const UpNextSheet = forwardRef<PanelHandle, UpNextSheetProps>(function UpNextSheet(
  { data, open, revealed, onOpen, onClose, onPlay },
  ref,
) {
  const t = useT();
  const items = [...data.nextEpisodes, ...data.recommendations];

  const grid = useGridFocus({
    count: items.length,
    cols: UP_NEXT_COLUMNS,
    onActivate: (i) => {
      const it = items[i];
      if (it) onPlay(it);
    },
    onExit: (edge) => {
      if (edge === 'top') onClose();
    },
    onBack: onClose,
  });

  // The sheet only owns the D-pad while open; otherwise the shell handles ▼.
  useImperativeHandle(
    ref,
    () => ({ onKey: (key: RemoteKey) => (open ? grid.onKey(key) : false) }),
    [open, grid.onKey],
  );

  // Rise / park. Animated rather than a CSS transition so the one sheet slides
  // the same way on every target.
  const slide = useRef(new Animated.Value(open ? 0 : 1)).current;
  useEffect(() => {
    const anim = Animated.timing(slide, {
      toValue: open ? 0 : 1,
      duration: 340,
      easing: Easing.bezier(...(motion.bezier.out as [number, number, number, number])),
      useNativeDriver: true,
    });
    anim.start();
    return () => anim.stop();
  }, [open, slide]);

  // Scroll the focused card into view on D-pad nav ONLY (keyNonce bumps on arrow
  // keys, not on hover), so the ring never leaves the viewport on a TV while a
  // pointer hover leaves the scroll position, and the layout under it, untouched.
  // Row offsets come from onLayout rather than from the DOM, so this works on a
  // TV where there is no scrollIntoView.
  const scroller = useRef<ScrollView>(null);
  const rowTop = useRef(new Map<number, number>());
  // React Native has no calc(), so the three-across card width is computed from
  // the measured row width instead of expressed as calc((100% - 52px) / 3).
  const [rowWidth, setRowWidth] = useState(0);
  const card = rowWidth > 0 ? cellWidth(rowWidth, UP_NEXT_COLUMNS, UP_NEXT_GAP) : undefined;
  // biome-ignore lint/correctness/useExhaustiveDependencies: grid.keyNonce is a change-trigger (re-run on D-pad moves only), intentionally not read in the body.
  useEffect(() => {
    if (!open) return;
    const y = rowTop.current.get(Math.floor(grid.index / UP_NEXT_COLUMNS));
    if (y != null) scroller.current?.scrollTo({ y: Math.max(0, y - 24), animated: true });
  }, [grid.keyNonce, open]);

  if (!open && (!revealed || items.length === 0)) return null;

  const sections = buildSections(data, t);
  const grouped = sections.length > 1;

  return (
    <>
      <Pressable
        onPress={onClose}
        accessibilityRole="button"
        accessibilityLabel={t('player.back')}
        focusable={false}
        pointerEvents={open ? 'auto' : 'none'}
        style={[SCRIM_BOX, gradient(SCRIM), { opacity: open ? 1 : 0 }]}
      />
      <Animated.View
        style={[
          SHEET_BOX,
          gradient(SHEET_FILL),
          {
            transform: [
              {
                translateY: slide.interpolate({
                  inputRange: [0, 1],
                  // Parked, all but PEEK_HEIGHT of the sheet sits below the edge.
                  outputRange: ['0%', `${100 - (PEEK_HEIGHT / (SHEET_FRACTION * 1080)) * 100}%`],
                }),
              },
            ],
          },
        ]}
      >
        <SheetHeader open={open} title={t('player.upNextTitle')} onToggle={open ? onClose : onOpen} />
        <ScrollView
          ref={scroller}
          scrollEnabled={open}
          showsVerticalScrollIndicator={false}
          contentContainerStyle={{ paddingHorizontal: 56, paddingTop: 4, paddingBottom: 56 }}
        >
          {sections.map((sec) => (
            <Box key={sec.id} mb={32}>
              {grouped ? <Txt style={[EYEBROW, { marginBottom: 14 }]}>{sec.title}</Txt> : null}
              <Box
                row
                wrap
                align="flex-start"
                gap={UP_NEXT_GAP}
                onLayout={(e) => setRowWidth(e.nativeEvent.layout.width)}
              >
                {sec.items.map((it, li) => {
                  const flat = sec.offset + li;
                  return (
                    <CardCell
                      key={it.id}
                      row={Math.floor(flat / UP_NEXT_COLUMNS)}
                      width={card}
                      onRowTop={(row, y) => rowTop.current.set(row, y)}
                    >
                      <UpNextCard
                        item={it}
                        focused={open && grid.index === flat}
                        onActivate={() => onPlay(it)}
                        onFocus={open ? grid.hover(flat) : undefined}
                      />
                    </CardCell>
                  );
                })}
              </Box>
            </Box>
          ))}
        </ScrollView>
      </Animated.View>
    </>
  );
});

/** Reports where its row starts, so the sheet can scroll a D-pad move into view
 * without reaching for the DOM. */
function CardCell({
  row,
  width,
  onRowTop,
  children,
}: Readonly<{
  row: number;
  width?: number;
  onRowTop: (row: number, y: number) => void;
  children: React.ReactNode;
}>) {
  const onLayout = (e: LayoutChangeEvent) => onRowTop(row, e.nativeEvent.layout.y);
  return (
    <Box onLayout={onLayout} w={width}>
      {children}
    </Box>
  );
}

const SCRIM_BOX = {
  position: 'absolute' as const,
  top: 0,
  right: 0,
  bottom: 0,
  left: 0,
  zIndex: 43,
};

const SHEET_BOX = {
  position: 'absolute' as const,
  left: 0,
  right: 0,
  bottom: 0,
  height: `${SHEET_FRACTION * 100}%` as const,
  zIndex: 45,
  overflow: 'hidden' as const,
};

/** The pressable header: title + a chevron that flips between the two states. */
function SheetHeader({
  open,
  title,
  onToggle,
}: Readonly<{ open: boolean; title: string; onToggle: () => void }>) {
  return (
    <Pressable onPress={onToggle} accessibilityRole="button" accessibilityLabel={title}>
      <Box row align="center" gap={14} px={56} pt={24} pb={16}>
        <Txt style={{ fontFamily: fonts.display, fontSize: 22, fontWeight: '700' }}>{title}</Txt>
        <Box style={{ transform: [{ rotate: open ? '0deg' : '180deg' }] }}>
          <Chevron />
        </Box>
      </Box>
    </Pressable>
  );
}

function Chevron() {
  return <IconCollapse size={20} stroke={2.2} color="accent" />;
}
