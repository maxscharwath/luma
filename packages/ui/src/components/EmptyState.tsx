// Centered "nothing here" block shared by the app's catalogue pages and the admin
// console (via @kroma/admin-kit): an icon, a headline, and an optional hint + action.

import type { ReactNode } from 'react';

export interface EmptyStateProps {
  icon: ReactNode;
  title: string;
  hint?: string;
  action?: ReactNode;
}

/** Centered "nothing here" block: icon, headline, optional hint and action. */
export function EmptyState({ icon, title, hint, action }: Readonly<EmptyStateProps>) {
  return (
    <div className="mt-16 flex flex-col items-center text-center">
      <div className="mb-3 text-dim">{icon}</div>
      <div className="text-[15.5px] font-semibold">{title}</div>
      {hint ? (
        <p className="mt-1 max-w-100 text-[13.5px] text-dim max-sm:text-[14.5px]">{hint}</p>
      ) : null}
      {action}
    </div>
  );
}
