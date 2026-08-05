import type { Metadata } from "next";

import { CONTACT_EMAIL, OPERATOR, OPERATOR_ADDRESS } from "@/legal/entity";
import { Clause, LegalShell, Row, Strong } from "@/legal/LegalPage";

/*
 * A PUBLIC surface (src/design/routes.ts → PUBLIC_PATHS). A privacy policy
 * behind a login is not a privacy policy: it has to be readable BEFORE someone
 * decides whether to sign in.
 *
 * Everything factual here was read out of the implementation, not templated:
 *   identity + provisioning   app/signup/actions.ts, crates/…/provision.rs
 *   device keys (sha256)      crates/brainiac-server/src/auth.rs::hash_token
 *   the one cookie            src/lib/auth.ts (SESSION_COOKIE, httpOnly), middleware.ts
 *   error diagnostics         instrumentation-client.ts, sentry.server.config.ts
 *   product analytics         src/analytics/* (Plausible, cookieless)
 *   LLM processing            crates/brainiac-gateway (Qwen/DashScope)
 *   LLM call metering         crates/brainiac-gateway/src/lighttrack.rs (no content)
 * If you change any of those, change this page in the same commit. A policy
 * that describes last month's product is a liability, not a document.
 */

export const metadata: Metadata = {
  title: "Privacy · Brainiac",
  description:
    "What Brainiac collects, why, who processes it, and how to have it deleted.",
};

export default function PrivacyPage() {
  return (
    <LegalShell
      kicker="legal"
      title="Privacy"
      intro={
        <>
          Brainiac stores what your coding agents learn so they can be reminded of
          it later. That means it holds text you and your agents chose to send, plus
          the small amount of account data needed to know whose project it is. This
          page says exactly what that is, who else touches it, and how to get rid of
          it. <Strong>It is written against the running code, not against a template.</Strong>
        </>
      }
    >
      <Clause id="who" n={1} heading="Who is responsible">
        <p>
          This service is operated by <Strong>{OPERATOR}</Strong>, {OPERATOR_ADDRESS}. For
          anything on this page — access, correction, export, deletion, or a
          complaint — write to <Strong>{CONTACT_EMAIL}</Strong>.
        </p>
        <p>
          Brainiac is also open-source software that can be self-hosted. If you are
          using someone else&apos;s deployment, that operator — not us — is the
          controller of the data in it.
        </p>
      </Clause>

      <Clause id="collect" n={2} heading="What is collected, and why">
        <Row term="Google account">
          When you sign in to claim a project we receive your{" "}
          <Strong>Google account identifier, email address and display name</Strong>{" "}
          from Google. We verify the sign-in server-side and store the identifier and
          email against your project. The identifier is how &ldquo;one project per
          account&rdquo; is enforced; the email is how a human recognises their own
          project later. We never receive your Google password.
        </Row>
        <Row term="Your project">
          A project (organisation, team and user record) is created for you on first
          sign-in, named from your account.
        </Row>
        <Row term="Device keys">
          Keys your agents authenticate with. The secret is shown to you exactly once
          and is <Strong>stored only as a SHA-256 hash</Strong> — we cannot recover or
          re-display it. A short non-secret prefix is kept so you can tell keys apart.
        </Row>
        <Row term="Memories">
          The text your agents submit, plus what the pipeline derives from it
          (structured claims, embeddings, provenance, review state) and the metadata
          you attach — team, project, repository, timestamps.{" "}
          <Strong>
            This content is entirely under your control: whatever your agent sends is
            what is stored.
          </Strong>{" "}
          Do not send secrets, credentials, or personal data about other people.
        </Row>
        <Row term="Server logs">
          Ordinary operational logs of requests to the service, used to run it and
          diagnose faults.
        </Row>
        <Row term="Error diagnostics">
          When error reporting is enabled on a deployment, crashes and exceptions are
          sent to Sentry: the error, a stack trace, the URL, and browser or server
          environment. An error message can incidentally include a fragment of what
          was being processed at the time.
        </Row>
        <Row term="Usage analytics">
          See <a href="#analytics" className="underline underline-offset-4 hover:text-white">clause 4</a>.
          Aggregate only, cookieless, no profile.
        </Row>
        <Row term="AI call metering">
          Counts of language-model calls the service makes on your behalf — provider,
          model, token counts, latency, success or failure.{" "}
          <Strong>No prompt or memory content is included</Strong> in these metering
          events.
        </Row>
        <p>
          There is no advertising, no tracking across other websites, no data broker,
          and nothing here is sold.
        </p>
      </Clause>

      <Clause id="basis" n={3} heading="Why we are allowed to hold it">
        <p>
          Account data, project records, device keys and memories are processed to{" "}
          <Strong>perform the service you asked for</Strong> — without them there is
          no project and nothing to remember.
        </p>
        <p>
          Server logs, error diagnostics and aggregate usage counts are processed on
          a <Strong>legitimate interest</Strong> in keeping the service running,
          secure and improvable. Because the analytics are aggregate and cookieless
          (clause 4), that interest does not come at the cost of a profile of you.
        </p>
      </Clause>

      <Clause id="analytics" n={4} heading="Analytics and cookies">
        <p>
          <Strong>This site sets exactly one cookie.</Strong> It is called{" "}
          <code className="text-white">bx_console</code>, it is set only when an
          operator signs in to the console with the deployment passcode, it is
          HttpOnly (no script can read it), and its only job is to keep that operator
          signed in. It is strictly necessary for a function you asked for, so no
          consent banner is required and none is shown.
        </p>
        <p>
          Signing in with Google keeps your sign-in state in your browser&apos;s own
          storage rather than in a cookie on this site. Google&apos;s own sign-in
          pages are Google&apos;s, and set their cookies under Google&apos;s policy.
        </p>
        <p>
          For product analytics we use <Strong>Plausible</Strong>, chosen precisely
          because it is <Strong>cookieless</Strong>: it sets no cookie, stores no
          identifier for you, and cannot follow you across sites. It records page
          views and three product events — a landing-page view, a completed sign-up,
          and an &ldquo;activation&rdquo; when knowledge is first added to or
          promoted within a project. Each carries at most a coarse shape
          (&ldquo;first project&rdquo; versus &ldquo;returning&rdquo;) and{" "}
          <Strong>never a user, project, or memory identifier</Strong>. Plausible
          derives country and device type from the connection and does not retain the
          IP address; for events sent by our server we deliberately do not forward
          your address at all.
        </p>
        <p>
          Analytics is off unless the deployment configures it. If it is off, no
          analytics request leaves your browser and no third-party script is loaded.
          <Strong>
            {" "}
            If a future version adopts a tool that stores an identifier on your
            device, this page changes and consent is asked for first.
          </Strong>
        </p>
      </Clause>

      <Clause id="shared" n={5} heading="Who else processes it">
        <p>These are the only third parties involved, each doing one job:</p>
        <Row term="Google">Verifies your sign-in (Firebase Authentication).</Row>
        <Row term="AI provider">
          Memory text is sent to a language-model provider (by default Alibaba
          Cloud&apos;s DashScope, running Qwen models) to extract structure and
          compute embeddings.{" "}
          <Strong>This is the one processor that sees your content.</Strong> A
          self-hosted deployment can point this at a different provider or run
          without it.
        </Row>
        <Row term="Sentry">Receives error diagnostics, when enabled.</Row>
        <Row term="Plausible">Receives aggregate usage events, when enabled.</Row>
        <Row term="Hosting">
          The infrastructure provider hosting the service and its database.
        </Row>
        <p>
          Beyond these we disclose data only where the law compels it, and we will
          tell you unless we are forbidden to.
        </p>
      </Clause>

      <Clause id="retention" n={6} heading="How long it is kept">
        <p>
          Memories and project data are kept until you delete them or ask us to
          delete your project — this is a memory product, so it deliberately does not
          expire your knowledge on a timer. Revoked device keys are kept as hashes
          for audit. Logs and error diagnostics are kept for a limited operational
          window and then discarded.
        </p>
        <p>
          Ask at <Strong>{CONTACT_EMAIL}</Strong> and we will delete your project and
          everything in it. Aggregate analytics counts contain nothing that
          identifies you and are not deleted individually, because there is no
          individual record in them to find.
        </p>
      </Clause>

      <Clause id="rights" n={7} heading="Your rights">
        <p>
          You can ask for a copy of your data, correction of it, deletion of it, or
          restriction of how it is used, and you can object to the processing done on
          a legitimate-interest basis. Write to <Strong>{CONTACT_EMAIL}</Strong>. If
          you are in the UK, EU or EEA you may also complain to your national data
          protection authority.
        </p>
      </Clause>

      <Clause id="security" n={8} heading="Security">
        <p>
          Traffic is served over HTTPS. Device keys are stored as hashes, never in
          the clear. Projects are isolated at the database level so one
          project&apos;s queries cannot reach another&apos;s rows. The operator
          console is behind its own gate and its session cookie is HttpOnly.
        </p>
        <p>
          <Strong>
            No system is perfectly secure, and this one is in beta.
          </Strong>{" "}
          Treat it accordingly: do not put credentials, keys, or regulated personal
          data into a memory.
        </p>
      </Clause>

      <Clause id="children" n={9} heading="Children">
        <p>
          This is a developer tool and is not directed at anyone under 16. We do not
          knowingly collect their data; if you believe we have, tell us and we will
          remove it.
        </p>
      </Clause>

      <Clause id="changes" n={10} heading="Changes">
        <p>
          When this page changes we update the date at the top. If a change
          materially affects what is collected or who receives it — including any
          change to the cookie position in clause 4 — we will say so before it takes
          effect for existing users.
        </p>
      </Clause>
    </LegalShell>
  );
}
