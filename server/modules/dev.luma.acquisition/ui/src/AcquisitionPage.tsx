// The Acquisition module page (`/admin/acquisition`): the acquisition settings
// group, rendered by the shared settings-view renderer over the
// `/api/admin/settings?view=acquisition` endpoint. Default export so the module
// runtime can React.lazy it into its own chunk.

import { SettingsView } from '@luma/module-sdk';

export default function AcquisitionPage() {
  return (
    <SettingsView
      view="acquisition"
      titleKey="admin.pageAcquisition"
      subtitleKey="admin.pageAcquisitionSub"
    />
  );
}
