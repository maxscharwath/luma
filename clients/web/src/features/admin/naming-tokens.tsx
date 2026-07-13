// Radarr-style "Jetons de nom de fichier" token picker: a per-field modal that
// lists the naming tokens grouped by category with a live example each, plus
// full-filename presets and a separator helper. Clicking a token inserts it at
// the cursor of the field being edited; clicking a preset replaces the field.

import type { NamingTemplatesView } from '@luma/core';
import { useT } from '@luma/ui';
import { IconX } from '@tabler/icons-react';
import { useRef, useState } from 'react';
import { Select } from '#web/shared/ui';

type FieldKey = keyof Omit<NamingTemplatesView, 'case'>;

type Token = Readonly<{ token: string; example: string }>;
type Group = Readonly<{ titleKey: string; tokens: readonly Token[] }>;

const QUALITY: Group = {
  titleKey: 'naming.grpQuality',
  tokens: [
    { token: '{Quality Full}', example: 'Bluray-1080p Proper' },
    { token: '{Quality Title}', example: 'Bluray-1080p' },
  ],
};

const MEDIAINFO: Group = {
  titleKey: 'naming.grpMediaInfo',
  tokens: [
    { token: '{MediaInfo VideoCodec}', example: 'x265' },
    { token: '{MediaInfo VideoBitDepth}', example: '10' },
    { token: '{MediaInfo VideoDynamicRange}', example: 'HDR' },
    { token: '{MediaInfo AudioCodec}', example: 'DTS' },
    { token: '{MediaInfo AudioChannels}', example: '5.1' },
    { token: '{MediaInfo AudioLanguages}', example: '[EN+FR]' },
    { token: '{MediaInfo SubtitleLanguages}', example: '[FR]' },
  ],
};

const RELEASE_GROUP: Group = {
  titleKey: 'naming.grpReleaseGroup',
  tokens: [{ token: '{Release Group}', example: 'RlsGrp' }],
};

const EDITION: Group = {
  titleKey: 'naming.grpEdition',
  tokens: [{ token: '{Edition Tags}', example: 'IMAX' }],
};

const MOVIE_GROUPS: readonly Group[] = [
  {
    titleKey: 'naming.grpMovie',
    tokens: [
      { token: '{Movie Title}', example: "Movie's Title" },
      { token: '{Movie CleanTitle}', example: 'Movies Title' },
      { token: '{Movie TitleThe}', example: "Movie's Title, The" },
      { token: '{Movie TitleFirstCharacter}', example: 'M' },
      { token: '{Release Year}', example: '2010' },
    ],
  },
  {
    titleKey: 'naming.grpMovieId',
    tokens: [
      { token: '{ImdbId}', example: 'tt12345' },
      { token: '{TmdbId}', example: '123456' },
    ],
  },
  QUALITY,
  MEDIAINFO,
  RELEASE_GROUP,
  EDITION,
];

const SERIES_GROUPS: readonly Group[] = [
  {
    titleKey: 'naming.grpSeries',
    tokens: [
      { token: '{Series Title}', example: 'Series Title' },
      { token: '{Series CleanTitle}', example: 'Series Title' },
      { token: '{Series TitleThe}', example: 'Series Title, The' },
      { token: '{Series TitleFirstCharacter}', example: 'S' },
      { token: '{Release Year}', example: '2008' },
    ],
  },
  {
    titleKey: 'naming.grpEpisode',
    tokens: [
      { token: '{season:00}', example: '01' },
      { token: '{episode:00}', example: '05' },
      { token: '{Episode Title}', example: 'Episode Title' },
    ],
  },
  QUALITY,
  MEDIAINFO,
  RELEASE_GROUP,
];

// Full-filename presets, as token lists joined by the chosen separator.
const MOVIE_PRESETS: readonly (readonly string[])[] = [
  ['{Movie Title}', '({Release Year})', '{Quality Full}'],
  [
    '{Movie CleanTitle}',
    '({Release Year})',
    '[{MediaInfo VideoDynamicRange}]',
    '{Quality Full}{-Release Group}',
  ],
];
const EPISODE_PRESETS: readonly (readonly string[])[] = [
  ['{Series Title}', '-', 'S{season:00}E{episode:00}', '-', '{Episode Title}', '{Quality Full}'],
];

const SEPARATORS: readonly { value: string; labelKey: string }[] = [
  { value: ' ', labelKey: 'naming.sepSpace' },
  { value: '.', labelKey: 'naming.sepPeriod' },
  { value: '_', labelKey: 'naming.sepUnderscore' },
  { value: '-', labelKey: 'naming.sepDash' },
];

const isEpisode = (f: FieldKey) =>
  f === 'seriesFolder' || f === 'seasonFolder' || f === 'episodeFile';

export function NamingTokenModal({
  fieldKey,
  fieldLabel,
  value,
  onChange,
  onClose,
}: Readonly<{
  fieldKey: FieldKey;
  fieldLabel: string;
  value: string;
  onChange: (v: string) => void;
  onClose: () => void;
}>) {
  const t = useT();
  const [separator, setSeparator] = useState(' ');
  const inputRef = useRef<HTMLInputElement>(null);

  const groups = isEpisode(fieldKey) ? SERIES_GROUPS : MOVIE_GROUPS;
  const presets = isEpisode(fieldKey) ? EPISODE_PRESETS : MOVIE_PRESETS;

  const insert = (token: string) => {
    const el = inputRef.current;
    const start = el?.selectionStart ?? value.length;
    const end = el?.selectionEnd ?? value.length;
    const next = value.slice(0, start) + token + value.slice(end);
    onChange(next);
    requestAnimationFrame(() => {
      el?.focus();
      const pos = start + token.length;
      el?.setSelectionRange(pos, pos);
    });
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: presentational backdrop; the click only dismisses the modal (a mouse convenience). Keyboard users close via the X / Close buttons.
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      onClick={onClose}
      role="presentation"
    >
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: the onClick only stops propagation so an inside-click doesn't reach the backdrop; there is no user action to mirror on the keyboard. */}
      <div
        className="flex max-h-[88vh] w-full max-w-3xl flex-col rounded-2xl border border-border bg-surface-1 shadow-pop"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <div className="flex items-center justify-between border-b border-white/[0.07] px-6 py-4">
          <div className="font-display text-[18px] font-bold">
            {t('naming.tokensTitle')} <span className="text-dim">· {fieldLabel}</span>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="text-dim hover:text-text"
            aria-label={t('common.close')}
          >
            <IconX size={18} stroke={2.2} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
          <div className="mb-4 flex items-center gap-2 text-[12px] font-semibold text-dim">
            {t('naming.separator')}
            <Select
              value={separator}
              onChange={setSeparator}
              ariaLabel={t('naming.separator')}
              options={SEPARATORS.map((s) => ({
                value: s.value,
                label: t(s.labelKey as Parameters<typeof t>[0]),
              }))}
            />
          </div>

          <Fieldset title={t('naming.grpPresets')}>
            {presets.map((parts) => {
              const tokenStr = parts.join(separator);
              return (
                <button
                  key={tokenStr}
                  type="button"
                  onClick={() => onChange(tokenStr)}
                  className="w-full rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-left hover:border-accent/50 hover:bg-white/[0.06]"
                >
                  <div className="font-mono text-[12px] text-[#86A8FF]">{tokenStr}</div>
                </button>
              );
            })}
          </Fieldset>

          {groups.map((g) => (
            <Fieldset key={g.titleKey} title={t(g.titleKey as Parameters<typeof t>[0])}>
              <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-3">
                {g.tokens.map((tok) => (
                  <button
                    key={tok.token}
                    type="button"
                    onClick={() => insert(tok.token)}
                    className="rounded-lg border border-white/10 bg-white/[0.03] px-2.5 py-1.5 text-left hover:border-accent/50 hover:bg-white/[0.06]"
                  >
                    <div className="truncate font-mono text-[11.5px] font-semibold text-white/80">
                      {tok.token}
                    </div>
                    <div className="truncate text-[11px] text-dim">
                      {example(tok.example, separator)}
                    </div>
                  </button>
                ))}
              </div>
            </Fieldset>
          ))}
        </div>

        <div className="flex items-center gap-3 border-t border-white/[0.07] px-6 py-4">
          <input
            ref={inputRef}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            className="min-w-0 flex-1 rounded-[9px] border border-border-strong bg-[#0F0F13] px-3.5 py-2.5 font-mono text-[13px] text-text outline-none focus:border-accent/60"
          />
          <button
            type="button"
            onClick={onClose}
            className="shrink-0 rounded-xl bg-accent px-5 py-2.5 text-[14px] font-bold text-accent-ink hover:bg-accent-hover"
          >
            {t('common.close')}
          </button>
        </div>
      </div>
    </div>
  );
}

function Fieldset({ title, children }: Readonly<{ title: string; children: React.ReactNode }>) {
  return (
    <fieldset className="mb-4">
      <legend className="mb-2 text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
        {title}
      </legend>
      <div className="flex flex-col gap-1.5">{children}</div>
    </fieldset>
  );
}

/** Show the token's example with the chosen separator swapped in for spaces. */
function example(ex: string, separator: string): string {
  return separator === ' ' ? ex : ex.replace(/ /g, separator);
}
