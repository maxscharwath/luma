import { useT } from '@kroma/ui';
import { Box, Button, TextField, Txt, useFocusNav } from '@kroma/ui/kit';
import { useEffect, useState } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useEnv } from '#tv/app/providers/env';
import { useNav } from '#tv/app/router';
import { AuthScreen, KromaMark, OnScreenKeyboard } from '#tv/shared/ui';

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
  const { physicalKeyboard } = useEnv();
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
      <Box mb={24}>
        <KromaMark size={32} />
      </Box>
      <Box w="100%" maxW={720}>
        <Txt variant="h1" style={{ fontSize: 38, fontWeight: '600', textAlign: 'center' }}>
          {t('connect.addServerTitle')}
        </Txt>
        <Txt
          style={{
            fontSize: 16,
            fontWeight: '500',
            textAlign: 'center',
            marginTop: 6,
            marginBottom: 24,
          }}
          color="textDim"
        >
          {t('connect.addServerSub')}
        </Txt>

        <TextField
          value={value}
          onChange={setValue}
          onSubmit={submit}
          icon="world-search"
          placeholder={t('connect.serverPlaceholder')}
          label={t('connect.addServerTitle')}
          keyboardType="url"
          physicalKeyboard={physicalKeyboard}
          mb={20}
          py={16}
          radius="md"
          bg="#0F0F13"
          textStyle={{ fontSize: 20, fontWeight: '600' }}
          trailing={
            <Button
              variant="glass"
              size="sm"
              focusScale={1.05}
              label={discovering ? t('common.detecting') : t('connect.detect')}
              onPress={discover}
              style={DETECT}
            />
          }
        />

        <OnScreenKeyboard
          value={value}
          onChange={setValue}
          onSubmit={submit}
          layout="url"
          submitLabel={t('connect.connect')}
        />

        <Txt
          style={{ fontSize: 14, fontWeight: '500', textAlign: 'center', marginTop: 20 }}
          color="rgba(244, 243, 240, 0.4)"
        >
          {t('connect.keyboardHint')}
        </Txt>
      </Box>
    </AuthScreen>
  );
}

const DETECT = { flexShrink: 0, backgroundColor: 'transparent', paddingHorizontal: 16 } as const;
