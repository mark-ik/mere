//! Read-only files-in-place discovery.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chartulary::{CLASS_FACET, Container, FacetId, FacetStore};
use serde_json::json;

use crate::{FILE_CLASS, FILE_DOCUMENT_FACET, KnotContentClasses, NOTE_CLASS, NOTE_DOCUMENT_FACET};

/// A file disclosed by Knot. Its bytes remain on disk and are not carried here.
#[derive(Clone, Debug, PartialEq)]
pub struct DiskDocument {
    /// Stable within a filesystem across ordinary renames.
    pub id: String,
    /// Shared graph vocabulary. `body` and `content` remain absent.
    pub container: Container,
    /// Native path used for reads and later write-through.
    pub path: PathBuf,
    /// Observed byte size.
    pub byte_size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FileIdentity {
    #[cfg(windows)]
    Windows {
        volume: u32,
        index: u64,
    },
    #[cfg(unix)]
    Unix {
        device: u64,
        inode: u64,
    },
    Fallback(PathBuf),
}

impl FileIdentity {
    fn read(path: &Path, _metadata: &Metadata) -> io::Result<Self> {
        #[cfg(windows)]
        {
            if let Some((volume, index)) = windows_file_identity(path)? {
                return Ok(Self::Windows { volume, index });
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            return Ok(Self::Unix {
                device: _metadata.dev(),
                inode: _metadata.ino(),
            });
        }
        #[allow(unreachable_code)]
        Ok(Self::Fallback(fs::canonicalize(path)?))
    }

    fn stable_id(&self) -> String {
        let material = format!("{self:?}");
        format!("knot:file:{}", blake3::hash(material.as_bytes()).to_hex())
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_file_identity(path: &Path) -> io::Result<Option<(u32, u64)>> {
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };

    let file = fs::File::open(path)?;
    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: `file` owns a valid handle for the duration of the call, and
    // Windows initializes the output structure when it succeeds.
    let succeeded =
        unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, information.as_mut_ptr()) };
    if succeeded == 0 {
        return Ok(None);
    }
    // SAFETY: the successful call initialized the structure.
    let information = unsafe { information.assume_init() };
    let index =
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow);
    Ok(Some((information.dwVolumeSerialNumber, index)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    identity: FileIdentity,
    path: PathBuf,
    byte_size: u64,
    modified_nanos: u128,
}

/// Configurable names a directory scan does not enter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IgnorePolicy {
    names: BTreeSet<String>,
    ignore_hidden: bool,
}

impl IgnorePolicy {
    /// Start with an empty policy.
    pub fn none() -> Self {
        Self {
            names: BTreeSet::new(),
            ignore_hidden: false,
        }
    }

    /// Ignore a file or directory with this exact name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.names.insert(name.into());
        self
    }

    /// Choose whether dot-prefixed names are ignored.
    pub fn with_hidden(mut self, ignore: bool) -> Self {
        self.ignore_hidden = ignore;
        self
    }

    fn ignores(&self, path: &Path) -> bool {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        self.names.contains(name) || (self.ignore_hidden && name.starts_with('.'))
    }
}

impl Default for IgnorePolicy {
    fn default() -> Self {
        Self::none()
            .with_name(".git")
            .with_name("target")
            .with_name("node_modules")
            .with_hidden(true)
    }
}

/// A directory index whose graph state contains references and observations,
/// never file bodies.
pub struct DirectorySource {
    root: PathBuf,
    ignore: IgnorePolicy,
    documents: BTreeMap<String, DiskDocument>,
    observations: BTreeMap<FileIdentity, Observation>,
    facets: FacetStore<String>,
    classes: KnotContentClasses,
    revision: u64,
}

impl DirectorySource {
    /// Open and scan a directory.
    pub fn open(root: impl AsRef<Path>) -> io::Result<Self> {
        Self::with_ignore(root, IgnorePolicy::default())
    }

    /// Open with a caller-selected ignore policy.
    pub fn with_ignore(root: impl AsRef<Path>, ignore: IgnorePolicy) -> io::Result<Self> {
        let root = fs::canonicalize(root)?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a directory", root.display()),
            ));
        }
        let mut source = Self {
            root,
            ignore,
            documents: BTreeMap::new(),
            observations: BTreeMap::new(),
            facets: FacetStore::new(),
            classes: KnotContentClasses::new(),
            revision: 0,
        };
        source.refresh()?;
        Ok(source)
    }

    /// Canonical directory being observed.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Current revision, incremented only when observed disk state changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Files ordered by stable id.
    pub fn documents(&self) -> impl Iterator<Item = &DiskDocument> {
        self.documents.values()
    }

    /// Runtime facets associated with the files.
    pub fn facets(&self) -> &FacetStore<String> {
        &self.facets
    }

    /// Mutable facet access for host-owned metadata.
    pub fn facets_mut(&mut self) -> &mut FacetStore<String> {
        &mut self.facets
    }

    /// Built-in classes and schemas.
    pub fn classes(&self) -> &KnotContentClasses {
        &self.classes
    }

    /// Re-scan the directory. Returns whether source-visible state changed.
    pub fn refresh(&mut self) -> io::Result<bool> {
        let mut next = Vec::new();
        self.walk(&self.root, &mut next)?;
        next.sort_by(|left, right| left.path.cmp(&right.path));

        let next_observations = next
            .iter()
            .cloned()
            .map(|observation| (observation.identity.clone(), observation))
            .collect::<BTreeMap<_, _>>();
        if next_observations == self.observations {
            return Ok(false);
        }

        let live_ids = next
            .iter()
            .map(|observation| observation.identity.stable_id())
            .collect::<BTreeSet<_>>();
        let retired_ids = self
            .documents
            .keys()
            .filter(|id| !live_ids.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        for id in retired_ids {
            self.facets.remove_node(&id);
        }

        let mut documents = BTreeMap::new();
        for observation in &next {
            let id = observation.identity.stable_id();
            let address = file_address(&observation.path);
            let extension = observation
                .path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase);
            let class = if extension.as_deref().is_some_and(is_note_extension) {
                NOTE_CLASS
            } else {
                FILE_CLASS
            };
            let title = observation
                .path
                .file_stem()
                .or_else(|| observation.path.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled")
                .to_string();
            let media_type = extension.as_deref().map(media_type_for_extension);
            let mut container = Container::new(id.clone())
                .with_address(address.clone())
                .with_title(title);
            container.media_type = media_type.map(str::to_string);

            self.facets
                .set(
                    id.clone(),
                    FacetId::new(CLASS_FACET),
                    json!(class),
                    &self.classes.validator,
                )
                .map_err(io::Error::other)?;
            self.facets
                .set(
                    id.clone(),
                    FacetId::new(FILE_DOCUMENT_FACET),
                    json!({
                        "version": 1,
                        "address": address,
                        "byte_size": observation.byte_size,
                        "extension": extension,
                    }),
                    &self.classes.validator,
                )
                .map_err(io::Error::other)?;
            if class == NOTE_CLASS {
                self.facets
                    .set(
                        id.clone(),
                        FacetId::new(NOTE_DOCUMENT_FACET),
                        json!({
                            "version": 1,
                            "format": extension.as_deref().unwrap_or("text"),
                        }),
                        &self.classes.validator,
                    )
                    .map_err(io::Error::other)?;
            } else {
                self.facets.remove(&id, &FacetId::new(NOTE_DOCUMENT_FACET));
            }
            documents.insert(
                id.clone(),
                DiskDocument {
                    id,
                    container,
                    path: observation.path.clone(),
                    byte_size: observation.byte_size,
                },
            );
        }

        self.documents = documents;
        self.observations = next_observations;
        self.revision = self.revision.saturating_add(1).max(1);
        Ok(true)
    }

    fn walk(&self, directory: &Path, output: &mut Vec<Observation>) -> io::Result<()> {
        let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            if self.ignore.ignores(&path) {
                continue;
            }
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                self.walk(&path, output)?;
            } else if metadata.is_file() {
                let modified_nanos = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |duration| duration.as_nanos());
                output.push(Observation {
                    identity: FileIdentity::read(&path, &metadata)?,
                    path,
                    byte_size: metadata.len(),
                    modified_nanos,
                });
            }
        }
        Ok(())
    }
}

fn is_note_extension(extension: &str) -> bool {
    matches!(extension, "knot" | "djot" | "md" | "markdown" | "txt")
}

fn media_type_for_extension(extension: &str) -> &'static str {
    match extension {
        "knot" => "text/vnd.knot",
        "djot" => "text/djot",
        "md" | "markdown" => "text/markdown",
        "txt" => "text/plain",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

fn file_address(path: &Path) -> String {
    #[cfg(windows)]
    {
        let path = path.to_string_lossy();
        let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
        format!("file:///{}", path.replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        format!("file://{}", path.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chartulary::{Addressed, FacetId};
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn scan_keeps_file_bytes_out_of_graph_state() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("field.knot"), "# Field note\n").unwrap();
        let source = DirectorySource::open(temp.path()).unwrap();
        let document = source.documents().next().unwrap();

        assert!(document.container.body.is_none());
        assert!(document.container.content.is_none());
        assert_eq!(
            document.container.primary_address().unwrap().scheme(),
            Some("file")
        );
        assert_eq!(
            document.container.media_type.as_deref(),
            Some("text/vnd.knot")
        );
    }

    #[test]
    fn rename_preserves_identity_and_host_facets() {
        let temp = tempdir().unwrap();
        let before = temp.path().join("before.md");
        let after = temp.path().join("after.md");
        fs::write(&before, "same bytes").unwrap();
        let mut source = DirectorySource::open(temp.path()).unwrap();
        let id = source.documents().next().unwrap().id.clone();
        source
            .facets_mut()
            .set(
                id.clone(),
                FacetId::new("knot.test-pin"),
                json!({"x": 4}),
                &chartulary::AcceptAll,
            )
            .unwrap();

        fs::rename(&before, &after).unwrap();
        assert!(source.refresh().unwrap());

        let renamed = source.documents().next().unwrap();
        assert_eq!(renamed.id, id);
        assert_eq!(renamed.path, fs::canonicalize(after).unwrap());
        assert_eq!(
            source.facets().get(&id, &FacetId::new("knot.test-pin")),
            Some(&json!({"x": 4}))
        );
    }

    #[test]
    fn ignore_policy_is_configurable() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join(".private.md"), "hidden").unwrap();
        fs::write(temp.path().join("visible.md"), "visible").unwrap();
        let default_source = DirectorySource::open(temp.path()).unwrap();
        assert_eq!(default_source.documents().count(), 1);

        let all = DirectorySource::with_ignore(temp.path(), IgnorePolicy::none()).unwrap();
        assert_eq!(all.documents().count(), 2);
    }
}
