use aws_sdk_s3::config::{
    BehaviorVersion, Credentials, Region, RequestChecksumCalculation, ResponseChecksumValidation,
};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use clap::Parser;
use pulldown_cmark::{html::push_html, Options, Parser as MarkdownParser};
use serde::Deserialize;
use std::{error::Error, path::Path, path::PathBuf};
use tokio::fs;
use uuid::Uuid;
use dirs;

#[derive(Debug, Deserialize)]
struct S3Config {
    domain: String,
    bucket: String,
    region: String,
    prefix: String,
    access_key_id: String,
    secret_access_key: String,
}

#[derive(Debug, Deserialize)]
struct AppConfig {
    s3: S3Config,
}

/// A simple markdown-to-HTML converter and uploader for Backblaze B2.
#[derive(Parser, Debug)]
#[command(name = "klistra", author, version, about)]
struct Cli {
    /// The markdown file to convert.
    file: String,

    /// Output the HTML locally with the same base name as the input file (extension .html).
    /// If this flag is provided, the file will NOT be uploaded.
    #[arg(short = 'f', long = "file-output", alias = "fo")]
    file_output: bool,

    /// Optional path to the config file. If not provided, will look in $HOME/.config/klistra/config.toml
    #[arg(short = 'c', long = "config")]
    config_path: Option<PathBuf>,
}

fn get_config_path(cli_config_path: Option<PathBuf>) -> Option<PathBuf> {
    // If config path is provided via CLI, use that
    if let Some(path) = cli_config_path {
        return Some(path);
    }

    // Otherwise, look in the default location: $HOME/.config/klistra/config.toml
    dirs::home_dir().map(|home| home.join(".config").join("klistra").join("config.toml"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // Get the config path
    let config_path = get_config_path(cli.config_path)
        .ok_or_else(|| "Could not determine config file path")?;

    // Check if the config file exists
    if !config_path.exists() {
        return Err(format!(
            "Config file not found at {}",
            config_path.display()
        )
        .into());
    }

    let settings = config::Config::builder()
        .add_source(config::File::with_name(
            config_path
                .to_str()
                .ok_or_else(|| "Invalid config path")?
        ))
        .build()?;
    let app_config: AppConfig = settings.try_deserialize()?;

    let markdown_content = fs::read_to_string(&cli.file).await?;

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);

    let parser = MarkdownParser::new_ext(&markdown_content, options);
    let mut html_output = String::new();
    push_html(&mut html_output, parser);

    let current_date = chrono::Local::now().format("%B %d, %Y").to_string();

    let title = Path::new(&cli.file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Document");

    let full_html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <style>
        :root {{
            --background: #121212;
            --text: rgba(255, 255, 255, 0.87);
            --text-secondary: rgba(255, 255, 255, 0.6);
            --max-width: 800px;
            --spacing: 2rem;
        }}

        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen-Sans, Ubuntu, Cantarell, "Helvetica Neue", sans-serif;
            background: var(--background);
            color: var(--text);
            line-height: 1.6;
            padding: var(--spacing);
        }}

        .container {{
            max-width: var(--max-width);
            margin: 0 auto;
            padding: var(--spacing);
        }}

        .date {{
            color: var(--text-secondary);
            margin-bottom: 1rem;
            font-size: 1rem;
        }}

        h1 {{
            font-size: 2.5rem;
            font-weight: 600;
            margin-bottom: 0.5rem;
            line-height: 1.2;
        }}

        h2 {{
            font-size: 1.75rem;
            color: var(--text);
            margin: 2rem 0 1rem;
        }}

        p {{
            margin-bottom: 1.5rem;
            font-size: 1.1rem;
        }}

        a {{
            color: #3B82F6;
            text-decoration: none;
        }}

        a:hover {{
            text-decoration: underline;
        }}

        code {{
            font-family: "SF Mono", "Segoe UI Mono", "Roboto Mono", Menlo, Courier, monospace;
            background: rgba(255, 255, 255, 0.1);
            padding: 0.2em 0.4em;
            border-radius: 3px;
            font-size: 0.9em;
        }}

        pre {{
            background: rgba(255, 255, 255, 0.1);
            padding: 1rem;
            border-radius: 4px;
            overflow-x: auto;
            margin: 1.5rem 0;
        }}

        pre code {{
            background: none;
            padding: 0;
        }}

        img {{
            max-width: 100%;
            height: auto;
            border-radius: 8px;
            margin: 1.5rem 0;
        }}

        .subtitle {{
            color: var(--text-secondary);
            font-size: 1.25rem;
            margin-bottom: 2rem;
        }}

        table {{
            width: 100%;
            border-collapse: collapse;
            margin-bottom: 1.5rem;
        }}

        th, td {{
            border: 1px solid rgba(255, 255, 255, 0.2);
            padding: 0.75rem;
            text-align: left;
        }}

        thead {{
            background-color: rgba(255, 255, 255, 0.1);
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="date">{}</div>
        {}
    </div>
</body>
</html>"#,
        title, current_date, html_output
    );

    if cli.file_output {
        let input_path = Path::new(&cli.file);
        let output_path: PathBuf = input_path.with_extension("html");

        if fs::metadata(&output_path).await.is_ok() {
            println!(
                "Local file '{}' already exists. Not overwriting.",
                output_path.display()
            );
        } else {
            fs::write(&output_path, &full_html).await?;
            println!("Local HTML file created: {}", output_path.display());
        }
        return Ok(());
    }

    let folder_name = Uuid::new_v4().to_string();

    let s3_conf = app_config.s3;
    let endpoint = format!("https://s3.{}.backblazeb2.com", s3_conf.region);
    let aws_config = aws_sdk_s3::Config::builder()
        .region(Region::new(s3_conf.region.clone()))
        .endpoint_url(endpoint)
        .force_path_style(true)
        .behavior_version(BehaviorVersion::latest())
        .use_fips(false)
        .use_dual_stack(false)
        .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
        .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
        .credentials_provider(Credentials::new(
            s3_conf.access_key_id,
            s3_conf.secret_access_key,
            None,
            None,
            "backblaze-credentials",
        ))
        .build();
    let client = Client::from_conf(aws_config);

    let key = format!(
        "{}/p/{}/index.html",
        s3_conf.prefix.trim_end_matches('/'),
        folder_name
    );

    client
        .put_object()
        .bucket(s3_conf.bucket.clone())
        .key(key.clone())
        .body(ByteStream::from(full_html.into_bytes()))
        .content_type("text/html")
        .send()
        .await?;

    let public_url = format!("{}/p/{}", s3_conf.domain, folder_name);

    println!("File uploaded successfully: {}", public_url);

    Ok(())
}
