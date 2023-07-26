use futures::TryFutureExt;
use leptos::IntoView;
use serde::Deserialize;

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

#[leptos::component]
pub fn Welcome(cx: leptos::Scope, fact: String) -> impl IntoView {
    leptos::view! { cx,
        <head>
            <title>NyaZoom</title>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1" />
            <link href="dist/css/main.css" rel="stylesheet" />
            <script src="dist/scripts/file_label.js" />
        </head>

        <body>
            <h1>NyaZoom<sup>2</sup></h1>
            <div class="form-wrapper">
                <form action="/upload" method="post" enctype="multipart/form-data" class="main-form">
                    <div class="cat-img-wrapper">
                        <img class="cat-img" src="https://cataas.com/cat?width=250&height=250" />
                    </div>

                    <input type="file" id="file" name="file" data-multiple-caption="{{count}} files selected" multiple />
                    <label for="file">Select Files</label>

                    <input type="submit" value="Get Link~" />
                    <p id="cat-fact">{fact}</p>
                </form>
            </div>
        </body>
    }
}
