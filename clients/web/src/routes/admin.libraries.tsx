import type { AdminLibrary } from '@luma/core';
import { useT } from '@luma/ui';
import {
  IconDeviceTv,
  IconFolder,
  IconMovie,
  IconMusic,
  IconPhoto,
  IconPlus,
  IconRefresh,
  IconX,
  type TablerIcon,
} from '@tabler/icons-react';
import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
import { Denied, HeaderAction, PageHeader, useCap, usePoll } from '#web/components/admin/shell';
import { Card, Modal, Select } from '#web/components/admin/ui';
import { formatBytes, relativeSeen } from '#web/lib/adminFormat';
import { useAuth } from '#web/lib/auth';

export const Route = createFileRoute('/admin/libraries')({
  component: LibrariesPage,
});

const ICONS: Record<string, TablerIcon> = {
  film: IconMovie,
  tv: IconDeviceTv,
  music: IconMusic,
  photo: IconPhoto,
};

function LibrariesPage() {
  if (!useCap('library.manage')) return <Denied />;
  return <LibrariesPageInner />;
}

function LibrariesPageInner() {
  const t = useT();
  const { client } = useAuth();
  const { data, reload } = usePoll(() => client.adminLibraries(), 8000, [client]);
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<AdminLibrary | null>(null);

  const libraries = data?.libraries ?? [];

  return (
    <>
      <PageHeader
        title={t('admin.librariesTitle')}
        subtitle={t('admin.librariesSub')}
        action={<HeaderAction label={t('admin.addLibrary')} onClick={() => setAdding(true)} />}
      />

      <div className="mt-6 grid grid-cols-2 gap-4">
        {libraries.map((l) => (
          <LibraryCard key={l.id} lib={l} onChanged={reload} onManage={() => setEditing(l)} />
        ))}
        {data && libraries.length === 0 ? (
          <Card className="col-span-2 px-6 py-10 text-center text-[14px] text-dim">
            {t('admin.noLibraries')}
          </Card>
        ) : null}
      </div>

      {adding ? (
        <AddLibraryModal
          onClose={() => setAdding(false)}
          onCreated={() => {
            setAdding(false);
            reload();
          }}
        />
      ) : null}
      {editing ? (
        <ManageLibraryModal
          lib={editing}
          onClose={() => setEditing(null)}
          onChanged={() => {
            setEditing(null);
            reload();
          }}
        />
      ) : null}
    </>
  );
}

function LibraryCard({
  lib,
  onChanged,
  onManage,
}: Readonly<{
  lib: AdminLibrary;
  onChanged: () => void;
  onManage: () => void;
}>) {
  const t = useT();
  const { client } = useAuth();
  const [newFolder, setNewFolder] = useState('');
  const [scanning, setScanning] = useState(false);
  const accent = '#84CE7E';

  async function addFolder() {
    const f = newFolder.trim();
    if (!f) return;
    await client.updateLibrary(lib.id, { folders: [...lib.folders, f] });
    setNewFolder('');
    onChanged();
  }
  async function removeFolder(path: string) {
    await client.updateLibrary(lib.id, { folders: lib.folders.filter((p) => p !== path) });
    onChanged();
  }
  async function scan() {
    setScanning(true);
    try {
      await client.scanLibrary(lib.id);
    } finally {
      setTimeout(() => setScanning(false), 1200);
    }
  }

  const LibIcon = ICONS[lib.kind] ?? IconMovie;

  return (
    <Card className="overflow-hidden">
      <div
        className="flex items-center gap-3.5 border-b border-white/5 px-5 py-4.5"
        style={{ background: 'rgba(132,206,126,.07)' }}
      >
        <span
          className="flex h-11.5 w-11.5 shrink-0 items-center justify-center rounded-xl"
          style={{ background: 'rgba(132,206,126,.16)', color: accent }}
        >
          <LibIcon size={22} stroke={1.8} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="font-display text-[18px] font-bold">{lib.name}</div>
          <div className="text-[12px] font-semibold text-text/45">
            {lib.kind === 'tv' ? t('admin.libKindShows') : t('admin.libKindVideo')} ·{' '}
            {t('admin.itemsCount', { count: lib.itemCount })}
          </div>
        </div>
        {lib.autoScan ? (
          <span className="inline-flex items-center gap-1.5 rounded-full bg-success/13 px-2.5 py-1 text-[11.5px] font-semibold text-success">
            {t('admin.autoScanBadge')}
          </span>
        ) : null}
      </div>

      <div className="flex items-stretch border-b border-white/5">
        <Stat label={t('admin.statSize')} value={formatBytes(lib.sizeBytes)} border />
        <Stat label={t('admin.statLastScan')} value={relativeSeen(lib.lastScan)} border />
        <Stat
          label={t('admin.statLocations')}
          value={t('admin.folderCount', { count: lib.folders.length })}
        />
      </div>

      <div className="flex flex-col gap-2.5 border-b border-white/5 px-5 pb-4 pt-3.5">
        <div className="text-[9.5px] font-bold uppercase tracking-[.12em] text-text/38">
          {t('admin.scannedFolders')}
        </div>
        {lib.folders.map((path) => (
          <div
            key={path}
            className="flex items-center gap-2.5 rounded-[9px] border border-border bg-[#0F0F13] px-3 py-2.5"
          >
            <IconFolder size={16} stroke={1.8} color={accent} />
            <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-text/78">
              {path}
            </span>
            <button
              type="button"
              onClick={() => void removeFolder(path)}
              className="shrink-0 text-text/35 hover:text-danger"
              aria-label={t('admin.removeFolder')}
            >
              <IconX size={15} stroke={2} />
            </button>
          </div>
        ))}
        <div className="flex gap-2">
          <input
            value={newFolder}
            onChange={(e) => setNewFolder(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && void addFolder()}
            placeholder="/media/films"
            className="min-w-0 flex-1 rounded-[9px] border border-dashed border-border-strong bg-transparent px-3 py-2.5 text-[12.5px] font-semibold text-text outline-none"
          />
          <button
            type="button"
            onClick={() => void addFolder()}
            className="inline-flex items-center gap-1.5 rounded-[9px] border border-dashed border-border-strong px-3 py-2.5 text-[12.5px] font-semibold text-text/70"
          >
            <IconPlus size={14} stroke={2.4} />
            {t('common.add')}
          </button>
        </div>
      </div>

      <div className="flex gap-2.5 px-5 py-3.5">
        <button
          type="button"
          onClick={() => void scan()}
          disabled={scanning}
          className="inline-flex items-center gap-1.5 rounded-[9px] bg-accent px-3.5 py-2 text-[13px] font-semibold text-accent-ink disabled:opacity-60"
        >
          <IconRefresh size={14} stroke={2.3} />
          {scanning ? t('admin.scanning') : t('admin.scan')}
        </button>
        <button
          type="button"
          onClick={onManage}
          className="rounded-[9px] border border-border-strong bg-surface-2 px-3.5 py-2 text-[13px] font-semibold text-text/78"
        >
          {t('common.manage')}
        </button>
      </div>
    </Card>
  );
}

function Stat({
  label,
  value,
  border,
}: Readonly<{ label: string; value: string; border?: boolean }>) {
  return (
    <div className={`flex-1 px-5 py-3.5 ${border ? 'border-r border-white/5' : ''}`}>
      <div className="mb-1.5 text-[9.5px] font-bold uppercase tracking-[.12em] text-text/38">
        {label}
      </div>
      <div className="text-[14px] font-semibold text-text/78">{value}</div>
    </div>
  );
}

function AddLibraryModal({
  onClose,
  onCreated,
}: Readonly<{ onClose: () => void; onCreated: () => void }>) {
  const t = useT();
  const { client } = useAuth();
  const movies = t('admin.kindMovies');
  const shows = t('admin.kindShows');
  const [name, setName] = useState('');
  const [kind, setKind] = useState(movies);
  const [folder, setFolder] = useState('');
  const [busy, setBusy] = useState(false);

  async function create() {
    if (!name.trim()) return;
    setBusy(true);
    try {
      await client.createLibrary({
        name: name.trim(),
        kind: kind === shows ? 'shows' : 'movies',
        folders: folder.trim() ? [folder.trim()] : [],
      });
      onCreated();
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title={t('admin.addLibrary')} onClose={onClose}>
      <Field label={t('admin.name')}>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={movies}
          className="w-full rounded-lg border border-border-strong bg-surface-2 px-3 py-2.5 text-[14px]"
        />
      </Field>
      <Field label={t('admin.libType')}>
        <Select value={kind} options={[movies, shows]} onChange={setKind} />
      </Field>
      <Field label={t('admin.firstFolder')}>
        <input
          value={folder}
          onChange={(e) => setFolder(e.target.value)}
          placeholder="/media/films"
          className="w-full rounded-lg border border-border-strong bg-surface-2 px-3 py-2.5 text-[14px]"
        />
      </Field>
      <div className="mt-5 flex justify-end gap-2.5">
        <button
          type="button"
          onClick={onClose}
          className="rounded-md px-4 py-2.5 text-[14px] font-semibold text-muted"
        >
          {t('common.cancel')}
        </button>
        <button
          type="button"
          onClick={() => void create()}
          disabled={busy || !name.trim()}
          className="rounded-md bg-accent px-5 py-2.5 text-[14px] font-bold text-accent-ink disabled:opacity-50"
        >
          {busy ? t('common.creating') : t('common.create')}
        </button>
      </div>
    </Modal>
  );
}

function ManageLibraryModal({
  lib,
  onClose,
  onChanged,
}: Readonly<{
  lib: AdminLibrary;
  onClose: () => void;
  onChanged: () => void;
}>) {
  const t = useT();
  const { client } = useAuth();
  const [name, setName] = useState(lib.name);
  const [autoScan, setAutoScan] = useState(lib.autoScan);
  const [busy, setBusy] = useState(false);

  async function save() {
    setBusy(true);
    try {
      await client.updateLibrary(lib.id, { name: name.trim(), autoScan });
      onChanged();
    } finally {
      setBusy(false);
    }
  }
  async function remove() {
    if (!confirm(t('admin.confirmDeleteLibrary', { name: lib.name }))) return;
    setBusy(true);
    try {
      await client.deleteLibrary(lib.id);
      onChanged();
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title={t('admin.manageLibrary', { name: lib.name })} onClose={onClose}>
      <Field label={t('admin.name')}>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="w-full rounded-lg border border-border-strong bg-surface-2 px-3 py-2.5 text-[14px]"
        />
      </Field>
      <label className="mb-4 flex cursor-pointer items-center gap-3">
        <input
          type="checkbox"
          checked={autoScan}
          onChange={(e) => setAutoScan(e.target.checked)}
          className="h-4 w-4 accent-(--luma-accent)"
        />
        <span className="text-[14px] font-semibold">{t('admin.autoScan')}</span>
      </label>
      <div className="flex items-center justify-between gap-3">
        <button
          type="button"
          onClick={() => void remove()}
          disabled={busy}
          className="text-[13px] font-semibold text-[#E8536A] disabled:opacity-40"
        >
          {t('common.delete')}
        </button>
        <div className="flex gap-2.5">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md px-4 py-2.5 text-[14px] font-semibold text-muted"
          >
            {t('common.cancel')}
          </button>
          <button
            type="button"
            onClick={() => void save()}
            disabled={busy}
            className="rounded-md bg-accent px-5 py-2.5 text-[14px] font-bold text-accent-ink disabled:opacity-50"
          >
            {busy ? t('common.saving') : t('common.save')}
          </button>
        </div>
      </div>
    </Modal>
  );
}

function Field({ label, children }: Readonly<{ label: string; children: React.ReactNode }>) {
  return (
    <div className="mb-4">
      <label className="mb-1.5 block text-[12px] font-bold uppercase tracking-[.12em] text-dim">
        {label}
      </label>
      {children}
    </div>
  );
}
