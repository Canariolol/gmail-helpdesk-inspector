use chrono::{Datelike, NaiveDate};

use crate::analysis::AnalysisRun;

pub struct ReportEmail {
    pub subject: String,
    pub html: String,
}

pub struct ReviewItem {
    pub subject: String,
    pub from_email: String,
    pub reason: String,
}

const BG: &str = "#faf3ec";
const SURFACE: &str = "#ffffff";
const SURFACE_SOFT: &str = "#fdf9f4";
const BORDER: &str = "#f0e3d6";
const TEXT: &str = "#44403c";
const TEXT_STRONG: &str = "#292524";
const TEXT_MUTED: &str = "#8c8178";
const PRIMARY: &str = "#f97316";
const PRIMARY_STRONG: &str = "#ea580c";
const RED_BG: &str = "#fee2e2";
const RED_FG: &str = "#b91c1c";
const MINT_BG: &str = "#d8f3e3";
const MINT_FG: &str = "#15803d";

pub fn build_report_email(
    run: &AnalysisRun,
    review_items: &[ReviewItem],
    web_base_url: &str,
) -> ReportEmail {
    let window = window_label_es(&run.config.date_from, &run.config.date_to);
    let subject = if review_items.is_empty() {
        format!("Reporte Helpdesk · {window} · todo en orden")
    } else {
        format!(
            "Reporte Helpdesk · {window} · {} {} por revisar",
            review_items.len(),
            if review_items.len() == 1 {
                "hilo"
            } else {
                "hilos"
            }
        )
    };
    let body = format!(
        "{}{}{}{}",
        metrics_section(run),
        findings_section(run, review_items.len()),
        review_section(review_items),
        cta_section(web_base_url)
    );
    ReportEmail {
        subject,
        html: wrap_html(
            "Tu reporte del Helpdesk",
            &window,
            "¡Buenos días! Aquí va el resumen de tu casilla, preciosura. ☕",
            &body,
        ),
    }
}

pub fn build_failure_email(
    date_from: &str,
    date_to: &str,
    reason: &str,
    web_base_url: &str,
) -> ReportEmail {
    let window = window_label_es(date_from, date_to);
    let body = format!(
        r#"<tr><td style="padding:0 28px 8px 28px;">
<div style="background:{RED_BG};border:1px solid {BORDER};border-radius:12px;padding:16px 20px;color:{RED_FG};font-size:14px;line-height:1.6;">
El análisis programado no pudo completarse:<br/><strong>{}</strong>
</div></td></tr>{}"#,
        escape_html(reason),
        cta_section_with_label(web_base_url, "Iniciar sesión en la aplicación")
    );
    ReportEmail {
        subject: format!("Reporte Helpdesk · {window} · el análisis falló"),
        html: wrap_html(
            "Ups, hoy no pudimos",
            &window,
            "El análisis programado tuvo un tropiezo. Te contamos qué pasó:",
            &body,
        ),
    }
}

fn wrap_html(title: &str, window: &str, greeting: &str, body_rows: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="es"><head><meta charset="utf-8"/></head>
<body style="margin:0;padding:0;background:{BG};">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:{BG};padding:24px 0;">
<tr><td align="center">
<table role="presentation" width="600" cellpadding="0" cellspacing="0" style="max-width:600px;width:100%;background:{SURFACE};border:1px solid {BORDER};border-radius:16px;overflow:hidden;font-family:'Nunito',ui-sans-serif,system-ui,-apple-system,'Segoe UI',sans-serif;">
<tr><td style="background:{PRIMARY};background:linear-gradient(135deg,#fb923c,{PRIMARY});padding:28px;">
<div style="color:#ffffff;font-size:22px;font-weight:800;">{}</div>
<div style="color:#ffedd5;font-size:14px;margin-top:4px;">{}</div>
</td></tr>
<tr><td style="padding:24px 28px 8px 28px;color:{TEXT};font-size:15px;line-height:1.6;">{}</td></tr>
{body_rows}
<tr><td style="padding:20px 28px 28px 28px;border-top:1px solid {BORDER};color:{TEXT_MUTED};font-size:12px;line-height:1.6;">
Generado automáticamente por Gmail Helpdesk Inspector · análisis programado de lunes a viernes a las 08:00 (America/Santiago).
</td></tr>
</table>
</td></tr>
</table>
</body></html>"#,
        escape_html(title),
        escape_html(window),
        escape_html(greeting),
    )
}

fn metrics_section(run: &AnalysisRun) -> String {
    let metrics = &run.metrics;
    let unanswered_colors = if metrics.unanswered > 0 {
        (RED_BG, RED_FG)
    } else {
        (MINT_BG, MINT_FG)
    };
    let cards = [
        metric_card("Hilos analizados", &metrics.total_threads.to_string(), None),
        metric_card(
            "Solicitudes válidas",
            &metrics.valid_requests.to_string(),
            None,
        ),
        metric_card(
            "Respondidas",
            &metrics.answered.to_string(),
            Some((MINT_BG, MINT_FG)),
        ),
        metric_card(
            "Sin responder",
            &metrics.unanswered.to_string(),
            Some(unanswered_colors),
        ),
        metric_card(
            "Primera respuesta (prom.)",
            &minutes_or_dash(metrics.avg_first_response_minutes),
            None,
        ),
        metric_card(
            "Primera respuesta (p90)",
            &minutes_or_dash(metrics.p90_first_response_minutes),
            None,
        ),
        metric_card("Hilos ambiguos", &metrics.ambiguous.to_string(), None),
        metric_card("Descartados", &metrics.ignored.to_string(), None),
        metric_card(
            "Confianza del reporte",
            &format!("{:.0}%", metrics.report_confidence * 100.0),
            None,
        ),
    ];
    let mut rows = String::new();
    for pair in cards.chunks(2) {
        rows.push_str(&format!(
            r#"<tr><td style="padding:4px 28px;"><table role="presentation" width="100%" cellpadding="0" cellspacing="0"><tr>{}</tr></table></td></tr>"#,
            pair.join(r#"<td style="width:8px;"></td>"#)
        ));
    }
    rows.push_str(&breakdown_row(run));
    rows
}

/// Shows where the non-valid threads landed, so wrongly-suppressed client
/// requests (which all collapse into "Descartados") are at least visible.
fn breakdown_row(run: &AnalysisRun) -> String {
    let b = &run.metrics.classification_breakdown;
    format!(
        r#"<tr><td style="padding:6px 28px 4px 28px;"><table role="presentation" width="100%" cellpadding="0" cellspacing="0"><tr><td style="background:{SURFACE_SOFT};border:1px solid {BORDER};border-radius:12px;padding:12px 16px;">
<div style="color:{TEXT_STRONG};font-size:13px;font-weight:700;">Destino de los hilos no válidos</div>
<div style="color:{TEXT_MUTED};font-size:12px;line-height:1.8;margin-top:4px;">Interno: {} · Automático: {} · Newsletter: {} · Spam: {} · Misc: {} · Ambiguo (a revisión): {}</div>
</td></tr></table></td></tr>"#,
        b.internal, b.automated, b.newsletter, b.spam, b.misc, b.ambiguous
    )
}

fn metric_card(label: &str, value: &str, colors: Option<(&str, &str)>) -> String {
    let (bg, fg) = colors.unwrap_or((SURFACE_SOFT, TEXT_STRONG));
    format!(
        r#"<td width="50%" style="background:{bg};border:1px solid {BORDER};border-radius:12px;padding:14px 16px;">
<div style="color:{TEXT_MUTED};font-size:12px;">{}</div>
<div style="color:{fg};font-size:22px;font-weight:800;margin-top:2px;">{}</div>
</td>"#,
        escape_html(label),
        escape_html(value),
    )
}

fn findings_section(run: &AnalysisRun, review_count: usize) -> String {
    let metrics = &run.metrics;
    let mut findings = Vec::new();
    if metrics.unanswered > 0 {
        findings.push(format!(
            "<strong>{}</strong> {} válidas {} sin respuesta.",
            metrics.unanswered,
            plural(metrics.unanswered, "solicitud", "solicitudes"),
            plural(metrics.unanswered, "sigue", "siguen"),
        ));
    }
    if metrics.ambiguous > 0 {
        findings.push(format!(
            "<strong>{}</strong> {} con clasificación ambigua.",
            metrics.ambiguous,
            plural(metrics.ambiguous, "hilo quedó", "hilos quedaron"),
        ));
    }
    if review_count > 0 {
        findings.push(format!(
            "<strong>{review_count}</strong> {} tu revisión manual (detalle más abajo).",
            plural(review_count as u64, "hilo espera", "hilos esperan"),
        ));
    }
    if findings.is_empty() {
        return format!(
            r#"<tr><td style="padding:12px 28px 4px 28px;">
<div style="background:{MINT_BG};border:1px solid {BORDER};border-radius:12px;padding:14px 18px;color:{MINT_FG};font-size:14px;">
Sin pendientes: todas las solicitudes válidas fueron atendidas. ✨
</div></td></tr>"#
        );
    }
    let items = findings
        .into_iter()
        .map(|finding| format!(r#"<li style="margin:4px 0;">{finding}</li>"#))
        .collect::<String>();
    format!(
        r#"<tr><td style="padding:12px 28px 4px 28px;color:{TEXT};font-size:14px;">
<div style="font-weight:800;color:{TEXT_STRONG};font-size:15px;margin-bottom:4px;">Hallazgos</div>
<ul style="margin:0;padding-left:20px;line-height:1.7;">{items}</ul>
</td></tr>"#
    )
}

fn review_section(items: &[ReviewItem]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let rows = items
        .iter()
        .map(|item| {
            format!(
                r#"<tr>
<td style="padding:10px 12px;border-top:1px solid {BORDER};color:{TEXT_STRONG};font-size:13px;font-weight:700;">{}</td>
<td style="padding:10px 12px;border-top:1px solid {BORDER};color:{TEXT};font-size:13px;">{}</td>
<td style="padding:10px 12px;border-top:1px solid {BORDER};color:{TEXT_MUTED};font-size:12px;">{}</td>
</tr>"#,
                escape_html(&item.subject),
                escape_html(&item.from_email),
                escape_html(&item.reason),
            )
        })
        .collect::<String>();
    format!(
        r#"<tr><td style="padding:16px 28px 4px 28px;">
<div style="font-weight:800;color:{TEXT_STRONG};font-size:15px;margin-bottom:8px;">Requieren tu revisión ({})</div>
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:{SURFACE_SOFT};border:1px solid {BORDER};border-radius:12px;overflow:hidden;">
<tr>
<td style="padding:10px 12px;color:{TEXT_MUTED};font-size:11px;text-transform:uppercase;letter-spacing:0.04em;">Asunto</td>
<td style="padding:10px 12px;color:{TEXT_MUTED};font-size:11px;text-transform:uppercase;letter-spacing:0.04em;">Remitente</td>
<td style="padding:10px 12px;color:{TEXT_MUTED};font-size:11px;text-transform:uppercase;letter-spacing:0.04em;">Motivo</td>
</tr>
{rows}
</table>
</td></tr>"#,
        items.len(),
    )
}

fn cta_section(web_base_url: &str) -> String {
    cta_section_with_label(web_base_url, "Revisar en la aplicación")
}

fn cta_section_with_label(web_base_url: &str, label: &str) -> String {
    format!(
        r#"<tr><td align="center" style="padding:20px 28px 24px 28px;">
<a href="{}" style="display:inline-block;background:{PRIMARY_STRONG};color:#ffffff;text-decoration:none;font-weight:800;font-size:14px;padding:12px 28px;border-radius:999px;">{}</a>
</td></tr>"#,
        escape_html(web_base_url),
        escape_html(label),
    )
}

pub(crate) fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// "vie 12 jun – dom 14 jun" para rangos, "jue 11 jun" para un solo día.
pub(crate) fn window_label_es(date_from: &str, date_to: &str) -> String {
    match (parse_date(date_from), parse_date(date_to)) {
        (Some(from), Some(to)) if from == to => day_label_es(from),
        (Some(from), Some(to)) => format!("{} – {}", day_label_es(from), day_label_es(to)),
        _ => format!("{date_from} – {date_to}"),
    }
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn day_label_es(date: NaiveDate) -> String {
    const DAYS: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];
    const MONTHS: [&str; 12] = [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
    ];
    format!(
        "{} {} {}",
        DAYS[date.weekday().num_days_from_monday() as usize],
        date.day(),
        MONTHS[date.month0() as usize],
    )
}

pub(crate) fn format_minutes_es(minutes: f64) -> String {
    let total = minutes.round().max(0.0) as u64;
    let (hours, rest) = (total / 60, total % 60);
    match (hours, rest) {
        (0, _) => format!("{rest} min"),
        (_, 0) => format!("{hours} h"),
        _ => format!("{hours} h {rest} min"),
    }
}

fn minutes_or_dash(minutes: Option<f64>) -> String {
    minutes
        .map(format_minutes_es)
        .unwrap_or_else(|| "—".to_string())
}

fn plural<'a>(count: u64, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::analysis::{AnalysisConfig, AnalysisMetrics, AnalysisRun, AnalysisStatus};

    fn run_with_metrics(metrics: AnalysisMetrics) -> AnalysisRun {
        AnalysisRun {
            id: "run-1".to_string(),
            user_email: "a@x.cl".to_string(),
            config: AnalysisConfig {
                date_from: "2026-06-12".to_string(),
                date_to: "2026-06-14".to_string(),
                time_from: "00:00".to_string(),
                time_to: "23:59".to_string(),
                timezone: "America/Santiago".to_string(),
                internal_domains: vec![],
                ignored_senders: vec![],
                ignored_domains: vec![],
                ignored_keywords: vec![],
            },
            status: AnalysisStatus::Completed,
            progress_message: String::new(),
            processed_threads: 0,
            total_candidate_threads: 0,
            metrics,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            error_message: None,
        }
    }

    #[test]
    fn subject_includes_window_and_review_count() {
        let run = run_with_metrics(AnalysisMetrics::default());
        let items = vec![ReviewItem {
            subject: "Ayuda".to_string(),
            from_email: "cliente@y.cl".to_string(),
            reason: "Confianza baja".to_string(),
        }];
        let email = build_report_email(&run, &items, "https://app.x.cl");
        assert_eq!(
            email.subject,
            "Reporte Helpdesk · vie 12 jun – dom 14 jun · 1 hilo por revisar"
        );
    }

    #[test]
    fn empty_review_list_renders_all_clear_variant() {
        let run = run_with_metrics(AnalysisMetrics::default());
        let email = build_report_email(&run, &[], "https://app.x.cl");
        assert!(email.subject.ends_with("todo en orden"));
        assert!(email.html.contains("Sin pendientes"));
        assert!(!email.html.contains("Requieren tu revisión"));
    }

    #[test]
    fn untrusted_subject_is_html_escaped() {
        let run = run_with_metrics(AnalysisMetrics::default());
        let items = vec![ReviewItem {
            subject: "<script>alert('x')</script>".to_string(),
            from_email: "cliente@y.cl".to_string(),
            reason: "Revisión".to_string(),
        }];
        let email = build_report_email(&run, &items, "https://app.x.cl");
        assert!(!email.html.contains("<script>"));
        assert!(email.html.contains("&lt;script&gt;"));
    }

    #[test]
    fn metrics_appear_in_html() {
        let metrics = AnalysisMetrics {
            total_threads: 12,
            valid_requests: 7,
            answered: 5,
            unanswered: 2,
            avg_first_response_minutes: Some(83.0),
            ..AnalysisMetrics::default()
        };
        let email = build_report_email(&run_with_metrics(metrics), &[], "https://app.x.cl");
        assert!(email.html.contains(">12<"));
        assert!(email.html.contains("1 h 23 min"));
        assert!(email.html.contains("siguen sin respuesta"));
    }

    #[test]
    fn failure_email_mentions_reason_and_links_to_app() {
        let email = build_failure_email(
            "2026-06-11",
            "2026-06-11",
            "Google rechazó el refresh token",
            "https://app.x.cl",
        );
        assert!(email.subject.contains("el análisis falló"));
        assert!(email.html.contains("Google rechazó el refresh token"));
        assert!(email.html.contains("https://app.x.cl"));
    }

    #[test]
    fn window_labels_render_in_spanish() {
        assert_eq!(
            window_label_es("2026-06-12", "2026-06-14"),
            "vie 12 jun – dom 14 jun"
        );
        assert_eq!(window_label_es("2026-06-11", "2026-06-11"), "jue 11 jun");
        assert_eq!(window_label_es("malo", "2026-06-11"), "malo – 2026-06-11");
    }

    #[test]
    fn minutes_format_uses_hours_and_minutes() {
        assert_eq!(format_minutes_es(45.0), "45 min");
        assert_eq!(format_minutes_es(83.0), "1 h 23 min");
        assert_eq!(format_minutes_es(120.4), "2 h");
        assert_eq!(format_minutes_es(0.0), "0 min");
    }
}
