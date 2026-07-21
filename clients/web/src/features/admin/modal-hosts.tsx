// Mount points for the admin console's imperative modals (react-call). Each
// callable needs exactly one Root rendered while it can be called; hosting them
// here in the admin layout (a) keeps their code in the admin bundle rather than
// the app root, and (b) lets every call site drop its `useState(open)` +
// conditional render in favour of `await SomeModal.call(props)`.
import { AddEngineModal } from '@kroma/admin-kit';
import { ExportModal, ImportModal } from '#web/features/admin/backup-modals';
import { StopStreamModal } from '#web/features/admin/dashboard-now-playing';
import { ScheduleModal } from '#web/features/admin/jobs-schedule';
import { AddLibraryModal, ManageLibraryModal } from '#web/features/admin/libraries-modals';
import { NamingTokenModal } from '#web/features/admin/naming-tokens';
import { PipelineDrawer } from '#web/features/admin/pipeline-drawer';
import { ReportDrawer } from '#web/features/admin/report-drawer';
import { RequestDrawer } from '#web/features/admin/request-drawer';
import { EditUserModal, InviteModal } from '#web/features/admin/users-modals';

export function AdminModalHosts() {
  return (
    <>
      <StopStreamModal />
      <EditUserModal />
      <InviteModal />
      <ExportModal />
      <ImportModal />
      <AddLibraryModal />
      <ManageLibraryModal />
      <ScheduleModal />
      <NamingTokenModal />
      <PipelineDrawer />
      <ReportDrawer />
      <RequestDrawer />
      {/* Shared engine-add modal (admin-kit), used by module pages (indexers,
          download clients). One root here covers every consumer. */}
      <AddEngineModal />
    </>
  );
}
