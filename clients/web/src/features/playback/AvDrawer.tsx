import type { AudioTrack, Translate } from '@luma/core';
import { channelLabel } from '@luma/core';
import { useT } from '@luma/ui';
import { langName } from '#web/features/catalog/detail';
import { IconCheck, IconClose } from '#web/features/playback/icons';
import {
  SUB_COLORS,
  type SubEdge,
  type SubSize,
  type SubtitleStyle,
  subtitleCss,
} from '#web/features/playback/subtitleStyle';
import type { MovieView } from '#web/shared/lib/api';

/** Track language name, or the localized "Unknown" when no code is present. */
function trackLang(t: Translate, code: string | null): string {
  return langName(t, code) ?? t('player.langUnknown');
}

const SECTION = 'mb-3.5 text-[12px] font-bold uppercase tracking-[.14em] text-white/45';

function Row({
  code,
  label,
  tag,
  active,
  disabled,
  onClick,
}: Readonly<{
  code: string;
  label: string;
  tag?: string;
  active: boolean;
  disabled?: boolean;
  onClick?: () => void;
}>) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`flex w-full items-center gap-3.5 rounded-xl border px-4 py-3.5 text-left transition-colors
        ${active ? 'border-accent/40 bg-accent-soft' : 'border-white/10 bg-white/4 hover:bg-white/8'}
        ${disabled ? 'cursor-not-allowed opacity-40' : 'cursor-pointer'}`}
    >
      <span className="min-w-9 rounded-md bg-white/8 py-1.5 text-center text-[12px] font-bold text-white/85">
        {code}
      </span>
      <span className="flex-1 text-[15px] font-semibold text-text">{label}</span>
      {tag ? (
        <span className="rounded bg-white/8 px-2 py-0.75 text-[10px] font-bold text-white/70">
          {tag}
        </span>
      ) : null}
      {active ? (
        <span className="text-accent">
          <IconCheck size={20} />
        </span>
      ) : null}
    </button>
  );
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: Readonly<{
  value: T;
  options: { v: T; label: string }[];
  onChange: (v: T) => void;
}>) {
  return (
    <div className="flex gap-1.5 rounded-md bg-[#1A1A20] p-1">
      {options.map((o) => (
        <button
          key={o.v}
          onClick={() => onChange(o.v)}
          className={`rounded-[7px] px-3.5 py-2 text-[13px] font-semibold transition-colors
            ${value === o.v ? 'bg-accent text-accent-ink' : 'text-white/70 hover:text-white'}`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

const SIZES: { v: SubSize; label: string }[] = [
  { v: 'sm', label: 'S' },
  { v: 'md', label: 'M' },
  { v: 'lg', label: 'L' },
  { v: 'xl', label: 'XL' },
];
/** Subtitle edge styles; labels are catalog keys translated at render. */
const EDGES: {
  v: SubEdge;
  labelKey: 'subtitle.shadow' | 'subtitle.outline' | 'subtitle.background' | 'subtitle.none';
}[] = [
  { v: 'shadow', labelKey: 'subtitle.shadow' },
  { v: 'outline', labelKey: 'subtitle.outline' },
  { v: 'box', labelKey: 'subtitle.background' },
  { v: 'none', labelKey: 'subtitle.none' },
];

/** Right-side audio + subtitle drawer (non-modal playback continues), with
 * the live subtitle-appearance controls. */
export function AvDrawer({
  item,
  audioTracks,
  audioIndex,
  onPickAudio,
  activeSub,
  onPickSub,
  subStyle,
  onStyleChange,
  onClose,
}: Readonly<{
  item: MovieView;
  audioTracks: AudioTrack[];
  audioIndex: number;
  onPickAudio: (index: number) => void;
  activeSub: number | null;
  onPickSub: (index: number | null) => void;
  subStyle: SubtitleStyle;
  onStyleChange: (next: Partial<SubtitleStyle>) => void;
  onClose: () => void;
}>) {
  const t = useT();
  const edgeOptions = EDGES.map((e) => ({ v: e.v, label: t(e.labelKey) }));
  return (
    <>
      <button
        className="absolute inset-0 z-68 cursor-default bg-black/35"
        onClick={onClose}
        aria-label={t('common.close')}
      />
      <div className="absolute inset-y-0 right-0 z-69 w-100 overflow-y-auto border-l border-white/10 bg-[rgba(16,16,20,.94)] p-7 backdrop-blur-2xl">
        <div className="mb-7 flex items-center justify-between">
          <h2 className="font-display text-[22px] font-bold text-text">
            {t('player.audioSubtitles')}
          </h2>
          <button
            onClick={onClose}
            className="flex h-9 w-9 items-center justify-center rounded-full bg-white/8 text-white hover:bg-white/16"
            aria-label={t('common.close')}
          >
            <IconClose />
          </button>
        </div>

        <div className={SECTION}>{t('player.audioTracks')}</div>
        <div className="mb-8 flex flex-col gap-2">
          {audioTracks.length > 0 ? (
            audioTracks.map((a) => {
              const ch = channelLabel(a.channels);
              return (
                <Row
                  key={a.index}
                  code={(a.language ?? '-').toUpperCase().slice(0, 3)}
                  label={a.title?.trim() || trackLang(t, a.language)}
                  tag={ch ? `${a.codec.toUpperCase()} · ${ch}` : a.codec.toUpperCase()}
                  active={audioIndex === a.index}
                  onClick={() => onPickAudio(a.index)}
                />
              );
            })
          ) : (
            <div className="text-[13px] text-white/45">{t('player.noAudioTracks')}</div>
          )}
        </div>

        <div className={SECTION}>{t('player.subtitles')}</div>
        <div className="mb-8 flex flex-col gap-2">
          <Row
            code="OFF"
            label={t('player.subtitlesOff')}
            active={activeSub == null}
            onClick={() => onPickSub(null)}
          />
          {item.subs.map((s) => {
            const selectable = Boolean(s.url);
            return (
              <Row
                key={s.index}
                code={(s.language ?? 'ST').toUpperCase().slice(0, 3)}
                label={trackLang(t, s.language)}
                tag={
                  selectable
                    ? s.codec.toUpperCase()
                    : `${s.codec.toUpperCase()} · ${t('player.pictureSub')}`
                }
                active={activeSub === s.index}
                disabled={!selectable}
                onClick={selectable ? () => onPickSub(s.index) : undefined}
              />
            );
          })}
        </div>

        {/* ---- subtitle appearance ---- */}
        <div className={SECTION}>{t('player.subAppearance')}</div>
        <div className="mb-4 flex aspect-21/9 items-end justify-center overflow-hidden rounded-xl bg-linear-to-br from-[#2A2440] to-[#0E1226] pb-4">
          <span style={subtitleCss(subStyle)}>{t('player.subPreview')}</span>
        </div>
        <div className="flex flex-col gap-4">
          <div className="flex items-center justify-between gap-3">
            <span className="text-[14px] font-semibold text-text">{t('player.subSize')}</span>
            <Segmented
              value={subStyle.size}
              options={SIZES}
              onChange={(v) => onStyleChange({ size: v })}
            />
          </div>
          <div className="flex items-center justify-between gap-3">
            <span className="text-[14px] font-semibold text-text">{t('player.subColor')}</span>
            <div className="flex gap-2.5">
              {SUB_COLORS.map((c) => (
                <button
                  key={c}
                  onClick={() => onStyleChange({ color: c })}
                  aria-label={c}
                  className="h-8 w-8 rounded-full"
                  style={{
                    background: c,
                    boxShadow:
                      subStyle.color === c
                        ? '0 0 0 2px var(--luma-accent)'
                        : '0 0 0 1px rgba(255,255,255,.2)',
                  }}
                />
              ))}
            </div>
          </div>
          <div className="flex items-center justify-between gap-3">
            <span className="text-[14px] font-semibold text-text">{t('player.subEdgeBg')}</span>
            <Segmented
              value={subStyle.edge}
              options={edgeOptions}
              onChange={(v) => onStyleChange({ edge: v })}
            />
          </div>
          {subStyle.edge === 'box' ? (
            <div className="flex items-center justify-between gap-4">
              <span className="whitespace-nowrap text-[14px] font-semibold text-text">
                {t('player.subBgOpacity')}
              </span>
              <div className="flex flex-1 items-center gap-3">
                <input
                  type="range"
                  min={0}
                  max={100}
                  value={subStyle.bgOpacity}
                  onChange={(e) => onStyleChange({ bgOpacity: Number(e.target.value) })}
                  className="flex-1 accent-accent"
                />
                <span className="w-10 text-right text-[13px] text-white/70">
                  {subStyle.bgOpacity}%
                </span>
              </div>
            </div>
          ) : null}
        </div>
      </div>
    </>
  );
}
