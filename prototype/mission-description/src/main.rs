//! PROTOTYPE, throw away: three ways to write a mission description, switchable with `?variant=A|B|C`.
//! Question: Trix or Markdown, and does the editor keep unsaved edits when a shard re-renders?

mod content;

use std::{collections::HashMap, sync::Mutex, time::Duration};

use serde::Deserialize;
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt, asset},
    context::{Cx, app_context},
    router::{
        RouterBuilderDiscoverExt, Slot,
        content::Form,
        error::{SeeOther, see_other},
        layout, module_router, page, query_params, route,
    },
    runtime::{Event, RouterBuilderRuntimeExt, Signal, connected, link, shard, signal},
    view::{Unescaped, View, component, emit, live, view},
};

use content::{SAMPLES, migrate_attachments, render_markdown, sanitize, trix_to_markdown};

#[tokio::main]
async fn main() {
    topcoat::start(
        module_router!()
            .assets(AssetBundle::load().unwrap())
            .app_context(Store::default())
            .discover()
            .runtime()
            .build(),
    )
    .await
    .unwrap();
}

#[derive(Default)]
struct Store {
    saved: Mutex<HashMap<(String, usize), String>>,
}

const VARIANTS: [(&str, &str); 3] = [
    ("A", "Trix WYSIWYG"),
    ("B", "Markdown, write/preview tabs"),
    ("C", "Markdown, toolbar and live split preview"),
];

#[query_params(error = bad_request)]
struct Query {
    variant: Option<String>,
    sample: Option<usize>,
    probe: Option<String>,
}

#[layout]
async fn layout(slot: Slot<'_>) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html>
            <head>
                <title>"PROTOTYPE: mission description"</title>
                topcoat::dev::script()
                topcoat::runtime::script()
                <link rel="stylesheet" href=(asset!("https://unpkg.com/trix@2.1.19/dist/trix.css"))>
                <script src=(asset!("https://unpkg.com/trix@2.1.19/dist/trix.umd.min.js"))></script>
                <link rel="stylesheet" href=(asset!("./proto.css"))>
                <script src=(asset!("./proto.js"))></script>
            </head>
            <body>(slot)</body>
        </html>
    })
}

#[page]
async fn page(cx: &Cx) -> Result<impl View> {
    let query = query_params::<Query>(cx)?;
    let variant = match query.variant.as_deref() {
        Some("B") => "B",
        Some("C") => "C",
        _ => "A",
    };
    let sample = query.sample.unwrap_or(0).min(SAMPLES.len() - 1);
    let inside = query.probe.as_deref() == Some("inside");
    let probe = if inside { "inside" } else { "outside" };

    let stored = stored_body(cx, variant, sample);
    let original = SAMPLES[sample].trix_html;
    let initial = initial_body(cx, variant, sample);
    let rendered = stored.as_deref().map(|body| match variant {
        "A" => sanitize(body),
        _ => render_markdown(body),
    });

    // Topcoat 0.10.0 hydration fails on a string signal containing `<` followed by a letter, so Trix gets an empty one.
    let text = signal(cx, {
        let initial = if variant == "A" { String::new() } else { initial.clone() };
        move || initial
    });
    let draft = signal(cx, || false);
    let rev = signal(cx, || 0usize);
    let url = |v: &str, s: usize, p: &str| format!("/?variant={v}&sample={s}&probe={p}");

    Ok(view! {
        <main>
            <p class="banner">
                "PROTOTYPE. Variant " (variant) ": "
                (VARIANTS.iter().find(|(key, _)| *key == variant).unwrap().1)
                ". State lives in memory and resets on restart."
            </p>

            <section class="probe">
                <b>"Sample: "</b>
                for (i, s) in SAMPLES.iter().enumerate() {
                    <a class=(if i == sample { "on" } else { "" }) href=(url(variant, i, probe))>(s.label)</a>
                    " "
                }
                <br>
                <b>"Editor placement: "</b>
                <a class=(if inside { "" } else { "on" }) href=(url(variant, sample, "outside"))>"outside any shard"</a>
                " "
                <a class=(if inside { "on" } else { "" }) href=(url(variant, sample, "inside"))>"inside a re-rendering form shard"</a>
                <br>
                <b>"Live region beside the form: "</b>
                ticker()
                <br>
                <b>"Runtime navigation: "</b>
                link(href: "/elsewhere", "open another page without a full reload")
            </section>

            <h1>"Edit mission"</h1>
            <form method="post" action="/save">
                <input type="hidden" name="variant" value=(variant)>
                <input type="hidden" name="sample" value=(sample)>
                <input type="hidden" name="probe" value=(probe)>

                if inside {
                    <p>
                        "Draft: " $(if draft.get() { "yes" } else { "no" }) " "
                        <button type="button" @click=$(|_e| draft.toggle())>"toggle draft (re-renders the form shard)"</button>
                        " "
                        <button type="button" @click=$(|_e| rev.increment())>"force form shard re-render"</button>
                    </p>
                }

                if inside {
                    form_shard(variant: variant.to_owned(), sample: sample, draft: $(draft.get()), rev: $(rev.get()), text: text)
                } else {
                    form_fields(variant: variant, initial: initial.clone(), draft: draft.get_untracked(), rev: 0, text: text)
                }

                <p><button type="submit">"Save"</button></p>
            </form>

            <section class="state">
                <h2>"Stored body"</h2>
                if let Some(body) = &stored {
                    <pre>(body)</pre>
                    <form method="post" action="/reset">
                        <input type="hidden" name="variant" value=(variant)>
                        <input type="hidden" name="sample" value=(sample)>
                        <input type="hidden" name="probe" value=(probe)>
                        <button type="submit">"Discard saved body"</button>
                    </form>
                } else {
                    <p>"Nothing saved yet. The editor starts from the migrated sample."</p>
                }

                <h2>"Rendered on the mission page"</h2>
                if let Some(html) = rendered {
                    <div class="rendered trix-content">(Unescaped::new_unchecked(html))</div>
                } else {
                    <p>"Save to see it."</p>
                }

                <h2>"Migration of this sample"</h2>
                <div class="split">
                    <div>
                        <h3>"Stored ActionText HTML (Rails)"</h3>
                        <pre>(original)</pre>
                    </div>
                    <div>
                        <h3>
                            if variant == "A" { "After attachment rewrite (Trix input)" } else { "Converted to Markdown by htmd" }
                        </h3>
                        <pre>(if variant == "A" { migrate_attachments(original) } else { trix_to_markdown(original) })</pre>
                    </div>
                </div>
            </section>
        </main>

        if cfg!(debug_assertions) {
            switcher(variant: variant, sample: sample, probe: probe)
        }
    })
}

#[component]
async fn switcher(variant: &str, sample: usize, probe: &str) -> Result<impl View> {
    let index = VARIANTS.iter().position(|(key, _)| *key == variant).unwrap();
    let prev = VARIANTS[(index + VARIANTS.len() - 1) % VARIANTS.len()].0;
    let next = VARIANTS[(index + 1) % VARIANTS.len()].0;
    Ok(view! {
        <nav class="switcher">
            <a id="variant-prev" href=(format!("/?variant={prev}&sample={sample}&probe={probe}"))>"◀"</a>
            <span>(variant) " (" (VARIANTS[index].1) ")"</span>
            <a id="variant-next" href=(format!("/?variant={next}&sample={sample}&probe={probe}"))>"▶"</a>
        </nav>
    })
}

fn stored_body(cx: &Cx, variant: &str, sample: usize) -> Option<String> {
    app_context::<Store>(cx).saved.lock().unwrap().get(&(variant.to_owned(), sample)).cloned()
}

fn initial_body(cx: &Cx, variant: &str, sample: usize) -> String {
    let original = SAMPLES[sample.min(SAMPLES.len() - 1)].trix_html;
    stored_body(cx, variant, sample).unwrap_or_else(|| match variant {
        "A" => migrate_attachments(original),
        _ => trix_to_markdown(original),
    })
}

// The shard loads the body itself: Topcoat 0.10.0 cannot hydrate a shard argument containing `<` followed by a letter.
#[shard]
async fn form_shard(cx: &Cx, variant: String, sample: usize, draft: bool, rev: usize, text: Signal<String>) -> Result<impl View> {
    let initial = initial_body(cx, &variant, sample);
    Ok(view! {
        form_fields(variant: &variant, initial: initial, draft: draft, rev: rev, text: text)
    })
}

#[component]
async fn form_fields(variant: &str, initial: String, draft: bool, rev: usize, text: Signal<String>) -> Result<impl View> {
    Ok(view! {
        <div class="fields">
            <p class="meta">"Form fields rendered on the server, re-render #" (rev) ", draft=" (draft)</p>
            <label>"Title " <input name="title" value="Operation Northern Watch"></label>
            if !draft {
                <label>"Public " <input type="checkbox" name="public"></label>
                <p class="meta">"This field exists only when the mission is not a draft. It sits above the description, so toggling draft shifts the elements before the editor."</p>
            }
            <label>"Description"</label>
            match variant {
                "A" => {
                    <input type="hidden" id="description" name="description" value=(initial)>
                    <trix-editor input="description" class="trix-content"></trix-editor>
                }
                "B" => {
                    tabbed_markdown(initial: initial, text: text)
                }
                _ => {
                    split_markdown(initial: initial, text: text)
                }
            }
        </div>
    })
}

#[component]
async fn tabbed_markdown(cx: &Cx, initial: String, text: Signal<String>) -> Result<impl View> {
    let preview = signal(cx, || false);
    Ok(view! {
        <div class="tabs">
            <button type="button" :class=$(if preview.get() { "" } else { "on" }) @click=$(|_e| preview.set(false))>"Write"</button>
            <button type="button" :class=$(if preview.get() { "on" } else { "" }) @click=$(|_e| preview.set(true))>"Preview"</button>
        </div>
        <textarea name="description" rows="16" :hidden=$(preview.get()) :value=$(text.get()) @input=$(|e: Event| text.set(e.target.value))>(initial)</textarea>
        <div :hidden=$(!preview.get())>
            markdown_preview(source: $(if preview.get() { text.get() } else { "".to_owned() }))
        </div>
        <p class="meta">"Markdown: **bold**, *italic*, # heading, - list, [link](url)"</p>
    })
}

#[component]
async fn split_markdown(initial: String, text: Signal<String>) -> Result<impl View> {
    Ok(view! {
        <div class="md-toolbar">
            <button type="button" data-md-wrap="**">"B"</button>
            <button type="button" data-md-wrap="*">"I"</button>
            <button type="button" data-md-wrap="~~">"S"</button>
            <button type="button" data-md-line="# ">"H"</button>
            <button type="button" data-md-line="- ">"• list"</button>
            <button type="button" data-md-line="1. ">"1. list"</button>
            <button type="button" data-md-line="> ">"quote"</button>
            <button type="button" data-md-link="">"link"</button>
        </div>
        <div class="split">
            <textarea name="description" rows="16" :value=$(text.get()) @input=$(|e: Event| text.set(e.target.value))>(initial)</textarea>
            markdown_preview(source: $(text.get()))
        </div>
    })
}

#[shard]
async fn markdown_preview(source: String) -> Result<impl View> {
    Ok(view! {
        <div class="rendered trix-content">(Unescaped::new_unchecked(render_markdown(&source)))</div>
    })
}

#[shard]
async fn ticker(cx: &Cx) -> Result<impl View> {
    Ok(live! {
        let mut count = 0u64;
        loop {
            let token = emit! { <span class="tick">"update #" (count)</span> }?;
            if !connected(cx) {
                break Ok(token);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            count += 1;
        }
    })
}

#[page("./elsewhere")]
async fn elsewhere() -> Result<impl View> {
    Ok(view! {
        <main>
            <p>"Another page, reached by runtime navigation."</p>
            link(href: "/", "back to the editor")
        </main>
    })
}

#[derive(Deserialize)]
struct SaveForm {
    variant: String,
    sample: usize,
    probe: String,
    description: Option<String>,
}

#[route(POST "/save")]
async fn save(cx: &Cx, Form(form): Form<SaveForm>) -> Result<SeeOther> {
    app_context::<Store>(cx)
        .saved
        .lock()
        .unwrap()
        .insert((form.variant.clone(), form.sample), form.description.unwrap_or_default());
    Ok(see_other(format!("/?variant={}&sample={}&probe={}", form.variant, form.sample, form.probe)))
}

#[route(POST "/reset")]
async fn reset(cx: &Cx, Form(form): Form<SaveForm>) -> Result<SeeOther> {
    app_context::<Store>(cx).saved.lock().unwrap().remove(&(form.variant.clone(), form.sample));
    Ok(see_other(format!("/?variant={}&sample={}&probe={}", form.variant, form.sample, form.probe)))
}

#[route(GET "/files/briefing-map.svg")]
async fn briefing_map() -> Result<impl topcoat::router::response::IntoResponse> {
    Ok(([(topcoat::router::header::CONTENT_TYPE, "image/svg+xml")], content::BRIEFING_MAP_SVG))
}
