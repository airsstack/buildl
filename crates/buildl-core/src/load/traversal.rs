//! The traversal: which build files are evaluated, and what their staged values become.
//!
//! Its own file because it is the whole of Load's logic: a breadth-first walk from the workspace
//! root along `subdir` requests, a conversion of every staged declaration into a validated one,
//! and one sort that makes the result independent of the order anything was evaluated or declared
//! in.
//!
//! Responsibilities: [`load`], and the private steps it is made of.
//!
//! Non-responsibilities: evaluating a build file, which the
//! [`DeclarationSource`] port does; and checking one declaration
//! against another, which the phase that builds the graph does.

use std::cmp::Ordering;
use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;

use crate::error::{ActionFound, DeclarationField, Error, Result};
use crate::ports::DeclarationSource;
use crate::types::{
    Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
    Directory, EntryName, Evaluated, Label, OutputName, Provenance, Rule, Setting, SourcePath,
    StagedAlias, StagedDeclaration, StagedItem, StagedRule, StagedSetting, StagedSubdir,
    StagedTarget, Target, TargetName, Written,
};

/// Evaluates the workspace's build files and returns their declarations, validated and sorted.
///
/// The walk starts at the workspace root and follows `subdir` requests breadth-first, evaluating
/// each directory once. Nothing is discovered from the filesystem: the evaluated build files are
/// exactly the root's and those a `subdir` call reaches. The result is sorted by declaring
/// directory, then declared name, then the whole declaration, so it does not depend on the order
/// anything was evaluated or declared in.
///
/// # Errors
///
/// Returns the first error met, one of:
///
/// - whatever `source` returns for a build file it cannot evaluate;
/// - [`Error::MissingBuildFile`] when the root or a requested directory holds no build file;
/// - [`Error::InvalidDeclaration`] when a staged value fails its grammar, including a `subdir`
///   that would leave the workspace;
/// - [`Error::ActionConflict`] when a target names both or neither of `rule` and `run`.
pub fn load<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>> {
    let mut queue = VecDeque::from([(Directory::root(), None)]);
    let mut seen = BTreeSet::from([Directory::root()]);
    let mut declarations = Vec::new();

    while let Some((directory, requested_by)) = queue.pop_front() {
        let file = build_file(&directory, entry);
        let staged = match source.evaluate(&file)? {
            Evaluated::Staged(staged) => staged,
            Evaluated::Absent => {
                return Err(Error::MissingBuildFile {
                    directory,
                    requested_by,
                });
            }
        };
        let provenance = file.provenance();
        for next in resolve_subdirs(&staged.subdirs, provenance)? {
            if seen.insert(next.clone()) {
                queue.push_back((next, Some(provenance.clone())));
            }
        }
        for declaration in staged.declarations {
            declarations.push(convert(declaration, provenance)?);
        }
    }

    sort(&mut declarations);
    Ok(declarations)
}

/// The build file of `directory`: the entry name, joined under the directory.
fn build_file(directory: &Directory, entry: &EntryName) -> BuildFile {
    let path = if directory.is_root() {
        PathBuf::from(entry.as_str())
    } else {
        PathBuf::from(format!("{directory}/{entry}"))
    };
    BuildFile::new(Provenance::new(path, directory.clone()))
}

/// Where a staged value was written: the build file, and the call's position in it.
struct Site<'a> {
    provenance: &'a Provenance,
    order: DeclarationOrder,
}

impl Site<'_> {
    /// The directory the build file is evaluated in.
    const fn directory(&self) -> &Directory {
        self.provenance.directory()
    }

    /// Wraps a grammar failure of `field` with this site.
    fn invalid(&self, field: DeclarationField) -> impl FnOnce(Error) -> Error + '_ {
        move |source| Error::InvalidDeclaration {
            provenance: self.provenance.clone(),
            order: self.order,
            field,
            source: Box::new(source),
        }
    }

    /// A target at this site found `found` instead of exactly one action.
    fn conflict(&self, found: ActionFound) -> Error {
        Error::ActionConflict {
            provenance: self.provenance.clone(),
            order: self.order,
            found,
        }
    }
}

/// Resolves a file's `subdir` requests under its directory, sorted and without repeats.
///
/// Sorting here makes the breadth-first order independent of the order the requests were made in.
fn resolve_subdirs(subdirs: &[StagedSubdir], provenance: &Provenance) -> Result<Vec<Directory>> {
    let mut resolved = Vec::with_capacity(subdirs.len());
    for subdir in subdirs {
        let site = Site {
            provenance,
            order: subdir.order,
        };
        let directory = subdir
            .path
            .under(site.directory())
            .map_err(site.invalid(DeclarationField::Subdir))?;
        resolved.push(directory);
    }
    resolved.sort();
    resolved.dedup();
    Ok(resolved)
}

/// Converts one staged declaration into a validated one.
fn convert(staged: StagedDeclaration, provenance: &Provenance) -> Result<Declaration> {
    let site = Site {
        provenance,
        order: staged.order,
    };
    let item = match staged.item {
        StagedItem::Target(target) => Declared::Target(convert_target(target, &site)?),
        StagedItem::Rule(rule) => Declared::Rule(convert_rule(rule, &site)?),
        StagedItem::Alias(alias) => Declared::Alias(convert_alias(&alias, &site)?),
        StagedItem::Setting(setting) => Declared::Setting(convert_setting(&setting, &site)?),
    };
    Ok(Declaration::new(provenance.clone(), item))
}

/// The label a declared name gets: always in the declaring directory.
fn declared_label(name: &Written<TargetName>, site: &Site<'_>) -> Result<Label> {
    let name = name.parse().map_err(site.invalid(DeclarationField::Name))?;
    Ok(Label::new(site.directory().clone(), name))
}

/// A `run` list, every argument validated and the list non-empty.
fn convert_command(run: &[Written<Argument>], site: &Site<'_>) -> Result<Command> {
    let mut arguments = Vec::with_capacity(run.len());
    for argument in run {
        arguments.push(
            argument
                .parse()
                .map_err(site.invalid(DeclarationField::Run))?,
        );
    }
    Command::new(arguments).map_err(site.invalid(DeclarationField::Run))
}

/// An input, joined under the declaring directory and validated as a workspace path.
fn convert_input(input: &Written<SourcePath>, site: &Site<'_>) -> Result<SourcePath> {
    let directory = site.directory();
    let joined = if directory.is_root() {
        input.as_written().to_owned()
    } else {
        format!("{directory}/{}", input.as_written())
    };
    SourcePath::parse(joined).map_err(site.invalid(DeclarationField::Input))
}

fn convert_target(staged: StagedTarget, site: &Site<'_>) -> Result<Target> {
    let label = declared_label(&staged.name, site)?;
    let action = match (staged.rule, staged.run) {
        (Some(rule), None) => Action::UseRule(
            rule.resolve(site.directory())
                .map_err(site.invalid(DeclarationField::Rule))?,
        ),
        (None, Some(run)) => Action::Run(convert_command(&run, site)?),
        (Some(_), Some(_)) => return Err(site.conflict(ActionFound::Both)),
        (None, None) => return Err(site.conflict(ActionFound::Neither)),
    };
    let mut inputs = Vec::with_capacity(staged.inputs.len());
    for input in &staged.inputs {
        inputs.push(convert_input(input, site)?);
    }
    let mut deps = Vec::with_capacity(staged.deps.len());
    for dep in &staged.deps {
        deps.push(
            dep.resolve(site.directory())
                .map_err(site.invalid(DeclarationField::Dep))?,
        );
    }
    let outputs = match staged.outputs {
        Some(written) => {
            let mut outputs = Vec::with_capacity(written.len());
            for output in &written {
                outputs.push(
                    output
                        .parse()
                        .map_err(site.invalid(DeclarationField::Output))?,
                );
            }
            outputs
        }
        None => vec![
            OutputName::parse(label.name().as_str())
                .map_err(site.invalid(DeclarationField::Output))?,
        ],
    };
    let mut env = Vec::with_capacity(staged.env.len());
    for name in &staged.env {
        env.push(name.parse().map_err(site.invalid(DeclarationField::Env))?);
    }
    Ok(Target {
        label,
        role: staged.role,
        action,
        inputs,
        deps,
        outputs,
        env,
        network: staged.network,
        freshness: staged.freshness,
    })
}

fn convert_rule(staged: StagedRule, site: &Site<'_>) -> Result<Rule> {
    let label = declared_label(&staged.name, site)?;
    let run = convert_command(&staged.run, site)?;
    let description = match staged.description {
        Some(description) => Some(
            description
                .parse()
                .map_err(site.invalid(DeclarationField::Description))?,
        ),
        None => None,
    };
    Ok(Rule {
        label,
        run,
        description,
    })
}

fn convert_alias(staged: &StagedAlias, site: &Site<'_>) -> Result<Alias> {
    let label = declared_label(&staged.name, site)?;
    let target = staged
        .target
        .resolve(site.directory())
        .map_err(site.invalid(DeclarationField::Target))?;
    Ok(Alias { label, target })
}

fn convert_setting(staged: &StagedSetting, site: &Site<'_>) -> Result<Setting> {
    let name = staged
        .name
        .parse()
        .map_err(site.invalid(DeclarationField::SettingName))?;
    let default = staged
        .default
        .parse()
        .map_err(site.invalid(DeclarationField::SettingValue))?;
    Ok(Setting { name, default })
}

/// The order Load returns declarations in: declaring directory, then declared name as bytes,
/// then the whole declaration.
///
/// The last key makes the order total without the order calls were made in, which a `pairs`
/// loop changes from one evaluation to the next.
pub(crate) fn declaration_order(a: &Declaration, b: &Declaration) -> Ordering {
    a.provenance()
        .directory()
        .cmp(b.provenance().directory())
        .then_with(|| a.item().name().cmp(b.item().name()))
        .then_with(|| a.cmp(b))
}

/// Sorts by [`declaration_order`].
fn sort(declarations: &mut [Declaration]) {
    declarations.sort_by(declaration_order);
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::{Path, PathBuf};

    use super::{build_file, convert, resolve_subdirs, sort};
    use crate::error::{ActionFound, DeclarationField, Error};
    use crate::types::{
        Action, Declaration, DeclarationOrder, Declared, Directory, EntryName, Freshness, Label,
        NetworkAccess, OutputName, Provenance, StagedAlias, StagedDeclaration, StagedItem,
        StagedRule, StagedSetting, StagedSubdir, StagedTarget, Target, TargetRole, Written,
    };

    fn file_in(directory: &str) -> Provenance {
        let directory = Directory::parse(directory).unwrap();
        let path = if directory.is_root() {
            PathBuf::from("build.lua")
        } else {
            PathBuf::from(format!("{directory}/build.lua"))
        };
        Provenance::new(path, directory)
    }

    fn staged_target(name: &str) -> StagedTarget {
        StagedTarget {
            name: Written::new(name),
            role: TargetRole::Build,
            rule: None,
            run: Some(vec![Written::new("cc")]),
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs: None,
            env: Vec::new(),
            network: NetworkAccess::Sealed,
            freshness: Freshness::Cached,
        }
    }

    fn declare(item: StagedItem) -> StagedDeclaration {
        StagedDeclaration {
            order: DeclarationOrder::new(4),
            item,
        }
    }

    fn convert_target_in(
        directory: &str,
        target: StagedTarget,
    ) -> crate::error::Result<Declaration> {
        convert(declare(StagedItem::Target(target)), &file_in(directory))
    }

    fn invalid_field(result: crate::error::Result<Declaration>) -> DeclarationField {
        match result.unwrap_err() {
            Error::InvalidDeclaration { field, order, .. } => {
                assert_eq!(order, DeclarationOrder::new(4));
                field
            }
            other => unreachable!("expected InvalidDeclaration, got {other:?}"),
        }
    }

    fn target_of(declaration: &Declaration) -> &Target {
        match declaration.item() {
            Declared::Target(target) => target,
            other => unreachable!("expected a target, got {other:?}"),
        }
    }

    #[test]
    fn the_build_file_is_the_entry_joined_under_its_directory() {
        let entry = EntryName::parse("build.lua").unwrap();
        let root = build_file(&Directory::root(), &entry);
        assert_eq!(root.file(), Path::new("build.lua"));
        assert!(root.directory().is_root());
        let lib = build_file(&Directory::parse("lib/text").unwrap(), &entry);
        assert_eq!(lib.file(), Path::new("lib/text/build.lua"));
        assert_eq!(lib.directory().as_str(), "lib/text");
    }

    #[test]
    fn subdirs_resolve_under_the_file_sorted_and_without_repeats() {
        let subdirs = ["zeta", "alpha", "zeta", "mid"].map(|raw| StagedSubdir {
            path: Written::new(raw),
            order: DeclarationOrder::new(0),
        });
        let resolved = resolve_subdirs(&subdirs, &file_in("lib")).unwrap();
        let rendered: Vec<&str> = resolved.iter().map(Directory::as_str).collect();
        assert_eq!(rendered, ["lib/alpha", "lib/mid", "lib/zeta"]);
    }

    #[test]
    fn a_subdir_that_leaves_the_workspace_is_an_invalid_declaration() {
        for raw in ["../x", "/x", ""] {
            let subdirs = [StagedSubdir {
                path: Written::new(raw),
                order: DeclarationOrder::new(7),
            }];
            match resolve_subdirs(&subdirs, &file_in("lib")).unwrap_err() {
                Error::InvalidDeclaration {
                    provenance,
                    order,
                    field,
                    ..
                } => {
                    assert_eq!(provenance, file_in("lib"));
                    assert_eq!(order, DeclarationOrder::new(7));
                    assert_eq!(field, DeclarationField::Subdir);
                }
                other => unreachable!("expected InvalidDeclaration, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_target_is_named_in_its_directory_and_resolves_every_dep_form() {
        let mut target = staged_target("app");
        target.deps = ["main.o", ":util.o", "//lib:text"]
            .map(Written::new)
            .to_vec();
        let declaration = convert_target_in("app", target).unwrap();
        assert_eq!(declaration.provenance(), &file_in("app"));
        let target = target_of(&declaration);
        assert_eq!(target.label.to_string(), "//app:app");
        let deps: Vec<String> = target.deps.iter().map(ToString::to_string).collect();
        assert_eq!(deps, ["//app:main.o", "//app:util.o", "//lib:text"]);
    }

    #[test]
    fn outputs_default_to_the_target_name() {
        let declaration = convert_target_in("app", staged_target("app")).unwrap();
        let outputs: Vec<&str> = target_of(&declaration)
            .outputs
            .iter()
            .map(OutputName::as_str)
            .collect();
        assert_eq!(outputs, ["app"]);
    }

    #[test]
    fn inputs_join_under_the_declaring_directory() {
        let mut target = staged_target("app");
        target.inputs = vec![Written::new("src/main.c")];
        let declaration = convert_target_in("app", target).unwrap();
        assert_eq!(target_of(&declaration).inputs[0].as_str(), "app/src/main.c");
        let mut at_root = staged_target("app");
        at_root.inputs = vec![Written::new("main.c")];
        let declaration = convert_target_in("", at_root).unwrap();
        assert_eq!(target_of(&declaration).inputs[0].as_str(), "main.c");
    }

    #[test]
    fn a_rule_reference_resolves_against_the_declaring_directory() {
        let mut target = staged_target("main.o");
        target.run = None;
        target.rule = Some(Written::new("cc"));
        let declaration = convert_target_in("app", target).unwrap();
        assert_eq!(
            target_of(&declaration).action,
            Action::UseRule(Label::parse("//app:cc").unwrap())
        );
    }

    #[test]
    fn a_target_needs_exactly_one_of_rule_and_run() {
        let mut both = staged_target("app");
        both.rule = Some(Written::new("cc"));
        let mut neither = staged_target("app");
        neither.run = None;
        for (target, expected) in [(both, ActionFound::Both), (neither, ActionFound::Neither)] {
            match convert_target_in("", target).unwrap_err() {
                Error::ActionConflict { found, order, .. } => {
                    assert_eq!(found, expected);
                    assert_eq!(order, DeclarationOrder::new(4));
                }
                other => unreachable!("expected ActionConflict, got {other:?}"),
            }
        }
    }

    #[test]
    fn an_empty_run_is_an_invalid_command() {
        let mut target = staged_target("app");
        target.run = Some(Vec::new());
        let err = convert_target_in("", target).unwrap_err();
        assert!(
            err.to_string()
                .ends_with(r#"invalid run: invalid command: "" — must hold at least one argument"#),
            "{err}"
        );
    }

    #[test]
    fn a_name_cannot_leave_its_directory() {
        let target = staged_target("//other:x");
        assert_eq!(
            invalid_field(convert_target_in("", target)),
            DeclarationField::Name
        );
    }

    #[test]
    fn each_invalid_target_field_is_named() {
        let mut dep = staged_target("app");
        dep.deps = vec![Written::new("//lib")];
        assert_eq!(
            invalid_field(convert_target_in("", dep)),
            DeclarationField::Dep
        );

        let mut input = staged_target("app");
        input.inputs = vec![Written::new("../x")];
        assert_eq!(
            invalid_field(convert_target_in("", input)),
            DeclarationField::Input
        );

        let mut output = staged_target("app");
        output.outputs = Some(vec![Written::new("/abs")]);
        assert_eq!(
            invalid_field(convert_target_in("", output)),
            DeclarationField::Output
        );

        let mut env = staged_target("app");
        env.env = vec![Written::new("1X")];
        assert_eq!(
            invalid_field(convert_target_in("", env)),
            DeclarationField::Env
        );

        let mut run = staged_target("app");
        run.run = Some(vec![Written::new("a\0b")]);
        assert_eq!(
            invalid_field(convert_target_in("", run)),
            DeclarationField::Run
        );

        let mut rule = staged_target("app");
        rule.run = None;
        rule.rule = Some(Written::new("//tools"));
        assert_eq!(
            invalid_field(convert_target_in("", rule)),
            DeclarationField::Rule
        );
    }

    #[test]
    fn rules_aliases_and_settings_convert_and_name_their_invalid_fields() {
        let file = file_in("");
        let rule = StagedRule {
            name: Written::new("cc"),
            run: vec![Written::new("cc"), Written::new("$in")],
            description: Some(Written::new("compile $in")),
        };
        let converted = convert(declare(StagedItem::Rule(rule.clone())), &file).unwrap();
        assert_eq!(converted.item().name(), "cc");

        let mut bad_description = rule;
        bad_description.description = Some(Written::new("two\nlines"));
        assert_eq!(
            invalid_field(convert(declare(StagedItem::Rule(bad_description)), &file)),
            DeclarationField::Description
        );

        let alias = StagedAlias {
            name: Written::new("default"),
            target: Written::new("app"),
        };
        let converted = convert(declare(StagedItem::Alias(alias)), &file).unwrap();
        match converted.item() {
            Declared::Alias(alias) => assert_eq!(alias.target.to_string(), "//:app"),
            other => unreachable!("expected an alias, got {other:?}"),
        }
        let bad_alias = StagedAlias {
            name: Written::new("default"),
            target: Written::new("a/b"),
        };
        assert_eq!(
            invalid_field(convert(declare(StagedItem::Alias(bad_alias)), &file)),
            DeclarationField::Target
        );

        let setting = StagedSetting {
            name: Written::new("test_filter"),
            default: Written::new(""),
        };
        assert!(convert(declare(StagedItem::Setting(setting)), &file).is_ok());
        let bad_name = StagedSetting {
            name: Written::new("a.b"),
            default: Written::new(""),
        };
        assert_eq!(
            invalid_field(convert(declare(StagedItem::Setting(bad_name)), &file)),
            DeclarationField::SettingName
        );
        let bad_value = StagedSetting {
            name: Written::new("x"),
            default: Written::new("a\0b"),
        };
        assert_eq!(
            invalid_field(convert(declare(StagedItem::Setting(bad_value)), &file)),
            DeclarationField::SettingValue
        );
    }

    #[test]
    fn sorting_orders_by_directory_then_name_then_value() {
        let declare_in = |directory: &str, name: &str| {
            convert_target_in(directory, staged_target(name)).unwrap()
        };
        let mut declarations = vec![
            declare_in("lib", "a"),
            declare_in("", "zeta"),
            declare_in("", "alpha"),
            declare_in("app", "m"),
        ];
        sort(&mut declarations);
        let rendered: Vec<String> = declarations
            .iter()
            .map(|d| target_of(d).label.to_string())
            .collect();
        assert_eq!(rendered, ["//:alpha", "//:zeta", "//app:m", "//lib:a"]);
    }

    #[test]
    fn declarations_sharing_a_directory_and_name_sort_by_content_in_either_order() {
        let tool = |program: &str| {
            let mut target = staged_target("a");
            target.run = Some(vec![Written::new(program)]);
            convert_target_in("lib", target).unwrap()
        };
        let sorted = |mut declarations: Vec<Declaration>| {
            sort(&mut declarations);
            declarations
        };
        let forward = sorted(vec![tool("ar"), tool("cc")]);
        let backward = sorted(vec![tool("cc"), tool("ar")]);
        assert_eq!(forward, backward);
    }
}
