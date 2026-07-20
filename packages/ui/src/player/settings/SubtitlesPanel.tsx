import type { RemoteKey, SubtitleGeneration } from '@kroma/core';
import { langName, subtitleEtaTime, subtitleStageKey } from '@kroma/core';
import { forwardRef, type ReactNode, useImperativeHandle, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { IconAi, IconDelete, IconOk } from '../icons';
import type { PanelHandle } from '../nav';
import type { PlayerSub } from '../types';
import { useListFocus } from '../useListFocus';
import { GenerateWizard } from './GenerateWizard';
import type { SubtitleGenBundle } from './gen';
import {
  panelList,
  rowCx,
  selectLabel,
  selectRow,
  selectRowOff,
  selectRowOn,
  selectSub,
} from './panelStyle';

interface SubtitlesPanelProps {
  subs: PlayerSub[];
  current: number | null;
  onSelect: (index: number | null) => void;
  gen: SubtitleGenBundle;
  onBack: () => void;
}

/** Violet "IA" pill shown on generated tracks / generation rows. */
const AI_BADGE =
  'inline-flex flex-none items-center gap-1 rounded-[5px] bg-[rgba(124,92,255,0.18)] px-1.5 py-0.5 font-sans font-bold text-[10px] text-[#B7A6FF]';

/**
 * Subtitle picker (§5): an "Off" row, the embedded / downloaded tracks (AI tracks
 * carry a violet "IA" badge + a delete control), the live generation rows, and a
 * "create missing" row (leading sparkle) that opens the {@link GenerateWizard}
 * inline. Selecting a track returns to the menu; the wizard captures the D-pad.
 */
export const SubtitlesPanel = forwardRef<PanelHandle, SubtitlesPanelProps>(function SubtitlesPanel(
  { subs, current, onSelect, gen, onBack },
  ref,
) {
  const t = useT();
  const [wizardOpen, setWizardOpen] = useState(false);
  const wizardRef = useRef<PanelHandle>(null);
  const sources = subs.filter((s) => s.url);

  // Focus flow: [Off, ...subs, (create row?)]. Gen rows are informational.
  const rowCount = 1 + subs.length + (gen.canCreate ? 1 : 0);
  const createIndex = gen.canCreate ? rowCount - 1 : -1;

  const activate = (i: number) => {
    if (i === 0) {
      onSelect(null);
      onBack();
      return;
    }
    if (i === createIndex) {
      setWizardOpen(true);
      return;
    }
    const s = subs[i - 1];
    if (s?.selectable) {
      onSelect(s.index);
      onBack();
    }
  };

  const focus = useListFocus({ count: rowCount, onActivate: activate, onBack });
  useImperativeHandle(
    ref,
    () => ({
      onKey: (k: RemoteKey) => (wizardOpen ? Boolean(wizardRef.current?.onKey(k)) : focus.onKey(k)),
    }),
    [wizardOpen, focus.onKey],
  );

  return (
    <div>
      <div className={panelList}>
        <Row
          label={t('player.subtitlesOff')}
          active={current == null}
          focused={focus.index === 0}
          onActivate={() => activate(0)}
          onFocus={focus.hover(0)}
        />
        {subs.map((s, i) => {
          const codec = s.codec.toUpperCase();
          const row = (
            <Row
              key={s.index}
              label={s.ai && s.label ? s.label : langName(t, s.language) || t('player.langUnknown')}
              sub={s.selectable ? codec : `${codec} · ${t('player.pictureSub')}`}
              badge={s.ai ? <AiBadge /> : null}
              active={current === s.index}
              disabled={!s.selectable}
              focused={focus.index === i + 1}
              onActivate={() => activate(i + 1)}
              onFocus={focus.hover(i + 1)}
            />
          );
          return s.ai && s.subId ? (
            <div key={s.index} className="flex items-center gap-2">
              <div className="min-w-0 flex-1">{row}</div>
              <TrashButton
                label={t('player.subGenDelete')}
                onClick={() => gen.onDelete(s.subId as string)}
              />
            </div>
          ) : (
            <div key={s.index}>{row}</div>
          );
        })}
        {gen.pending.map((g) => (
          <GenRow key={g.id} gen={g} onCancel={() => gen.onCancel(g.id)} />
        ))}
        {gen.canCreate && !wizardOpen ? (
          <Row
            icon={<IconAi size={22} />}
            label={t('player.subCreateMissing')}
            focused={focus.index === createIndex}
            onActivate={() => setWizardOpen(true)}
            onFocus={focus.hover(createIndex)}
          />
        ) : null}
      </div>

      {gen.canCreate && wizardOpen ? (
        <div className="mt-3">
          <GenerateWizard
            ref={wizardRef}
            caps={gen.caps}
            sources={sources}
            onStart={gen.onStart}
            onClose={() => setWizardOpen(false)}
          />
        </div>
      ) : null}
    </div>
  );
});

/** A subtitle list row: optional leading icon, label (+ sub-line), optional
 * badge, an accent check when active. Rendered as a real button (focus/OK). */
function Row({
  icon,
  label,
  sub,
  badge,
  active,
  disabled,
  focused,
  onActivate,
  onFocus,
}: Readonly<{
  icon?: ReactNode;
  label: string;
  sub?: string;
  badge?: ReactNode;
  active?: boolean;
  disabled?: boolean;
  focused: boolean;
  onActivate: () => void;
  onFocus: () => void;
}>) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onActivate}
      onMouseEnter={onFocus}
      className={`${rowCx(selectRow, selectRowOn, selectRowOff, focused)} ${
        disabled ? 'opacity-40 cursor-not-allowed' : ''
      }`}
    >
      {icon ? <span className="flex flex-none text-text">{icon}</span> : null}
      <span className="min-w-0 flex-1">
        <span className={`block truncate ${selectLabel}`}>{label}</span>
        {sub ? <span className={`block ${selectSub}`}>{sub}</span> : null}
      </span>
      {badge}
      {active ? (
        <span className="flex flex-none text-accent">
          <IconOk size={24} />
        </span>
      ) : null}
    </button>
  );
}

function AiBadge() {
  return (
    <span className={AI_BADGE}>
      <IconAi size={11} />
      IA
    </span>
  );
}

/** Small trash control beside a deletable AI track / generation row. */
function TrashButton({ label, onClick }: Readonly<{ label: string; onClick: () => void }>) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className="flex flex-none h-9 w-9 items-center justify-center rounded-md border-none cursor-pointer text-[rgba(255,255,255,0.5)] bg-[rgba(255,255,255,0.04)]"
    >
      <IconDelete size={16} />
    </button>
  );
}

/** A live generation row (violet "IA" treatment): engine + stage + percent, a
 * violet progress bar + ETA, and a trash control that cancels / discards it. */
function GenRow({ gen, onCancel }: Readonly<{ gen: SubtitleGeneration; onCancel: () => void }>) {
  const t = useT();
  const pct = Math.round(gen.progress * 100);
  const err = gen.status === 'error';
  const engine = gen.mode === 'translate' ? t('player.subAiBadge') : 'Whisper';
  return (
    <div className="rounded-[14px] border border-[rgba(124,92,255,0.4)] bg-[rgba(124,92,255,0.06)] p-4">
      <div className="flex items-center gap-3.5">
        <span className="min-w-0 flex-1 font-sans font-semibold text-[16px] text-text">
          {gen.lang ?? ''}
        </span>
        <AiBadge />
        <TrashButton label={t('player.subGenCancel')} onClick={onCancel} />
      </div>
      <div className="mt-2 flex items-center justify-between font-sans text-[13px]">
        <span
          title={err ? (gen.error ?? undefined) : undefined}
          className={`flex items-center gap-2 ${err ? 'text-[#e8536a]' : 'text-[#9a8ff0]'}`}
        >
          {!err ? <span className="h-1.5 w-1.5 rounded-full bg-[#8b7ff0]" /> : null}
          {err
            ? (gen.error ?? t(subtitleStageKey(gen.stage)))
            : `${engine} · ${t(subtitleStageKey(gen.stage))}`}
        </span>
        <span className="font-bold text-[#b3a9f5] tabular-nums">{err ? '' : `${pct} %`}</span>
      </div>
      {!err ? (
        <>
          <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-[rgba(255,255,255,0.1)]">
            <div
              className="h-full rounded-full bg-[#7c6ff5] transition-[width] duration-500"
              style={{ width: `${pct}%` }}
            />
          </div>
          {gen.etaSec != null ? (
            <div className="mt-1.5 font-sans text-[12px] text-[rgba(255,255,255,0.4)]">
              {t('player.subEta', { time: subtitleEtaTime(gen.etaSec) })}
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}
