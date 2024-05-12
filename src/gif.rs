use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Response {
    next: String,
    results: Vec<ResponseObject>,
}

#[derive(Deserialize)]
struct ResponseObject {
    created: f32,
    hasaudio: bool,
    id: String,
    media_formats: HashMap<ContentFormat, MediaObject>,
}

#[derive(Deserialize)]
struct MediaObject {
    url: String,
    dims: [i32; 2], // width, height
    duration: f32,
    size: i32,
}

#[derive(PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
// TODO(smolck): No idea if this is right lol
enum ContentFormat {
    // Preview,
    Gif,
    #[serde(rename = "gifpreview")]
    GifPreview,

    MediumGif,
    #[serde(rename = "mediumgifpreview")]
    MediumGifPreview,

    TinyGif,
    #[serde(rename = "tinygifpreview")]
    TinyGifPreview,

    NanoGif,
    #[serde(rename = "nanogifpreview")]
    NanoGifPreview,

    Mp4,
    #[serde(rename = "mp4preview")]
    Mp4Preview,

    LoopedMp4,
    #[serde(rename = "loopedmp4preview")]
    LoopedMp4Preview,

    TinyMp4,
    #[serde(rename = "tinymp4preview")]
    TinyMp4Preview,

    NanoMp4,
    #[serde(rename = "nanomp4preview")]
    NanoMp4Preview,

    Webm,
    #[serde(rename = "webmpreview")]
    WebmPreview,

    TinyWebm,
    #[serde(rename = "tinywebmpreview")]
    TinyWebmPreview,

    NanoWebm,
    #[serde(rename = "nanowebmpreview")]
    NanoWebmPreview,

    #[serde(rename = "webp_transparent")]
    WebpTransparent,
    #[serde(rename = "webppreview_transparent")]
    WebpPreviewTransparent,

    #[serde(rename = "tinywebp_transparent")]
    TinyWebpTransparent,
    #[serde(rename = "tinywebppreview_transparent")]
    TinyWebpPreviewTransparent,

    #[serde(rename = "nanowebp_transparent")]
    NanoWebpTransparent,
    #[serde(rename = "nanowebppreview_transparent")]
    NanoWebpPreviewTransparent,

    #[serde(rename = "gif_transparent")]
    GifTransparent,
    #[serde(rename = "gifpreview_transparent")]
    GifPreviewTransparent,

    #[serde(rename = "tinygif_transparent")]
    TinyGifTransparent,
    #[serde(rename = "tinygifpreview_transparent")]
    TinyGifPreviewTransparent,

    #[serde(rename = "nanogif_transparent")]
    NanoGifTransparent,
    #[serde(rename = "nanogifpreview_transparent")]
    NanoGifPreviewTransparent,
}

pub struct Gif {
    pub height: i32,
    pub width: i32,
    pub size: i32,
    pub url: String,
}

impl Gif {
    pub fn search(agent: &ureq::Agent, api_key: &str, query: &str) -> Self {
        let response = agent
            .get("https://tenor.googleapis.com/v2/search")
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("Charset", "utf-8")
            .query("q", query)
            .query("key", api_key)
            .query("limit", "1")
            .call()
            .unwrap();

        let response: Response = serde_json::de::from_reader(&mut response.into_reader()).unwrap();
        let gif_info = &response.results[0].media_formats[&ContentFormat::TinyGif];
        let [width, height] = gif_info.dims;

        Self {
            width,
            height,
            size: gif_info.size,
            url: gif_info.url.clone(),
        }
    }
}
