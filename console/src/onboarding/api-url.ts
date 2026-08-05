import "server-only";

/**
 * The API base a DEVELOPER reaches from their own machine.
 *
 * Deliberately not `configFromEnv().baseUrl`: the console's own
 * `BRAINIAC_API_URL` is frequently a private address (a docker service name on
 * the compose network), which is correct for the console and useless in a
 * command a human pastes into their checkout. `BRAINIAC_PUBLIC_API_URL` is the
 * override a deployment sets when the two differ.
 *
 * Never a secret — the onboarding pairing is what carries authority, and it
 * carries it over the device flow, not in this string.
 */
export function publicApiUrl(): string {
  return (
    process.env.BRAINIAC_PUBLIC_API_URL ??
    process.env.BRAINIAC_API_URL ??
    "http://127.0.0.1:8600"
  );
}
