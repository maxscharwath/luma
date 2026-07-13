import { useT } from '@luma/ui';
import { IconWorldSearch } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useNav } from '#tv/app/router';
import { useFocusNav } from '#tv/app/useFocusNav';
import { AuthScreen, LumaMark, OnScreenKeyboard, TvTextEntry } from '#tv/shared/ui';

/**
 * Add a (distant) server by address, via an on-screen URL keyboard. Reached on
 * first run (no server saved) and from the add-profile wizard's "Add manually".
 * On submit the server is upserted and the flow advances to Quick Connect. A
 * Detect button kicks off LAN discovery and prefills the field.
 */
export function TvConnect() {
  const nav = useNav();
  const t = useT();
  const { addServer, discover, discovered, discovering } = useConnection();
  const [value, setValue] = useState('');
  // `connect` is only ever reached from the Add-profile wizard, so Back returns
  // there (never a dead-end at the launch screen).
  useFocusNav({ onBack: nav.back, resetKey: discovered.length });

  // When LAN discovery finds something, prefill the field (host only) so OK adds it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: prefill once when discovery yields a hit; intentionally not re-run on `value` edits.
  useEffect(() => {
    const found = discovered.at(-1);
    if (found && !value) {
      try {
        setValue(new URL(found).host);
      } catch {
        setValue(found);
      }
    }
  }, [discovered]);

  const submit = () => {
    let url = value.trim();
    if (!url) return;
    if (!/^https?:\/\//.test(url)) url = `http://${url}`;
    addServer(url);
    nav.go('quick');
  };

  return (
    <AuthScreen>
      <div className="mb-6">
        <LumaMark size={32} />
      </div>
      <div className="w-full max-w-[720px]">
        <h1 className="m-0 mb-1.5 text-center font-display text-[38px] font-semibold">
          {t('connect.addServerTitle')}
        </h1>
        <p className="m-0 mb-6 text-center font-sans text-[16px] font-medium text-dim">
          {t('connect.addServerSub')}
        </p>

        <div className="mb-5 flex items-center gap-3 rounded-[13px] border border-border-strong bg-[#0F0F13] px-5 py-4">
          <IconWorldSearch size={20} className="flex-none text-dim" stroke={1.7} />
          <TvTextEntry
            value={value}
            onChange={setValue}
            onSubmit={submit}
            placeholder={t('connect.serverPlaceholder')}
            ariaLabel={t('connect.addServerTitle')}
            inputMode="url"
            textClassName="min-w-0 flex-1 overflow-hidden whitespace-nowrap font-sans text-[20px] font-semibold text-text"
            placeholderClassName="text-[rgba(244,243,240,0.3)]"
            cursorClassName="ml-px inline-block h-5.5 w-0.5 translate-y-1 bg-accent animate-[tv-blink_1s_step-end_infinite]"
          />
          <button
            data-focus=""
            type="button"
            onClick={discover}
            className="flex-none cursor-pointer rounded-lg border border-border-strong bg-transparent px-4 py-2 font-sans text-[14px] font-semibold text-muted outline-none transition-transform focus:scale-[1.05] focus:border-accent focus:text-accent"
          >
            {discovering ? t('common.detecting') : t('connect.detect')}
          </button>
        </div>

        <OnScreenKeyboard
          value={value}
          onChange={setValue}
          onSubmit={submit}
          layout="url"
          submitLabel={t('connect.connect')}
        />

        <div className="mt-5 text-center font-sans text-[14px] font-medium text-[rgba(244,243,240,0.4)]">
          {t('connect.keyboardHint')}
        </div>
      </div>
    </AuthScreen>
  );
}
