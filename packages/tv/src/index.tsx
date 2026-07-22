import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { TvApp, type TvAppProps } from '#tv/app/TvApp';

export type { TvAppProps } from '#tv/app/TvApp';
export { TvApp } from '#tv/app/TvApp';

/** Mount the shared TV experience into #root. Called by each platform shell. */
export function mountTv(props: TvAppProps = {}): void {
  const el = document.getElementById('root');
  if (!el) throw new Error('KROMA TV: #root element not found');
  createRoot(el).render(
    <StrictMode>
      <TvApp {...props} />
    </StrictMode>,
  );
}
