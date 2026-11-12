import type { Metadata } from "next";
import {
  Bullets,
  Clause,
  CONTACT_EMAIL,
  LegalPage,
} from "@/components/legal/LegalPage";
import { getTranslator } from "@/lib/i18n/server";
import { richText } from "@/lib/i18n/rich";
import { localizeHref } from "@/lib/i18n/config";

export async function generateMetadata(): Promise<Metadata> {
  const { t } = await getTranslator();

  return {
    title: `${t("legal.terms.title")} - Tokens`,
    description: t("legal.terms.metaDescription"),
    robots: { index: true, follow: true },
  };
}

export default async function TermsPage() {
  const { locale, t } = await getTranslator();
  const s = (section: string) => t(`legal.terms.sections.${section}`);

  /**
   * One bullet per catalogue key, in the order the clause lists them.
   *
   * `richText` is applied per bullet rather than the bullet being a raw string
   * so a translation can put a link or emphasis inside one item without this
   * file hardcoding where the words fall.
   */
  const bullets = (section: string, keys: string[]) =>
    keys.map((key) => richText(t(`legal.terms.sections.${section}.${key}`), {}));

  return (
    <LegalPage
      title={t("legal.terms.title")}
      description={t("legal.terms.description")}
      updated={t("legal.terms.updated")}
      englishHref="/terms"
    >
      <Clause heading={t("legal.terms.sections.agreement.heading")}>
        <p>
          {richText(t("legal.terms.sections.agreement.body"), {
            ci: <a href="https://tokens.ci">tokens.ci</a>,
          })}
        </p>
      </Clause>

      <Clause heading={t("legal.terms.sections.whoMayUse.heading")}>
        <p>{s("whoMayUse.p1")}</p>
        <p>{s("whoMayUse.p2")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.whatItIs.heading")}>
        <p>{s("whatItIs.p1")}</p>
        <p>{s("whatItIs.p2")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.publicData.heading")}>
        <p>{s("publicData.p1")}</p>
        <p>{s("publicData.p2")}</p>
        <p>{s("publicData.p3")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.honestNumbers.heading")}>
        <p>{s("honestNumbers.intro")}</p>
        <Bullets
          items={bullets("honestNumbers", [
            "item1",
            "item2",
            "item3",
            "item4",
            "item5",
          ])}
        />
        <p>{s("honestNumbers.outro")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.mustNot.heading")}>
        <Bullets items={bullets("mustNot", ["item1", "item2", "item3", "item4"])} />
        <p>
          {richText(t("legal.terms.sections.mustNot.outro"), {
            email: <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a>,
          })}
        </p>
      </Clause>

      <Clause heading={t("legal.terms.sections.enforcement.heading")}>
        <p>{s("enforcement.p1")}</p>
        <p>
          {richText(t("legal.terms.sections.enforcement.p2"), {
            shameLink: (
              <a href={localizeHref("/shame", locale)}>{t("nav.hallOfShame")}</a>
            ),
          })}
        </p>
        <p>
          {richText(t("legal.terms.sections.enforcement.p3"), {
            email: <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a>,
          })}
        </p>
      </Clause>

      <Clause heading={t("legal.terms.sections.accuracy.heading")}>
        <p>{s("accuracy.body")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.software.heading")}>
        <p>{s("software.body")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.availability.heading")}>
        <p>{s("availability.p1")}</p>
        <p>{s("availability.p2")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.noWarranty.heading")}>
        <p className="uppercase">{s("noWarranty.body")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.liability.heading")}>
        <p className="uppercase">{s("liability.p1")}</p>
        <p>{s("liability.p2")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.indemnity.heading")}>
        <p>{s("indemnity.body")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.governingLaw.heading")}>
        <p>{s("governingLaw.body")}</p>
      </Clause>

      <Clause heading={t("legal.terms.sections.everythingElse.heading")}>
        <p>
          {richText(t("legal.terms.sections.everythingElse.body"), {
            privacyLink: (
              <a href={localizeHref("/privacy", locale)}>{t("legal.privacy.title")}</a>
            ),
          })}
        </p>
      </Clause>

      <Clause heading={t("legal.terms.sections.contact.heading")}>
        <p>
          <a href={`mailto:${CONTACT_EMAIL}`}>{CONTACT_EMAIL}</a>
        </p>
      </Clause>
    </LegalPage>
  );
}
