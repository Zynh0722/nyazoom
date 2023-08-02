use futures::TryFutureExt;
use leptos::{component, view, Children, IntoView, Scope};
use serde::Deserialize;

use crate::state::UploadRecord;

#[derive(Debug, Deserialize)]
pub struct CatFact {
    pub fact: String,
}

pub async fn get_cat_fact() -> String {
    reqwest::get("https://catfact.ninja/fact")
        .and_then(|res| res.json())
        .map_ok(|cf: CatFact| cf.fact)
        .await
        .unwrap_or_else(|_| String::from("The cat fact goddess has failed me :<"))
}

// {https://api.thecatapi.com/v1/images/search?size=small&format=src}
// {https://cataas.com/cat?width=250&height=250}
#[component]
pub fn Welcome(cx: Scope, fact: String) -> impl IntoView {
    view! { cx,
        <HtmxPage>
            <div class="form-wrapper">
                <WelcomeView fact />
            </div>
        </HtmxPage>
    }
}

#[component]
pub fn WelcomeView(cx: Scope, fact: String) -> impl IntoView {
    view! {
        cx,
        <form id="form" hx-swap="outerHTML" hx-post="/upload" hx-encoding="multipart/form-data" class="column-container">
            <div class="cat-img-wrapper">
                <img class="cat-img" src="https://api.thecatapi.com/v1/images/search?size=small&format=src" />
            </div>
            <input type="file" id="file" name="file" data-multiple-caption="{{count}} files selected" multiple />
            <label for="file">Select Files</label>

            <input type="submit" value="Get Link~" />
            <p id="cat-fact">{fact}</p>
            <progress id="progress" class="htmx-indicator" value="0" max="100"></progress>
        </form>
        <script src="/scripts/loading_progress.js" />
    }
}

// <link href="../dist/css/link.css" rel="stylesheet" />
// #TODO: Handle pushing cleaner
#[component]
pub fn DownloadLinkPage(cx: Scope, id: String, record: UploadRecord) -> impl IntoView {
    view! { cx,
        <HtmxPage>
            <div class="form-wrapper">
                <LinkView id record />
            </div>
        </HtmxPage>
    }
}

#[component]
pub fn HtmxPage(cx: Scope, children: Children) -> impl IntoView {
    view! { cx,
        <head>
            <title>Nyazoom</title>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1" />
            <link href="/css/main.css" rel="stylesheet" />
            <link href="/css/link.css" rel="stylesheet" />
            <script src="/scripts/file_label.js" />
            <script src="/scripts/link.js" />
            <script src="https://unpkg.com/htmx.org@1.9.4" integrity="sha384-zUfuhFKKZCbHTY6aRR46gxiqszMk5tcHjsVFxnUo8VMus4kHGVdIYVbOYYNlKmHV" crossorigin="anonymous"></script>
        </head>

        <body>
            <h1>NyaZoom<sup>2</sup></h1>
            {children(cx)}
        </body>
    }
}

#[component]
pub fn LinkView(cx: Scope, id: String, record: UploadRecord) -> impl IntoView {
    let downloads_remaining = record.max_downloads - record.downloads;
    let plural = if downloads_remaining > 1 { "s" } else { "" };
    view! {
        cx,
        <div class="column-container">
            <div class="link-wrapper">
                <a id="link" href="/download/{id}">Download Now!</a>
            </div>

            <div class="link-wrapper" hx-get="/link/{id}/remaining" hx-trigger="click from:#link delay:0.2s, every 10s" >
                You have {record.downloads_remaining()} download{plural} remaining!
            </div>
            <button class="return-button" onclick="clipboard()">Copy to Clipboard</button>


            <a href="/" class="return-button">Return to home</a>
        </div>
    }
}
