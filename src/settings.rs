use lazy_static::lazy_static;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(feature = "redactions")]
use crate::{
    content::Content,
    redaction::{ContentPath, Redaction, Selector},
};

lazy_static! {
    static ref DEFAULT_SETTINGS: Arc<ActualSettings> = Arc::new(ActualSettings {
        sort_maps: false,
        snapshot_path: "snapshots".into(),
        #[cfg(feature = "redactions")]
        redactions: Redactions::default(),
    });
}
thread_local!(static CURRENT_SETTINGS: RefCell<Settings> = RefCell::new(Settings::new()));

/// Represents stored redactions.
#[cfg(feature = "redactions")]
#[derive(Clone, Default)]
pub struct Redactions(Vec<(Selector<'static>, Redaction)>);

#[cfg(feature = "redactions")]
impl<'a> From<Vec<(&'a str, Content)>> for Redactions {
    fn from(value: Vec<(&'a str, Content)>) -> Redactions {
        Redactions(
            value
                .into_iter()
                .map(|x| {
                    (
                        Selector::parse(x.0).unwrap().make_static(),
                        Redaction::Static(x.1),
                    )
                })
                .collect(),
        )
    }
}

#[derive(Clone)]
#[doc(hidden)]
pub struct ActualSettings {
    pub sort_maps: bool,
    pub snapshot_path: PathBuf,
    #[cfg(feature = "redactions")]
    pub redactions: Redactions,
}

/// Configures how insta operates at test time.
///
/// Settings are always bound to a thread and some default settings
/// are always available.  Settings can be either temporarily bound
/// of permanently.
///
/// This can be used to influence how the snapshot macros operate.
/// For instance it can be useful to force ordering of maps when
/// unordered structures are used through settings.
///
/// Settings can also be configured with the `with_settings!` macro.
///
/// Example:
///
/// ```rust,ignore
/// use insta;
///
/// let mut settings = insta::Settings::new();
/// settings.set_sort_maps(true);
/// settings.bind(|| {
///     insta::assert_snapshot!(...);
/// });
/// ```
#[derive(Clone)]
pub struct Settings {
    inner: Arc<ActualSettings>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            inner: DEFAULT_SETTINGS.clone(),
        }
    }
}

impl Settings {
    /// Returns the default settings.
    pub fn new() -> Settings {
        Settings::default()
    }

    /// Internal helper for macros
    #[doc(hidden)]
    pub fn _private_inner_mut(&mut self) -> &mut ActualSettings {
        Arc::make_mut(&mut self.inner)
    }

    /// Enables forceful sorting of maps before serialization.
    ///
    /// Note that this only applies to snapshots that undergo serialization
    /// (eg: does not work for `assert_debug_snapshot!`.)
    ///
    /// The default value is `false`.
    pub fn set_sort_maps(&mut self, value: bool) {
        self._private_inner_mut().sort_maps = value;
    }

    /// Returns the current value for map sorting.
    pub fn sort_maps(&self) -> bool {
        self.inner.sort_maps
    }

    /// Registers redactions that should be applied.
    ///
    /// This can be useful if redactions must be shared across multiple
    /// snapshots.
    ///
    /// Note that this only applies to snapshots that undergo serialization
    /// (eg: does not work for `assert_debug_snapshot!`.)
    #[cfg(feature = "redactions")]
    pub fn add_redaction<I: Into<Content>>(&mut self, selector: &str, replacement: I) {
        self._private_inner_mut().redactions.0.push((
            Selector::parse(selector).unwrap().make_static(),
            Redaction::Static(replacement.into()),
        ));
    }

    /// Registers a replacement callback.
    ///
    /// This works similar to a redaction but instead of changing the value it
    /// asserts the value at a certain place.  This function is internally
    /// supposed to call things like `assert_eq!`.
    ///
    /// Example:
    ///
    /// ```rust
    /// # use insta::Settings;
    /// # let mut settings = Settings::new();
    /// settings.add_dynamic_redaction(".id", |value, path| {
    ///     assert_eq!(path.to_string(), ".id");
    ///     assert_eq!(
    ///         value
    ///             .as_str()
    ///             .unwrap()
    ///             .chars()
    ///             .filter(|&c| c == '-')
    ///             .count(),
    ///         4
    ///     );
    ///     "[uuid]"
    /// });
    /// ```
    #[cfg(feature = "redactions")]
    pub fn add_dynamic_redaction<I, F>(&mut self, selector: &str, func: F)
    where
        I: Into<Content>,
        F: Fn(Content, ContentPath<'_>) -> I + Send + Sync + 'static,
    {
        self._private_inner_mut().redactions.0.push((
            Selector::parse(selector).unwrap().make_static(),
            Redaction::Replacement(Arc::new(Box::new(move |c, p| func(c, p).into()))),
        ));
    }

    /// Registers an assertion callback.
    ///
    /// This works similar to a redaction but instead of changing the value it
    /// asserts the value at a certain place.  This function is internally
    /// supposed to call things like `assert_eq!`.
    ///
    /// Example:
    ///
    /// ```rust
    /// # use insta::Settings;
    /// # let mut settings = Settings::new();
    /// settings.add_assertion(".id", |value, path| {
    ///     assert_eq!(path.to_string(), ".id");
    ///     assert_eq!(
    ///         value
    ///             .as_str()
    ///             .unwrap()
    ///             .chars()
    ///             .filter(|&c| c == '-')
    ///             .count(),
    ///         4
    ///     );
    /// });
    /// ```
    #[cfg(feature = "redactions")]
    pub fn add_assertion<F>(&mut self, selector: &str, func: F)
    where
        F: Fn(&Content, ContentPath<'_>) + Send + Sync + 'static,
    {
        self._private_inner_mut().redactions.0.push((
            Selector::parse(selector).unwrap().make_static(),
            Redaction::Assertion(Arc::new(Box::new(func))),
        ));
    }

    /// Replaces the currently set redactions.
    ///
    /// The default set is empty.
    #[cfg(feature = "redactions")]
    pub fn set_redactions<R: Into<Redactions>>(&mut self, redactions: R) {
        self._private_inner_mut().redactions = redactions.into();
    }

    /// Removes all redactions.
    #[cfg(feature = "redactions")]
    pub fn clear_redactions(&mut self) {
        self._private_inner_mut().redactions.0.clear();
    }

    /// Iterate over the redactions.
    #[cfg(feature = "redactions")]
    pub(crate) fn iter_redactions(&self) -> impl Iterator<Item = (&Selector, &Redaction)> {
        self.inner.redactions.0.iter().map(|&(ref a, ref b)| (a, b))
    }

    /// Sets the snapshot path.
    ///
    /// If not absolute it's relative to where the test is in.
    ///
    /// Defaults to `snapshots`.
    pub fn set_snapshot_path<P: AsRef<Path>>(&mut self, path: P) {
        self._private_inner_mut().snapshot_path = path.as_ref().to_path_buf();
    }

    /// Returns the snapshot path.
    pub fn snapshot_path(&self) -> &Path {
        &self.inner.snapshot_path
    }

    /// Runs a function with the current settings bound to the thread.
    pub fn bind<F: FnOnce()>(&self, f: F) {
        CURRENT_SETTINGS.with(|x| {
            let old = {
                let mut current = x.borrow_mut();
                let old = current.inner.clone();
                current.inner = self.inner.clone();
                old
            };
            f();
            let mut current = x.borrow_mut();
            current.inner = old;
        })
    }

    /// Binds the settings to the current thread permanently.
    pub fn bind_to_thread(&self) {
        CURRENT_SETTINGS.with(|x| {
            x.borrow_mut().inner = self.inner.clone();
        })
    }

    /// Runs a function with the current settings.
    pub(crate) fn with<R, F: FnOnce(&Settings) -> R>(f: F) -> R {
        CURRENT_SETTINGS.with(|x| f(&*x.borrow()))
    }
}
