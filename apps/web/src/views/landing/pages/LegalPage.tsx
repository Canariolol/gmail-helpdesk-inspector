import { AlertTriangle } from "lucide-react";
import { PublicPage } from "../PublicPage";
import type { LegalDoc } from "./legalContent";

type Props = {
  doc: LegalDoc;
  onLogin: () => void;
  onSignup: () => void;
};

export function LegalPage({ doc, onLogin, onSignup }: Props) {
  return (
    <PublicPage onLogin={onLogin} onSignup={onSignup}>
      <article className="lp-legal">
        <header className="lp-legal-head">
          <span className="lp-focus-eyebrow">Legal</span>
          <h1>{doc.title}</h1>
          <p className="lp-legal-updated">Última actualización: {doc.updated}</p>
          <p className="lp-legal-draft">
            <AlertTriangle size={16} />
            Documento en borrador: el texto es de ejemplo y está pendiente de revisión legal.
          </p>
          <p className="lp-legal-intro">{doc.intro}</p>
        </header>

        <div className="lp-legal-body">
          {doc.sections.map((section, index) => (
            <section key={section.heading}>
              <h2>
                {index + 1}. {section.heading}
              </h2>
              <p>{section.body}</p>
            </section>
          ))}
        </div>
      </article>
    </PublicPage>
  );
}
