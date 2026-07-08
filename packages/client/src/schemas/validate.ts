// Defensive boundary validation that doesn't change return types.
//
// `validate(schema, data)` asserts `data` matches `schema` (throwing a ZodError
// on mismatch) and returns it unchanged, keeping its declared ts-rs type. This
// lets a client method opt into runtime validation of a response without
// rippling branded/inferred types through every call site add one `.then` and
// the method keeps returning its generated type.

import type { z } from 'zod';

export function validate<T>(schema: z.ZodType, data: T): T {
  schema.parse(data);
  return data;
}
