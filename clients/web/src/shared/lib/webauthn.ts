// Browser-side WebAuthn plumbing. The server speaks the webauthn-rs JSON shape
// (binary fields as base64url strings); the `navigator.credentials` API speaks
// ArrayBuffers. These helpers convert between the two and drive the two
// ceremonies, so the card/login code just deals with plain client calls.
//
// WebAuthn is only available in a secure context (HTTPS or localhost); callers
// must gate their UI on `passkeysSupported()` first.

import type { WebAuthnCredential, WebAuthnOptions } from '@kroma/core';

/** Whether the browser can run a WebAuthn ceremony here. */
export function passkeysSupported(): boolean {
  return (
    typeof window !== 'undefined' &&
    window.isSecureContext &&
    typeof window.PublicKeyCredential !== 'undefined' &&
    !!navigator.credentials?.create
  );
}

/** base64url string → bytes. */
function decode(s: string): Uint8Array {
  const pad = s.length % 4 === 0 ? '' : '='.repeat(4 - (s.length % 4));
  const bin = atob((s + pad).replace(/-/g, '+').replace(/_/g, '/'));
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}

/** bytes → base64url string (no padding). */
function encode(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  let bin = '';
  for (let i = 0; i < bytes.length; i += 1) bin += String.fromCharCode(bytes[i] as number);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/** Convert the server's `publicKey` options (base64url binary fields) into the
 * ArrayBuffer form `navigator.credentials` expects. Handles both the creation
 * (`user.id`, `excludeCredentials`) and request (`allowCredentials`) shapes. */
function toBufferOptions(
  publicKey: Record<string, unknown>,
): PublicKeyCredentialCreationOptions & PublicKeyCredentialRequestOptions {
  const pk: Record<string, unknown> = { ...publicKey };
  pk.challenge = decode(pk.challenge as string);
  if (pk.user) {
    const user = { ...(pk.user as Record<string, unknown>) };
    user.id = decode(user.id as string);
    pk.user = user;
  }
  const convert = (list: unknown) =>
    (list as { id: string }[] | undefined)?.map((c) => ({ ...c, id: decode(c.id) }));
  if (pk.excludeCredentials) pk.excludeCredentials = convert(pk.excludeCredentials);
  if (pk.allowCredentials) pk.allowCredentials = convert(pk.allowCredentials);
  return pk as unknown as PublicKeyCredentialCreationOptions & PublicKeyCredentialRequestOptions;
}

/** Run the registration ceremony → the credential JSON to send to `finish`. */
export async function createPasskey(options: WebAuthnOptions): Promise<WebAuthnCredential> {
  const publicKey = toBufferOptions(options.publicKey);
  const cred = (await navigator.credentials.create({ publicKey })) as PublicKeyCredential | null;
  if (!cred) throw new Error('passkey creation cancelled');
  const res = cred.response as AuthenticatorAttestationResponse;
  return {
    id: cred.id,
    rawId: encode(cred.rawId),
    type: cred.type,
    response: {
      attestationObject: encode(res.attestationObject),
      clientDataJSON: encode(res.clientDataJSON),
      transports: res.getTransports?.() ?? [],
    },
    clientExtensionResults: cred.getClientExtensionResults(),
  };
}

/** Run the authentication ceremony → the assertion JSON to send to `finish`. */
export async function getPasskey(options: WebAuthnOptions): Promise<WebAuthnCredential> {
  const publicKey = toBufferOptions(options.publicKey);
  const cred = (await navigator.credentials.get({ publicKey })) as PublicKeyCredential | null;
  if (!cred) throw new Error('passkey request cancelled');
  const res = cred.response as AuthenticatorAssertionResponse;
  return {
    id: cred.id,
    rawId: encode(cred.rawId),
    type: cred.type,
    response: {
      authenticatorData: encode(res.authenticatorData),
      clientDataJSON: encode(res.clientDataJSON),
      signature: encode(res.signature),
      userHandle: res.userHandle ? encode(res.userHandle) : null,
    },
    clientExtensionResults: cred.getClientExtensionResults(),
  };
}
