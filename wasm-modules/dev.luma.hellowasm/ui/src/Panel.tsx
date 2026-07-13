import type { ModuleComponentProps } from '@luma/module-sdk';
import { useEffect, useState } from 'react';

// The module's page. It calls its OWN backend -- the sandboxed WASM guest -- via
// the host API at /api/plugin/dev.luma.hellowasm/ping and shows the JSON, proving
// the backend + frontend halves of a runtime-installed module work together.
export default function Panel({ host }: ModuleComponentProps) {
  const [result, setResult] = useState('loading...');
  useEffect(() => {
    host.api
      .get<unknown>('/plugin/dev.luma.hellowasm/ping')
      .then((r) => setResult(JSON.stringify(r, null, 2)))
      .catch((e) => setResult(`error: ${String(e)}`));
  }, [host]);
  return (
    <section className="mx-auto flex w-full max-w-2xl flex-col gap-4 p-6">
      <h1 className="text-2xl font-bold text-text">Hello WASM</h1>
      <p className="text-sm text-muted">
        This module was installed at runtime with no server rebuild. The page is a Module Federation
        remote; the JSON below comes from its sandboxed WASM backend via
        <code className="text-dim"> /api/plugin/dev.luma.hellowasm/ping</code>.
      </p>
      <pre className="overflow-x-auto rounded-lg border border-border bg-black/20 p-4 text-xs text-text">
        {result}
      </pre>
    </section>
  );
}
