import type { Metadata } from "next";
import {
  Bullets,
  Clause,
  CONTACT_EMAIL,
  LegalPage,
} from "@/components/legal/LegalPage";
import { getTranslator } from "@/lib/i18n/server";
import { richText } from "@/lib/i18n/rich";

export async function generateMetadata(): Promise<Metadata> {
  const { t } = await getTranslator();

  return {
    title: `${t("legal.privacy.title")} - Tokens`,
    description: t("legal.privacy.metaDescription"),
    robots: { index: true, follow: true },
  };
}

/** Inline code, sized to sit inside the prose without reflowing the line. */
function Code({ children }: { children: React.ReactNode }) {
  return <code className="font-mono text-[13px]">{children}</code>;
}

export default async function PrivacyPage() {
  const { t } = await getTranslator();
  const s = (section: string) => t(`legal.privacy.sections.${section}`);

  return (
    <LegalPage
      title={t("legal.privacy.title")}
      description={t("legal.privacy.description")}
      updated={t("legal.privacy.updated")}
      englishHref="/privacy"
    >
      <Clause heading={t("legal.privacy.sections.shortVersion.heading")}>
        <p>{s("shortVersion.p1")}</p>
        <p>{s("shortVersion.p2")}</p>
        <p>
          {richText(t("legal.privacy.sections.shortVersion.p3"), {
            dryRun: <Code>tokens submit --dry-run</Code>,
          })}
        </p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.whoWeAre.heading")}>
        <p>
          {richText(t("legal.privacy.sections.whoWeAre.body"), {
            ci: <a href="https://tokens.ci">tokens.ci</a>,
            email: <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a>,
          })}
        </p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.whatWeCollect.heading")}>
        <p>
          <strong className="font-medium text-foreground">
            {s("whatWeCollect.p1Strong")}
          </strong>{" "}
          {richText(t("legal.privacy.sections.whatWeCollect.p1"), {
            scopes: <Code>read:user user:email</Code>,
          })}
        </p>
        <p>
          <strong className="font-medium text-foreground">
            {s("whatWeCollect.p2Strong")}
          </strong>{" "}
          {s("whatWeCollect.p2")}
        </p>
        <p>
          <strong className="font-medium text-foreground">
            {s("whatWeCollect.p3Strong")}
          </strong>{" "}
          {s("whatWeCollect.p3")}
        </p>
        <p>
          <strong className="font-medium text-foreground">
            {s("whatWeCollect.p4Strong")}
          </strong>{" "}
          {s("whatWeCollect.p4")}
        </p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.neverCollect.heading")}>
        <Bullets
          items={["item1", "item2", "item3", "item4"].map((key) =>
            s(`neverCollect.${key}`),
          )}
        />
      </Clause>

      <Clause heading={t("legal.privacy.sections.whyWeUse.heading")}>
        <Bullets
          items={["item1", "item2", "item3", "item4"].map((key) =>
            s(`whyWeUse.${key}`),
          )}
        />
        <p>{s("whyWeUse.outro")}</p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.whatIsPublic.heading")}>
        <p>
          {richText(t("legal.privacy.sections.whatIsPublic.p1"), {
            example: <Code>jo****ro</Code>,
          })}
        </p>
        <p>
          {richText(t("legal.privacy.sections.whatIsPublic.p2"), {
            command: <Code>tokens login --private</Code>,
          })}
        </p>
        <p>{s("whatIsPublic.p3")}</p>
        <p>{s("whatIsPublic.p4")}</p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.cookies.heading")}>
        <p>
          {richText(t("legal.privacy.sections.cookies.p1"), {
            cookie: <Code>tt_session</Code>,
          })}
        </p>
        <p>{s("cookies.p2")}</p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.sharing.heading")}>
        <p>{s("sharing.p1")}</p>
        <p>{s("sharing.p2")}</p>
        <Bullets
          items={[1, 2, 3, 4].map((n) => (
            <>
              <strong className="font-medium text-foreground">
                {s(`sharing.provider${n}Name`)}
              </strong>{" "}
              {s(`sharing.provider${n}`)}
            </>
          ))}
        />
        <p>{s("sharing.p3")}</p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.whereStored.heading")}>
        <p>{s("whereStored.body")}</p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.retention.heading")}>
        <p>{s("retention.p1")}</p>
        <p>{s("retention.p2")}</p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.yourChoices.heading")}>
        <p>{s("yourChoices.intro")}</p>
        <Bullets
          items={[1, 2, 3, 4].map((n) => (
            <>
              <strong className="font-medium text-foreground">
                {s(`yourChoices.item${n}Strong`)}
              </strong>{" "}
              {n === 1
                ? richText(s("yourChoices.item1"), {
                    command: <Code>tokens delete-submitted-data</Code>,
                  })
                : s(`yourChoices.item${n}`)}
            </>
          ))}
        />
      </Clause>

      <Clause heading={t("legal.privacy.sections.california.heading")}>
        <p>{s("california.p1")}</p>
        <p>
          {richText(t("legal.privacy.sections.california.p2"), {
            strong: (
              <strong className="font-medium text-foreground">
                {s("california.p2Strong")}
              </strong>
            ),
          })}
        </p>
        <p>{s("california.p3")}</p>
        <p>
          {richText(t("legal.privacy.sections.california.p4"), {
            email: <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a>,
          })}
        </p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.otherStates.heading")}>
        <p>{s("otherStates.body")}</p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.children.heading")}>
        <p>
          {richText(t("legal.privacy.sections.children.body"), {
            email: <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a>,
          })}
        </p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.security.heading")}>
        <p>{s("security.body")}</p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.changes.heading")}>
        <p>{s("changes.body")}</p>
      </Clause>

      <Clause heading={t("legal.privacy.sections.contact.heading")}>
        <p>
          <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a>
        </p>
      </Clause>
    </LegalPage>
  );
}
