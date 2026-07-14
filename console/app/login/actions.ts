"use server";

import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import {
  configuredPasscode,
  safeEqual,
  SESSION_COOKIE,
  SESSION_MAX_AGE,
  sessionToken,
} from "@/lib/auth";

/** Only ever redirect within this app — never to an attacker-supplied origin. */
function safeNext(raw: unknown): string {
  const next = typeof raw === "string" ? raw : "";
  return next.startsWith("/") && !next.startsWith("//") ? next : "/console";
}

export async function login(formData: FormData) {
  const next = safeNext(formData.get("next"));
  const entered = String(formData.get("passcode") ?? "");
  const passcode = configuredPasscode();

  // redirect() works by throwing, so it must never sit inside a try/catch.
  if (!passcode) {
    redirect(`/login?err=unconfigured&next=${encodeURIComponent(next)}`);
  }
  if (!safeEqual(entered, passcode)) {
    redirect(`/login?err=bad&next=${encodeURIComponent(next)}`);
  }

  const jar = await cookies();
  jar.set(SESSION_COOKIE, await sessionToken(passcode), {
    httpOnly: true,
    sameSite: "lax",
    secure: process.env.NODE_ENV === "production",
    path: "/",
    maxAge: SESSION_MAX_AGE,
  });

  redirect(next);
}

export async function logout() {
  const jar = await cookies();
  jar.delete(SESSION_COOKIE);
  redirect("/");
}
