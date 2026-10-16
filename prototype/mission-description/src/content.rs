//! PROTOTYPE: sample mission descriptions in the stored ActionText format, and their conversions.

pub struct Sample {
    pub label: &'static str,
    pub trix_html: &'static str,
}

pub const SAMPLES: [Sample; 3] = [
    Sample {
        label: "Short",
        trix_html: "<div>Operation Northern Watch. Standard loadout, <strong>no thermals</strong>.<br>Briefing at 19:30, step-off at 20:00.</div>",
    },
    Sample {
        label: "Structured briefing",
        trix_html: concat!(
            "<h1>Situation</h1>",
            "<div>Insurgents hold the villages of <em>Zargabad</em> and <em>Shamali</em>. ",
            "Civilians are <strong>still present</strong> in both.<br><br></div>",
            "<h1>Mission</h1>",
            "<ol><li>Clear Shamali.</li><li>Secure the bridge at grid 045 112.</li>",
            "<li>Hold until relieved.</li></ol>",
            "<h1>Signals</h1>",
            "<pre>PLT  41.0\n1-1  42.1\n1-2  42.2\nAIR  51.0</pre>",
            "<blockquote>Do not engage until fired upon. ROE changes on the word VIPER.</blockquote>",
            "<div><del>Armour support</del> cancelled, see the ",
            "<a href=\"https://steamcommunity.com/sharedfiles/filedetails/?id=843425103\">ACE wiki</a> for medevac.</div>",
            "<ul><li>NVGs issued</li><li>Two FAKs each</li></ul>",
        ),
    },
    Sample {
        label: "With image embed",
        trix_html: concat!(
            "<div>Map of the AO:</div>",
            "<action-text-attachment sgid=\"BAh7CEkiCGdpZAY6BkVUSSIrZ2lkOi8vaWJzaXRlL0FjdGl2ZVN0b3JhZ2U6OkJsb2IvNDI_ZXhwaXJlc19pbgY7AFRJIgxwdXJwb3NlBjsAVEkiD2F0dGFjaGFibGUGOwBUSSIPZXhwaXJlc19hdAY7AFQw--0123abcd\" ",
            "content-type=\"image/svg+xml\" url=\"https://ibsite.example/rails/active_storage/blobs/redirect/eyJfcmFpbHMi/briefing-map.svg\" ",
            "filename=\"briefing-map.svg\" filesize=\"48213\" width=\"800\" height=\"450\" previewable=\"true\" presentation=\"gallery\"></action-text-attachment>",
            "<div>Insertion by helo at <strong>LZ Hawk</strong>.</div>",
        ),
    },
];

/// Rewrites each `<action-text-attachment>` to an `<img>` that points at the migrated file.
pub fn migrate_attachments(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find("<action-text-attachment") {
        out.push_str(&rest[..start]);
        let close = "</action-text-attachment>";
        let end = rest[start..].find(close).map_or(rest.len(), |i| start + i + close.len());
        let tag = &rest[start..end];
        let filename = attribute(tag, "filename").unwrap_or("attachment");
        out.push_str(&format!("<img src=\"/files/{filename}\" alt=\"{filename}\">"));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let len = tag[start..].find('"')?;
    Some(&tag[start..start + len])
}

pub fn trix_to_markdown(html: &str) -> String {
    // htmd drops `<del>` and turns a `<pre>` without `<code>` into plain paragraphs.
    let html = migrate_attachments(html)
        .replace("<del>", "~~")
        .replace("</del>", "~~")
        .replace("<pre>", "<pre><code>")
        .replace("</pre>", "</code></pre>");
    htmd::convert(&html).unwrap_or_default()
}

pub fn sanitize(html: &str) -> String {
    ammonia::clean(html)
}

pub fn render_markdown(source: &str) -> String {
    let options = pulldown_cmark::Options::ENABLE_STRIKETHROUGH | pulldown_cmark::Options::ENABLE_TABLES;
    let parser = pulldown_cmark::Parser::new_ext(source, options);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    sanitize(&html)
}

pub const BRIEFING_MAP_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="225" viewBox="0 0 400 225"><rect width="400" height="225" fill="#c9d6a3"/><path d="M0 160 C100 120 200 200 400 140" stroke="#5b8bd6" stroke-width="14" fill="none"/><circle cx="120" cy="80" r="10" fill="#b33"/><text x="136" y="86" font-family="sans-serif" font-size="16">Shamali</text><circle cx="290" cy="60" r="10" fill="#b33"/><text x="306" y="66" font-family="sans-serif" font-size="16">LZ Hawk</text></svg>"##;
