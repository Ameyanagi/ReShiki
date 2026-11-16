//! github.com links, typed, pasted or dropped: a repository, a folder or a
//! file at a branch or commit, a branch, or a commit.
use super::RepoKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub key: RepoKey,
    /// A branch or commit; `None` is the default branch.
    pub reference: Option<String>,
    /// A folder or file of the repository (`""` is the top).
    pub path: String,
    /// `path` names a file (a `blob` link).
    pub file: bool,
}

impl Link {
    /// `lab-notes / scheme / route.rsk`.
    pub fn place(&self) -> String {
        std::iter::once(self.key.repo.as_str())
            .chain(self.path.split('/').filter(|name| !name.is_empty()))
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

/// A link to github.com, or to `web` (the GitHub in use); `None` for any
/// other text.
pub fn parse(text: &str, web: &str) -> Option<Link> {
    let text = text.trim();
    let rest = strip_scheme(text);
    let host = strip_scheme(web).trim_end_matches('/');
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let rest = ["github.com", host]
        .iter()
        .find_map(|host| rest.strip_prefix(host)?.strip_prefix('/'))?;
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let parts: Vec<String> = rest
        .split('/')
        .filter(|part| !part.is_empty())
        .map(decode)
        .collect::<Option<_>>()?;
    let [owner, repo, more @ ..] = parts.as_slice() else {
        return None;
    };
    let key = key(owner, repo.strip_suffix(".git").unwrap_or(repo))?;
    let (reference, path, file) = match more {
        [kind, reference, path @ ..] if kind == "tree" || kind == "blob" => {
            (Some(reference.clone()), path.join("/"), kind == "blob")
        }
        [kind, commit, ..] if kind == "commit" => (Some(commit.clone()), String::new(), false),
        // Issues, pull requests and the rest: the repository.
        _ => (None, String::new(), false),
    };
    Some(Link {
        key,
        reference,
        path,
        file,
    })
}

/// `owner/repo`, or a link as [`parse`] reads it.
pub fn parse_repo(text: &str, web: &str) -> Option<Link> {
    parse(text, web).or_else(|| {
        let (owner, repo) = text.trim().split_once('/')?;
        Some(Link {
            key: key(owner, repo)?,
            reference: None,
            path: String::new(),
            file: false,
        })
    })
}

fn strip_scheme(text: &str) -> &str {
    ["https://", "http://"]
        .iter()
        .find_map(|scheme| text.strip_prefix(scheme))
        .unwrap_or(text)
}

/// GitHub's names: letters, digits, `-`, `_` and `.`.
fn key(owner: &str, repo: &str) -> Option<RepoKey> {
    let name = |name: &str| {
        !name.is_empty()
            && name != "."
            && name != ".."
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    (name(owner) && name(repo)).then(|| RepoKey {
        owner: owner.to_owned(),
        repo: repo.to_owned(),
    })
}

fn decode(part: &str) -> Option<String> {
    let mut out = Vec::new();
    let mut bytes = part.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex: String = bytes.by_ref().take(2).map(char::from).collect();
            out.push(u8::from_str_radix(&hex, 16).ok()?);
        } else {
            out.push(byte);
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEB: &str = "http://127.0.0.1:47123";

    fn link(owner: &str, repo: &str, reference: Option<&str>, path: &str, file: bool) -> Link {
        Link {
            key: RepoKey {
                owner: owner.into(),
                repo: repo.into(),
            },
            reference: reference.map(str::to_owned),
            path: path.into(),
            file,
        }
    }

    #[test]
    fn links_name_a_repository_folder_file_branch_or_commit() {
        let parse = |text| parse(text, "https://github.com");
        assert_eq!(
            parse("https://github.com/alice/ligands"),
            Some(link("alice", "ligands", None, "", false))
        );
        assert_eq!(
            parse("github.com/alice/ligands.git"),
            Some(link("alice", "ligands", None, "", false))
        );
        assert_eq!(
            parse("  https://www.github.com/alice/ligands/tree/main/aromatics/  "),
            Some(link("alice", "ligands", Some("main"), "aromatics", false))
        );
        assert_eq!(
            parse("https://github.com/alice/ligands/blob/main/aromatics/furan%20(2).rsk?plain=1"),
            Some(link(
                "alice",
                "ligands",
                Some("main"),
                "aromatics/furan (2).rsk",
                true
            ))
        );
        // A branch other than the default, and a commit.
        assert_eq!(
            parse("https://github.com/alice/ligands/tree/draft"),
            Some(link("alice", "ligands", Some("draft"), "", false))
        );
        assert_eq!(
            parse("https://github.com/alice/ligands/commit/0123abcd#diff"),
            Some(link("alice", "ligands", Some("0123abcd"), "", false))
        );
        // Issues and the like name the repository.
        assert_eq!(
            parse("https://github.com/alice/ligands/issues/3"),
            Some(link("alice", "ligands", None, "", false))
        );
        for other in [
            "",
            "alice/ligands",
            "https://gitlab.com/alice/ligands",
            "https://github.com/alice",
            "https://github.com/alice/../x",
            "https://github.com/a lice/x",
            "benzene",
        ] {
            assert_eq!(parse(other), None, "{other}");
        }
    }

    #[test]
    fn the_github_in_use_and_a_bare_name_are_links_too() {
        assert_eq!(
            parse("http://127.0.0.1:47123/alice/ligands", WEB),
            Some(link("alice", "ligands", None, "", false))
        );
        assert_eq!(
            parse_repo(" alice/ligands ", WEB),
            Some(link("alice", "ligands", None, "", false))
        );
        assert_eq!(parse_repo("alice", WEB), None);
        assert_eq!(
            parse("https://github.com/alice/ligands/blob/main/a/b.rsk", WEB)
                .unwrap()
                .place(),
            "ligands / a / b.rsk"
        );
    }
}
