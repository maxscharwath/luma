import { useT } from '@kroma/ui';

/** Centered top toast for transient player notices (audio re-encode, resume, errors). */
export function Toast({
  variant,
  onDismiss,
  action,
  children,
}: Readonly<{
  variant: 'info' | 'danger';
  onDismiss: () => void;
  action?: React.ReactNode;
  children: React.ReactNode;
}>) {
  const t = useT();
  const border = variant === 'danger' ? 'border-danger/40' : 'border-white/15';
  return (
    <div
      className={`absolute left-1/2 top-6 z-40 flex max-w-160 -translate-x-1/2 items-center gap-3 rounded-xl border ${border} bg-black/80 px-4 py-3 backdrop-blur-md`}
    >
      <span className="text-[13px] text-white/90">{children}</span>
      {action}
      <button
        type="button"
        onClick={onDismiss}
        className="text-white/50 hover:text-white"
        aria-label={t('player.dismiss')}
      >
        ✕
      </button>
    </div>
  );
}
