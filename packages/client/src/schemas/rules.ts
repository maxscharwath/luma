// Shared field-validation rules, defined once and reused by every client app
// (web / TV / desktop …) for form validation. They mirror the server's Rust
// checks (see `server/src/api/accounts.rs`) so the client rejects the same input
// the server would the server remains the authority, this is just fast, shared,
// consistent client-side feedback.

import { z } from 'zod';

/** Email: trimmed + lower-cased, then a real email check (zod's `z.email`). */
export const emailRule = z.string().trim().toLowerCase().pipe(z.email({ error: 'auth.emailInvalid' }));

/** Password: at least 4 characters (server: `password.len() < 4` → reject). */
export const passwordRule = z.string().min(4, { message: 'auth.passwordTooShort' });

/** Username: trimmed, non-empty (server: rejects an empty display name). */
export const usernameRule = z.string().trim().min(1, { message: 'auth.usernameInvalid' });

/** PIN: exactly 4 digits (server: `is_valid_pin`). */
export const pinRule = z.string().regex(/^\d{4}$/, { message: 'auth.pinInvalid' });

/** Convenience booleans for inline form gating (no thrown errors). */
const passes = (rule: z.ZodType) => (s: string) => rule.safeParse(s).success;
export const isEmail = passes(emailRule);
export const isPassword = passes(passwordRule);
export const isUsername = passes(usernameRule);
export const isPin = passes(pinRule);
