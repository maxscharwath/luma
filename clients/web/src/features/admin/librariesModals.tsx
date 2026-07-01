import type { AdminLibrary } from '@luma/core';
import { useT } from '@luma/ui';
import { useState } from 'react';
import { FolderPicker } from '#web/features/admin/FolderPicker';
import { useAsyncAction } from '#web/features/admin/shell';
import { Field, Modal, ModalActions, SegmentedControl } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';

/** Library kind as accepted by the create/update API: `""` = Auto. */
export type LibKind = '' | 'movies' | 'shows' | 'mixed';

/** Map whatever `kind` the server stores/returns onto the picker's value set so
 *  the current type is preselected (old `film`/`tv` and new `movies`/`shows`). */
export function normalizeLibKind(kind: string): LibKind {
  if (kind === 'shows' || kind === 'tv') return 'shows';
  if (kind === 'movies' || kind === 'film') return 'movies';
  if (kind === 'mixed') return 'mixed';
  return '';
}

/** Segmented Auto / Films / Séries / Mixte type picker, shared by the create
 *  modal and the per-library card. */
export function LibraryTypeSelect({
  value,
  onChange,
}: Readonly<{ value: LibKind; onChange: (v: LibKind) => void }>) {
  const t = useT();
  return (
    <SegmentedControl<LibKind>
      value={value}
      onChange={onChange}
      options={[
        { value: '', label: t('admin.typeAuto') },
        { value: 'movies', label: t('admin.typeMovies') },
        { value: 'shows', label: t('admin.typeShows') },
        { value: 'mixed', label: t('admin.typeMixed') },
      ]}
    />
  );
}

export function AddLibraryModal({
  onClose,
  onCreated,
}: Readonly<{ onClose: () => void; onCreated: () => void }>) {
  const t = useT();
  const { client } = useAuth();
  const [name, setName] = useState('');
  const [kind, setKind] = useState<LibKind>('');
  const [folder, setFolder] = useState('');
  const { busy, run } = useAsyncAction();

  const create = () => {
    if (!name.trim()) return;
    run(async () => {
      await client.createLibrary({
        name: name.trim(),
        kind,
        folders: folder.trim() ? [folder.trim()] : [],
      });
      onCreated();
    });
  };

  return (
    <Modal title={t('admin.addLibrary')} onClose={onClose}>
      <Field label={t('admin.name')}>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t('admin.kindMovies')}
          className="w-full rounded-lg border border-border-strong bg-surface-2 px-3 py-2.5 text-[14px]"
        />
      </Field>
      <Field label={t('admin.libraryType')}>
        <LibraryTypeSelect value={kind} onChange={setKind} />
      </Field>
      <Field label={t('admin.firstFolder')}>
        <FolderPicker value={folder} onChange={setFolder} />
      </Field>
      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={() => void create()}
        confirmLabel={busy ? t('common.creating') : t('common.create')}
        busy={busy}
        disabled={!name.trim()}
      />
    </Modal>
  );
}

export function ManageLibraryModal({
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
  const { busy, run } = useAsyncAction();

  const save = () =>
    run(async () => {
      await client.updateLibrary(lib.id, { name: name.trim(), autoScan });
      onChanged();
    });
  const remove = () => {
    if (!confirm(t('admin.confirmDeleteLibrary', { name: lib.name }))) return;
    run(async () => {
      await client.deleteLibrary(lib.id);
      onChanged();
    });
  };

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
      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={() => void save()}
        confirmLabel={busy ? t('common.saving') : t('common.save')}
        busy={busy}
        destructive={{ label: t('common.delete'), onClick: () => void remove() }}
      />
    </Modal>
  );
}
