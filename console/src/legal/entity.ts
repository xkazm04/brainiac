/*
 * The one place the operator fills in who they are.
 *
 * These pages are TEMPLATE-GRADE: every factual statement about what the
 * product does is real and was read out of the code, but the identity of the
 * entity offering it cannot be. Fill these four values before launch — the
 * pages render the placeholder verbatim, on purpose, so an unfilled policy is
 * visibly unfinished rather than quietly wrong.
 *
 * NOT env-driven: a legal page that says something different depending on
 * which container served it is not a policy. It is a constant of the
 * deployment and belongs in the source.
 *
 * ⚠ These pages are a starting point drafted against the implementation, not
 * legal advice. Have a lawyer review them before you take money or process
 * data for anyone in a regulated jurisdiction.
 */

/** The legal entity or individual operating this deployment. */
export const OPERATOR = "[OPERATOR NAME]";

/** Postal address — required by GDPR Art. 13 and most app stores. */
export const OPERATOR_ADDRESS = "[OPERATOR POSTAL ADDRESS]";

/** Where privacy requests and legal notices go. */
export const CONTACT_EMAIL = "[contact@example.com]";

/** Whose law governs the terms. */
export const GOVERNING_LAW = "[JURISDICTION]";

/**
 * Last substantive revision. Bump it when you change what a page SAYS, not
 * when you restyle it — a date that moves for a typo teaches readers to ignore
 * it.
 */
export const LEGAL_LAST_UPDATED = "5 August 2026";
