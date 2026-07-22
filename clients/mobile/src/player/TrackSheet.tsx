// Player settings panel: audio track, subtitles, speed, volume filter. In the
// landscape player it slides in as a blurred side panel (safe-area aware, so
// the notch / Dynamic Island never crops it); in portrait (tablets) it behaves
// as a bottom sheet.

import { audioTrackLabel, audioTracksOf, langName, type MediaItem } from '@kroma/core';
import { BlurView } from 'expo-blur';
import {
  Modal,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  useWindowDimensions,
  View,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Chip } from '../components/ui';
import { useT } from '../lib/i18n';
import { colors, radius, spacing, type } from '../lib/theme';
import type { Engine } from './engine';
import { CheckIcon } from './icons';

const SPEEDS = [0.75, 1, 1.25, 1.5, 2];

import type { Subtitles } from './useSubtitles';

function Row({
  label,
  selected,
  onPress,
}: Readonly<{ label: string; selected: boolean; onPress(): void }>) {
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [styles.row, pressed && { backgroundColor: colors.surfaceHigh }]}
    >
      <Text style={[styles.rowLabel, selected && { color: colors.accent, fontWeight: '700' }]}>
        {label}
      </Text>
      {selected ? <CheckIcon size={17} color={colors.accent} /> : null}
    </Pressable>
  );
}

export function TrackSheet({
  visible,
  onClose,
  engine,
  subs,
  item,
}: Readonly<{
  visible: boolean;
  onClose(): void;
  engine: Engine;
  subs: Subtitles;
  item: MediaItem;
}>) {
  const t = useT();
  const insets = useSafeAreaInsets();
  const { width, height } = useWindowDimensions();
  const landscape = width > height;
  const itemAudio = audioTracksOf(item);
  // Offline the picker reflects what the downloaded FILE contains (native
  // tracks; an old single-track download must not offer languages that are
  // not in the file). The remux preserves track order, so when the counts
  // line up the richer server labels (language · channels · codec) apply.
  const audio = engine.offline
    ? engine.localAudio.map((native, i) => ({
        index: i,
        label:
          (engine.localAudio.length === itemAudio.length
            ? audioTrackLabel(t, itemAudio[i])
            : undefined) ??
          (native.label?.trim() || langName(t, native.language) || `#${i + 1}`),
      }))
    : itemAudio.map((track, i) => ({
        index: track.index,
        label: audioTrackLabel(t, track) ?? `#${i + 1}`,
      }));

  const panel = (
    <View
      style={[
        landscape
          ? [
              styles.sidePanel,
              {
                width: Math.min(400, width * 0.46),
                paddingTop: insets.top + spacing.sm,
                paddingBottom: Math.max(insets.bottom, spacing.md),
                paddingRight: Math.max(insets.right, spacing.md),
              },
            ]
          : [styles.bottomPanel, { paddingBottom: Math.max(insets.bottom, spacing.md) }],
      ]}
    >
      <BlurView
        tint="dark"
        intensity={Platform.OS === 'ios' ? 60 : 0}
        style={[StyleSheet.absoluteFill, Platform.OS !== 'ios' && styles.androidPanelBg]}
      />
      <ScrollView showsVerticalScrollIndicator={false} contentContainerStyle={styles.scroll}>
        {audio.length > 1 ? (
          <View>
            <Text style={styles.group}>{t('player.audioTracks')}</Text>
            {audio.map((track) => (
              <Row
                key={track.index}
                label={track.label}
                selected={engine.audioIndex === track.index}
                onPress={() => engine.setAudio(track.index)}
              />
            ))}
          </View>
        ) : null}

        <Text style={styles.group}>{t('player.subtitles')}</Text>
        <Row
          label={t('player.subtitlesOff')}
          selected={subs.active === null}
          onPress={() => subs.pick(null)}
        />
        {subs.tracks.map((track) => (
          <Row
            key={track.index}
            label={
              (track.label?.trim() || langName(t, track.language) || `#${track.index + 1}`) +
              (track.ai ? ' · IA' : '')
            }
            selected={subs.active === track.index}
            onPress={() => subs.pick(track.index)}
          />
        ))}

        <Text style={styles.group}>{t('player.speed')}</Text>
        <View style={styles.chipRow}>
          {SPEEDS.map((s) => (
            <Chip
              key={s}
              label={s === 1 ? t('player.normalSpeed') : `${s}x`}
              active={engine.rate === s}
              onPress={() => engine.setRate(s)}
            />
          ))}
        </View>

        {engine.offline ? null : (
          // The volume filter is server DSP: not available on a local file.
          <View>
            <Text style={styles.group}>{t('player.audioFilters')}</Text>
            <Row
              label={t('player.audioFilterOff')}
              selected={engine.filter === 'off'}
              onPress={() => engine.setFilter('off')}
            />
            <Row
              label={t('player.audioFilterStandard')}
              selected={engine.filter === 'standard'}
              onPress={() => engine.setFilter('standard')}
            />
            <Row
              label={t('player.audioFilterNight')}
              selected={engine.filter === 'night'}
              onPress={() => engine.setFilter('night')}
            />
          </View>
        )}
      </ScrollView>
    </View>
  );

  return (
    <Modal
      visible={visible}
      transparent
      animationType={landscape ? 'fade' : 'slide'}
      onRequestClose={onClose}
      supportedOrientations={['portrait', 'landscape', 'landscape-left', 'landscape-right']}
    >
      <View style={landscape ? styles.overlayRow : styles.overlayColumn}>
        <Pressable style={styles.backdrop} onPress={onClose} />
        {panel}
      </View>
    </Modal>
  );
}

const styles = StyleSheet.create({
  overlayRow: { flex: 1, flexDirection: 'row' },
  overlayColumn: { flex: 1, flexDirection: 'column', justifyContent: 'flex-end' },
  backdrop: { flex: 1, backgroundColor: 'rgba(0, 0, 0, 0.45)' },
  sidePanel: {
    height: '100%',
    borderTopLeftRadius: radius.xl,
    borderBottomLeftRadius: radius.xl,
    overflow: 'hidden',
    paddingLeft: spacing.md,
  },
  bottomPanel: {
    maxHeight: '70%',
    borderTopLeftRadius: radius.xl,
    borderTopRightRadius: radius.xl,
    overflow: 'hidden',
    paddingHorizontal: spacing.md,
    paddingTop: spacing.sm,
  },
  androidPanelBg: { backgroundColor: 'rgba(18, 18, 22, 0.97)' },
  scroll: { paddingBottom: spacing.md },
  group: {
    ...type.small,
    textTransform: 'uppercase',
    letterSpacing: 1,
    marginTop: spacing.md,
    marginBottom: spacing.xs,
    paddingHorizontal: spacing.sm,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    minHeight: 44,
    paddingHorizontal: spacing.sm,
    borderRadius: radius.sm,
    gap: spacing.sm,
  },
  rowLabel: { ...type.body, color: colors.textDim, flexShrink: 1 },
  chipRow: { flexDirection: 'row', gap: 8, paddingHorizontal: spacing.sm, flexWrap: 'wrap' },
});
