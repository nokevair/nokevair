//! Tracks blog posts and their metadata, and provides a mechanism
//! for reading this metadata from a TOML file.

use chashmap::CHashMap;
use serde::Deserialize;

use std::fs;
use std::sync::{RwLock, RwLockReadGuard, PoisonError};

use super::{Log, Cfg};

/// Describes the format of the blog manifest TOML file.
#[derive(Deserialize)]
struct Manifest {
    /// The `posts` field of the manifest file.
    posts: Vec<ManifestPost>,
}

/// Describes the representation of a single post in the manifest TOML file.
#[derive(Deserialize)]
struct ManifestPost {
    /// The `title` field of an entry in the manifest file.
    title: String,
    /// The `id` field of an entry in the manifest file.
    id: String,
    /// The `date` field of an entry in the manifest file.
    date: String,
}

impl Manifest {
    /// Read and parse the manifest file to obtain metadata for all blog posts.
    fn load(log: &Log, cfg: &Cfg) -> Option<Self> {
        let contents = match fs::read(&cfg.paths.blog) {
            Ok(c) => c,
            Err(e) => { log.err(format_args!("while reading blog manifest: {}", e)); return None }
        };
        match toml::from_slice(&contents) {
            Ok(s) => Some(s),
            Err(e) => { log.err(format_args!("while parsing blog manifest: {}", e)); None }
        }
    }
}

/// Tracks information about every blog post.
pub struct Blog {
    /// A list of all post IDs in the order they appeared in the manifest
    /// (there is no way to extract this from `posts` without cloning the entire map).
    ids: RwLock<Vec<String>>,
    /// Associates post `id`s with their content.
    posts: CHashMap<String, Post>,
}

/// Tracks information about a single blog post.
pub struct Post {
    /// The title of the post.
    pub title: String,
    /// The date of the post.
    pub date: String,
}

impl Blog {
    /// Load posts from the TOML manifest file.
    pub fn load(log: &Log, cfg: &Cfg) -> Option<Self> {
        let self_ = Self {
            ids: RwLock::default(),
            posts: CHashMap::new(),
        };
        if self_.reload(log, cfg) {
            Some(self_)
        } else {
            None
        }
    }

    /// Remove all existing posts and reread the TOML manifest file. Note that
    /// this does not update the actual content of the posts - that will only
    /// be visible after a call to `AppState::reload_templates`.
    pub fn reload(&self, log: &Log, cfg: &Cfg) -> bool {
        let manifest = match Manifest::load(log, cfg) {
            Some(m) => m,
            None => return false,
        };
        
        let mut ids = self.ids.write().unwrap_or_else(PoisonError::into_inner);

        ids.clear();
        self.posts.clear();
        
        for m_post in manifest.posts {
            ids.push(m_post.id.clone());
            self.posts.insert(m_post.id, Post {
                title: m_post.title,
                date: m_post.date,
            });
        }

        let num_posts = ids.len();
        log.info(format_args!(
            "loaded {} blog post{}",
            num_posts,
            if num_posts == 1 { "" } else { "s" }
        ));

        true
    }

    /// Return a handle to a vector containing the IDs of all posts.
    pub fn ids(&self) -> RwLockReadGuard<Vec<String>> {
        self.ids.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Return a handle to the metadata associated with a particular post.
    pub fn metadata(&self, id: &str) -> Option<chashmap::ReadGuard<String, Post>> {
        self.posts.get(id)
    }
}
