use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mdpub", version, about = "Publish markdown to your self-hosted Zola blog")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create mdpub.toml in the current directory
    Init {
        /// rsync/SSH target, e.g. deploy@203.0.113.7
        #[arg(long)]
        server: String,
        /// Public base URL of the blog, e.g. https://blog.example.com
        #[arg(long)]
        base_url: String,
        /// Docroot on the server
        #[arg(long, default_value = "/var/www/blog")]
        docroot: String,
        /// Zola site directory relative to mdpub.toml
        #[arg(long, default_value = "blog")]
        site_dir: PathBuf,
    },
    /// Import an article into the Zola site, build, and deploy it
    Publish {
        /// Source markdown file (YAML frontmatter optional)
        file: PathBuf,
        /// Import and build, but do not deploy or record state
        #[arg(long)]
        dry_run: bool,
        /// Republish even if the content is unchanged
        #[arg(long)]
        force: bool,
        /// Publish as a Zola draft (page is not rendered publicly)
        #[arg(long)]
        draft: bool,
    },
    /// Serve the site locally with live reload (zola serve)
    Preview {
        /// Do not open the browser automatically
        #[arg(long)]
        no_open: bool,
    },
    /// Show tracked articles and whether they changed since publish
    Status,
    /// Remove a published article from the site and redeploy
    Unpublish {
        /// Source markdown file previously published
        file: PathBuf,
    },
}
