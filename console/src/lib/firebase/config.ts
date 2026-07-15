/*
 * Firebase web config — the CLIENT half of Google sign-in.
 *
 * These values are public by design. A Firebase web `apiKey` is not a secret: it
 * identifies the project to Google's auth endpoints and is visible in any
 * browser's network tab. What actually protects the project is the authorized-
 * domain list plus server-side verification of the returned ID token (see
 * `admin.ts`) — never the key. So they live in NEXT_PUBLIC_* and that is correct,
 * not a leak.
 *
 * Absent config = feature off. The console must keep working with no Firebase at
 * all: the operator passcode gate is the primary way in and does not involve any
 * of this. `isFirebaseConfigured()` is what every caller branches on.
 */

export interface FirebaseWebConfig {
  apiKey: string;
  authDomain: string;
  projectId: string;
  appId: string;
}

export function firebaseWebConfig(): FirebaseWebConfig | null {
  const apiKey = process.env.NEXT_PUBLIC_FIREBASE_API_KEY?.trim();
  const authDomain = process.env.NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN?.trim();
  const projectId = process.env.NEXT_PUBLIC_FIREBASE_PROJECT_ID?.trim();
  const appId = process.env.NEXT_PUBLIC_FIREBASE_APP_ID?.trim();
  if (!apiKey || !authDomain || !projectId || !appId) return null;
  return { apiKey, authDomain, projectId, appId };
}

export function isFirebaseConfigured(): boolean {
  return firebaseWebConfig() !== null;
}
