import type { Metadata } from "next";

import { CONTACT_EMAIL, GOVERNING_LAW, OPERATOR, OPERATOR_ADDRESS } from "@/legal/entity";
import { Clause, LegalShell, Row, Strong } from "@/legal/LegalPage";

/*
 * A PUBLIC surface (src/design/routes.ts → PUBLIC_PATHS), for the same reason
 * the privacy page is: nobody can agree to terms they cannot read until after
 * they have signed in.
 *
 * The commercial facts here are the ones actually implemented:
 *   free, no payment path exists anywhere in the product
 *   one project per Google account — enforced in crates/…/provision.rs, and
 *     surfaced verbatim by app/signup/actions.ts ("one per account for now")
 *   device keys are shown once and stored as a SHA-256 hash (auth.rs)
 * Do not add a clause about something the code does not do.
 */

export const metadata: Metadata = {
  title: "Terms · Brainiac",
  description:
    "The terms for using Brainiac during its free beta: one project per account, your content stays yours, no warranty.",
};

export default function TermsPage() {
  return (
    <LegalShell
      kicker="legal"
      title="Terms of Service"
      intro={
        <>
          Short version: <Strong>Brainiac is free while it is in beta</Strong>, you
          get one project per Google account, whatever your agents store stays yours,
          and because it is beta it comes with no promises about uptime or
          durability. The clauses below are the binding version. Using the service
          means you accept them.
        </>
      }
    >
      <Clause id="parties" n={1} heading="Who these terms are between">
        <p>
          They are between you and <Strong>{OPERATOR}</Strong>, {OPERATOR_ADDRESS}{" "}
          (&ldquo;we&rdquo;), and they cover the hosted Brainiac service and this
          console. Contact: <Strong>{CONTACT_EMAIL}</Strong>.
        </p>
        <p>
          Brainiac&apos;s source is separately licensed; nothing here changes the
          rights that licence gives you to run your own copy.
        </p>
      </Clause>

      <Clause id="account" n={2} heading="Accounts and the free beta">
        <Row term="Free">
          The service is currently free. <Strong>There is no payment path in the
          product</Strong> and we are not charging anyone. If that changes we will
          give notice before it applies to you, and you will be able to export or
          delete your data instead of paying.
        </Row>
        <Row term="One project">
          One project per Google account while it is free. Signing in again returns
          the project you already have rather than creating another.
        </Row>
        <Row term="Sign-in">
          You need a Google account with a verified email. You are responsible for
          keeping access to it — losing it means losing the way back to your project.
        </Row>
        <Row term="Beta">
          The service is under active development. Features can change or be removed,
          and data can be migrated between formats.
        </Row>
      </Clause>

      <Clause id="keys" n={3} heading="Device keys">
        <p>
          A device key authenticates your agents. It is displayed{" "}
          <Strong>exactly once</Strong> and stored only as a hash — we cannot recover
          it for you, and neither can anyone who breaks into the database. Treat it
          like a password: anyone holding it can read and write your project.
        </p>
        <p>
          You are responsible for what happens under your keys. Tell us at{" "}
          <Strong>{CONTACT_EMAIL}</Strong> if one leaks, and issue a new one.
        </p>
      </Clause>

      <Clause id="content" n={4} heading="Your content">
        <p>
          <Strong>What your agents store is yours.</Strong> We claim no ownership of
          it. You grant us only the permission needed to run the service on it: to
          store it, index it, embed it, run it through the extraction pipeline
          (including sending it to the AI provider named in the{" "}
          <a href="/privacy#shared" className="underline underline-offset-4 hover:text-white">
            privacy policy
          </a>
          ), and serve it back to your own agents.
        </p>
        <p>
          You confirm you have the right to submit what you submit.{" "}
          <Strong>
            Do not submit secrets, credentials, or personal data about other people
          </Strong>{" "}
          — the product is designed to remember things for a long time, which is
          exactly the wrong property for a leaked API key.
        </p>
      </Clause>

      <Clause id="acceptable" n={5} heading="Acceptable use">
        <p>Do not use the service to:</p>
        <Row term="Break the law">
          Store or distribute unlawful material, or infringe anyone&apos;s rights.
        </Row>
        <Row term="Attack it">
          Probe, overload, or circumvent the authentication, rate limits or project
          isolation — yours or anyone else&apos;s.
        </Row>
        <Row term="Resell it">
          Operate the free tier as a backend for a product you sell, or evade the
          one-project limit with multiple accounts.
        </Row>
        <Row term="Dump data">
          Use it as bulk storage unrelated to engineering knowledge, or bury it in
          automated traffic that degrades it for others.
        </Row>
      </Clause>

      <Clause id="ai" n={6} heading="What the AI produces">
        <p>
          Brainiac extracts claims, resolves contradictions and composes pages using
          language models. <Strong>Model output can be wrong, and confidently so.</Strong>{" "}
          Everything the service surfaces — a canonical memory, a standard, a
          composed page, a health score — is a suggestion to be reviewed by a person,
          not a fact and not advice. The review and governance features exist because
          this is true; use them.
        </p>
      </Clause>

      <Clause id="availability" n={7} heading="Availability, suspension, and ending it">
        <p>
          There is <Strong>no uptime commitment and no support commitment</Strong>{" "}
          during the beta. We may change, suspend or discontinue any part of the
          service, and we will make a reasonable effort to give notice before
          anything that would lose your data.
        </p>
        <p>
          We may suspend or delete a project that breaches clause 5, or that is
          endangering the service for others. You can stop at any time and ask us to
          delete your project (
          <a href="/privacy#retention" className="underline underline-offset-4 hover:text-white">
            privacy, clause 6
          </a>
          ). <Strong>Keep your own backups of anything you cannot lose.</Strong>
        </p>
      </Clause>

      <Clause id="warranty" n={8} heading="No warranty">
        <p>
          The service is provided <Strong>&ldquo;as is&rdquo; and &ldquo;as
          available&rdquo;</Strong>, without warranties of any kind, express or
          implied, including fitness for a particular purpose, accuracy, and
          uninterrupted or error-free operation. Some jurisdictions do not allow
          excluding certain warranties; where that is so, this clause applies as far
          as the law permits and no further.
        </p>
      </Clause>

      <Clause id="liability" n={9} heading="Limitation of liability">
        <p>
          To the extent the law allows, we are not liable for indirect, incidental,
          special or consequential damages, nor for lost profits, lost data, or the
          cost of substitute services. Because the service is provided free of
          charge, our total liability to you for any claim is limited to{" "}
          <Strong>the amount you have paid for it, which is nothing</Strong>.
        </p>
        <p>
          Nothing here limits liability that cannot lawfully be limited — including
          for death or personal injury caused by negligence, or for fraud.
        </p>
      </Clause>

      <Clause id="changes" n={10} heading="Changes to these terms">
        <p>
          We may update these terms; the date at the top moves when we do. For
          material changes we will give notice before they take effect. Continuing to
          use the service after that means you accept the new version — and if you do
          not, delete your project.
        </p>
      </Clause>

      <Clause id="law" n={11} heading="Governing law">
        <p>
          These terms are governed by the laws of <Strong>{GOVERNING_LAW}</Strong>,
          and disputes go to its courts. If you are a consumer, this does not remove
          the protection of the mandatory law of the country you live in.
        </p>
        <p>
          If a clause turns out to be unenforceable, the rest stays in force. Not
          enforcing something once does not waive it.
        </p>
      </Clause>
    </LegalShell>
  );
}
