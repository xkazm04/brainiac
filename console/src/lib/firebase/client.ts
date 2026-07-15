"use client";

import { getApp, getApps, initializeApp } from "firebase/app";
import { GoogleAuthProvider, getAuth, signInWithPopup } from "firebase/auth";

import { firebaseWebConfig } from "./config";

/*
 * Google sign-in, browser half. Lazily initialized so a console with no Firebase
 * config never loads the SDK or throws — the operator passcode gate is the
 * primary way in and must keep working untouched.
 *
 * The ID token this returns is a CLAIM, not proof. It is only worth anything once
 * the server verifies it (`admin.ts`), which is where the uid that provisioning
 * trusts actually comes from.
 */

/** Sign in with Google and return a fresh ID token, or null if unconfigured. */
export async function signInWithGoogle(): Promise<string | null> {
  const cfg = firebaseWebConfig();
  if (!cfg) return null;
  const app = getApps().length ? getApp() : initializeApp(cfg);
  const auth = getAuth(app);
  const provider = new GoogleAuthProvider();
  // Always show the chooser: on a shared machine, silently reusing the last
  // Google session is how someone provisions a project onto a colleague's account.
  provider.setCustomParameters({ prompt: "select_account" });
  const cred = await signInWithPopup(auth, provider);
  return cred.user.getIdToken();
}
