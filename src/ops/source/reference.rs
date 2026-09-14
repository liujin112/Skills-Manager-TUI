use crate::meta::Source;
use anyhow::{Result, bail};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum InstallRef {
    Local(PathBuf),
    Git {
        url: String,
        branch: Option<String>,
        subpath: Option<String>,
    },
    Archive {
        url: String,
        subpath: Option<String>,
    },
    /// HTTP endpoints without an explicit Git or archive form are probed once
    /// during acquisition. Persisted sources always use the resolved provider.
    Url {
        url: String,
        branch: Option<String>,
        subpath: Option<String>,
    },
}

impl InstallRef {
    pub fn is_remote(&self) -> bool {
        !matches!(self, Self::Local(_))
    }
    pub fn url(&self) -> Option<&str> {
        match self {
            Self::Local(_) => None,
            Self::Git { url, .. } | Self::Archive { url, .. } | Self::Url { url, .. } => Some(url),
        }
    }
    pub fn subpath(&self) -> Option<&str> {
        match self {
            Self::Local(_) => None,
            Self::Git { subpath, .. }
            | Self::Archive { subpath, .. }
            | Self::Url { subpath, .. } => subpath.as_deref(),
        }
    }
    pub fn from_source(source: &Source) -> Result<Self> {
        Ok(match source {
            Source::Git {
                url,
                branch,
                subpath,
                ..
            } => Self::Git {
                url: url.clone(),
                branch: branch.clone(),
                subpath: subpath.clone(),
            },
            Source::Archive { url, subpath, .. } => Self::Archive {
                url: url.clone(),
                subpath: subpath.clone(),
            },
            Source::Local { .. } => bail!("local skills have no remote source"),
        })
    }
}

/// Existing paths and Git shorthand retain their syntax. Download URLs are
/// kept verbatim, including signed queries and `@` characters.
pub fn parse_ref(input: &str, branch: Option<&str>, subpath: Option<&str>) -> Result<InstallRef> {
    let path = crate::paths::expand_tilde(input);
    if path.exists() {
        return Ok(InstallRef::Local(std::fs::canonicalize(path)?));
    }
    let mut branch = branch.map(str::to_string);
    let mut subpath = subpath
        .map(|s| s.trim_matches('/').to_string())
        .filter(|s| !s.is_empty());
    if let Some(path) = &subpath {
        crate::repository::validate_subpath(path)?;
    }
    let http = input.starts_with("http://") || input.starts_with("https://");
    if http {
        let path = input.split(['?', '#']).next().unwrap_or(input);
        if let Some(rest) = input
            .strip_prefix("https://github.com/")
            .or_else(|| input.strip_prefix("http://github.com/"))
        {
            let parts: Vec<_> = rest.trim_end_matches('/').split('/').collect();
            if parts.len() < 2 || parts[..2].iter().any(|s| s.is_empty()) {
                bail!("cannot parse GitHub URL: {input}");
            }
            if !input.contains(['?', '#'])
                && (parts.len() == 2 || (parts.len() >= 4 && parts[2] == "tree"))
            {
                let (repo, inline_branch) = git_branch(parts[1]);
                let url = format!(
                    "https://github.com/{}/{}",
                    parts[0],
                    repo.trim_end_matches(".git")
                );
                if branch.is_none() {
                    branch = inline_branch.map(str::to_string);
                }
                if parts.len() >= 4 {
                    if branch.is_none() {
                        branch = Some(parts[3].into());
                    }
                    if subpath.is_none() && parts.len() > 4 {
                        subpath = Some(parts[4..].join("/"));
                    }
                }
                return Ok(InstallRef::Git {
                    url,
                    branch,
                    subpath,
                });
            }
        }
        let lower = path.to_ascii_lowercase();
        let archive = [
            ".zip", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tbz", ".tar.xz", ".txz",
            ".gz", ".bz2", ".xz",
        ]
        .iter()
        .any(|suffix| lower.ends_with(suffix));
        if archive {
            anyhow::ensure!(
                branch.is_none(),
                "--branch applies only to Git repositories; use the archive URL for the desired version"
            );
            return Ok(InstallRef::Archive {
                url: input.into(),
                subpath,
            });
        }
        let (base, inline_branch) = git_branch(input);
        if !input.contains(['?', '#']) && base.ends_with(".git") {
            if branch.is_none() {
                branch = inline_branch.map(str::to_string);
            }
            return Ok(InstallRef::Git {
                url: base.into(),
                branch,
                subpath,
            });
        }
        return Ok(InstallRef::Url {
            url: input.into(),
            branch,
            subpath,
        });
    }
    if input.starts_with("git@")
        || input.starts_with("ssh://")
        || input.starts_with("file://")
        || input.ends_with(".git")
    {
        let (url, inline_branch) = if input.starts_with("git@") {
            (input, None)
        } else {
            git_branch(input)
        };
        if branch.is_none() {
            branch = inline_branch.map(str::to_string);
        }
        return Ok(InstallRef::Git {
            url: url.into(),
            branch,
            subpath,
        });
    }
    anyhow::ensure!(!input.contains("://"), "unsupported source URL scheme");
    let parts: Vec<_> = input.trim_matches('/').split('/').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        bail!("cannot parse reference: {input} (not a path, URL, or owner/repo)");
    }
    let (repo, inline_branch) = git_branch(parts[1]);
    if branch.is_none() {
        branch = inline_branch.map(str::to_string);
    }
    if subpath.is_none() && parts.len() > 2 {
        subpath = Some(parts[2..].join("/"));
    }
    Ok(InstallRef::Git {
        url: format!("https://github.com/{}/{repo}", parts[0]),
        branch,
        subpath,
    })
}

fn git_branch(input: &str) -> (&str, Option<&str>) {
    match input.rsplit_once('@') {
        Some((base, branch))
            if !base.is_empty() && !branch.is_empty() && !branch.contains(['/', ':']) =>
        {
            (base, Some(branch))
        }
        _ => (input, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_urls_keep_queries_and_github_paths() {
        for url in [
            "https://github.com/o/r/archive/refs/heads/main.zip?token=a@b",
            "https://host.test/bundle.tar.gz",
        ] {
            assert!(
                matches!(parse_ref(url,None,None).unwrap(),InstallRef::Archive{url:actual,..} if actual==url)
            );
        }
        for url in [
            "https://host.test/download?id=a@b",
            "https://github.com/o/r/releases/download/v1/skills",
        ] {
            assert!(
                matches!(parse_ref(url,None,None).unwrap(),InstallRef::Url{url:actual,..} if actual==url)
            );
        }
    }
    #[test]
    fn archive_rejects_git_branch_and_unsafe_path() {
        assert!(parse_ref("https://host.test/a.zip", Some("main"), None).is_err());
        assert!(parse_ref("https://host.test/a.zip", None, Some("../elsewhere")).is_err());
    }

    #[test]
    fn github_repository_and_tree_paths_take_precedence_over_filename_suffixes() {
        assert!(matches!(
            parse_ref("https://github.com/acme/skills.zip", None, None).unwrap(),
            InstallRef::Git { .. }
        ));
        assert!(
            matches!(parse_ref("https://github.com/acme/skills/tree/main/tools.zip", None, None).unwrap(), InstallRef::Git { subpath: Some(path), .. } if path == "tools.zip")
        );
    }
}
