use typst::{Library, syntax::FileId};
use typst_kit::files::{FileStore, FsRoot};
use typst_utils::LazyHash;

const ENTRYPOINT: &str = "main.typ";

pub struct World {
    library: LazyHash<Library>,
    files: FileStore<Files>,
}

struct Files {
    main: FileId,
    project: FsRoot,
}
