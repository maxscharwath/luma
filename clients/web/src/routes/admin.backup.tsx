import { useT } from '@kroma/ui';
import { IconAlertTriangle, IconDownload, IconUpload } from '@tabler/icons-react';
import { createFileRoute } from '@tanstack/react-router';
import { useRef, useState } from 'react';
import { ExportModal, ImportModal, isEncryptedFile } from '#web/features/admin/backup-modals';
import { Denied, PageHeader, useCap } from '#web/features/admin/shell';
import { C, Card, Section } from '#web/features/admin/ui';

export const Route = createFileRoute('/admin/backup')({
  component: BackupPage,
});

function BackupPage() {
  const t = useT();
  const canManage = useCap('settings.manage');
  const fileRef = useRef<HTMLInputElement>(null);
  const [showExport, setShowExport] = useState(false);
  const [importTarget, setImportTarget] = useState<{ file: File; encrypted: boolean } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  if (!canManage) return <Denied />;

  async function onFilePicked(file: File) {
    setNotice(null);
    setImportTarget({ file, encrypted: await isEncryptedFile(file) });
  }

  function closeImport() {
    setImportTarget(null);
    if (fileRef.current) fileRef.current.value = '';
  }

  return (
    <>
      <PageHeader title={t('admin.backupTitle')} subtitle={t('admin.backupSub')} />

      <Card className="mt-6 flex items-start gap-3 px-5 py-4">
        <IconAlertTriangle size={20} stroke={1.8} color={C.accent} className="mt-0.5 shrink-0" />
        <p className="text-[13.5px] font-medium text-text/70">{t('admin.backupWarning')}</p>
      </Card>

      <Section title={t('admin.backupExportTitle')}>
        <ActionRow
          desc={t('admin.backupExportDesc')}
          action={
            <PrimaryButton onClick={() => setShowExport(true)} icon={IconDownload}>
              {t('admin.backupExport')}
            </PrimaryButton>
          }
        />
      </Section>

      <Section title={t('admin.backupImportTitle')}>
        <ActionRow
          desc={t('admin.backupImportDesc')}
          action={
            <>
              <input
                ref={fileRef}
                type="file"
                accept=".zip,.kroma,.json,application/zip,application/json"
                className="hidden"
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (file) void onFilePicked(file);
                }}
              />
              <PrimaryButton onClick={() => fileRef.current?.click()} icon={IconUpload}>
                {t('admin.backupImport')}
              </PrimaryButton>
            </>
          }
        />
        {notice ? (
          <p className="mt-3 text-[13px] font-semibold" style={{ color: C.green }}>
            {notice}
          </p>
        ) : null}
      </Section>

      {showExport ? <ExportModal onClose={() => setShowExport(false)} /> : null}
      {importTarget ? (
        <ImportModal
          file={importTarget.file}
          encrypted={importTarget.encrypted}
          onClose={closeImport}
          onDone={(msg) => {
            closeImport();
            setNotice(msg);
          }}
        />
      ) : null}
    </>
  );
}

function ActionRow({ desc, action }: Readonly<{ desc: string; action: React.ReactNode }>) {
  return (
    <Card className="flex items-center justify-between gap-5 px-5.5 py-4.5">
      <p className="max-w-160 text-[13.5px] text-dim">{desc}</p>
      <div className="shrink-0">{action}</div>
    </Card>
  );
}

function PrimaryButton({
  onClick,
  icon: Icon,
  children,
}: Readonly<{ onClick: () => void; icon: typeof IconDownload; children: React.ReactNode }>) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex items-center gap-2 rounded-[9px] border border-[#F4B642]/25 bg-[#F4B642]/12 px-3.75 py-2.25 text-[13px] font-semibold text-[#F4B642]"
    >
      <Icon size={16} stroke={1.9} />
      {children}
    </button>
  );
}
