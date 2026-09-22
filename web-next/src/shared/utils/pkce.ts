/**
 * PKCE helpers for OAuth 2.1 / OIDC login flows.
 */

export function generateCodeVerifier(length: number = 64): string {
  const array = new Uint8Array(length);

  crypto.getRandomValues(array);

  return base64UrlEncode(array);
}

export async function generateCodeChallenge(
  codeVerifier: string,
): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(codeVerifier);
  const hash = await crypto.subtle.digest("SHA-256", data);

  return base64UrlEncode(new Uint8Array(hash));
}

function base64UrlEncode(buffer: Uint8Array): string {
  const base64 = btoa(String.fromCharCode(...buffer));

  return base64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export async function generatePKCE(): Promise<{
  codeVerifier: string;
  codeChallenge: string;
  codeChallengeMethod: "S256";
}> {
  const codeVerifier = generateCodeVerifier();
  const codeChallenge = await generateCodeChallenge(codeVerifier);

  return {
    codeVerifier,
    codeChallenge,
    codeChallengeMethod: "S256",
  };
}

export function generateState(): string {
  const array = new Uint8Array(16);

  crypto.getRandomValues(array);

  return base64UrlEncode(array);
}

export function generateNonce(length: number = 32): string {
  const array = new Uint8Array(length);

  crypto.getRandomValues(array);

  return base64UrlEncode(array);
}

const PKCE_STORAGE_KEY = "oauth_pkce";
const STATE_STORAGE_KEY = "oauth_state";
const NONCE_STORAGE_KEY = "oauth_nonce";
const RETURN_PATH_KEY = "oauth_return_path";

export function savePKCE(
  codeVerifier: string,
  state: string,
  nonce: string,
  returnPath?: string,
): void {
  sessionStorage.setItem(PKCE_STORAGE_KEY, codeVerifier);
  sessionStorage.setItem(STATE_STORAGE_KEY, state);
  sessionStorage.setItem(NONCE_STORAGE_KEY, nonce);

  if (returnPath) {
    sessionStorage.setItem(RETURN_PATH_KEY, returnPath);
  }
}

export function consumePKCE(): {
  codeVerifier: string | null;
  state: string | null;
  nonce: string | null;
  returnPath: string | null;
} {
  const codeVerifier = sessionStorage.getItem(PKCE_STORAGE_KEY);
  const state = sessionStorage.getItem(STATE_STORAGE_KEY);
  const nonce = sessionStorage.getItem(NONCE_STORAGE_KEY);
  const returnPath = sessionStorage.getItem(RETURN_PATH_KEY);

  sessionStorage.removeItem(PKCE_STORAGE_KEY);
  sessionStorage.removeItem(STATE_STORAGE_KEY);
  sessionStorage.removeItem(NONCE_STORAGE_KEY);
  sessionStorage.removeItem(RETURN_PATH_KEY);

  return { codeVerifier, state, nonce, returnPath };
}

export function verifyState(receivedState: string | null): boolean {
  const savedState = sessionStorage.getItem(STATE_STORAGE_KEY);

  return savedState !== null && savedState === receivedState;
}
