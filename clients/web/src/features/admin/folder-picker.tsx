// A server-side folder browser for the library admin: walk the NAS volumes and
// their subdirectories, then commit the current directory as the picked folder.
// Controlled: `value` is the committed path, `onChange` fires when the operator
// presses "use this folder". Browsing state is internal (seeded from `value`).
import type { AdminFsList } from '@luma/core';
import { useT } from '@luma/ui';
import { IconCheck, IconChevronRight, IconCornerLeftUp, IconFolder } from '@tabler/icons-react';
import { type ReactNode, useEffect, useState } from 'react';
import { lumaClient } from '#web/shared/lib/api';

export function FolderPicker({
  value,
  onChange,
}: Readonly<{ value: string; onChange: (path: string) => void }>) {
  const t = useT();
  const [path, setPath] = useState(value || '');
  const [list, setList] = useState<AdminFsList | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let active = true;
    setLoading(true);
    lumaClient()
      .adminBrowseFolders(path)
      .then((res) => {
        if (active) setList(res);
      })
      .catch(() => {
        if (active) setList(null);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [path]);

  const atRoot = path === '';
  const entries = list?.entries ?? [];
  const canSelect = !atRoot;
  const isSelected = !atRoot && value === path;

  let listBody: ReactNode;
  if (loading && entries.length === 0) {
    listBody = (
      <div className="px-3 py-6 text-center text-[12.5px] text-dim">{t('common.loading')}</div>
    );
  } else if (entries.length === 0) {
    listBody = (
      <div className="px-3 py-6 text-center text-[12.5px] text-dim">{t('admin.noSubfolders')}</div>
    );
  } else {
    listBody = entries.map((e) => (
      <button
        key={e.path}
        type="button"
        onClick={() => setPath(e.path)}
        className="flex w-full items-center gap-2.5 px-3 py-2.25 text-left hover:bg-white/5"
      >
        <IconFolder size={16} stroke={1.8} className="shrink-0 text-accent" />
        <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-text/78">
          {e.name}
        </span>
        <IconChevronRight size={14} stroke={2} className="shrink-0 text-text/35" />
      </button>
    ));
  }

  return (
    <div className="overflow-hidden rounded-[10px] border border-border-strong bg-[#0F0F13]">
      <div className="flex items-center gap-2 border-b border-white/6 px-3 py-2.5">
        <button
          type="button"
          onClick={() => list?.parent != null && setPath(list.parent)}
          disabled={!list || list.parent == null}
          aria-label={t('admin.parentFolder')}
          title={t('admin.parentFolder')}
          className="shrink-0 rounded-md p-1 text-text/55 hover:text-text disabled:opacity-30"
        >
          <IconCornerLeftUp size={16} stroke={2} />
        </button>
        <IconFolder size={15} stroke={1.8} className="shrink-0 text-accent" />
        <span className="min-w-0 flex-1 truncate text-[12.5px] font-semibold text-text/80">
          {atRoot ? t('admin.volumes') : path}
        </span>
      </div>

      <div className="max-h-52 overflow-y-auto">{listBody}</div>

      <div className="flex items-center justify-between gap-2 border-t border-white/6 px-3 py-2.5">
        <span className="min-w-0 flex-1 truncate text-[11.5px] font-semibold text-dim">
          {value || ''}
        </span>
        <button
          type="button"
          onClick={() => onChange(path)}
          disabled={!canSelect}
          className={`inline-flex shrink-0 items-center gap-1.5 rounded-[8px] px-3 py-1.75 text-[12.5px] font-bold transition-colors disabled:opacity-40 ${
            isSelected
              ? 'bg-success/15 text-success'
              : 'bg-accent text-accent-ink hover:bg-accent-hover'
          }`}
        >
          <IconCheck size={14} stroke={2.4} />
          {t('admin.selectFolder')}
        </button>
      </div>
    </div>
  );
}
