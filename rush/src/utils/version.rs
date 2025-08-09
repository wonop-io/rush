use serde::Deserialize;

#[derive(Deserialize)]
struct Release {
    url: String,
    tag_name: String,
    name: String,
    draft: bool,
    prerelease: bool,
}

pub async fn check_version() {
    let version = env!("CARGO_PKG_VERSION");
    let url = "https://api.github.com/repos/wonop-io/rush/releases/latest";
    let client = reqwest::Client::new();
    
    let resp = match client
        .get(url)
        .header("User-Agent", "rush")
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(_) => return, // Silently skip version check if network request fails
    };

    let release: Release = match resp.json().await {
        Ok(release) => release,
        Err(_) => return, // Silently skip if parsing fails
    };

    let latest_version = release
        .tag_name
        .replace("v.", "")
        .replace('v', "")
        .replace(' ', "");
    
    let current_version = match semver::Version::parse(version) {
        Ok(v) => v,
        Err(_) => return,
    };
    
    let latest_version = match semver::Version::parse(&latest_version) {
        Ok(v) => v,
        Err(_) => return,
    };

    if latest_version > current_version {
        println!("============================================================");
        println!("* A new version of Rush is available: {}", release.tag_name);
        println!("* Please update it by running:");
        println!("* ");
        println!("* cargo install rush-cli --force");
        println!("* ");
        println!("============================================================");
        println!();
        std::process::exit(1);
    }
}