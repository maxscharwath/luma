import { Button, Logo, useT } from '@luma/ui';
import { useState } from 'react';
import { useConnection } from '#tv/connection';
import { useFocusNav } from '#tv/useFocusNav';

/**
 * Server discovery / connection screen. Prop-free — reads everything from
 * useConnection(). The router's guard shows it whenever status !== 'ready'.
 */
export function TvConnect() {
  const { status, serverUrl, error, platform, connect, discover } = useConnection();
  const t = useT();
  const [value, setValue] = useState(serverUrl ?? 'http://luma.local:4040');
  const discovering = status === 'discovering';
  // Wire the remote: spatial focus + OK across the input and buttons. Re-runs on
  // status change so focus lands on the right control (button vs. form).
  useFocusNav({ resetKey: status });

  let heading = t('connect.serverNotFound');
  if (discovering) heading = t('connect.searchingServer');
  else if (status === 'connecting') heading = t('connect.connectingServer');

  let sub = t('connect.serverNotFoundHint', { platform });
  if (discovering) sub = t('connect.discoveryHint');
  else if (status === 'connecting') sub = t('connect.connectingTo', { url: serverUrl ?? '' });

  return (
    <div className="grid min-h-screen place-items-center p-16 text-center">
      <div className="max-w-170">
        <div className="mb-7">
          <Logo size={44} />
        </div>
        <h1 className="m-0 mb-3 font-display text-[38px] font-bold">{heading}</h1>
        <p className="font-display text-[20px] font-normal text-muted">{sub}</p>
        {error ? <p className="font-sans text-[13px] text-dim">{error}</p> : null}

        {discovering ? (
          <div className="mt-6">
            <Button data-focus="" onClick={discover}>
              {t('connect.searchAgain')}
            </Button>
          </div>
        ) : (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              connect(value.trim());
            }}
            className="mx-auto mt-6 flex w-full max-w-130 flex-col gap-4"
          >
            <input
              data-focus=""
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder="http://luma.local:4040"
              spellCheck={false}
              className="w-full rounded-md border border-border-strong bg-surface-2 px-5 py-4 text-center font-sans text-[18px] text-text"
            />
            <div className="flex justify-center gap-3.5">
              <Button type="submit" data-focus="">
                {t('connect.connect')}
              </Button>
              <Button type="button" variant="glass" data-focus="" onClick={discover}>
                {t('connect.detect')}
              </Button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
