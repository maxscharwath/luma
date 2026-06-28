import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { TvApp, type TvAppProps } from '#tv/TvApp';

export type { TvAppProps } from '#tv/TvApp';
export { TvApp } from '#tv/TvApp';
export { useFocusNav } from '#tv/useFocusNav';

/** Mount the shared TV experience into #root. Called by each platform shell. */
export function mountTv(props: TvAppProps = {}): void {
  const el = document.getElementById('root');
  if (!el) throw new Error('LUMA TV: #root element not found');
  createRoot(el).render(
    <StrictMode>
      <TvApp {...props} />
    </StrictMode>,
  );
}
