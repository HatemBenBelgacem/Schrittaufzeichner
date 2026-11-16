//! Erstellt aus den Schritten einen HTML-Bericht.
//! Die Bilder werden direkt in die Datei eingebettet, damit man nur eine Datei weitergeben muss.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine as _;

use crate::recorder::Step;

pub fn write_report(dir: &Path, title: &str, steps: &[Step]) -> Result<PathBuf> {
    let mut html = String::new();
    let created = chrono::Local::now().format("%d.%m.%Y %H:%M:%S");

    let _ = write!(
        html,
        r#"<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  body {{ font-family: Segoe UI, system-ui, sans-serif; margin: 0; background: #f4f5f7; color: #1d1f23; }}
  header {{ background: #1f3a5f; color: #fff; padding: 24px 32px; }}
  header h1 {{ margin: 0 0 4px; font-size: 24px; }}
  header p {{ margin: 0; opacity: .8; }}
  main {{ max-width: 1100px; margin: 0 auto; padding: 24px 16px; }}
  nav ol {{ background: #fff; border-radius: 8px; padding: 16px 16px 16px 40px; }}
  nav a {{ color: #1f3a5f; }}
  section {{ background: #fff; border-radius: 8px; margin: 24px 0; padding: 16px 20px;
             box-shadow: 0 1px 3px rgba(0,0,0,.08); page-break-inside: avoid; }}
  section h2 {{ margin: 0 0 6px; font-size: 18px; }}
  .meta {{ color: #5b616b; font-size: 14px; margin-bottom: 10px; }}
  .comment {{ background: #fff8d6; border-left: 4px solid #e0b400; padding: 8px 12px; margin: 8px 0 12px;
              white-space: pre-wrap; }}
  img {{ width: 100%; height: auto; border: 1px solid #d6d9de; border-radius: 4px; }}
  @media print {{ body {{ background: #fff; }} nav {{ display: none; }} section {{ box-shadow: none; }} }}
</style>
</head>
<body>
<header>
  <h1>{title}</h1>
  <p>Erstellt am {created} &middot; {count} Schritte</p>
</header>
<main>
<nav><ol>
"#,
        title = esc(title),
        count = steps.len(),
    );

    for s in steps {
        let _ = writeln!(
            html,
            r##"<li><a href="#schritt-{nr}">{text}</a></li>"##,
            nr = s.nr,
            text = esc(&step_sentence(s)),
        );
    }
    html.push_str("</ol></nav>\n");

    for s in steps {
        let bytes = std::fs::read(&s.image_path)
            .with_context(|| format!("Bild lesen: {}", s.image_path.display()))?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);

        let _ = write!(
            html,
            r#"<section id="schritt-{nr}">
  <h2>Schritt {nr}: {sentence}</h2>
  <div class="meta">{time} &middot; Position {x}, {y}{app}</div>
"#,
            nr = s.nr,
            sentence = esc(&step_sentence(s)),
            time = s.time.format("%H:%M:%S"),
            x = s.x,
            y = s.y,
            app = if s.app_name.is_empty() {
                String::new()
            } else {
                format!(" &middot; Programm: {}", esc(&s.app_name))
            },
        );
        if !s.comment.trim().is_empty() {
            let _ = writeln!(html, r#"  <div class="comment">{}</div>"#, esc(s.comment.trim()));
        }
        let _ = writeln!(
            html,
            r#"  <img src="data:image/jpeg;base64,{b64}" alt="Screenshot Schritt {nr}">
</section>"#,
            nr = s.nr,
        );
    }

    html.push_str("</main>\n</body>\n</html>\n");

    let path = dir.join("Bericht.html");
    std::fs::write(&path, html).with_context(|| format!("{}", path.display()))?;
    Ok(path)
}

/// Kurzer Satz, z. B. «Linksklick in „Rechner“».
pub fn step_sentence(s: &Step) -> String {
    let target = if !s.window_title.trim().is_empty() {
        s.window_title.trim()
    } else {
        s.app_name.trim()
    };
    if target.is_empty() {
        s.button.label().to_string()
    } else {
        format!("{} in «{}»", s.button.label(), target)
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
