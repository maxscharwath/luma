import { useT } from '@kroma/ui';

/** Fast-forward chevrons glyph for the skip-intro pill. */
function IconSkipIntro() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M4 5l8 7-8 7V5zm9 0l8 7-8 7V5z" />
    </svg>
  );
}

/** Netflix-style "Skip intro" pill, shown bottom-right (above the control bar)
 * while playback sits inside an episode's `intro` marker. Clicking seeks to the
 * end of the intro. Rendered conditionally by the Player. */
export function SkipIntroButton({ onSkip }: Readonly<{ onSkip: () => void }>) {
  const t = useT();
  return (
    <button
      type="button"
      onClick={onSkip}
      className="absolute bottom-28 right-4 z-50 flex items-center gap-2 rounded-full border border-white/15 bg-[rgba(18,18,22,.88)] px-5 py-3 text-[14px] font-semibold text-white shadow-pop backdrop-blur-xl transition-colors hover:bg-white/15 sm:right-8"
    >
      <IconSkipIntro />
      {t('player.skipIntro')}
    </button>
  );
}
