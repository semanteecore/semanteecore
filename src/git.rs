use git2_commit;
use std::path::Path;
use semver::Version;
use std::error::Error;
use git2::{self, Repository, Commit, Config, Signature};
use commit_analyzer::{self, CommitType};

struct Author {
    pub name: String,
    pub email: String
}

fn range_to_head(commit: &str) -> String {
    format!("{}..HEAD", commit)
}

fn format_commit(commit: Commit) -> String {
    format!("{}\n{}", commit.id(), commit.message().unwrap_or(""))
}

fn add<P: AsRef<Path>>(repo: &str, files: &[P]) -> Result<(), git2::Error> {
    let repo = try!(Repository::open(repo));
    let mut index = try!(repo.index());

    for path in files {
        let _ = try!(index.add_path(path.as_ref()));
    }

    index.write()
}

fn get_signature() -> Result<Author, git2::Error> {
    let config = try!(Config::open_default());
    let author = try!(config.get_string("user.name"));
    let email = try!(config.get_string("user.email"));
    Ok(Author {
        name: author.to_string(),
        email: email.to_string()
    })
}

fn commit(repo: &str, name: &str, email: &str, message: &str) -> Result<(), git2::Error> {
    let signature = try!(Signature::now(name, email));
    let update_ref = Some("HEAD");

    let repo = try!(Repository::open(repo));

    let oid = try!(repo.refname_to_id("HEAD"));
    let parent_commit = try!(repo.find_commit(oid));
    let parents = vec![&parent_commit];

    let mut index = try!(repo.index());
    let tree_oid = try!(index.write_tree());
    let tree = try!(repo.find_tree(tree_oid));

    repo
        .commit(update_ref, &signature, &signature, message, &tree, &parents)
        .map(|_| ())
}

pub fn latest_tag(path: &str) -> Option<Version> {
    let repo = match Repository::open(path) {
        Ok(repo) => repo,
        Err(_) => return None
    };

    let tags = match repo.tag_names(None) {
        Ok(tags) => tags,
        Err(_) => return None
    };

    tags.iter()
        .map(|tag| tag.unwrap())
        .filter_map(|tag| Version::parse(&tag[1..]).ok())
        .max()
}

pub fn version_bump_since_latest(path: &str) -> CommitType {
    match latest_tag(path) {
        Some(t) => {
            let tag = format!("v{}", t.to_string());
            version_bump_since_tag(path, &tag)
        },
        None => CommitType::Major
    }
}

pub fn version_bump_since_tag(path: &str, tag: &str) -> CommitType {
    let tag = range_to_head(tag);

    let repo = Repository::open(path).expect("Open repository failed");

    let mut walker = repo.revwalk().expect("Creating a revwalk failed");
    walker.push_range(&tag).expect("Adding a range failed");

    let tag = walker.map(|c| repo.find_commit(c).expect("No commit found"))
        .map(|c| format_commit(c))
        .map(|c| commit_analyzer::analyze_single(&c).expect("Analyzing commit failed"))
        .max().unwrap_or(CommitType::Unknown);

    tag
}

pub fn generate_commit_message(new_version: &str) -> String {
    format!("Bump version to {}", new_version).into()
}

pub fn commit_files(repository_path: &str, new_version: &str) -> Result<(), String> {
    let files = vec!["Cargo.toml", "Changelog.md"];
    match add(&repository_path, &files[..]) {
        Ok(_) => {},
        Err(err) => return Err(err.description().into())
    }
    let author = match get_signature() {
        Ok(author) => author,
        Err(err) => return Err(err.description().into())
    };

    match commit(repository_path, &author.name, &author.email, &generate_commit_message(new_version)) {
        Ok(_) => Ok(()),
        Err(err) => Err(err.description().into())
    }
}

pub fn tag(repository_path: &str, tag_name: &str, tag_message: &str) -> Result<(), String> {
    let author = match get_signature() {
        Ok(author) => author,
        Err(err) => return Err(err.description().into())
    };

    git2_commit::tag(repository_path, &author.name, &author.email, &tag_name, &tag_message)
        .map_err(|err| err.description().into())
}
