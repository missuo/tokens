import type { Metadata } from "next";
import {
  Bullets,
  Clause,
  CONTACT_EMAIL,
  LegalPage,
} from "@/components/legal/LegalPage";

export const metadata: Metadata = {
  title: "Terms of Service - Tokens",
  description: "The rules for using Tokens, and what we do and do not promise.",
  robots: { index: true, follow: true },
};

const UPDATED = "25 July 2026";

export default function TermsPage() {
  return (
    <LegalPage
      title="Terms of Service"
      description="The rules for using Tokens, and what we do and do not promise."
      updated={UPDATED}
    >
      <Clause heading="Agreement">
        <p>
          These terms are between you and Vincent Yang, the operator of Tokens
          (<a href="https://tokens.ci">tokens.ci</a>), and cover the website, the
          public API, the command-line tool and the iOS app. Using any of them
          means you accept these terms. If you do not accept them, do not use
          Tokens.
        </p>
      </Clause>

      <Clause heading="Who may use it">
        <p>
          You must be at least 13 years old, and at least 16 where your
          jurisdiction sets that threshold. You need a GitHub account, and you
          must be the person that account belongs to. One person, one account.
        </p>
        <p>
          You are responsible for what happens under your account and for
          keeping your API tokens private. An API token can submit usage on your
          behalf; treat it like a password. If you think one has leaked, revoke
          it in Settings.
        </p>
      </Clause>

      <Clause heading="What Tokens is">
        <p>
          A public leaderboard for AI coding usage. Our CLI reads the session
          files your AI coding tools write, totals them on your machine, and
          uploads only the totals, which we then rank and display.
        </p>
        <p>
          It is free. There is no paid tier, and no payment is ever taken.
        </p>
      </Clause>

      <Clause heading="Your data is public">
        <p>
          Publishing your standing is the point of the service. By submitting,
          you agree that your username, display name, avatar, usage totals,
          per-client and per-model breakdown, daily history, device names and
          rank are public — visible to anyone, served through our public API,
          and rendered into embeddable cards and share images that others may
          post elsewhere.
        </p>
        <p>
          You keep whatever rights you have in that data. You grant us a
          worldwide, non-exclusive, royalty-free licence to host, reproduce and
          display it for the purpose of operating and promoting Tokens. The
          licence ends when you delete the data, except for copies already made
          by third parties and for the ban records described below.
        </p>
        <p>
          Do not submit usage under an account whose GitHub profile carries
          content you are not entitled to publish.
        </p>
      </Clause>

      <Clause heading="Honest numbers">
        <p>
          A leaderboard is worth nothing if its numbers are invented. Usage is
          self-reported and we cannot cryptographically prove any of it, so this
          rule carries the whole service. You must not:
        </p>
        <Bullets
          items={[
            "Fabricate, inflate or otherwise manipulate submitted usage.",
            "Modify the CLI, or write your own client, in order to report usage you did not incur.",
            "Submit another person's usage as your own, or copy figures from another profile.",
            "Generate usage for the sole purpose of moving up the ranking.",
            "Operate multiple accounts to inflate a single person's standing.",
          ]}
        />
        <p>
          Reading the CLI, forking it, and building your own tools on the open
          API are all fine and encouraged. The line is reporting usage that did
          not happen.
        </p>
      </Clause>

      <Clause heading="Other things you must not do">
        <Bullets
          items={[
            "Break the law, or use Tokens to harass, defame or impersonate anyone.",
            "Attack the service — scrape at a rate that degrades it, attempt to overwhelm it, or probe for vulnerabilities without permission.",
            "Try to access another user's account, session or API tokens.",
            "Circumvent a ban, including by creating a replacement account.",
          ]}
        />
        <p>
          If you find a security vulnerability, please report it to{" "}
          <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a> before
          disclosing it. We will not pursue good-faith researchers who do.
        </p>
      </Clause>

      <Clause heading="Enforcement and the Hall of Shame">
        <p>
          When a submission looks fabricated we check it against the raw
          submitted data. If we conclude an account submitted fraudulent usage,
          we ban it: it can no longer sign in or submit, and none of its data
          counts toward any ranking.
        </p>
        <p>
          Banned accounts are listed publicly on the{" "}
          <a href="/shame">Hall of Shame</a> with their username partially
          masked, and their records are retained as evidence of the ban rather
          than deleted. We mask the name so that a ban cannot double as
          publicity.
        </p>
        <p>
          If you believe a ban was a mistake, email{" "}
          <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a> and we will
          look at it again. Reports of suspicious profiles go to the same
          address.
        </p>
      </Clause>

      <Clause heading="Accuracy">
        <p>
          Cost figures are estimates. They are computed from public pricing data
          and the token counts your tools recorded; they are not invoices, they
          do not account for your plan, discounts or credits, and they will not
          match what a provider actually bills you. Rankings depend on
          self-reported data and may be wrong or incomplete. Do not rely on any
          of it for financial or business decisions.
        </p>
      </Clause>

      <Clause heading="The software">
        <p>
          Tokens is open source under the MIT licence. Your use of the source
          code is governed by that licence, not by these terms — including the
          warranty disclaimer it contains. These terms govern your use of the
          hosted service at tokens.ci.
        </p>
      </Clause>

      <Clause heading="Availability and changes">
        <p>
          We may change, suspend or discontinue any part of Tokens at any time,
          and we may change these terms. Material changes will be reflected in
          the date at the top of this page. Continuing to use Tokens after a
          change means you accept it.
        </p>
        <p>
          You may stop using Tokens whenever you like and delete your account
          from Settings. We may suspend or terminate an account that breaches
          these terms.
        </p>
      </Clause>

      <Clause heading="No warranty">
        <p className="uppercase">
          Tokens is provided &ldquo;as is&rdquo; and &ldquo;as
          available&rdquo;, without warranty of any kind, express or implied,
          including any implied warranty of merchantability, fitness for a
          particular purpose, title or non-infringement. We do not warrant that
          the service will be uninterrupted, secure, or free of errors, or that
          any figure it displays is accurate.
        </p>
      </Clause>

      <Clause heading="Limitation of liability">
        <p className="uppercase">
          To the fullest extent permitted by law, neither the operator nor any
          contributor is liable for any indirect, incidental, special,
          consequential, exemplary or punitive damages, or for any loss of
          profits, revenue, data, goodwill or reputation, arising out of or
          related to your use of Tokens, on any theory of liability, even if
          advised of the possibility of such damages. Our total aggregate
          liability arising out of or related to these terms or the service
          shall not exceed one hundred United States dollars (US$100).
        </p>
        <p>
          Some jurisdictions do not allow the exclusion of certain warranties or
          the limitation of certain damages. Where that is the case, the above
          applies to the fullest extent the law allows, and nothing here limits
          liability that cannot lawfully be limited.
        </p>
      </Clause>

      <Clause heading="Indemnity">
        <p>
          You agree to indemnify and hold harmless the operator and contributors
          from any claim, demand, loss or expense, including reasonable legal
          fees, arising out of your use of Tokens, the data you submit, or your
          breach of these terms.
        </p>
      </Clause>

      <Clause heading="Governing law">
        <p>
          These terms are governed by the laws of the State of California,
          without regard to its conflict-of-laws rules. You and we agree to the
          exclusive jurisdiction of the state and federal courts located in
          California for any dispute that is not otherwise resolved, and each
          side waives any objection to venue there. Nothing in this clause
          removes any right you have to bring a claim in a small-claims court,
          or any non-waivable right under the law of your place of residence.
        </p>
      </Clause>

      <Clause heading="Everything else">
        <p>
          If any provision of these terms is held unenforceable, the rest
          remains in force and the unenforceable part is limited to the minimum
          extent necessary. Our failure to enforce a provision is not a waiver
          of it. These terms, together with the{" "}
          <a href="/privacy">Privacy Policy</a>, are the entire agreement
          between you and us regarding the service. You may not assign them; we
          may assign them in connection with a transfer of the project.
        </p>
      </Clause>

      <Clause heading="Contact">
        <p>
          <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a>
        </p>
      </Clause>
    </LegalPage>
  );
}
