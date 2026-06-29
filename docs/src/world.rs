use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

use az::SaturatingAs;
use typst::{
    Features, Library, LibraryExt, World,
    diag::{FileError, FileResult, StrResult, eco_format},
    ecow::EcoString,
    foundations::{
        Bytes, Datetime, Duration, IntoValue, Label, Module, NativeElement, Scope, ShowFn, Target,
        array, elem, func,
    },
    introspection::MetadataElem,
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    visualize::ImageElem,
};
use typst_html::{HtmlAttrs, HtmlElem, attr, tag};
use typst_kit::{
    datetime::Time,
    diagnostics::DiagnosticWorld,
    files::{FileLoader, FileStore, FsRoot},
};
use typst_utils::{LazyHash, PicoStr};

use crate::Config;
use crate::execution::compile_code_cell;

pub static PROJECT_ROOT: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

const ENTRYPOINT: &str = "main.typ";

fn docs_root() -> PathBuf {
    // When running via `cargo run -p docs`, CARGO_MANIFEST_DIR points to docs/
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(manifest_dir);
    }
    // Fallback: current directory
    PathBuf::from(".")
}

pub static FONTS: LazyLock<(LazyHash<FontBook>, Vec<Font>)> = LazyLock::new(|| {
    let fonts: Vec<_> = typst_assets::fonts()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect();
    let book = FontBook::from_fonts(&fonts);
    (LazyHash::new(book), fonts)
});

pub struct DocWorld {
    library: LazyHash<Library>,
    files: FileStore<DocsFiles>,
    now: Time,
}

impl DocWorld {
    pub fn new(config: &Config) -> Self {
        let root = docs_root();
        let entrypoint = root.join(ENTRYPOINT);

        if let Some(ref input) = config.input {
            if let Ok(abs_input) = input.canonicalize()
                && let Some(parent) = abs_input.parent()
            {
                *PROJECT_ROOT.write().unwrap() = Some(parent.to_path_buf());
            }
        } else if let Ok(abs_input) = entrypoint.canonicalize()
            && let Some(parent) = abs_input.parent()
        {
            *PROJECT_ROOT.write().unwrap() = Some(parent.to_path_buf());
        }

        Self {
            library: LazyHash::new(library()),
            files: FileStore::new(DocsFiles::new(config.input.as_deref())),
            now: Time::system(),
        }
    }

    pub fn dependencies(&mut self) -> impl Iterator<Item = PathBuf> + '_ {
        let (loader, deps) = self.files.dependencies();
        deps.filter_map(|id| loader.resolve(id).ok())
    }

    pub fn reset(&mut self) {
        self.files.reset();
        self.now.reset();
    }
}

impl World for DocWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.0
    }

    fn main(&self) -> FileId {
        self.files.loader().main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.1.get(index).cloned()
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.now.today(offset)
    }
}

impl DiagnosticWorld for DocWorld {
    fn name(&self, id: FileId) -> String {
        let vpath = id.vpath();
        match id.root() {
            VirtualRoot::Project => vpath.get_without_slash().into(),
            VirtualRoot::Package(package) => {
                format!("{package}{}", vpath.get_with_slash())
            }
        }
    }
}

struct DocsFiles {
    pub main: FileId,
    pub project: FsRoot,
}

impl DocsFiles {
    fn new(input: Option<&Path>) -> Self {
        let root = docs_root();
        let path = match input {
            Some(p) => p.canonicalize().unwrap(),
            None => root.join(ENTRYPOINT).canonicalize().unwrap(),
        };

        let project_root: PathBuf = path.parent().unwrap().into();

        let main = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::virtualize(&project_root, &path).unwrap(),
        )
        .intern();

        Self {
            main,
            project: FsRoot::new(project_root),
        }
    }

    fn resolve(&self, id: FileId) -> FileResult<PathBuf> {
        match id.root() {
            VirtualRoot::Project => Ok(self.project.resolve(id.vpath())),
            VirtualRoot::Package(spec) => Err(FileError::Other(Some(eco_format!(
                "packages not supported: {spec}"
            )))),
        }
    }
}

impl FileLoader for DocsFiles {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        match id.root() {
            VirtualRoot::Project => self.project.load(id.vpath()),
            VirtualRoot::Package(spec) => Err(FileError::Other(Some(eco_format!(
                "packages not supported in gaanim docs: {spec}"
            )))),
        }
    }
}

fn library() -> Library {
    let mut lib = Library::builder().with_features(Features::all()).build();
    let scope = lib.global.scope_mut();
    scope.define("stdx", stdx_module());
    lib.rules.replace(Target::Html, PATCHED_IMAGE_RULE);
    lib
}

fn stdx_module() -> Module {
    let mut scope = Scope::new();
    scope.define_elem::<ConfigElem>();
    scope.define_func::<compile_code_cell>();
    scope.define_func::<read_font>();
    Module::new("stdx", scope)
}

#[func]
fn read_font(post_script_name: EcoString) -> StrResult<Bytes> {
    Ok(FONTS
        .1
        .iter()
        .find(|font| font.post_script_name().as_deref() == Some(post_script_name.as_str()))
        .map(|font| font.data().clone())
        .ok_or("unknown font")?)
}

#[elem]
pub struct ConfigElem {
    pub content_base: EcoString,
    pub asset_base: EcoString,
}

const PATCHED_IMAGE_RULE: ShowFn<ImageElem> = |elem, engine, styles| {
    fn encode_hash(hash: u128) -> String {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        URL_SAFE_NO_PAD.encode(hash.to_be_bytes())
    }

    let image = elem.decode(engine, styles)?;

    let web_image = typst_svg::WebImage::new(&image);
    let hash = typst_utils::hash128(&web_image.data);
    let base = styles.get_ref(ConfigElem::asset_base).as_ref();
    let path = eco_format!(
        "{base}images/{}.{}",
        encode_hash(hash),
        web_image.format.extension()
    );
    let label = Label::new(PicoStr::intern("metadata-asset")).unwrap();

    let meta = MetadataElem::new(array![path.clone(), web_image.data].into_value())
        .pack()
        .labelled(label);

    let mut attrs = HtmlAttrs::new();
    attrs.push(attr::src, path);

    if let Some(alt) = elem.alt.get_cloned(styles) {
        attrs.push(attr::alt, alt);
    }

    let cast = |v: f64| eco_format!("{}", v.round().saturating_as::<i64>());
    attrs.push(attr::width, cast(image.width()));
    attrs.push(attr::height, cast(image.height()));

    let img = HtmlElem::new(tag::img).with_attrs(attrs).pack();
    Ok(meta + img)
};
