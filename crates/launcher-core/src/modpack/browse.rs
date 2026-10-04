use anyhow::Context;
use reqwest::Client;
use serde::Deserialize;

const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const CURSEFORGE_API: &str = "https://api.curseforge.com/v1";
const CF_GAME_ID: u32 = 432;
const CF_CLASS_MODS: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowseSource {
    #[default]
    Modrinth,
    CurseForge,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModHit {
    pub source: BrowseSource,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameContext {
    pub mc_version: Option<String>,
    pub loader: Option<String>,
}

impl GameContext {
    pub fn from_parts(mc_version: Option<&str>, loader: Option<&str>) -> Self {
        let loader = loader
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter(|s| !matches!(s.to_ascii_lowercase().as_str(), "vanilla" | "none"))
            .map(str::to_ascii_lowercase);
        Self {
            mc_version: mc_version
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            loader,
        }
    }
}

pub async fn search_mods(
    client: &Client,
    source: BrowseSource,
    query: &str,
    ctx: &GameContext,
    api_key: Option<&str>,
) -> anyhow::Result<Vec<ModHit>> {
    match source {
        BrowseSource::Modrinth => search_modrinth(client, query, ctx).await,
        BrowseSource::CurseForge => search_curseforge(client, query, ctx, api_key).await,
    }
}

pub async fn resolve_mod_file(
    client: &Client,
    source: BrowseSource,
    project_id: &str,
    ctx: &GameContext,
    api_key: Option<&str>,
) -> anyhow::Result<(String, String)> {
    match source {
        BrowseSource::Modrinth => resolve_modrinth(client, project_id, ctx).await,
        BrowseSource::CurseForge => {
            let id: u64 = project_id
                .parse()
                .map_err(|_| anyhow::anyhow!("Bad CurseForge project id"))?;
            resolve_curseforge(client, id, ctx, api_key).await
        }
    }
}

fn modrinth_facets(ctx: &GameContext) -> String {
    let mut facets = vec!["[\"project_type:mod\"]".to_string()];
    if let Some(v) = &ctx.mc_version {
        facets.push(format!("[\"versions:{v}\"]"));
    }
    if let Some(l) = &ctx.loader {
        facets.push(format!("[\"categories:{l}\"]"));
    }
    format!("[{}]", facets.join(","))
}

async fn search_modrinth(
    client: &Client,
    query: &str,
    ctx: &GameContext,
) -> anyhow::Result<Vec<ModHit>> {
    #[derive(Deserialize)]
    struct Hit {
        #[serde(alias = "id")]
        project_id: String,
        slug: String,
        title: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        author: String,
        #[serde(rename = "downloads", default)]
        downloads: u64,
        #[serde(rename = "icon_url", default)]
        icon_url: Option<String>,
    }
    #[derive(Deserialize)]
    struct Response {
        hits: Vec<Hit>,
    }

    let url = format!(
        "{MODRINTH_API}/search?query={}&limit=20&facets={}",
        urlencoding::encode(query),
        urlencoding::encode(&modrinth_facets(ctx)),
    );
    let resp: Response = client
        .get(&url)
        .send()
        .await
        .context("Modrinth search request failed")?
        .json()
        .await
        .context("Modrinth search response was not JSON")?;

    Ok(resp
        .hits
        .into_iter()
        .map(|h| ModHit {
            source: BrowseSource::Modrinth,
            project_id: h.project_id,
            slug: h.slug,
            title: h.title,
            author: h.author,
            description: h.description,
            downloads: h.downloads,
            icon_url: h.icon_url,
        })
        .collect())
}

fn modrinth_version_matches(
    game_versions: &[String],
    loaders: &[String],
    ctx: &GameContext,
) -> bool {
    if let Some(v) = &ctx.mc_version {
        if !game_versions.iter().any(|g| g == v) {
            return false;
        }
    }
    if let Some(l) = &ctx.loader {
        if !loaders.iter().any(|d| d.eq_ignore_ascii_case(l)) {
            return false;
        }
    }
    true
}

async fn resolve_modrinth(
    client: &Client,
    project_id: &str,
    ctx: &GameContext,
) -> anyhow::Result<(String, String)> {
    #[derive(Deserialize)]
    struct File {
        #[serde(rename = "filename", alias = "file_name")]
        filename: String,
        url: String,
        #[serde(rename = "primary", default)]
        primary: bool,
    }
    #[derive(Deserialize)]
    struct Version {
        #[serde(default)]
        game_versions: Vec<String>,
        #[serde(default)]
        loaders: Vec<String>,
        #[serde(default)]
        files: Vec<File>,
        #[serde(rename = "version_type", default)]
        version_type: String,
    }

    let url = format!("{MODRINTH_API}/project/{project_id}/version");
    let versions: Vec<Version> = client
        .get(&url)
        .send()
        .await
        .context("Modrinth version request failed")?
        .json()
        .await
        .context("Modrinth version response was not JSON")?;

    let mut release = Vec::new();
    let mut prerelease = Vec::new();
    for v in versions {
        if v.files.is_empty() || !modrinth_version_matches(&v.game_versions, &v.loaders, ctx) {
            continue;
        }
        if v.version_type == "release" {
            release.push(v);
        } else {
            prerelease.push(v);
        }
    }
    let picked = release
        .into_iter()
        .next()
        .or_else(|| prerelease.into_iter().next());
    let version = picked.ok_or_else(|| {
        anyhow::anyhow!(
            "No file matches {}{}",
            ctx.mc_version.as_deref().unwrap_or("any version"),
            ctx.loader
                .as_deref()
                .map(|l| format!(" · {l}"))
                .unwrap_or_default()
        )
    })?;

    let file = version
        .files
        .iter()
        .find(|f| f.primary)
        .or(version.files.first())
        .context("Version has no files")?;
    Ok((file.filename.clone(), file.url.clone()))
}

fn forgecdn_url(file_id: u64, filename: &str) -> String {
    format!(
        "https://edge.forgecdn.net/files/{}/{}/{}",
        file_id / 1000,
        file_id % 1000,
        urlencoding::encode(filename),
    )
}

fn cf_versions_match(entries: &[String], ctx: &GameContext) -> bool {
    let has = |want: &str, ignore_case: bool| {
        entries.iter().any(|e| {
            if ignore_case {
                e.eq_ignore_ascii_case(want)
            } else {
                e == want
            }
        })
    };
    if let Some(v) = &ctx.mc_version {
        if !has(v, false) {
            return false;
        }
    }
    if let Some(l) = &ctx.loader {
        if !has(l, true) {
            return false;
        }
    }
    true
}

async fn cf_get<T: serde::de::DeserializeOwned>(
    client: &Client,
    url: &str,
    api_key: Option<&str>,
) -> anyhow::Result<T> {
    let key = api_key.filter(|k| !k.trim().is_empty()).ok_or_else(|| {
        anyhow::anyhow!("CurseForge needs an API key — grab a free one at console.curseforge.com")
    })?;
    let resp = client
        .get(url)
        .header("x-api-key", key.trim())
        .send()
        .await
        .context("CurseForge request failed")?;
    let status = resp.status();
    if status.as_u16() == 403 {
        anyhow::bail!("CurseForge rejected the API key — check it in the search bar");
    }
    if !status.is_success() {
        anyhow::bail!("CurseForge returned {}", status);
    }
    resp.json()
        .await
        .context("CurseForge response was not JSON")
}

async fn search_curseforge(
    client: &Client,
    query: &str,
    ctx: &GameContext,
    api_key: Option<&str>,
) -> anyhow::Result<Vec<ModHit>> {
    #[derive(Deserialize)]
    struct Author {
        #[serde(default)]
        name: String,
    }
    #[derive(Deserialize)]
    struct Logo {
        #[serde(default)]
        url: Option<String>,
    }
    #[derive(Deserialize)]
    struct CfMod {
        id: u64,
        #[serde(default)]
        slug: String,
        name: String,
        #[serde(default)]
        summary: String,
        #[serde(rename = "downloadCount", default)]
        downloads: u64,
        #[serde(default)]
        logo: Option<Logo>,
        #[serde(default)]
        authors: Vec<Author>,
    }
    #[derive(Deserialize)]
    struct Response {
        data: Vec<CfMod>,
    }

    let mut url = format!(
        "{CURSEFORGE_API}/mods/search?gameId={CF_GAME_ID}&classId={CF_CLASS_MODS}&searchFilter={}&pageSize=20",
        urlencoding::encode(query),
    );
    if let Some(v) = &ctx.mc_version {
        url.push_str(&format!("&gameVersion={}", urlencoding::encode(v)));
    }
    let resp: Response = cf_get(client, &url, api_key).await?;

    Ok(resp
        .data
        .into_iter()
        .map(|m| ModHit {
            source: BrowseSource::CurseForge,
            project_id: m.id.to_string(),
            slug: m.slug,
            title: m.name,
            author: m
                .authors
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_default(),
            description: m.summary,
            downloads: m.downloads,
            icon_url: m.logo.and_then(|l| l.url),
        })
        .collect())
}

async fn resolve_curseforge(
    client: &Client,
    project_id: u64,
    ctx: &GameContext,
    api_key: Option<&str>,
) -> anyhow::Result<(String, String)> {
    #[derive(Deserialize)]
    struct Sortable {
        #[serde(rename = "gameVersionName", default)]
        name: Option<String>,
        #[serde(default)]
        game_version: Option<String>,
    }
    #[derive(Deserialize)]
    struct CfFile {
        id: u64,
        #[serde(rename = "fileName", default)]
        file_name: String,
        #[serde(rename = "downloadUrl", default)]
        download_url: Option<String>,
        #[serde(rename = "sortableGameVersions", default)]
        sortable: Vec<Sortable>,
    }
    #[derive(Deserialize)]
    struct Response {
        data: Vec<CfFile>,
    }

    let mut url =
        format!("{CURSEFORGE_API}/mods/{project_id}/files?pageSize=50&sortField=2&sortOrder=desc",);
    if let Some(v) = &ctx.mc_version {
        url.push_str(&format!("&gameVersion={}", urlencoding::encode(v)));
    }
    let resp: Response = cf_get(client, &url, api_key).await?;

    if ctx.mc_version.is_none() && ctx.loader.is_none() {
        let f = resp
            .data
            .into_iter()
            .find(|f| !f.file_name.is_empty())
            .ok_or_else(|| anyhow::anyhow!("No files published for this project"))?;
        return Ok((
            f.file_name.clone(),
            cf_file_url(&f.download_url, f.id, &f.file_name),
        ));
    }

    for f in &resp.data {
        if f.file_name.is_empty() {
            continue;
        }
        let entries: Vec<String> = f
            .sortable
            .iter()
            .filter_map(|s| s.name.clone().or_else(|| s.game_version.clone()))
            .collect();
        if entries.is_empty() || cf_versions_match(&entries, ctx) {
            return Ok((
                f.file_name.clone(),
                cf_file_url(&f.download_url, f.id, &f.file_name),
            ));
        }
    }
    anyhow::bail!(
        "No CurseForge file matches {}",
        ctx.mc_version.as_deref().unwrap_or("any version")
    )
}

fn cf_file_url(download_url: &Option<String>, file_id: u64, file_name: &str) -> String {
    download_url
        .clone()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| forgecdn_url(file_id, file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facets_include_project_type_and_context() {
        let ctx = GameContext::default();
        assert_eq!(modrinth_facets(&ctx), "[[\"project_type:mod\"]]");

        let ctx = GameContext::from_parts(Some("1.21.11"), Some("fabric"));
        let s = modrinth_facets(&ctx);
        assert!(s.contains("\"project_type:mod\""));
        assert!(s.contains("\"versions:1.21.11\""));
        assert!(s.contains("\"categories:fabric\""));
    }

    #[test]
    fn game_context_normalises_loader() {
        assert_eq!(
            GameContext::from_parts(Some(" 1.21.11 "), Some("Fabric"))
                .loader
                .as_deref(),
            Some("fabric")
        );
        assert_eq!(GameContext::from_parts(None, Some("vanilla")).loader, None);
        assert_eq!(GameContext::from_parts(Some(""), None).mc_version, None);
    }

    #[test]
    fn modrinth_version_matching() {
        let ctx = GameContext::from_parts(Some("1.21.11"), Some("fabric"));
        assert!(modrinth_version_matches(
            &["1.21.11".into()],
            &["Fabric".into()],
            &ctx
        ));
        assert!(!modrinth_version_matches(
            &["1.20.1".into()],
            &["Fabric".into()],
            &ctx
        ));
        assert!(!modrinth_version_matches(
            &["1.21.11".into()],
            &["Forge".into()],
            &ctx
        ));
        let any = GameContext::default();
        assert!(modrinth_version_matches(&[], &[], &any));
    }

    #[test]
    fn cf_version_matching() {
        let ctx = GameContext::from_parts(Some("1.21.11"), Some("fabric"));
        assert!(cf_versions_match(
            &["1.21.11".into(), "Fabric".into()],
            &ctx
        ));
        assert!(!cf_versions_match(
            &["1.19.2".into(), "Fabric".into()],
            &ctx
        ));
        assert!(!cf_versions_match(
            &["1.21.11".into(), "Forge".into()],
            &ctx
        ));
    }

    #[test]
    fn forgecdn_url_splits_file_id() {
        assert_eq!(
            forgecdn_url(1234567, "sodium.jar"),
            "https://edge.forgecdn.net/files/1234/567/sodium.jar"
        );
        assert_eq!(
            forgecdn_url(999, "a b.jar"),
            "https://edge.forgecdn.net/files/0/999/a%20b.jar"
        );
    }

    #[test]
    #[ignore = "network"]
    fn modrinth_search_and_resolve_live() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = Client::builder()
            .user_agent("Aethel-Launcher/test")
            .build()
            .unwrap();
        let ctx = GameContext::from_parts(Some("1.21.11"), Some("fabric"));
        rt.block_on(async {
            let hits = search_mods(&client, BrowseSource::Modrinth, "sodium", &ctx, None)
                .await
                .expect("search");
            assert!(!hits.is_empty());
            let sodium = hits
                .iter()
                .find(|h| h.slug == "sodium")
                .expect("sodium in results");
            let (filename, url) = resolve_mod_file(
                &client,
                BrowseSource::Modrinth,
                &sodium.project_id,
                &ctx,
                None,
            )
            .await
            .expect("resolve");
            assert!(filename.ends_with(".jar"), "{filename}");
            assert!(url.contains("cdn.modrinth.com"), "{url}");
            assert!(
                !filename.contains("neoforge"),
                "wrong loader picked: {filename}"
            );
        });
    }
}
