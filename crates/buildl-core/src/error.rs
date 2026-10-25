//! The crate's one error type and the `Result` alias every fallible call returns.
//!
//! A single file rather than one error type per module: a caller branches on a field, never on
//! the text of a message, and every module in this crate returns the same `Result` without a
//! conversion at the boundary.
//!
//! Responsibilities:
//!
//! - [`Error`] — every way a call into this crate can fail.
//! - [`NameKind`] — which domain name an [`Error::InvalidName`] is about.
//! - [`EvaluationFailure`] and [`EvaluationLimit`] — how an adapter failed to evaluate a build
//!   file.
//! - [`DeclarationField`] — which field of a declaration an [`Error::InvalidDeclaration`] is
//!   about.
//! - [`ActionFound`] — what an [`Error::ActionConflict`] found instead of one action.
//! - [`DeclaredKind`] — what a label an [`Error::WrongReferenceKind`] is about was declared as.
//! - [`DeclarationSite`] — a declared label, its build file and whose code declared it: the
//!   declaration holding a bad reference, one target on a dependency cycle, one declaration of a
//!   duplicated label, or the target a test run named that is not a test.
//! - [`PortName`] — which port an [`Error::Adapter`] came through.
//! - [`TreeFault`] — why a list of files an [`Error::InvalidTree`] is about is not a tree.
//! - [`Result`] — the crate-wide alias.
//!
//! Non-responsibilities: presentation. [`Error`]'s `Display` renders one diagnostic, on one line
//! unless an adapter's own message spans several. An error is returned, and its caller shows it.

use core::fmt;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use crate::types::{
    Ceiling, Declaration, DeclarationOrder, DenialReason, Diagnostic, Directory, EnvName, EnvValue,
    FieldName, InputKind, Label, Node, Origin, OutputPath, PluginName, PluginViolation, Program,
    Provenance, RemoteUrl, SettingName, SourcePath, TargetRole, TreePath, Want, WantedSet, Written,
};

/// The result of any fallible call in this crate.
pub type Result<T> = core::result::Result<T, Error>;

/// Every way a call into this crate can fail.
///
/// Non-exhaustive: adding a variant is not a breaking change for downstream crates.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A domain name did not satisfy its grammar.
    #[error("invalid {kind}: {value:?} — {reason}")]
    InvalidName {
        /// Which kind of name was being built.
        kind: NameKind,
        /// The rejected input, as given.
        value: String,
        /// Why the grammar rejected it.
        reason: &'static str,
    },
    /// A remote cache is given a token but named by a `grpc://` URL, which sends it unencrypted.
    #[error("a token would be sent in clear text to {url}; use a grpcs:// URL")]
    InsecureToken {
        /// The remote cache's URL.
        url: RemoteUrl,
    },
    /// A declaration holds a value its kind of target does not take.
    #[error("{reason}")]
    NotAccepted {
        /// Why the declaration cannot hold the value.
        reason: &'static str,
    },
    /// A value could not be serialized as JSON at all.
    #[error("value could not be serialized as canonical JSON")]
    CanonicalJson {
        /// The underlying `serde_json` failure.
        #[source]
        source: serde_json::Error,
    },
    /// A floating-point number reached the canonical serializer.
    #[error("floating-point value at {path} cannot appear in canonical JSON")]
    FloatRejected {
        /// Where in the value the float was found, as a JSONPath-style location.
        path: String,
    },
    /// Bytes could not be read as a value: they are not JSON, or the JSON does not describe a
    /// valid value of the type asked for.
    #[error("canonical JSON could not be read: {source}")]
    UnreadableJson {
        /// The underlying `serde_json` failure, which names the line and column.
        #[source]
        source: serde_json::Error,
    },
    /// Bytes describe a valid value, but are not the bytes the canonical serializer writes for
    /// it.
    #[error("JSON is not in canonical form: it differs from it at byte {offset}")]
    NonCanonicalJson {
        /// The index of the first byte that differs from the canonical form of the value.
        offset: usize,
    },
    /// A file an adapter keeps a workspace's state in does not hold what the adapter writes
    /// there.
    ///
    /// Distinct from [`Error::Adapter`], which is a file that could not be read at all: this
    /// one was read, and its content is refused. `source` is the reading failure,
    /// [`Error::UnreadableJson`] or [`Error::NonCanonicalJson`].
    #[error("{} is not a file buildl wrote: {source}", .file.display())]
    CorruptState {
        /// The file, as the adapter opened it.
        file: PathBuf,
        /// Why its content was refused.
        #[source]
        source: Box<Self>,
    },
    /// An adapter could not evaluate a build file.
    #[error("{provenance}: {failure}: {diagnostic}")]
    Evaluation {
        /// The build file.
        provenance: Provenance,
        /// What kind of failure it was.
        failure: EvaluationFailure,
        /// The adapter's message, for display only.
        diagnostic: Diagnostic,
    },
    /// A directory that had to be evaluated holds no build file.
    #[error(
        "no build file in //{directory}{}",
        requested_by_suffix(.requested_by.as_ref())
    )]
    MissingBuildFile {
        /// The directory.
        directory: Directory,
        /// The build file whose `subdir` call requested it; `None` for the workspace root.
        requested_by: Option<Provenance>,
    },
    /// A value in a declaration did not satisfy its grammar.
    #[error(
        "{provenance}{}: declaration {order}: invalid {field}: {source}",
        plugin_suffix(.origin)
    )]
    InvalidDeclaration {
        /// The build file.
        provenance: Provenance,
        /// Whose code made the declaration.
        origin: Origin,
        /// The call's position in the file.
        order: DeclarationOrder,
        /// The field holding the value.
        field: DeclarationField,
        /// The grammar failure.
        #[source]
        source: Box<Self>,
    },
    /// A target named both or neither of `rule` and `run`.
    #[error(
        "{provenance}{}: declaration {order}: a target names {found}",
        plugin_suffix(.origin)
    )]
    ActionConflict {
        /// The build file.
        provenance: Provenance,
        /// Whose code made the declaration.
        origin: Origin,
        /// The call's position in the file.
        order: DeclarationOrder,
        /// What was found instead of exactly one.
        found: ActionFound,
    },
    /// Two evaluations of the same workspace declared different things.
    #[error("{provenance}: declarations differ between two evaluations")]
    Nondeterministic {
        /// The build file to fix: the one whose declaration the other evaluation lacks.
        provenance: Provenance,
        /// The first evaluation's declaration at the first difference; `None` past its end.
        first_run: Option<Box<Declaration>>,
        /// The second evaluation's declaration at the first difference; `None` past its end.
        second_run: Option<Box<Declaration>>,
    },
    /// The build files, evaluated with the results of the discovery targets, declared different
    /// discovery targets than they did without them.
    #[error(
        "{provenance}: discovery target {label} differs once discovery results are read; a \
         discovery target may not depend on a discovery result"
    )]
    DiscoveryChanged {
        /// The first discovery target, in declaration order, that one evaluation declared and
        /// the other did not or declared differently.
        label: Label,
        /// The build file that declares it.
        provenance: Provenance,
    },
    /// A label was declared more than once, as any mix of target, rule and alias.
    #[error("{label} is declared more than once: {}", declaring_sites(.sites))]
    DuplicateLabel {
        /// The label.
        label: Label,
        /// Every declaration of it, with its build file and whose code made it, in the
        /// declarations' own order.
        sites: Vec<DeclarationSite>,
    },
    /// A build setting was declared more than once.
    #[error("setting {name} is declared more than once: {}", sites_list(.sites))]
    DuplicateSetting {
        /// The setting.
        name: SettingName,
        /// The build file of every declaration of it, in the declarations' own order.
        sites: Vec<Provenance>,
    },
    /// A reference names a label nothing declares.
    #[error(
        "{}: {field} {reference} is not declared{}",
        .site,
        suggestion_suffix(.suggestion.as_ref())
    )]
    UnknownReference {
        /// The declaration holding the reference, and its build file.
        site: Box<DeclarationSite>,
        /// Which field holds it: a dependency, a rule reference or an alias's target.
        field: DeclarationField,
        /// The label that was named.
        reference: Label,
        /// The nearest declared label the field could have named, if one is near enough.
        suggestion: Option<Label>,
    },
    /// A reference names a declaration of a kind its field cannot name.
    #[error(
        "{}: {field} {reference} names {}, declared in {declared_in}",
        .site,
        .found.with_article()
    )]
    WrongReferenceKind {
        /// The declaration holding the reference, and its build file.
        site: Box<DeclarationSite>,
        /// Which field holds it: a dependency, a rule reference or an alias's target.
        field: DeclarationField,
        /// The label that was named.
        reference: Label,
        /// What that label was declared as.
        found: DeclaredKind,
        /// The build file that declared it.
        declared_in: Provenance,
    },
    /// A command references a build setting nothing declares.
    #[error(
        "{}: setting {name} is not declared{}",
        .site,
        suggestion_suffix(.suggestion.as_ref())
    )]
    UnknownSetting {
        /// The target or rule holding the reference in its command, or the rule holding it in
        /// its description, and its build file.
        site: Box<DeclarationSite>,
        /// The name after `$opt:`, as written.
        name: String,
        /// The nearest declared setting, if one is near enough.
        suggestion: Option<SettingName>,
    },
    /// The targets' dependencies form a cycle.
    #[error("dependency cycle: {}", cycle_path(.path))]
    DependencyCycle {
        /// The targets on the cycle, lowest label first; each depends on the next, and the last
        /// on the first.
        path: Vec<DeclarationSite>,
    },
    /// The workspace declares more targets than the graph has ids for.
    #[error("{count} targets are more than a graph can number")]
    TooManyTargets {
        /// How many targets were declared.
        count: usize,
    },
    /// A build wants something its workspace's ceiling does not allow.
    #[error("{}", refusal(.want, .site.as_deref(), .ceiling))]
    Refused {
        /// What was wanted.
        want: Want,
        /// The lowest-labelled target wanting it, and its build file; `None` for a directory
        /// the build writes into, which the workspace names and no target declares.
        site: Option<Box<DeclarationSite>>,
        /// What the workspace allows.
        ceiling: Box<Ceiling>,
    },
    /// A plugin's target wants something its plugin does not allow.
    #[error("{}: {}", .violation.site, .violation)]
    PluginRefused {
        /// The want, the target that made it, and the plugin that does not allow it.
        violation: Box<PluginViolation>,
    },
    /// The approver did not approve what a build wants.
    #[error("{}", denial(.reason, .unapproved))]
    NotApproved {
        /// What was not approved.
        unapproved: Box<WantedSet>,
        /// Why, in words for the user.
        reason: DenialReason,
    },
    /// A command names a program that cannot be found.
    #[error(
        "{}: program {program} was not found",
        .site
    )]
    MissingTool {
        /// The lowest-labelled target whose command names the program, and its build file.
        site: Box<DeclarationSite>,
        /// The program.
        program: Program,
    },
    /// A target declares an environment variable the workspace's execution strategy sets itself.
    #[error(
        "{}: env {name} is set to {value} by the sandbox and cannot be declared",
        .site
    )]
    PinnedEnv {
        /// The variable.
        name: EnvName,
        /// The value the strategy gives it.
        value: EnvValue,
        /// The lowest-labelled target declaring the variable, and its build file.
        site: Box<DeclarationSite>,
    },
    /// A target reads a workspace file that does not exist.
    #[error(
        "{}: input {path} does not exist",
        .site
    )]
    MissingInput {
        /// The lowest-labelled target declaring the input, and its build file.
        site: Box<DeclarationSite>,
        /// The file.
        path: SourcePath,
    },
    /// A target reads a workspace path that holds something other than a regular file.
    #[error(
        "{}: input {path} is {found}; an input must be a regular file",
        .site
    )]
    InputNotAFile {
        /// The lowest-labelled target declaring the input, and its build file.
        site: Box<DeclarationSite>,
        /// The path.
        path: SourcePath,
        /// What stands there.
        found: InputKind,
    },
    /// A setting was given a value for this run, but nothing declares it.
    #[error("setting {name} is overridden but not declared")]
    UnknownOverride {
        /// The setting that was named.
        name: SettingName,
    },
    /// A run or a query was asked for a target, and no target or alias has that label.
    #[error(
        "target {label} was asked for but is not declared{}",
        suggestion_suffix(.suggestion.as_ref())
    )]
    UnknownTarget {
        /// The label that was asked for.
        label: Label,
        /// The nearest declared target or alias, if one is near enough.
        suggestion: Option<Label>,
    },
    /// A test run was asked for a target that is not a test.
    #[error(
        "target {asked} was asked for as a test, but {} is declared with {} in {}",
        .site.label,
        .declared_with.declaring_function(),
        .site.located()
    )]
    NotATest {
        /// The label that was asked for.
        asked: Label,
        /// The role the target it names was declared with, which says which function declared it.
        declared_with: TargetRole,
        /// The target it names, which is the same label unless an alias was followed, and its
        /// build file.
        site: Box<DeclarationSite>,
    },
    /// A query was written in neither of the two forms a query takes.
    #[error("query `{expression}` is not deps(<target>) or rdeps(<target>)")]
    InvalidQuery {
        /// The query as it was written.
        expression: String,
    },
    /// A log was asked for a target with no run in it.
    #[error(
        "target {label} has no run in the log{}{}",
        suggestion_suffix(.suggestion.as_ref()),
        damaged_suffix(.damaged)
    )]
    NoRun {
        /// The label that was asked for.
        label: Label,
        /// The nearest label the log has a run for, if one is near enough.
        suggestion: Option<Label>,
        /// Every damaged line of the log, by number, in order: one of them may have been this
        /// target's run.
        damaged: Vec<NonZeroUsize>,
    },
    /// What a target printed in its last logged run is no longer in the content store.
    #[error("target {label}: the output of its last logged run is no longer in the store")]
    MissingOutput {
        /// The target whose output is gone.
        label: Label,
    },
    /// A plan was handed over with a graph it was not made from.
    #[error("the plan was not made from this graph")]
    ForeignPlan,
    /// Uploading to the remote cache was asked for, and actions run bare.
    ///
    /// A bare action can read files it did not declare, so a result it produced is not known to
    /// depend only on its declared inputs, and no one else could trust it.
    #[error(
        "uploading to the remote cache needs actions to run in the input sandbox, \
         but strategy is \"bare\"; set strategy to \"sandbox\", or stop uploading by \
         unsetting BUILDL_REMOTE_UPLOAD or dropping --remote-upload"
    )]
    UploadUnsandboxed,
    /// Two outputs would be written to one place: the same path, or a path that another output
    /// needs as a directory.
    #[error(
        "{path}: {} ({}) and {} ({}) cannot both write here",
        .first.label,
        .first.located(),
        .second.label,
        .second.located()
    )]
    OutputConflict {
        /// The shorter of the two paths: the one both outputs need.
        path: OutputPath,
        /// The lower-labelled of the two targets, and its build file.
        first: Box<DeclarationSite>,
        /// The other target, and its build file; the same target when it declares both outputs.
        second: Box<DeclarationSite>,
    },
    /// A command names no program once its placeholders are expanded: it expanded to no
    /// argument, or its first argument is empty or still holds a `$`.
    #[error(
        "{}: the command names no program once expanded",
        .site
    )]
    NoProgram {
        /// The target whose command it is, and its build file.
        site: Box<DeclarationSite>,
    },
    /// The dispatcher returned success and left targets without an outcome: it lost a task, or
    /// the result of one, and with it everything that waited on that target.
    ///
    /// Not an [`Error::Adapter`]: that carries an adapter's own failure as text for display,
    /// while this names the targets left without an outcome, as data.
    #[error("{}", lost_by_the_dispatcher(.targets))]
    DispatcherFault {
        /// The targets without an outcome. Never empty; in label order.
        targets: Vec<Label>,
    },
    /// The dispatcher returned success and left build files unevaluated: it lost a task, or the
    /// result of one.
    ///
    /// The load's counterpart of [`Error::DispatcherFault`], naming directories, not targets.
    #[error("{}", lost_build_files(.directories))]
    LostBuildFiles {
        /// The directories whose build file was not evaluated. Never empty; in directory order.
        directories: Vec<Directory>,
    },
    /// An adapter could not do what a port asked of it.
    #[error("{port}: {diagnostic}")]
    Adapter {
        /// The port whose adapter failed.
        port: PortName,
        /// The adapter's message, for display only.
        diagnostic: Diagnostic,
    },
    /// A list of files is not a directory tree.
    #[error("not a tree: {fault}")]
    InvalidTree {
        /// What is wrong with the list.
        fault: TreeFault,
    },
    /// A plugin's directory holds something other than regular files with Unicode names.
    #[error(
        "plugin {plugin}: {} is not a regular file with a Unicode name; a plugin holds regular files only",
        .path.display()
    )]
    PluginTree {
        /// The plugin.
        plugin: PluginName,
        /// The first such path found.
        path: PathBuf,
    },
    /// A plugin `buildl.toml` names is not there to use: it is a directory that does not
    /// exist, a git plugin that `buildl.lock` does not pin as `buildl.toml` names it or whose
    /// store is missing, or a git revision that holds no file.
    #[error("plugin {plugin} is not fetched: {reason}")]
    PluginNotFetched {
        /// The plugin.
        plugin: PluginName,
        /// What is missing, and what to do about it.
        reason: String,
    },
    /// A plugin's files are not the ones `buildl.lock` pinned: the store was changed after the
    /// plugin was fetched.
    #[error("plugin {plugin} was changed after it was fetched: {reason}")]
    PluginChanged {
        /// The plugin.
        plugin: PluginName,
        /// How the files differ from the lock.
        reason: String,
    },
    /// `git` could not fetch a plugin: it is not installed, or it refused the repository, the
    /// revision or the checkout.
    #[error("plugin {plugin} could not be fetched: {diagnostic}")]
    PluginFetch {
        /// The plugin.
        plugin: PluginName,
        /// What `git` printed, or why it could not be run, for display only.
        diagnostic: Diagnostic,
    },
}

/// A lost load as one sentence naming the port at fault and the directories whose build file it
/// left unevaluated, the workspace root shown as `//`.
fn lost_build_files(directories: &[Directory]) -> String {
    let shown: Vec<String> = directories.iter().map(|d| format!("//{d}")).collect();
    format!(
        "{}: returned without evaluating the build files of {}",
        PortName::Dispatcher,
        shown.join(", ")
    )
}

/// A dispatcher fault as one sentence naming the port at fault and the targets it left without
/// an outcome.
fn lost_by_the_dispatcher(targets: &[Label]) -> String {
    let labels: Vec<String> = targets.iter().map(ToString::to_string).collect();
    format!(
        "{}: returned without an outcome for {}",
        PortName::Dispatcher,
        labels.join(", ")
    )
}

/// The suffix naming who required a missing build file.
fn requested_by_suffix(requested_by: Option<&Provenance>) -> String {
    requested_by.map_or_else(
        || " (the workspace root)".to_owned(),
        |provenance| format!(" (requested by {provenance})"),
    )
}

/// The build files of a duplicated name, separated by commas.
fn sites_list(sites: &[Provenance]) -> String {
    let rendered: Vec<String> = sites.iter().map(ToString::to_string).collect();
    rendered.join(", ")
}

/// The build file of every declaration of a duplicated label, separated by commas, each followed
/// by the plugin chain when a plugin made it.
fn declaring_sites(sites: &[DeclarationSite]) -> String {
    let rendered: Vec<String> = sites
        .iter()
        .map(|site| format!("{}{}", site.provenance, plugin_suffix(&site.origin)))
        .collect();
    rendered.join(", ")
}

/// ` (plugin <chain>)` when a plugin made the declaration; nothing for a build file.
fn plugin_suffix(origin: &Origin) -> String {
    match origin {
        Origin::BuildFile => String::new(),
        chain @ Origin::Plugins(_) => format!(" (plugin {chain})"),
    }
}

/// The suffix offering the nearest declared name, when there is one.
fn suggestion_suffix<T: fmt::Display>(suggestion: Option<&T>) -> String {
    suggestion.map_or_else(String::new, |nearest| format!(" (did you mean {nearest}?)"))
}

/// The suffix counting the damaged lines of a log, when there are any.
fn damaged_suffix(damaged: &[NonZeroUsize]) -> String {
    let numbers: Vec<String> = damaged.iter().map(ToString::to_string).collect();
    match numbers.as_slice() {
        [] => String::new(),
        [only] => format!("; 1 damaged line was skipped (line {only})"),
        many => format!(
            "; {} damaged lines were skipped (lines {})",
            many.len(),
            many.join(", ")
        ),
    }
}

/// A cycle as its labels and build files, closed by repeating the first label.
fn cycle_path(path: &[DeclarationSite]) -> String {
    let mut rendered: Vec<String> = path
        .iter()
        .map(|site| format!("{} ({})", site.label, site.located()))
        .collect();
    if let Some(first) = path.first() {
        rendered.push(first.label.to_string());
    }
    rendered.join(" -> ")
}

/// A denial as its reason, then the wants it covers on lines of their own; the reason alone when
/// the set is empty.
fn denial(reason: &DenialReason, unapproved: &WantedSet) -> String {
    if unapproved.is_empty() {
        reason.to_string()
    } else {
        format!("{reason}\n{unapproved}")
    }
}

/// A refusal as one sentence: who wanted what, and what the ceiling allows of that kind.
fn refusal(want: &Want, site: Option<&DeclarationSite>, ceiling: &Ceiling) -> String {
    let (wanted, list, allowed) = match want {
        Want::Program(program) => (
            format!("program {program}"),
            "proc.run",
            allowed_list(&ceiling.run),
        ),
        Want::Env(name) => (
            format!("environment variable {name}"),
            "env.read",
            allowed_list(&ceiling.env),
        ),
        Want::Read(path) => (
            format!("input {path}"),
            "fs.read",
            allowed_list(ceiling.read.iter().map(directory_text)),
        ),
        Want::Write(directory) => (
            format!("directory {}", directory_text(directory)),
            "fs.write",
            allowed_list(ceiling.write.iter().map(directory_text)),
        ),
        Want::Network(label) => (
            format!("network of {label}"),
            "network",
            "nothing".to_owned(),
        ),
    };
    let holder = site.map_or_else(String::new, |site| format!("{site}: "));
    format!("{holder}{wanted} is outside the ceiling ({list} allows: {allowed})")
}

/// The entries of one ceiling list, separated by commas; `nothing` for an empty list.
pub(crate) fn allowed_list<T: fmt::Display>(entries: impl IntoIterator<Item = T>) -> String {
    let rendered: Vec<String> = entries.into_iter().map(|entry| entry.to_string()).collect();
    if rendered.is_empty() {
        "nothing".to_owned()
    } else {
        rendered.join(", ")
    }
}

/// A directory as a refusal shows it: the workspace root, whose text is empty, as `.`.
pub(crate) fn directory_text(directory: &Directory) -> &str {
    if directory.is_root() {
        "."
    } else {
        directory.as_str()
    }
}

/// What a label was declared as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeclaredKind {
    /// A build or test target.
    Target,
    /// A rule.
    Rule,
    /// An alias.
    Alias,
}

impl DeclaredKind {
    /// The kind with its indefinite article, for a sentence.
    const fn with_article(self) -> &'static str {
        match self {
            Self::Target => "a target",
            Self::Rule => "a rule",
            Self::Alias => "an alias",
        }
    }
}

impl fmt::Display for DeclaredKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Target => "target",
            Self::Rule => "rule",
            Self::Alias => "alias",
        })
    }
}

/// A declared label, the build file that declared it, and whose code made the declaration.
///
/// Names the declaration an error is about: the one holding a bad reference, one target on a
/// dependency cycle, one declaration of a duplicated label, or the target a test run named that
/// is not a test. Displays as `<file>: <label>`, followed by `(plugin <chain>)` when a
/// plugin made the declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationSite {
    /// The declared label.
    pub label: Label,
    /// The build file that declared it.
    pub provenance: Provenance,
    /// Whose code made the declaration: the build file itself, or a plugin chain.
    pub origin: Origin,
}

impl DeclarationSite {
    /// The build file, then the plugin chain when a plugin made the declaration: `build.lua` or
    /// `build.lua, plugin ourgo@0.1.0 > go@1.2.0`.
    fn located(&self) -> String {
        match &self.origin {
            Origin::BuildFile => self.provenance.to_string(),
            chain @ Origin::Plugins(_) => format!("{}, plugin {chain}", self.provenance),
        }
    }
}

impl fmt::Display for DeclarationSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}{}",
            self.provenance,
            self.label,
            plugin_suffix(&self.origin)
        )
    }
}

impl From<&Node> for DeclarationSite {
    /// The site of a graph node: its label, the build file that declared it, and its origin.
    fn from(node: &Node) -> Self {
        Self {
            label: node.label.clone(),
            provenance: node.provenance.clone(),
            origin: node.origin.clone(),
        }
    }
}

/// Which port an [`Error::Adapter`] came through.
///
/// Non-exhaustive for the same reason [`Error`] is: a new port is not a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PortName {
    /// The workspace manifest.
    Manifest,
    /// The machine actions run on.
    Host,
    /// File metadata and content digests.
    Digester,
    /// The recorded digests of files already hashed.
    StatCache,
    /// The programs commands name.
    ToolResolver,
    /// The recorded results of earlier builds.
    ActionCache,
    /// The store of content by digest: outputs, and what actions printed.
    ContentStore,
    /// The build log.
    EventLog,
    /// The build log, read back.
    RunHistory,
    /// What runs an action.
    ExecStrategy,
    /// What hands actions to workers.
    Dispatcher,
    /// The record of the graph a build resolved.
    GraphRecord,
    /// What lists and removes what the content store holds.
    Sweep,
    /// What settles whether a build may run.
    Approver,
    /// Where plugins are fetched to and the lock that pins them.
    PluginStore,
    /// The cache of results shared between machines.
    RemoteCache,
}

impl fmt::Display for PortName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Manifest => "manifest",
            Self::Host => "host",
            Self::Digester => "digester",
            Self::StatCache => "stat cache",
            Self::ToolResolver => "tool resolver",
            Self::ActionCache => "action cache",
            Self::ContentStore => "content store",
            Self::EventLog => "event log",
            Self::RunHistory => "run history",
            Self::ExecStrategy => "exec strategy",
            Self::Dispatcher => "dispatcher",
            Self::GraphRecord => "graph record",
            Self::Sweep => "sweep",
            Self::Approver => "approver",
            Self::PluginStore => "plugin store",
            Self::RemoteCache => "remote cache",
        })
    }
}

/// A ceiling an evaluation can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvaluationLimit {
    /// The instruction ceiling.
    Instructions,
    /// The memory ceiling.
    Memory,
    /// The byte budget on what one file may stage: the in-memory size of each staged record plus
    /// the byte length of its text, up to the memory ceiling.
    Staging,
    /// The cap on directory entries the host-side source walk may visit for one build file.
    Walk,
}

impl fmt::Display for EvaluationLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Instructions => "instruction ceiling",
            Self::Memory => "memory ceiling",
            Self::Staging => "staging cap",
            Self::Walk => "walk cap",
        })
    }
}

/// How an adapter failed to evaluate one build file.
///
/// Closed for an adapter, which reports every failure through it without a new variant; open
/// for a caller matching on it, which keeps a wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvaluationFailure {
    /// The build file does not parse.
    Syntax,
    /// Evaluation raised an error.
    Runtime,
    /// The runtime refused an operation the declaration policy does not grant.
    Refused,
    /// A ceiling was reached.
    LimitReached {
        /// Which ceiling.
        limit: EvaluationLimit,
    },
    /// An option table holds a field its primitive does not accept.
    UnknownField {
        /// The field, as written.
        field: Written<FieldName>,
    },
    /// A field holds a value of the wrong type.
    WrongFieldType {
        /// The field, as written.
        field: Written<FieldName>,
    },
}

impl fmt::Display for EvaluationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax => f.write_str("syntax error"),
            Self::Runtime => f.write_str("runtime error"),
            Self::Refused => f.write_str("operation refused"),
            Self::LimitReached { limit } => write!(f, "{limit} reached"),
            Self::UnknownField { field } => write!(f, "unknown field {:?}", field.as_written()),
            Self::WrongFieldType { field } => {
                write!(f, "field {:?} has the wrong type", field.as_written())
            }
        }
    }
}

/// Which field of a declaration an [`Error::InvalidDeclaration`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeclarationField {
    /// A target's, rule's or alias's name.
    Name,
    /// An entry of `deps`.
    Dep,
    /// An entry of `inputs`.
    Input,
    /// An entry of `outputs`.
    Output,
    /// An entry of `env`.
    Env,
    /// The `run` command or one of its arguments.
    Run,
    /// The `rule` reference.
    Rule,
    /// A rule's `desc`.
    Description,
    /// An alias's target.
    Target,
    /// A setting's name.
    SettingName,
    /// A setting's default value.
    SettingValue,
    /// A target's `always` flag.
    Freshness,
    /// A `subdir` request.
    Subdir,
}

impl fmt::Display for DeclarationField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Name => "name",
            Self::Dep => "dep",
            Self::Input => "input",
            Self::Output => "output",
            Self::Env => "env",
            Self::Run => "run",
            Self::Rule => "rule",
            Self::Description => "description",
            Self::Target => "target",
            Self::SettingName => "setting name",
            Self::SettingValue => "setting value",
            Self::Freshness => "freshness",
            Self::Subdir => "subdir",
        })
    }
}

/// What an [`Error::ActionConflict`] found instead of exactly one of `rule` and `run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionFound {
    /// Both were given.
    Both,
    /// Neither was given.
    Neither,
}

impl fmt::Display for ActionFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Both => "both 'rule' and 'run'",
            Self::Neither => "neither 'rule' nor 'run'",
        })
    }
}

/// Which domain name an [`Error::InvalidName`] is about.
///
/// A type rather than a string so classification is read as a field. Non-exhaustive for the same
/// reason [`Error`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NameKind {
    /// A workspace-relative directory path.
    Directory,
    /// The name half of a label.
    TargetName,
    /// A whole label.
    Label,
    /// A content digest.
    Digest,
    /// A workspace-relative source file path.
    SourcePath,
    /// A path inside a target's output directory.
    OutputName,
    /// An environment variable name.
    EnvName,
    /// One element of a command's argument vector.
    Argument,
    /// A whole command: a non-empty argument vector.
    Command,
    /// A rule's one-line description.
    Description,
    /// A build setting's name.
    SettingName,
    /// A build setting's value.
    SettingValue,
    /// The file name every directory's build file has.
    EntryName,
    /// A field name in a declaration's option table.
    FieldName,
    /// The name of a program a command runs.
    Program,
    /// An environment variable's value.
    EnvValue,
    /// A host platform: an operating system and an architecture.
    Platform,
    /// How many actions a build runs at once.
    Workers,
    /// How long one action may run.
    Timeout,
    /// The execution strategy a workspace runs its actions under.
    Isolation,
    /// How a workspace settles whether a build may run.
    Approver,
    /// The path of a file inside a directory an action left as an output.
    TreePath,
    /// The name of a plugin.
    PluginName,
    /// The version text a plugin's manifest states.
    PluginVersion,
    /// The chain of plugins that declared a target.
    PluginChain,
    /// A full git commit id.
    CommitId,
    /// A git tag name.
    TagName,
    /// The URL of a git repository.
    GitUrl,
    /// The URL of a remote cache.
    RemoteUrl,
    /// The instance name a remote cache is addressed under.
    InstanceName,
    /// A path to a file the host reads, such as a certificate or a token.
    HostPath,
    /// The name of the request header a remote cache's token is sent in.
    HeaderName,
}

impl fmt::Display for NameKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Directory => "directory",
            Self::TargetName => "target name",
            Self::Label => "label",
            Self::Digest => "digest",
            Self::SourcePath => "source path",
            Self::OutputName => "output name",
            Self::EnvName => "environment variable name",
            Self::Argument => "argument",
            Self::Command => "command",
            Self::Description => "description",
            Self::SettingName => "setting name",
            Self::SettingValue => "setting value",
            Self::EntryName => "entry file name",
            Self::FieldName => "field name",
            Self::Program => "program",
            Self::EnvValue => "environment variable value",
            Self::Platform => "platform",
            Self::Workers => "worker count",
            Self::Timeout => "timeout",
            Self::Isolation => "execution strategy",
            Self::Approver => "approver mode",
            Self::TreePath => "tree path",
            Self::PluginName => "plugin name",
            Self::PluginVersion => "plugin version",
            Self::PluginChain => "plugin chain",
            Self::CommitId => "commit id",
            Self::TagName => "tag name",
            Self::GitUrl => "git URL",
            Self::RemoteUrl => "remote cache URL",
            Self::InstanceName => "instance name",
            Self::HostPath => "host path",
            Self::HeaderName => "header name",
        })
    }
}

/// Why a list of files an [`Error::InvalidTree`] is about is not a directory tree.
///
/// Non-exhaustive for the same reason [`Error`] is.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TreeFault {
    /// It holds no file.
    Empty,
    /// It lists this path twice.
    Repeated(TreePath),
    /// It lists a file at this path, and another file below it.
    Clash(TreePath),
}

impl fmt::Display for TreeFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("it holds no file"),
            Self::Repeated(path) => write!(f, "{path} is listed twice"),
            Self::Clash(path) => write!(f, "{path} is both a file and a directory"),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;
    use std::num::NonZeroUsize;
    use std::path::PathBuf;

    use super::{
        ActionFound, DeclarationField, DeclarationSite, DeclaredKind, Error, EvaluationFailure,
        EvaluationLimit, NameKind, PortName, TreeFault,
    };
    use crate::types::{
        Argument, Ceiling, Command, DeclarationOrder, DenialReason, Diagnostic, Directory, EnvName,
        EnvValue, FieldName, Freshness, InputKind, Label, NetworkAccess, Node, Origin, OutputName,
        OutputPath, PluginName, PluginRef, PluginVersion, PluginViolation, Program, Provenance,
        RemoteUrl, SettingName, SourcePath, TargetName, TargetRole, TreePath, Want, WantedSet,
        Written,
    };

    fn lib_file() -> Provenance {
        Provenance::new(
            PathBuf::from("lib/build.lua"),
            Directory::parse("lib").unwrap(),
        )
    }

    fn root_file() -> Provenance {
        Provenance::new(PathBuf::from("build.lua"), Directory::root())
    }

    fn tools_file() -> Provenance {
        Provenance::new(
            PathBuf::from("tools/build.lua"),
            Directory::parse("tools").unwrap(),
        )
    }

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    /// The declaration `raw`, declared in the root build file.
    fn root_site(raw: &str) -> DeclarationSite {
        DeclarationSite {
            label: label(raw),
            provenance: root_file(),
            origin: Origin::BuildFile,
        }
    }

    #[test]
    fn duplicate_label_names_every_site() {
        let err = Error::DuplicateLabel {
            label: label("//:a"),
            sites: vec![root_site("//:a"), root_site("//:a")],
        };
        assert_eq!(
            err.to_string(),
            "//:a is declared more than once: build.lua, build.lua"
        );
    }

    #[test]
    fn duplicate_label_names_the_plugin_chain_of_the_declaration_a_plugin_made() {
        let err = Error::DuplicateLabel {
            label: label("//:a"),
            sites: vec![
                root_site("//:a"),
                DeclarationSite {
                    origin: ourgo_chain(),
                    ..root_site("//:a")
                },
            ],
        };
        assert_eq!(
            err.to_string(),
            "//:a is declared more than once: build.lua, build.lua (plugin ourgo@0.1.0 > go@1.2.0)"
        );
    }

    #[test]
    fn duplicate_setting_names_every_site() {
        let err = Error::DuplicateSetting {
            name: SettingName::parse("test_filter").unwrap(),
            sites: vec![root_file(), lib_file()],
        };
        assert_eq!(
            err.to_string(),
            "setting test_filter is declared more than once: build.lua, lib/build.lua"
        );
    }

    #[test]
    fn unknown_reference_names_the_site_the_field_and_the_nearest_label() {
        let err = Error::UnknownReference {
            site: Box::new(root_site("//:app")),
            field: DeclarationField::Dep,
            reference: label("//:mian.o"),
            suggestion: Some(label("//:main.o")),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:app: dep //:mian.o is not declared (did you mean //:main.o?)"
        );
    }

    #[test]
    fn unknown_reference_without_a_near_label_offers_nothing() {
        let err = Error::UnknownReference {
            site: Box::new(root_site("//:default")),
            field: DeclarationField::Target,
            reference: label("//:nope"),
            suggestion: None,
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:default: target //:nope is not declared"
        );
    }

    #[test]
    fn wrong_reference_kind_names_what_was_found_and_where() {
        let err = Error::WrongReferenceKind {
            site: Box::new(root_site("//:app")),
            field: DeclarationField::Dep,
            reference: label("//:cc"),
            found: DeclaredKind::Rule,
            declared_in: tools_file(),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:app: dep //:cc names a rule, declared in tools/build.lua"
        );
    }

    #[test]
    fn every_declared_kind_renders_alone_and_in_a_sentence() {
        let cases = [
            (DeclaredKind::Target, "target", "names a target,"),
            (DeclaredKind::Rule, "rule", "names a rule,"),
            (DeclaredKind::Alias, "alias", "names an alias,"),
        ];
        for (found, alone, in_a_sentence) in cases {
            assert_eq!(found.to_string(), alone);
            let err = Error::WrongReferenceKind {
                site: Box::new(root_site("//:app")),
                field: DeclarationField::Rule,
                reference: label("//:x"),
                found,
                declared_in: root_file(),
            };
            assert!(err.to_string().contains(in_a_sentence), "{err}");
        }
    }

    #[test]
    fn unknown_setting_names_the_site_and_the_nearest_setting() {
        let err = Error::UnknownSetting {
            site: Box::new(root_site("//:go_test")),
            name: "test_fliter".to_owned(),
            suggestion: Some(SettingName::parse("test_filter").unwrap()),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:go_test: setting test_fliter is not declared \
             (did you mean test_filter?)"
        );
    }

    #[test]
    fn dependency_cycle_lists_every_target_and_closes_on_the_first() {
        let err = Error::DependencyCycle {
            path: vec![
                DeclarationSite {
                    label: label("//:a"),
                    provenance: root_file(),
                    origin: Origin::BuildFile,
                },
                DeclarationSite {
                    label: label("//lib:b"),
                    provenance: lib_file(),
                    origin: Origin::BuildFile,
                },
            ],
        };
        assert_eq!(
            err.to_string(),
            "dependency cycle: //:a (build.lua) -> //lib:b (lib/build.lua) -> //:a"
        );
    }

    #[test]
    fn a_cycle_of_one_names_its_target_twice() {
        let err = Error::DependencyCycle {
            path: vec![DeclarationSite {
                label: label("//:a"),
                provenance: root_file(),
                origin: Origin::BuildFile,
            }],
        };
        assert_eq!(
            err.to_string(),
            "dependency cycle: //:a (build.lua) -> //:a"
        );
    }

    #[test]
    fn too_many_targets_states_the_count() {
        let err = Error::TooManyTargets { count: 7 };
        assert_eq!(
            err.to_string(),
            "7 targets are more than a graph can number"
        );
    }

    #[test]
    fn a_site_is_a_nodes_label_and_its_build_file() {
        let node = Node {
            label: label("//lib:text"),
            provenance: lib_file(),
            origin: Origin::BuildFile,
            narrowing: Vec::new(),
            role: TargetRole::Build,
            run: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
            description: None,
            inputs: Vec::new(),
            outputs: vec![OutputName::parse("text").unwrap()],
            env: Vec::new(),
            network: NetworkAccess::Sealed,
            freshness: Freshness::Cached,
        };
        assert_eq!(
            DeclarationSite::from(&node),
            DeclarationSite {
                label: label("//lib:text"),
                provenance: lib_file(),
                origin: Origin::BuildFile,
            }
        );
    }

    /// A plugin chain `ourgo@0.1.0 > go@1.2.0`.
    fn ourgo_chain() -> Origin {
        let plugin = |name: &str, version: &str| PluginRef {
            name: PluginName::parse(name).unwrap(),
            version: PluginVersion::parse(version).unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        };
        Origin::plugins(vec![plugin("ourgo", "0.1.0"), plugin("go", "1.2.0")]).unwrap()
    }

    #[test]
    fn a_site_with_a_plugin_origin_names_the_chain() {
        let site = DeclarationSite {
            origin: ourgo_chain(),
            ..root_site("//:app")
        };
        assert_eq!(
            site.to_string(),
            "build.lua: //:app (plugin ourgo@0.1.0 > go@1.2.0)"
        );
        let missing = Error::MissingTool {
            site: Box::new(site.clone()),
            program: Program::parse("cc").unwrap(),
        };
        assert_eq!(
            missing.to_string(),
            "build.lua: //:app (plugin ourgo@0.1.0 > go@1.2.0): program cc was not found"
        );
        let cycle = Error::DependencyCycle { path: vec![site] };
        assert_eq!(
            cycle.to_string(),
            "dependency cycle: //:app (build.lua, plugin ourgo@0.1.0 > go@1.2.0) -> //:app"
        );
    }

    #[test]
    fn a_build_file_site_reads_as_today() {
        assert_eq!(root_site("//:app").to_string(), "build.lua: //:app");
        let err = Error::MissingTool {
            site: Box::new(root_site("//:app")),
            program: Program::parse("cc").unwrap(),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:app: program cc was not found"
        );
    }

    #[test]
    fn missing_tool_names_the_site_and_the_program() {
        let err = Error::MissingTool {
            site: Box::new(root_site("//:app")),
            program: Program::parse("cc").unwrap(),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:app: program cc was not found"
        );
    }

    #[test]
    fn pinned_env_renders_its_site_name_and_value() {
        let err = Error::PinnedEnv {
            name: EnvName::parse("TZ").unwrap(),
            value: EnvValue::parse("UTC").unwrap(),
            site: Box::new(root_site("//a:y")),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //a:y: env TZ is set to UTC by the sandbox and cannot be declared"
        );
    }

    #[test]
    fn missing_input_names_the_site_and_the_path() {
        let err = Error::MissingInput {
            site: Box::new(root_site("//:app")),
            path: SourcePath::parse("src/main.c").unwrap(),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:app: input src/main.c does not exist"
        );
    }

    #[test]
    fn an_input_that_is_not_a_file_names_the_site_the_path_and_what_was_found() {
        for (found, text) in [
            (
                InputKind::Directory,
                "build.lua: //:app: input src is a directory; an input must be a regular file",
            ),
            (
                InputKind::Symlink,
                "build.lua: //:app: input src is a symbolic link; an input must be a regular file",
            ),
            (
                InputKind::Special,
                "build.lua: //:app: input src is a pipe, a socket or a device; an input must be a regular file",
            ),
        ] {
            let err = Error::InputNotAFile {
                site: Box::new(root_site("//:app")),
                path: SourcePath::parse("src").unwrap(),
                found,
            };
            assert_eq!(err.to_string(), text);
        }
    }

    /// A ceiling allowing `cc` and `ld`, `HOME` and `PATH`, reads under `src`, and no write.
    fn ceiling() -> Ceiling {
        Ceiling {
            run: ["cc", "ld"]
                .into_iter()
                .map(|raw| Program::parse(raw).unwrap())
                .collect(),
            env: ["PATH", "HOME"]
                .into_iter()
                .map(|raw| EnvName::parse(raw).unwrap())
                .collect(),
            read: BTreeSet::from([Directory::parse("src").unwrap()]),
            write: BTreeSet::new(),
        }
    }

    #[test]
    fn not_approved_shows_its_reason_then_the_wants() {
        let err = Error::NotApproved {
            unapproved: Box::new(WantedSet {
                programs: BTreeSet::from([Program::parse("protoc").unwrap()]),
                env: BTreeSet::new(),
                write: BTreeSet::from([Directory::parse("out").unwrap()]),
                network: BTreeSet::new(),
            }),
            reason: DenialReason::new("not approved"),
        };
        assert_eq!(
            err.to_string(),
            "not approved\n  run programs: protoc\n  write into:   out"
        );
    }

    #[test]
    fn not_approved_with_nothing_unapproved_shows_only_its_reason() {
        let err = Error::NotApproved {
            unapproved: Box::new(WantedSet {
                programs: BTreeSet::new(),
                env: BTreeSet::new(),
                write: BTreeSet::new(),
                network: BTreeSet::new(),
            }),
            reason: DenialReason::new("not approved"),
        };
        assert_eq!(err.to_string(), "not approved");
    }

    #[test]
    fn a_refused_program_names_the_site_and_what_may_run() {
        let err = Error::Refused {
            want: Want::Program(Program::parse("docker").unwrap()),
            site: Some(Box::new(root_site("//:image"))),
            ceiling: Box::new(ceiling()),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:image: program docker is outside the ceiling (proc.run allows: cc, ld)"
        );
    }

    /// A violation of `go@1.3.0` by `//:app`, made by `ourgo@0.1.0 > go@1.3.0`, of `want`, with
    /// `allowed` listed.
    fn violation(want: Want, allowed: &[&str]) -> Error {
        let plugin = |name: &str, version: &str| PluginRef {
            name: PluginName::parse(name).unwrap(),
            version: PluginVersion::parse(version).unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        };
        let chain = vec![plugin("ourgo", "0.1.0"), plugin("go", "1.3.0")];
        Error::PluginRefused {
            violation: Box::new(PluginViolation {
                want,
                site: DeclarationSite {
                    label: label("//:app"),
                    provenance: root_file(),
                    origin: Origin::plugins(chain.clone()).unwrap(),
                },
                plugin: chain[1].clone(),
                allowed: allowed.iter().map(|text| (*text).to_owned()).collect(),
            }),
        }
    }

    #[test]
    fn a_plugin_tree_names_the_plugin_and_the_path() {
        let err = Error::PluginTree {
            plugin: PluginName::parse("go").unwrap(),
            path: PathBuf::from(".buildl/plugins/go/c0ffee/bin/link"),
        };
        assert_eq!(
            err.to_string(),
            "plugin go: .buildl/plugins/go/c0ffee/bin/link is not a regular file with a Unicode \
             name; a plugin holds regular files only"
        );
    }

    #[test]
    fn a_plugin_not_fetched_names_the_plugin_and_the_reason() {
        let err = Error::PluginNotFetched {
            plugin: PluginName::parse("go").unwrap(),
            reason: "run `buildl plugins fetch`".to_owned(),
        };
        assert_eq!(
            err.to_string(),
            "plugin go is not fetched: run `buildl plugins fetch`"
        );
    }

    #[test]
    fn a_plugin_changed_names_the_plugin_and_the_reason() {
        let err = Error::PluginChanged {
            plugin: PluginName::parse("go").unwrap(),
            reason: "its files differ from buildl.lock".to_owned(),
        };
        assert_eq!(
            err.to_string(),
            "plugin go was changed after it was fetched: its files differ from buildl.lock"
        );
    }

    #[test]
    fn a_plugin_fetch_names_the_plugin_and_what_git_said() {
        let err = Error::PluginFetch {
            plugin: PluginName::parse("go").unwrap(),
            diagnostic: Diagnostic::new("fatal: couldn't find remote ref refs/tags/v9"),
        };
        assert_eq!(
            err.to_string(),
            "plugin go could not be fetched: fatal: couldn't find remote ref refs/tags/v9"
        );
    }

    #[test]
    fn a_plugin_refused_program_names_the_site_the_plugin_and_what_it_allows() {
        let err = violation(Want::Program(Program::parse("curl").unwrap()), &["go"]);
        assert_eq!(
            err.to_string(),
            "build.lua: //:app (plugin ourgo@0.1.0 > go@1.3.0): program curl is outside plugin \
             go@1.3.0 (proc.run allows: go)"
        );
    }

    #[test]
    fn a_plugin_refused_variable_reads_like_a_refused_program() {
        let err = violation(Want::Env(EnvName::parse("HOME").unwrap()), &[]);
        assert_eq!(
            err.to_string(),
            "build.lua: //:app (plugin ourgo@0.1.0 > go@1.3.0): environment variable HOME is \
             outside plugin go@1.3.0 (env.read allows: nothing)"
        );
    }

    #[test]
    fn a_plugin_refused_network_says_the_network_is_not_allowed() {
        let err = violation(Want::Network(label("//:app")), &[]);
        assert_eq!(
            err.to_string(),
            "build.lua: //:app (plugin ourgo@0.1.0 > go@1.3.0): network is outside plugin \
             go@1.3.0 (network not allowed)"
        );
    }

    #[test]
    fn a_refused_variable_names_the_site_and_what_may_be_read() {
        let err = Error::Refused {
            want: Want::Env(EnvName::parse("AWS_SECRET").unwrap()),
            site: Some(Box::new(root_site("//:app"))),
            ceiling: Box::new(ceiling()),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:app: environment variable AWS_SECRET is outside the ceiling \
             (env.read allows: HOME, PATH)"
        );
    }

    #[test]
    fn a_refused_input_names_the_site_and_where_inputs_may_lie() {
        let err = Error::Refused {
            want: Want::Read(SourcePath::parse("lib/key.pem").unwrap()),
            site: Some(Box::new(DeclarationSite {
                label: label("//lib:text"),
                provenance: lib_file(),
                origin: Origin::BuildFile,
            })),
            ceiling: Box::new(ceiling()),
        };
        assert_eq!(
            err.to_string(),
            "lib/build.lua: //lib:text: input lib/key.pem is outside the ceiling \
             (fs.read allows: src)"
        );
    }

    #[test]
    fn a_refused_directory_has_no_site_and_an_empty_list_allows_nothing() {
        let err = Error::Refused {
            want: Want::Write(Directory::parse("out").unwrap()),
            site: None,
            ceiling: Box::new(ceiling()),
        };
        assert_eq!(
            err.to_string(),
            "directory out is outside the ceiling (fs.write allows: nothing)"
        );
    }

    #[test]
    fn a_refused_network_reads_as_network() {
        let err = Error::Refused {
            want: Want::Network(Label::parse("//:fetch").unwrap()),
            site: None,
            ceiling: Box::new(ceiling()),
        };
        assert_eq!(
            err.to_string(),
            "network of //:fetch is outside the ceiling (network allows: nothing)"
        );
    }

    #[test]
    fn the_workspace_root_renders_as_a_dot_in_a_refusal() {
        let mut allows_root = ceiling();
        allows_root.read = BTreeSet::from([Directory::root(), Directory::parse("src").unwrap()]);
        allows_root.write = BTreeSet::from([Directory::parse("out").unwrap()]);
        let read = Error::Refused {
            want: Want::Read(SourcePath::parse("lib/key.pem").unwrap()),
            site: Some(Box::new(root_site("//:app"))),
            ceiling: Box::new(allows_root.clone()),
        };
        assert_eq!(
            read.to_string(),
            "build.lua: //:app: input lib/key.pem is outside the ceiling (fs.read allows: ., src)"
        );
        let write = Error::Refused {
            want: Want::Write(Directory::root()),
            site: None,
            ceiling: Box::new(allows_root),
        };
        assert_eq!(
            write.to_string(),
            "directory . is outside the ceiling (fs.write allows: out)"
        );
    }

    #[test]
    fn unknown_override_names_the_setting() {
        let err = Error::UnknownOverride {
            name: SettingName::parse("test_fliter").unwrap(),
        };
        assert_eq!(
            err.to_string(),
            "setting test_fliter is overridden but not declared"
        );
    }

    #[test]
    fn unknown_target_names_the_label_and_the_nearest_one() {
        let err = Error::UnknownTarget {
            label: label("//:libs"),
            suggestion: Some(label("//:lib")),
        };
        assert_eq!(
            err.to_string(),
            "target //:libs was asked for but is not declared (did you mean //:lib?)"
        );
    }

    #[test]
    fn unknown_target_without_a_near_label_offers_nothing() {
        let err = Error::UnknownTarget {
            label: label("//:nope"),
            suggestion: None,
        };
        assert_eq!(
            err.to_string(),
            "target //:nope was asked for but is not declared"
        );
    }

    #[test]
    fn an_unsandboxed_upload_says_how_to_sandbox_or_stop_uploading() {
        assert_eq!(
            Error::UploadUnsandboxed.to_string(),
            "uploading to the remote cache needs actions to run in the input sandbox, \
             but strategy is \"bare\"; set strategy to \"sandbox\", or stop uploading by \
             unsetting BUILDL_REMOTE_UPLOAD or dropping --remote-upload"
        );
    }

    #[test]
    fn not_a_test_names_the_target_and_its_file() {
        let err = Error::NotATest {
            asked: label("//:default"),
            declared_with: TargetRole::Build,
            site: Box::new(root_site("//:app")),
        };
        assert_eq!(
            err.to_string(),
            "target //:default was asked for as a test, but //:app is declared with b.target in build.lua"
        );
    }

    #[test]
    fn not_a_test_names_the_function_that_declared_a_discovery_target() {
        let err = Error::NotATest {
            asked: label("//:pkgs"),
            declared_with: TargetRole::Discovery,
            site: Box::new(root_site("//:pkgs")),
        };
        assert_eq!(
            err.to_string(),
            "target //:pkgs was asked for as a test, but //:pkgs is declared with b.discover in build.lua"
        );
    }

    #[test]
    fn not_a_test_names_the_plugin_chain_of_a_target_a_plugin_declared() {
        let err = Error::NotATest {
            asked: label("//:default"),
            declared_with: TargetRole::Build,
            site: Box::new(DeclarationSite {
                origin: ourgo_chain(),
                ..root_site("//:app")
            }),
        };
        assert_eq!(
            err.to_string(),
            "target //:default was asked for as a test, but //:app is declared with b.target in \
             build.lua, plugin ourgo@0.1.0 > go@1.2.0"
        );
    }

    #[test]
    fn invalid_query_names_the_query_and_the_two_forms() {
        let err = Error::InvalidQuery {
            expression: "dep(//:a)".to_owned(),
        };
        assert_eq!(
            err.to_string(),
            "query `dep(//:a)` is not deps(<target>) or rdeps(<target>)"
        );
    }

    fn line(number: usize) -> NonZeroUsize {
        NonZeroUsize::new(number).unwrap()
    }

    #[test]
    fn no_run_names_the_target_and_a_suggestion() {
        let err = Error::NoRun {
            label: label("//:ap"),
            suggestion: Some(label("//:app")),
            damaged: Vec::new(),
        };
        assert_eq!(
            err.to_string(),
            "target //:ap has no run in the log (did you mean //:app?)"
        );
        let without = Error::NoRun {
            label: label("//:ap"),
            suggestion: None,
            damaged: Vec::new(),
        };
        assert_eq!(without.to_string(), "target //:ap has no run in the log");
    }

    #[test]
    fn no_run_names_the_damaged_lines() {
        let two = Error::NoRun {
            label: label("//:app"),
            suggestion: None,
            damaged: vec![line(14), line(90)],
        };
        assert_eq!(
            two.to_string(),
            "target //:app has no run in the log; 2 damaged lines were skipped (lines 14, 90)"
        );
        let one = Error::NoRun {
            label: label("//:app"),
            suggestion: None,
            damaged: vec![line(14)],
        };
        assert_eq!(
            one.to_string(),
            "target //:app has no run in the log; 1 damaged line was skipped (line 14)"
        );
    }

    #[test]
    fn missing_output_names_the_target() {
        let err = Error::MissingOutput {
            label: label("//:app"),
        };
        assert_eq!(
            err.to_string(),
            "target //:app: the output of its last logged run is no longer in the store"
        );
    }

    #[test]
    fn a_foreign_plan_says_the_plan_does_not_belong_to_the_graph() {
        assert_eq!(
            Error::ForeignPlan.to_string(),
            "the plan was not made from this graph"
        );
    }

    #[test]
    fn an_output_conflict_names_the_path_and_both_targets_with_their_files() {
        let err = Error::OutputConflict {
            path: OutputPath::of(
                &Directory::parse("out").unwrap(),
                &label("//:lib"),
                &OutputName::parse("lib").unwrap(),
            ),
            first: Box::new(root_site("//:lib")),
            second: Box::new(DeclarationSite {
                label: label("//lib:text"),
                provenance: lib_file(),
                origin: Origin::BuildFile,
            }),
        };
        assert_eq!(
            err.to_string(),
            "out/lib: //:lib (build.lua) and //lib:text (lib/build.lua) cannot both write here"
        );
    }

    #[test]
    fn an_output_conflict_names_the_plugin_chain_of_a_target_a_plugin_declared() {
        let err = Error::OutputConflict {
            path: OutputPath::of(
                &Directory::parse("out").unwrap(),
                &label("//:lib"),
                &OutputName::parse("lib").unwrap(),
            ),
            first: Box::new(DeclarationSite {
                origin: ourgo_chain(),
                ..root_site("//:lib")
            }),
            second: Box::new(root_site("//:other")),
        };
        assert_eq!(
            err.to_string(),
            "out/lib: //:lib (build.lua, plugin ourgo@0.1.0 > go@1.2.0) and //:other (build.lua) \
             cannot both write here"
        );
    }

    #[test]
    fn an_output_conflict_inside_one_target_names_that_target_twice() {
        let err = Error::OutputConflict {
            path: OutputPath::of(
                &Directory::parse("out").unwrap(),
                &label("//:app"),
                &OutputName::parse("app").unwrap(),
            ),
            first: Box::new(root_site("//:app")),
            second: Box::new(root_site("//:app")),
        };
        assert_eq!(
            err.to_string(),
            "out/app: //:app (build.lua) and //:app (build.lua) cannot both write here"
        );
    }

    #[test]
    fn no_program_names_the_site_of_the_command() {
        let err = Error::NoProgram {
            site: Box::new(root_site("//:app")),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:app: the command names no program once expanded"
        );
    }

    #[test]
    fn a_program_a_command_names_only_once_expanded_is_refused_like_any_other() {
        let err = Error::Refused {
            want: Want::Program(Program::parse("out/app").unwrap()),
            site: Some(Box::new(root_site("//:app_test"))),
            ceiling: Box::new(Ceiling {
                run: BTreeSet::from([Program::parse("cc").unwrap()]),
                env: BTreeSet::new(),
                read: BTreeSet::new(),
                write: BTreeSet::new(),
            }),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:app_test: program out/app is outside the ceiling (proc.run allows: cc)"
        );
    }

    #[test]
    fn adapter_names_the_port_before_the_adapters_own_message() {
        let err = Error::Adapter {
            port: PortName::StatCache,
            diagnostic: Diagnostic::new("statcache.json: permission denied"),
        };
        assert_eq!(
            err.to_string(),
            "stat cache: statcache.json: permission denied"
        );
    }

    #[test]
    fn a_dispatcher_fault_names_the_dispatcher_and_the_targets_left_without_an_outcome() {
        let lost = Error::DispatcherFault {
            targets: vec![label("//:a"), label("//lib:b")],
        };
        assert_eq!(
            lost.to_string(),
            "dispatcher: returned without an outcome for //:a, //lib:b"
        );
    }

    #[test]
    fn lost_build_files_names_each_directory() {
        let lost = Error::LostBuildFiles {
            directories: vec![Directory::root(), Directory::parse("lib").unwrap()],
        };
        assert_eq!(
            lost.to_string(),
            "dispatcher: returned without evaluating the build files of //, //lib"
        );
    }

    #[test]
    fn every_port_name_renders_a_human_phrase() {
        let cases = [
            (PortName::Manifest, "manifest"),
            (PortName::Host, "host"),
            (PortName::Digester, "digester"),
            (PortName::StatCache, "stat cache"),
            (PortName::ToolResolver, "tool resolver"),
            (PortName::ActionCache, "action cache"),
            (PortName::ContentStore, "content store"),
            (PortName::EventLog, "event log"),
            (PortName::RunHistory, "run history"),
            (PortName::ExecStrategy, "exec strategy"),
            (PortName::Dispatcher, "dispatcher"),
            (PortName::GraphRecord, "graph record"),
            (PortName::Sweep, "sweep"),
            (PortName::Approver, "approver"),
            (PortName::PluginStore, "plugin store"),
            (PortName::RemoteCache, "remote cache"),
        ];
        for (port, rendered) in cases {
            assert_eq!(port.to_string(), rendered);
        }
    }

    #[test]
    fn the_remote_cache_port_is_named_in_errors() {
        let err = Error::Adapter {
            port: PortName::RemoteCache,
            diagnostic: Diagnostic::new("connection refused"),
        };
        assert_eq!(PortName::RemoteCache.to_string(), "remote cache");
        assert_eq!(err.to_string(), "remote cache: connection refused");
    }

    #[test]
    fn name_kind_renders_a_human_phrase() {
        assert_eq!(NameKind::Directory.to_string(), "directory");
        assert_eq!(NameKind::TargetName.to_string(), "target name");
        assert_eq!(NameKind::Label.to_string(), "label");
        assert_eq!(NameKind::Digest.to_string(), "digest");
        assert_eq!(NameKind::SourcePath.to_string(), "source path");
        assert_eq!(NameKind::OutputName.to_string(), "output name");
        assert_eq!(NameKind::EnvName.to_string(), "environment variable name");
        assert_eq!(NameKind::Argument.to_string(), "argument");
        assert_eq!(NameKind::Command.to_string(), "command");
        assert_eq!(NameKind::Description.to_string(), "description");
        assert_eq!(NameKind::SettingName.to_string(), "setting name");
        assert_eq!(NameKind::SettingValue.to_string(), "setting value");
        assert_eq!(NameKind::EntryName.to_string(), "entry file name");
        assert_eq!(NameKind::FieldName.to_string(), "field name");
        assert_eq!(NameKind::Program.to_string(), "program");
        assert_eq!(NameKind::EnvValue.to_string(), "environment variable value");
        assert_eq!(NameKind::Platform.to_string(), "platform");
        assert_eq!(NameKind::Workers.to_string(), "worker count");
        assert_eq!(NameKind::Timeout.to_string(), "timeout");
        assert_eq!(NameKind::Isolation.to_string(), "execution strategy");
        assert_eq!(NameKind::Approver.to_string(), "approver mode");
        assert_eq!(NameKind::TreePath.to_string(), "tree path");
        assert_eq!(NameKind::PluginName.to_string(), "plugin name");
        assert_eq!(NameKind::PluginVersion.to_string(), "plugin version");
        assert_eq!(NameKind::PluginChain.to_string(), "plugin chain");
        assert_eq!(NameKind::RemoteUrl.to_string(), "remote cache URL");
        assert_eq!(NameKind::InstanceName.to_string(), "instance name");
        assert_eq!(NameKind::HostPath.to_string(), "host path");
        assert_eq!(NameKind::HeaderName.to_string(), "header name");
    }

    #[test]
    fn an_insecure_token_names_the_url_and_the_fix() {
        let url = RemoteUrl::parse("grpc://cache:50051").unwrap();
        assert_eq!(
            Error::InsecureToken { url }.to_string(),
            "a token would be sent in clear text to grpc://cache:50051; use a grpcs:// URL"
        );
    }

    #[test]
    fn invalid_name_states_what_was_found() {
        let err = Error::InvalidName {
            kind: NameKind::Label,
            value: "//lib".to_owned(),
            reason: "must hold ':' and a target name",
        };
        assert_eq!(
            err.to_string(),
            r#"invalid label: "//lib" — must hold ':' and a target name"#
        );
    }

    #[test]
    fn canonical_json_states_the_value_could_not_be_serialized() {
        let Err(source) = serde_json::to_value(u128::MAX) else {
            unreachable!("u128::MAX has no JSON number representation")
        };
        let err = Error::CanonicalJson { source };
        assert_eq!(
            err.to_string(),
            "value could not be serialized as canonical JSON"
        );
    }

    #[test]
    fn float_rejected_states_where_the_float_was_found() {
        let err = Error::FloatRejected {
            path: "$.timings.elapsed".to_owned(),
        };
        assert_eq!(
            err.to_string(),
            "floating-point value at $.timings.elapsed cannot appear in canonical JSON"
        );
    }

    #[test]
    fn unreadable_json_states_what_could_not_be_read() {
        let Err(source) = serde_json::from_slice::<u32>(b"[]") else {
            unreachable!("an array is not a number")
        };
        let err = Error::UnreadableJson { source };
        assert_eq!(
            err.to_string(),
            "canonical JSON could not be read: invalid type: sequence, expected u32 at line 1 \
             column 0"
        );
    }

    #[test]
    fn non_canonical_json_states_where_the_bytes_differ() {
        let err = Error::NonCanonicalJson { offset: 36 };
        assert_eq!(
            err.to_string(),
            "JSON is not in canonical form: it differs from it at byte 36"
        );
    }

    #[test]
    fn corrupt_state_names_the_file_and_keeps_the_reading_failure() {
        let err = Error::CorruptState {
            file: PathBuf::from("/ws/.buildl/cache.json"),
            source: Box::new(Error::NonCanonicalJson { offset: 36 }),
        };
        assert_eq!(
            err.to_string(),
            "/ws/.buildl/cache.json is not a file buildl wrote: JSON is not in canonical form: \
             it differs from it at byte 36"
        );
        assert_eq!(
            std::error::Error::source(&err).unwrap().to_string(),
            "JSON is not in canonical form: it differs from it at byte 36"
        );
        let Error::CorruptState { source, .. } = err else {
            unreachable!("built as CorruptState above")
        };
        assert!(matches!(*source, Error::NonCanonicalJson { offset: 36 }));
    }

    #[test]
    fn evaluation_names_the_file_the_failure_and_the_message() {
        let err = Error::Evaluation {
            provenance: lib_file(),
            failure: EvaluationFailure::Syntax,
            diagnostic: Diagnostic::new("unexpected symbol near '}'"),
        };
        assert_eq!(
            err.to_string(),
            "lib/build.lua: syntax error: unexpected symbol near '}'"
        );
    }

    #[test]
    fn every_evaluation_failure_renders_a_human_phrase() {
        let field = || Written::<FieldName>::new("dep");
        let cases = [
            (EvaluationFailure::Syntax, "syntax error"),
            (EvaluationFailure::Runtime, "runtime error"),
            (EvaluationFailure::Refused, "operation refused"),
            (
                EvaluationFailure::LimitReached {
                    limit: EvaluationLimit::Instructions,
                },
                "instruction ceiling reached",
            ),
            (
                EvaluationFailure::LimitReached {
                    limit: EvaluationLimit::Memory,
                },
                "memory ceiling reached",
            ),
            (
                EvaluationFailure::LimitReached {
                    limit: EvaluationLimit::Staging,
                },
                "staging cap reached",
            ),
            (
                EvaluationFailure::LimitReached {
                    limit: EvaluationLimit::Walk,
                },
                "walk cap reached",
            ),
            (
                EvaluationFailure::UnknownField { field: field() },
                r#"unknown field "dep""#,
            ),
            (
                EvaluationFailure::WrongFieldType { field: field() },
                r#"field "dep" has the wrong type"#,
            ),
        ];
        for (failure, rendered) in cases {
            assert_eq!(failure.to_string(), rendered);
        }
    }

    #[test]
    fn missing_build_file_names_the_root_or_the_requester() {
        let root = Error::MissingBuildFile {
            directory: Directory::root(),
            requested_by: None,
        };
        assert_eq!(root.to_string(), "no build file in // (the workspace root)");
        let requested = Error::MissingBuildFile {
            directory: Directory::parse("lib/text").unwrap(),
            requested_by: Some(lib_file()),
        };
        assert_eq!(
            requested.to_string(),
            "no build file in //lib/text (requested by lib/build.lua)"
        );
    }

    #[test]
    fn invalid_declaration_names_the_site_the_field_and_the_grammar() {
        let source = TargetName::parse("a/b").unwrap_err();
        let source_text = source.to_string();
        let err = Error::InvalidDeclaration {
            provenance: lib_file(),
            origin: Origin::BuildFile,
            order: DeclarationOrder::new(2),
            field: DeclarationField::Dep,
            source: Box::new(source),
        };
        assert_eq!(
            err.to_string(),
            r#"lib/build.lua: declaration #2: invalid dep: invalid target name: "a/b" — may hold only ASCII letters, digits, '.', '-' and '_'"#
        );
        assert_eq!(
            std::error::Error::source(&err).unwrap().to_string(),
            source_text
        );
    }

    #[test]
    fn not_accepted_states_why_the_declaration_cannot_hold_the_value() {
        let err = Error::NotAccepted {
            reason: "a discovery target is always cached",
        };
        assert_eq!(err.to_string(), "a discovery target is always cached");
    }

    #[test]
    fn invalid_declaration_names_the_plugin_chain_of_a_declaration_a_plugin_made() {
        let err = Error::InvalidDeclaration {
            provenance: lib_file(),
            origin: ourgo_chain(),
            order: DeclarationOrder::new(2),
            field: DeclarationField::Dep,
            source: Box::new(TargetName::parse("a/b").unwrap_err()),
        };
        assert!(
            err.to_string().starts_with(
                "lib/build.lua (plugin ourgo@0.1.0 > go@1.2.0): declaration #2: invalid dep: "
            ),
            "{err}"
        );
    }

    #[test]
    fn every_declaration_field_renders_a_human_phrase() {
        let cases = [
            (DeclarationField::Name, "name"),
            (DeclarationField::Dep, "dep"),
            (DeclarationField::Input, "input"),
            (DeclarationField::Output, "output"),
            (DeclarationField::Env, "env"),
            (DeclarationField::Run, "run"),
            (DeclarationField::Rule, "rule"),
            (DeclarationField::Description, "description"),
            (DeclarationField::Target, "target"),
            (DeclarationField::SettingName, "setting name"),
            (DeclarationField::SettingValue, "setting value"),
            (DeclarationField::Freshness, "freshness"),
            (DeclarationField::Subdir, "subdir"),
        ];
        for (field, rendered) in cases {
            assert_eq!(field.to_string(), rendered);
        }
    }

    #[test]
    fn action_conflict_states_what_was_found() {
        let both = Error::ActionConflict {
            provenance: lib_file(),
            origin: Origin::BuildFile,
            order: DeclarationOrder::new(0),
            found: ActionFound::Both,
        };
        assert_eq!(
            both.to_string(),
            "lib/build.lua: declaration #0: a target names both 'rule' and 'run'"
        );
        let neither = Error::ActionConflict {
            provenance: lib_file(),
            origin: Origin::BuildFile,
            order: DeclarationOrder::new(1),
            found: ActionFound::Neither,
        };
        assert_eq!(
            neither.to_string(),
            "lib/build.lua: declaration #1: a target names neither 'rule' nor 'run'"
        );
    }

    #[test]
    fn action_conflict_names_the_plugin_chain_of_a_declaration_a_plugin_made() {
        let err = Error::ActionConflict {
            provenance: lib_file(),
            origin: ourgo_chain(),
            order: DeclarationOrder::new(0),
            found: ActionFound::Both,
        };
        assert_eq!(
            err.to_string(),
            "lib/build.lua (plugin ourgo@0.1.0 > go@1.2.0): declaration #0: a target names both \
             'rule' and 'run'"
        );
    }

    #[test]
    fn nondeterministic_names_the_file_to_fix() {
        let err = Error::Nondeterministic {
            provenance: lib_file(),
            first_run: None,
            second_run: None,
        };
        assert_eq!(
            err.to_string(),
            "lib/build.lua: declarations differ between two evaluations"
        );
    }

    #[test]
    fn discovery_changed_names_the_target_and_its_file() {
        let err = Error::DiscoveryChanged {
            label: Label::parse("//lib:pkgs").unwrap(),
            provenance: lib_file(),
        };
        assert_eq!(
            err.to_string(),
            "lib/build.lua: discovery target //lib:pkgs differs once discovery results are \
             read; a discovery target may not depend on a discovery result"
        );
    }

    #[test]
    fn an_invalid_tree_says_what_is_wrong_with_the_list() {
        let path = TreePath::parse("assets/app.js").unwrap();
        for (fault, rendered) in [
            (TreeFault::Empty, "not a tree: it holds no file"),
            (
                TreeFault::Repeated(path.clone()),
                "not a tree: assets/app.js is listed twice",
            ),
            (
                TreeFault::Clash(path),
                "not a tree: assets/app.js is both a file and a directory",
            ),
        ] {
            assert_eq!(Error::InvalidTree { fault }.to_string(), rendered);
        }
    }
}
