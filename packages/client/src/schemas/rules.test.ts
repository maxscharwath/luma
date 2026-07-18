import { describe, expect, it } from 'vitest';
import {
  emailRule,
  isEmail,
  isPassword,
  isPin,
  isUsername,
  passwordRule,
  pinRule,
  usernameRule,
} from './rules';

describe('emailRule', () => {
  it('trims and lower-cases a valid email', () => {
    expect(emailRule.parse('  User@Example.COM ')).toBe('user@example.com');
  });

  it('rejects a malformed email', () => {
    expect(emailRule.safeParse('nope').success).toBe(false);
    expect(emailRule.safeParse('a@').success).toBe(false);
    expect(emailRule.safeParse('').success).toBe(false);
  });
});

describe('passwordRule', () => {
  it('requires at least 4 characters', () => {
    expect(passwordRule.safeParse('abcd').success).toBe(true);
    expect(passwordRule.safeParse('abc').success).toBe(false);
  });
});

describe('usernameRule', () => {
  it('trims and requires a non-empty name', () => {
    expect(usernameRule.parse('  bob ')).toBe('bob');
    expect(usernameRule.safeParse('   ').success).toBe(false);
    expect(usernameRule.safeParse('').success).toBe(false);
  });
});

describe('pinRule', () => {
  it('requires exactly four digits', () => {
    expect(pinRule.safeParse('1234').success).toBe(true);
    expect(pinRule.safeParse('0000').success).toBe(true);
    expect(pinRule.safeParse('123').success).toBe(false);
    expect(pinRule.safeParse('12345').success).toBe(false);
    expect(pinRule.safeParse('12a4').success).toBe(false);
  });
});

describe('convenience booleans', () => {
  it('mirror the rules without throwing', () => {
    expect(isEmail('a@b.co')).toBe(true);
    expect(isEmail('x')).toBe(false);
    expect(isPassword('abcd')).toBe(true);
    expect(isPassword('a')).toBe(false);
    expect(isUsername('bob')).toBe(true);
    expect(isUsername('  ')).toBe(false);
    expect(isPin('0000')).toBe(true);
    expect(isPin('00')).toBe(false);
  });
});
