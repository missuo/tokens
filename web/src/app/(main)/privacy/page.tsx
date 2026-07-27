import type { Metadata } from "next";
import {
  Bullets,
  Clause,
  CONTACT_EMAIL,
  LegalPage,
} from "@/components/legal/LegalPage";

export const metadata: Metadata = {
  title: "Privacy Policy - Tokens",
  description:
    "What Tokens collects, why, who it goes to, and how to get it deleted.",
  robots: { index: true, follow: true },
};

const UPDATED = "25 July 2026";

export default function PrivacyPage() {
  return (
    <LegalPage
      title="Privacy Policy"
      description="What we collect, why we collect it, who it goes to, and how to get it removed."
      updated={UPDATED}
    >
      <Clause heading="The short version">
        <p>
          Tokens counts how many tokens you spend on AI coding tools and ranks
          that publicly. To do it we need your GitHub identity and the usage
          totals our CLI computes on your machine.
        </p>
        <p>
          The CLI reads the session files your AI coding tools already write to
          disk and adds them up locally. Only the totals are uploaded — token
          counts, cost figures, model names, client names and timestamps. The
          contents of those sessions never leave your machine: not your prompts,
          not the model&apos;s replies, not your code, not your file paths, not
          your repository or directory names.
        </p>
        <p>
          You can verify this rather than trust it. The CLI is open source, one
          parser per client, and{" "}
          <code className="font-mono text-[13px]">tokens submit --dry-run</code>{" "}
          prints the exact payload without uploading anything.
        </p>
      </Clause>

      <Clause heading="Who we are">
        <p>
          Tokens (<a href="https://tokens.ci">tokens.ci</a>) is an independent
          open-source project operated by Vincent Yang. For anything in this
          policy, including privacy requests, write to{" "}
          <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a>.
        </p>
      </Clause>

      <Clause heading="What we collect">
        <p>
          <strong className="font-medium text-foreground">
            From GitHub, when you sign in.
          </strong>{" "}
          We use GitHub OAuth with the{" "}
          <code className="font-mono text-[13px]">read:user user:email</code>{" "}
          scopes and store your GitHub numeric ID, username, display name,
          avatar URL, email address, and the public profile links GitHub exposes
          (such as a personal site or social handles). We never receive your
          GitHub password, and we do not request access to your repositories.
        </p>
        <p>
          <strong className="font-medium text-foreground">
            From the CLI, when you submit.
          </strong>{" "}
          Daily totals of tokens and estimated cost, broken down by AI coding
          client and by model, with the dates they belong to. Each submission is
          attributed to a device you name, so the same day reported from a
          laptop and a desktop is not double-counted.
        </p>
        <p>
          <strong className="font-medium text-foreground">
            To keep you signed in.
          </strong>{" "}
          A session record holding a hashed session token, its expiry, and the
          browser user-agent string of the session. Personal API tokens you
          create for the CLI, with the name you gave them and when they were
          last used.
        </p>
        <p>
          <strong className="font-medium text-foreground">
            If an account is banned.
          </strong>{" "}
          The time of the ban and the reason for it.
        </p>
      </Clause>

      <Clause heading="What we never collect">
        <Bullets
          items={[
            "Prompts, completions, or any conversation content.",
            "Source code, file contents, file paths, repository names or directory names.",
            "Your GitHub repositories, issues, or any private GitHub data.",
            "Payment details. Tokens is free and takes no payments.",
          ]}
        />
      </Clause>

      <Clause heading="Why we use it">
        <Bullets
          items={[
            "To build your public profile and place you on the leaderboard — the purpose of the service.",
            "To keep you signed in and to authenticate CLI submissions.",
            "To detect and act on fabricated submissions, which is what keeps the ranking meaningful.",
            "To answer you when you contact us.",
          ]}
        />
        <p>
          We do not use your data to train machine-learning models, and we do
          not profile you for advertising.
        </p>
      </Clause>

      <Clause heading="What is public">
        <p>
          Tokens is a public leaderboard, so this matters more here than on most
          sites. Your username, display name, avatar, usage totals, per-client
          and per-model breakdown, daily history, device names and rank are
          visible to anyone, including people who are not signed in, and are
          served through our public API, embeddable cards and share images.
        </p>
        <p>
          Your email address is never shown publicly and is not part of any API
          response.
        </p>
        <p>
          Because profiles are public, the iOS app needs no sign-in: it simply
          reads the public profile of whichever GitHub username you enter.
        </p>
      </Clause>

      <Clause heading="Cookies">
        <p>
          One cookie, <code className="font-mono text-[13px]">tt_session</code>,
          set only after you sign in. It is HTTP-only, Secure, SameSite=Lax, and
          expires after 30 days. It exists solely to keep you signed in.
        </p>
        <p>
          There are no advertising cookies, no analytics cookies, and no
          third-party trackers on this site. We do not run Google Analytics or
          any comparable product.
        </p>
      </Clause>

      <Clause heading="Who we share it with">
        <p>
          We do not sell personal information, and we do not share it for
          cross-context behavioural advertising. We have never done either.
        </p>
        <p>
          We rely on a small number of infrastructure providers who process data
          on our behalf:
        </p>
        <Bullets
          items={[
            <>
              <strong className="font-medium text-foreground">
                Cloudflare
              </strong>{" "}
              — hosting, edge caching and network protection for the site and
              API.
            </>,
            <>
              <strong className="font-medium text-foreground">Aiven</strong> —
              the managed PostgreSQL database where the records above are
              stored.
            </>,
            <>
              <strong className="font-medium text-foreground">GitHub</strong> —
              authentication, and the source of the profile fields and avatar
              images we display.
            </>,
          ]}
        />
        <p>
          We may also disclose information where the law requires it, or where
          it is necessary to investigate fraud or abuse of the leaderboard.
        </p>
      </Clause>

      <Clause heading="Where it is stored">
        <p>
          Data is stored in the United States. If you use Tokens from outside
          the United States, you are sending your information there.
        </p>
      </Clause>

      <Clause heading="How long we keep it">
        <p>
          Profile and usage data is kept while your account exists, because a
          leaderboard with a gap in its history is not a leaderboard. Sessions
          expire after 30 days. Expired device-authorisation codes are
          short-lived and cleared automatically.
        </p>
        <p>
          Deleting your account removes your profile, submissions, daily
          breakdown, devices, sessions and API tokens. Records of banned
          accounts are retained as evidence of the ban, with usernames partially
          masked.
        </p>
      </Clause>

      <Clause heading="Your choices">
        <p>These are built into the product; you do not have to ask us:</p>
        <Bullets
          items={[
            <>
              <strong className="font-medium text-foreground">
                Delete your usage data
              </strong>{" "}
              but keep your account — Settings, or{" "}
              <code className="font-mono text-[13px]">
                tokens delete-submitted-data
              </code>{" "}
              from the CLI.
            </>,
            <>
              <strong className="font-medium text-foreground">
                Delete your account
              </strong>{" "}
              and everything attached to it — Settings.
            </>,
            <>
              <strong className="font-medium text-foreground">
                Revoke API tokens
              </strong>{" "}
              at any time — Settings.
            </>,
            <>
              <strong className="font-medium text-foreground">
                Stop submitting
              </strong>{" "}
              — stop the background service; nothing is sent unless the CLI
              sends it.
            </>,
          ]}
        />
      </Clause>

      <Clause heading="California privacy rights">
        <p>
          If you are a California resident, the California Consumer Privacy Act
          as amended by the CPRA gives you the right to know what personal
          information we collect and how we use it, to request a copy of it, to
          request correction of inaccurate information, to request deletion, and
          to opt out of the sale or sharing of personal information.
        </p>
        <p>
          On that last point there is nothing to opt out of:{" "}
          <strong className="font-medium text-foreground">
            we do not sell or share personal information
          </strong>
          , and we do not process sensitive personal information for purposes
          requiring a right to limit.
        </p>
        <p>
          The categories we collect map to the CCPA as: identifiers (GitHub ID,
          username, email, avatar URL); internet or network activity
          (submission timestamps, session user-agent); and commercial-adjacent
          information in the form of your self-reported AI tool usage totals.
          Sources, purposes and recipients are described in the sections above.
        </p>
        <p>
          Use the controls in Settings, or email{" "}
          <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a>. We will
          verify a request by asking you to confirm control of the GitHub
          account in question. We will not discriminate against you for
          exercising any of these rights. You may use an authorised agent; we
          may ask for proof of their authority.
        </p>
      </Clause>

      <Clause heading="Other US state privacy laws">
        <p>
          Residents of states with comparable laws — including Virginia,
          Colorado, Connecticut, Utah and Texas — have similar rights of access,
          correction, deletion and portability. The same contact address and the
          same Settings controls serve those requests. Where a state law
          provides an appeal process for a refused request, you may appeal by
          replying to our response.
        </p>
      </Clause>

      <Clause heading="Children">
        <p>
          Tokens is not directed to children. You must be at least 13 to use it,
          and at least 16 if you are in a jurisdiction that sets that threshold.
          We do not knowingly collect information from children below those
          ages. If you believe a child has created an account, write to{" "}
          <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a> and we will
          remove it.
        </p>
      </Clause>

      <Clause heading="Security">
        <p>
          Session tokens are stored hashed, never in plaintext. Traffic is
          served over HTTPS. The session cookie is HTTP-only, so page scripts
          cannot read it. No system is perfectly secure, and we cannot guarantee
          absolute security, but if we discover a breach affecting your personal
          information we will notify affected users and any regulator the law
          requires.
        </p>
      </Clause>

      <Clause heading="Changes">
        <p>
          If this policy changes materially we will update the date at the top
          of this page and, where the change is significant, say so on the site.
          Continuing to use Tokens after a change means you accept the updated
          policy.
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
