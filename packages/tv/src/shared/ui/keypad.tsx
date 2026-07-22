// The D-pad numeric keypad for the PIN screen.

import { Box, Focusable, Txt } from '@kroma/ui/kit';

const KEY = {
  height: 72,
  width: 88,
  alignItems: 'center' as const,
  justifyContent: 'center' as const,
  borderRadius: 22,
  backgroundColor: 'rgba(255, 255, 255, 0.06)',
};

const FOCUSED = { backgroundColor: 'rgba(244, 182, 66, 0.18)' };

const ROWS = [
  ['1', '2', '3'],
  ['4', '5', '6'],
  ['7', '8', '9'],
];

/**
 * A D-pad numeric keypad for the PIN screen: 1-9, then 0 / ⌫. There is no OK
 * button: the PIN validates automatically once the last digit is entered.
 */
export function Keypad({
  onDigit,
  onDelete,
}: Readonly<{
  onDigit: (d: string) => void;
  onDelete: () => void;
}>) {
  const key = (label: string, onPress: () => void, fontSize = 28) => (
    <Focusable
      key={label}
      onPress={onPress}
      label={label}
      focusScale={1.08}
      ring={false}
      style={KEY}
      focusedStyle={FOCUSED}
    >
      {({ focused }) => (
        <Txt style={{ fontSize, fontWeight: '700' }} color={focused ? 'accent' : 'text'}>
          {label}
        </Txt>
      )}
    </Focusable>
  );
  return (
    <Box gap={13}>
      {ROWS.map((row) => (
        <Box key={row.join('')} row gap={13}>
          {row.map((d) => key(d, () => onDigit(d)))}
        </Box>
      ))}
      <Box row gap={13}>
        {/* Spacer keeps 0 under the centre column now that OK is gone. */}
        <Box w={88} h={72} />
        {key('0', () => onDigit('0'))}
        {key('⌫', onDelete, 22)}
      </Box>
    </Box>
  );
}
