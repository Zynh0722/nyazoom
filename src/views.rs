use futures::TryFutureExt;
use leptos::{Children, IntoView};
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
#[leptos::component]
pub fn Welcome(cx: leptos::Scope, fact: String) -> impl IntoView {
    leptos::view! { cx,
        <HtmxPage>
            <h1>NyaZoom<sup>2</sup></h1>
            <div class="form-wrapper">
                <form action="/upload" method="post" enctype="multipart/form-data" class="main-form">
                    <div class="cat-img-wrapper">
                        <img class="cat-img" src="https://api.thecatapi.com/v1/images/search?size=small&format=src" />
                    </div>

                    <input type="file" id="file" name="file" data-multiple-caption="{count} files selected" multiple />
                    <label for="file">Select Files</label>

                    <input type="submit" value="Get Link~" />
                    <p id="cat-fact">{fact}</p>
                </form>
            </div>
        </HtmxPage>
    }
}

// <link href="../dist/css/link.css" rel="stylesheet" />
// #TODO: Handle pushing cleaner
#[leptos::component]
pub fn DownloadLink(cx: leptos::Scope, id: String, record: UploadRecord) -> impl IntoView {
    let downloads_remaining = record.max_downloads - record.downloads;
    let plural = if downloads_remaining > 1 { "s" } else { "" };
    leptos::view! { cx,
        <HtmxPage>
            <div class="link-wrapper">
                <a id="link" href=format!("/download/{id}")>Download Now!</a>
            </div>

            <div class="link-wrapper">
                You have {record.max_downloads - record.downloads} download{plural} remaining!
            </div>
            <button class="return-button" onclick="clipboard()">Copy to Clipboard</button>


            <a href="/" class="return-button">Return to home</a>
        </HtmxPage>
    }
}

#[leptos::component]
pub fn HtmxPage(cx: leptos::Scope, children: Children) -> impl IntoView {
    leptos::view! { cx,
        <head>
            <title>Nyazoom</title>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1" />
            <link href="/css/main.css" rel="stylesheet" />
            <link href="/css/link.css" rel="stylesheet" />
            <script src="/scripts/file_label.js" />
            <script src="/scripts/link.js" />
        </head>

        <body>
            {children(cx)}
        </body>
    }
}
