//! Nouvelle version publiée sur GitHub (`Update/Releases.swift`) : l'app Windows propose l'installeur de la
//! dernière release, sans l'installer elle-même (l'installation automatique demanderait une clé de signature).

use serde::Deserialize;

pub const REPOSITORY: &str = "adnmac/Foluna";

/// Release plus récente que la version installée.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    /// Page de la release (notes et téléchargements).
    pub page: String,
    /// Installeur Windows (`Foluna-X.Y.Z-windows-setup.exe`), s'il est joint.
    pub installer: Option<String>,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// « v0.10.2 » → [0, 10, 2].
fn parse_version(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches('v').split('.').map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0)).collect()
}

/// Release plus récente que `current` dans la réponse de l'API GitHub (`releases/latest`), avec un installeur Windows.
pub fn parse(json: &[u8], current: &str) -> Option<Release> {
    let r: GitHubRelease = serde_json::from_slice(json).ok()?;
    if r.draft || r.prerelease || parse_version(&r.tag_name) <= parse_version(current) {
        return None;
    }
    let installer = r
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with("-windows-setup.exe"))
        .or_else(|| r.assets.iter().find(|a| a.name.to_lowercase().ends_with("-windows.msi")))
        .map(|a| a.browser_download_url.clone());
    // Une release sans installeur Windows (publiée pour le Mac seulement) n'est pas proposée.
    installer.as_ref()?;
    Some(Release { version: r.tag_name.trim_start_matches('v').to_string(), page: r.html_url, installer })
}

/// Interroge GitHub (API publique, sans compte). Erreur réseau ou hors ligne : None.
pub fn check(current: &str) -> Option<Release> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    // TLS du système (schannel sous Windows) : ureq le demande explicitement, sinon il panique.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build())
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .build()
        .into();
    let body = agent
        .get(&url)
        .header("User-Agent", &format!("Foluna/{current} (Windows)"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .ok()?
        .into_body()
        .read_to_vec()
        .ok()?;
    parse(&body, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESPONSE: &str = r#"{"tag_name":"v0.7.0","html_url":"https://github.com/adnmac/Foluna/releases/tag/v0.7.0","draft":false,"prerelease":false,
        "assets":[{"name":"Foluna-0.7.0.dmg","browser_download_url":"https://x/Foluna-0.7.0.dmg"},
                  {"name":"Foluna-0.7.0-windows-setup.exe","browser_download_url":"https://x/Foluna-0.7.0-windows-setup.exe"}]}"#;

    #[test]
    fn newer_release_with_installer() {
        let r = parse(RESPONSE.as_bytes(), "0.6.0").unwrap();
        assert_eq!(r.version, "0.7.0");
        assert_eq!(r.installer.as_deref(), Some("https://x/Foluna-0.7.0-windows-setup.exe"));
        assert!(parse(RESPONSE.as_bytes(), "0.7.0").is_none(), "à jour");
        assert!(parse(RESPONSE.as_bytes(), "0.10.0").is_none(), "plus récent que GitHub");
    }

    #[test]
    fn mac_only_release_is_ignored() {
        let mac_only = RESPONSE.replace("Foluna-0.7.0-windows-setup.exe", "Foluna-0.7.0.zip");
        assert!(parse(mac_only.as_bytes(), "0.6.0").is_none());
        assert!(parse(b"not json", "0.6.0").is_none());
    }

    /// Vraie requête à GitHub : réponse ou None (hors ligne, réseau filtré), jamais de panique.
    #[test]
    fn check_never_panics() {
        let _ = check("0.0.1");
    }
}
