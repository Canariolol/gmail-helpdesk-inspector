//! Detección del proveedor de correo de un dominio a partir de su DNS.
//!
//! El sufijo del dominio miente: `west-ingenieria.cl` es Google Workspace y
//! `ninfasolutions.com` es Hostinger. Por eso se consulta el DNS real.
//!
//! Se resuelve por DNS-over-HTTPS con el cliente HTTP que ya existe en vez de
//! sumar un resolver como dependencia. El host de DoH es fijo y el dominio del
//! usuario viaja como parámetro de consulta, así que no hay superficie de SSRF.
//!
//! **Desenmascarado:** un dominio detrás de un gateway de seguridad (Proofpoint,
//! Mimecast…) tiene MX del gateway, no del proveedor real. En ese caso se mira el
//! SPF, que sí declara la infraestructura de envío real (`_spf.google.com`,
//! `spf.protection.outlook.com`) porque el gateway solo intercepta la entrada.

use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};

const DOH_ENDPOINT: &str = "https://dns.google/resolve";
const DOH_TIMEOUT: Duration = Duration::from_secs(5);
const DNS_TYPE_MX: u16 = 15;
const DNS_TYPE_TXT: u16 = 16;

const GOOGLE_MX_SUFFIXES: [&str; 2] = ["google.com", "googlemail.com"];
const MICROSOFT_MX_SUFFIXES: [&str; 3] = ["outlook.com", "office365.com", "microsoft.com"];

/// Gateways de seguridad que se ponen delante del proveedor real y ocultan el MX.
/// Si aparece uno nuevo, se agrega aquí: la tabla de espera guarda los MX
/// observados justamente para detectarlos.
const GATEWAY_MX_SUFFIXES: [&str; 10] = [
    "pphosted.com",   // Proofpoint
    "ppe-hosted.com", // Proofpoint Essentials
    "mimecast.com",   // Mimecast
    "mimecast.co.za", // Mimecast (ZA)
    "barracudanetworks.com",
    "iphmx.com",       // Cisco Secure Email
    "messagelabs.com", // Broadcom/Symantec
    "mailcontrol.com", // Forcepoint
    "hornetsecurity.com",
    "retarus.com",
];

/// Marcadores de SPF que delatan al proveedor real detrás de un gateway.
const GOOGLE_SPF_MARKER: &str = "_spf.google.com";
const MICROSOFT_SPF_MARKER: &str = "spf.protection.outlook.com";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedProvider {
    Google,
    Microsoft,
    /// El correo del dominio no está en Google ni Microsoft (Hostinger, cPanel,
    /// Zoho…). Todavía no hay adaptador para ese caso.
    Unsupported,
    /// No se pudo determinar: sin MX, gateway que no se pudo desenmascarar, o
    /// error de DNS.
    Unknown,
}

/// Qué dice el MX del dominio. Separa "es un gateway" de "es otro proveedor"
/// porque solo el primero justifica mirar el SPF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MxVerdict {
    Google,
    Microsoft,
    /// Gateway de seguridad: oculta al proveedor real.
    Gateway,
    /// Proveedor de correo propio distinto de Google/Microsoft. Es la respuesta final.
    Other,
    /// Sin registros MX utilizables.
    None,
}

/// Resultado completo de la detección. Los MX se conservan para poder reconocer
/// gateways todavía desconocidos a partir de los casos reales.
#[derive(Debug, Clone)]
pub struct Detection {
    pub provider: DetectedProvider,
    pub mx_hosts: Vec<String>,
}

/// Correo de alguien cuyo proveedor aún no soportamos, para avisarle cuando exista.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderWaitlistEntry {
    /// Correo normalizado: una fila por persona a la que avisar.
    pub id: String,
    pub email: String,
    pub domain: String,
    pub detected: DetectedProvider,
    /// MX observados. Alimenta la revisión de gateways que aún no clasificamos.
    #[serde(default)]
    pub mx_hosts: Vec<String>,
    /// Cuenta de Mira desde la que se hizo la consulta.
    pub requested_by: String,
    pub created_at: DateTime<Utc>,
}

/// Dominio de un correo, normalizado. `None` si no parece un correo.
pub fn domain_of(email: &str) -> Option<String> {
    let trimmed = email.trim();
    let (local, domain) = trimmed.rsplit_once('@')?;
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return None;
    }
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    // Evita que un valor con espacios o barras se cuele a la consulta DNS.
    if domain.is_empty()
        || !domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return None;
    }
    Some(domain)
}

fn matches_suffix(host: &str, suffix: &str) -> bool {
    host == suffix || host.ends_with(&format!(".{suffix}"))
}

fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// Clasifica los hosts MX de un dominio.
pub fn classify_mx_hosts<I, S>(hosts: I) -> MxVerdict
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut verdict = MxVerdict::None;
    for host in hosts {
        let host = normalize_host(host.as_ref());
        if host.is_empty() {
            continue;
        }
        if GOOGLE_MX_SUFFIXES
            .iter()
            .any(|suffix| matches_suffix(&host, suffix))
        {
            return MxVerdict::Google;
        }
        if MICROSOFT_MX_SUFFIXES
            .iter()
            .any(|suffix| matches_suffix(&host, suffix))
        {
            return MxVerdict::Microsoft;
        }
        if GATEWAY_MX_SUFFIXES
            .iter()
            .any(|suffix| matches_suffix(&host, suffix))
        {
            // Un gateway no es respuesta final, pero tampoco descarta que haya un
            // MX directo del proveedor más adelante en la lista.
            verdict = MxVerdict::Gateway;
            continue;
        }
        if verdict != MxVerdict::Gateway {
            verdict = MxVerdict::Other;
        }
    }
    verdict
}

/// Busca en el SPF del dominio la infraestructura de envío real. Es lo que
/// desenmascara a un dominio que entra por un gateway de seguridad.
pub fn classify_spf(txt_records: &str) -> Option<DetectedProvider> {
    let spf = txt_records.to_ascii_lowercase();
    if !spf.contains("v=spf1") {
        return None;
    }
    if spf.contains(GOOGLE_SPF_MARKER) {
        return Some(DetectedProvider::Google);
    }
    if spf.contains(MICROSOFT_SPF_MARKER) {
        return Some(DetectedProvider::Microsoft);
    }
    None
}

#[derive(Debug, Deserialize)]
struct DohResponse {
    #[serde(rename = "Status", default)]
    status: i32,
    #[serde(rename = "Answer", default)]
    answer: Vec<DohAnswer>,
}

#[derive(Debug, Deserialize)]
struct DohAnswer {
    #[serde(rename = "type", default)]
    kind: u16,
    #[serde(default)]
    data: String,
}

/// Extrae el host de un `data` de MX con formato `"10 aspmx.l.google.com."`.
fn mx_host(data: &str) -> Option<&str> {
    data.split_whitespace().nth(1).or_else(|| {
        let single = data.trim();
        (!single.is_empty()).then_some(single)
    })
}

async fn dns_query(http: &Client, domain: &str, kind: &str) -> anyhow::Result<DohResponse> {
    http.get(DOH_ENDPOINT)
        .query(&[("name", domain), ("type", kind)])
        .header("accept", "application/dns-json")
        .timeout(DOH_TIMEOUT)
        .send()
        .await
        .context("failed to query DNS-over-HTTPS")?
        .error_for_status()
        .context("DNS-over-HTTPS returned an error status")?
        .json::<DohResponse>()
        .await
        .context("failed to decode the DNS-over-HTTPS response")
}

/// Detecta el proveedor de un dominio: MX primero y, si hay un gateway delante,
/// SPF para desenmascarar al proveedor real.
pub async fn detect(http: &Client, domain: &str) -> anyhow::Result<Detection> {
    let mx = dns_query(http, domain, "MX").await?;
    let mx_hosts: Vec<String> = if mx.status == 0 {
        mx.answer
            .iter()
            .filter(|answer| answer.kind == DNS_TYPE_MX)
            .filter_map(|answer| mx_host(&answer.data))
            .map(normalize_host)
            .collect()
    } else {
        Vec::new()
    };

    let provider = match classify_mx_hosts(&mx_hosts) {
        MxVerdict::Google => DetectedProvider::Google,
        MxVerdict::Microsoft => DetectedProvider::Microsoft,
        MxVerdict::Other => DetectedProvider::Unsupported,
        MxVerdict::None => DetectedProvider::Unknown,
        // Detrás de un gateway el MX no dice nada del proveedor real: si el SPF
        // tampoco lo delata, es `Unknown` (no sabemos), nunca `Unsupported`.
        MxVerdict::Gateway => {
            let txt = dns_query(http, domain, "TXT").await?;
            let records = txt
                .answer
                .iter()
                .filter(|answer| answer.kind == DNS_TYPE_TXT)
                .map(|answer| answer.data.replace('"', ""))
                .collect::<Vec<_>>()
                .join(" ");
            classify_spf(&records).unwrap_or(DetectedProvider::Unknown)
        }
    };

    Ok(Detection { provider, mx_hosts })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_domain_and_rejects_junk() {
        assert_eq!(
            domain_of(" Ana@West-Ingenieria.CL "),
            Some("west-ingenieria.cl".to_string())
        );
        assert_eq!(domain_of("no-arroba.cl"), None);
        assert_eq!(domain_of("ana@"), None);
        assert_eq!(domain_of("@dominio.cl"), None);
        assert_eq!(domain_of("ana@sinpunto"), None);
        // Un valor con espacios o barras no debe llegar a la consulta DNS.
        assert_eq!(domain_of("ana@evil.com/path"), None);
        assert_eq!(domain_of("ana@evil com"), None);
    }

    #[test]
    fn classifies_google_and_microsoft_by_mx() {
        assert_eq!(
            classify_mx_hosts(["aspmx.l.google.com.", "alt1.aspmx.l.google.com."]),
            MxVerdict::Google
        );
        assert_eq!(
            classify_mx_hosts(["contoso-cl.mail.protection.outlook.com."]),
            MxVerdict::Microsoft
        );
    }

    #[test]
    fn classifies_own_providers_as_other() {
        assert_eq!(classify_mx_hosts(["mx1.hostinger.com."]), MxVerdict::Other);
        assert_eq!(classify_mx_hosts(["mx.zoho.com."]), MxVerdict::Other);
    }

    #[test]
    fn flags_security_gateways_instead_of_calling_them_unsupported() {
        assert_eq!(
            classify_mx_hosts(["mx0a-001b2d01.pphosted.com."]),
            MxVerdict::Gateway
        );
        assert_eq!(
            classify_mx_hosts(["cl-smtp-inbound1.mimecast.com."]),
            MxVerdict::Gateway
        );
    }

    /// Un MX directo del proveedor gana sobre el gateway aunque venga después.
    #[test]
    fn a_direct_provider_mx_wins_over_a_gateway() {
        assert_eq!(
            classify_mx_hosts([
                "mx0a-001b2d01.pphosted.com.",
                "contoso.mail.protection.outlook.com.",
            ]),
            MxVerdict::Microsoft
        );
    }

    #[test]
    fn domain_without_mx_is_none() {
        assert_eq!(classify_mx_hosts(Vec::<String>::new()), MxVerdict::None);
    }

    /// Un sufijo no debe coincidir por substring: `notgoogle.com` no es Google.
    #[test]
    fn suffix_match_respects_label_boundaries() {
        assert_eq!(classify_mx_hosts(["mx.notgoogle.com."]), MxVerdict::Other);
        assert_eq!(
            classify_mx_hosts(["mail.fakeoutlook.com."]),
            MxVerdict::Other
        );
    }

    #[test]
    fn spf_unmasks_the_real_provider_behind_a_gateway() {
        assert_eq!(
            classify_spf("v=spf1 include:_spf.google.com include:_spf.pphosted.com ~all"),
            Some(DetectedProvider::Google)
        );
        assert_eq!(
            classify_spf("v=spf1 include:spf.protection.outlook.com -all"),
            Some(DetectedProvider::Microsoft)
        );
    }

    #[test]
    fn spf_without_a_known_provider_stays_undecided() {
        assert_eq!(
            classify_spf("v=spf1 include:_spf.mail.hostinger.com ~all"),
            None
        );
        // Un TXT que no es SPF (verificación de dominio) no debe decidir nada.
        assert_eq!(classify_spf("google-site-verification=abc123"), None);
    }

    #[test]
    fn parses_mx_data_with_and_without_priority() {
        assert_eq!(
            mx_host("10 aspmx.l.google.com."),
            Some("aspmx.l.google.com.")
        );
        assert_eq!(mx_host("aspmx.l.google.com."), Some("aspmx.l.google.com."));
        assert_eq!(mx_host("   "), None);
    }
}
