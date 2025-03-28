#![allow(unused)] // just cuz we have JSON deserialized stuff that we don't all use
                  // TODO(smolcK): we could do something about that idk, I like having the types
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug)]
pub enum GiphySearchError {
    NoGifs,
    UreqError(ureq::Error),
}

#[derive(Debug, Deserialize)]
struct GiphyMeta {
    status: i32,
    msg: String,
    response_id: String,
}

#[derive(Debug, Deserialize)]
struct GiphyResponse {
    data: Vec<GiphyResponseData>,
    meta: GiphyMeta,
}

#[derive(Debug, Deserialize)]
struct GiphyImageData {
    height: String,
    width: String,
    size: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct GiphyImages {
    original: GiphyImageData,
    preview_gif: GiphyImageData,
    #[serde(rename = "480w_still")]
    foureightyw_still: GiphyImageData,
}

#[derive(Debug, Deserialize)]
struct GiphyResponseData {
    #[serde(rename = "type")]
    t: String,
    images: GiphyImages,
    url: String,
    source: String,
    title: String,
}

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

    #[serde(rename = "webp")]
    Webp,

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
    pub preview_url: String,
    pub preview_height: i32,
    pub preview_width: i32,
    pub preview_size: i32,
    pub preview_mimetype: String,
}

impl Gif {
    pub fn search(agent: &ureq::Agent, api_key: &str, query: &str) -> Result<Self, ureq::Error> {
        let response = agent
            .get("https://tenor.googleapis.com/v2/search")
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("Charset", "utf-8")
            // TODO(smolck): I hope this sanitizes this cuz it's user input lol
            .query("q", query)
            .query("key", api_key)
            .query("limit", "1")
            .call()?;

        let response: Response = serde_json::de::from_reader(&mut response.into_reader()).unwrap();
        let gif_info = &response.results[0].media_formats[&ContentFormat::TinyGif];
        let [width, height] = gif_info.dims;
        let preview_info = &response.results[0].media_formats[&ContentFormat::TinyGifPreview];
        let [preview_width, preview_height] = preview_info.dims;

        Ok(Self {
            width,
            height,
            size: gif_info.size,
            url: gif_info.url.clone(),
            preview_height,
            preview_width,
            preview_url: preview_info.url.clone(),
            preview_size: preview_info.size,
            preview_mimetype: "image/png".to_owned(),
        })
    }

    pub fn search_giphy(
        agent: &ureq::Agent,
        api_key: &str,
        query: &str,
    ) -> Result<Self, GiphySearchError> {
        let response = agent
            .get("https://api.giphy.com/v1/gifs/search")
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("Charset", "utf-8")
            .query("api_key", api_key)
            .query("q", query)
            .query("limit", "10")
            .call();

        let Ok(response) = response else {
            let Err(response) = response else {
                unreachable!()
            };
            return Err(GiphySearchError::UreqError(response));
        };

        let response: GiphyResponse = serde_json::de::from_reader(&mut response.into_reader())
            .expect("Couldnt deserialize giphy response for some reason");
        if response.data.len() == 0 {
            return Err(GiphySearchError::NoGifs);
        }

        let idx = 0; // TODO(smolck): Randomly choose of the 10 we get
        let gif = &response.data[idx];
        let og = &gif.images.original;
        let preview = &gif.images.foureightyw_still;

        Ok(Self {
            width: og.width.parse().unwrap(),
            height: og.height.parse().unwrap(),
            size: og.size.parse().unwrap(),
            url: og.url.to_owned(),
            preview_url: preview.url.to_owned(),
            preview_height: preview.height.parse().unwrap(),
            preview_width: preview.width.parse().unwrap(),
            preview_size: preview.size.parse().unwrap(),
            preview_mimetype: "image/jpeg".to_owned(),
        })
    }
}
