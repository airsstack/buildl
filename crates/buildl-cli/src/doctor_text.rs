//! The text `doctor` prints: the workspace, its approver, its run settings, its ceiling, one row
//! per want saying whether the ceiling allows it and whether it is approved, each row followed
//! by the plugins behind it, and every want a plugin does not allow.
//!
//! Its own file because it is the one command whose output is a report laid out in columns,
//! read by a person deciding what to allow.
//!
//! Responsibilities: [`doctor_text`].
//!
//! Non-responsibilities: computing the report, which the library does; and writing it, which
//! the command does.

use core::fmt::Display;
use std::collections::BTreeSet;

use buildl::Doctor;
use buildl::buildl_core::{Attribution, Directory, Execution, Want, WantedSet, Workers};

/// The spaces after the longest cell of a column.
const GAP: usize = 2;

/// The text of `doctor`'s report, ending in a newline.
///
/// Every path is relative to the workspace root but the root itself. Each column of the wants
/// table is as wide as its longest cell, its head included, and two spaces, so the text is the
/// same on every run.
///
/// Under a want that plugins' targets are behind, one line for each chain reads
/// `    declared by plugin go@1.2.0 for //services/api:server`. When a plugin does not allow a want
/// of its targets, a `plugin violations` section ends the report with one line for each, worded
/// as the refusal of a build would word it; the command still succeeds.
pub(crate) fn doctor_text(doctor: &Doctor) -> String {
    let checkup = doctor.checkup();
    let workspace = checkup.workspace();
    let ceiling = &workspace.ceiling;
    let mut lines = vec![
        format!("workspace  {}", doctor.root().display()),
        format!("approver   {}", workspace.approver),
        format!("execution  {}", execution(&workspace.execution)),
        "ceiling".to_owned(),
    ];
    let entries = [
        ("proc.run", joined(&ceiling.run)),
        ("env.read", joined(&ceiling.env)),
        ("fs.read", directories(&ceiling.read)),
        ("fs.write", directories(&ceiling.write)),
    ];
    let width = column_width(entries.iter().map(|(key, _)| *key));
    lines.extend(
        entries
            .iter()
            .map(|(key, value)| format!("  {key:<width$}{value}")),
    );

    let wants = wants(
        checkup.wanted(),
        checkup.outside(),
        doctor.unapproved(),
        checkup.attribution(),
    );
    let rows: Vec<Row> = wants.iter().map(|(row, _)| row.clone()).collect();
    let head = [
        "wants".to_owned(),
        "ceiling".to_owned(),
        "approved".to_owned(),
    ];
    let first = column_width(column(&head, &rows, 0));
    let second = column_width(column(&head, &rows, 1));
    let table =
        |[want, allowed, approved]: &Row| format!("{want:<first$}{allowed:<second$}{approved}");
    lines.push(table(&head));
    for (row, attributed) in &wants {
        lines.push(table(row));
        lines.extend(attributed.iter().map(|line| format!("    {line}")));
    }
    if !checkup.plugin_violations().is_empty() {
        lines.push("plugin violations".to_owned());
        lines.extend(
            checkup
                .plugin_violations()
                .iter()
                .map(|violation| format!("  {}: {violation}", violation.site)),
        );
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// One row of the wants table: the want, what the ceiling says of it, and whether it is
/// approved.
type Row = [String; 3];

/// A row of the wants table and the lines saying which plugins are behind its want.
type Wanted = (Row, Vec<String>);

/// The rows of the wants table, in the order of `wanted`: programs, variables, directories,
/// then the network, each with its attribution lines.
fn wants(
    wanted: &WantedSet,
    outside: &WantedSet,
    unapproved: &WantedSet,
    attribution: &Attribution,
) -> Vec<Wanted> {
    let approved = |no: bool| if no { "no" } else { "yes" };
    let row = |want: String, refused: bool, no: bool| {
        if refused {
            [want, "REFUSED".to_owned(), "-".to_owned()]
        } else {
            [want, "allowed".to_owned(), approved(no).to_owned()]
        }
    };
    let mut rows = Vec::new();
    for program in &wanted.programs {
        rows.push((
            row(
                format!("  run {program}"),
                outside.programs.contains(program),
                unapproved.programs.contains(program),
            ),
            attribution.lines(&Want::Program(program.clone())),
        ));
    }
    for name in &wanted.env {
        rows.push((
            row(
                format!("  read {name}"),
                outside.env.contains(name),
                unapproved.env.contains(name),
            ),
            attribution.lines(&Want::Env(name.clone())),
        ));
    }
    for directory in &wanted.write {
        rows.push((
            row(
                format!("  write {}", path(directory)),
                outside.write.contains(directory),
                unapproved.write.contains(directory),
            ),
            Vec::new(),
        ));
    }
    for label in &wanted.network {
        rows.push((
            [
                format!("  network {label}"),
                "-".to_owned(),
                approved(unapproved.network.contains(label)).to_owned(),
            ],
            attribution.lines(&Want::Network(label.clone())),
        ));
    }
    rows
}

/// The cells of column `index`, the head's first.
fn column<'a>(head: &'a Row, rows: &'a [Row], index: usize) -> impl Iterator<Item = &'a str> {
    std::iter::once(head[index].as_str()).chain(rows.iter().map(move |row| row[index].as_str()))
}

/// The width of a column holding `cells`: its longest cell and the gap.
fn column_width<'a>(cells: impl Iterator<Item = &'a str>) -> usize {
    cells.map(|cell| cell.chars().count()).max().unwrap_or(0) + GAP
}

/// The execution settings as one line: `workers`, `timeout` and `strategy`.
fn execution(execution: &Execution) -> String {
    let per_core = if execution.workers == Workers::PerCore {
        " (one per core)"
    } else {
        ""
    };
    format!(
        "workers {}{per_core}, timeout {}, strategy {}",
        execution.workers, execution.timeout, execution.strategy
    )
}

/// The members of `set` joined by `, `, or `-` when there are none.
fn joined<T: Display>(set: &BTreeSet<T>) -> String {
    if set.is_empty() {
        return "-".to_owned();
    }
    set.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The directories of `set` joined by `, `, the root written `.`; or `-` when there are none.
fn directories(set: &BTreeSet<Directory>) -> String {
    joined(&set.iter().map(path).collect())
}

/// `directory` as a path relative to the root: `.` for the root itself.
fn path(directory: &Directory) -> String {
    if directory.is_root() {
        ".".to_owned()
    } else {
        directory.to_string()
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

    use buildl::Doctor;
    use buildl::buildl_core::{
        ApproverMode, Attribution, Ceiling, Checkup, DeclarationSite, Directory, EnvName,
        Execution, Isolation, Label, Origin, PluginChain, PluginName, PluginRef, PluginVersion,
        PluginViolation, Program, Provenance, Timeout, Want, WantedSet, Workers, Workspace,
    };

    use super::doctor_text;

    fn programs(names: &[&str]) -> BTreeSet<Program> {
        names
            .iter()
            .map(|name| Program::parse(*name).unwrap())
            .collect()
    }

    fn env(names: &[&str]) -> BTreeSet<EnvName> {
        names
            .iter()
            .map(|name| EnvName::parse(*name).unwrap())
            .collect()
    }

    /// The directories of `paths`, `.` being the root.
    fn dirs(paths: &[&str]) -> BTreeSet<Directory> {
        paths
            .iter()
            .map(|path| match *path {
                "." => Directory::root(),
                path => Directory::parse(path).unwrap(),
            })
            .collect()
    }

    fn labels(raws: &[&str]) -> BTreeSet<Label> {
        raws.iter().map(|raw| Label::parse(raw).unwrap()).collect()
    }

    /// The workspace of the documented example: `go` and `protoc` to run, `HOME` and `PATH` to read,
    /// every file to read, `out` and `.buildl` to write, under `approver`.
    fn workspace(approver: ApproverMode, workers: Workers) -> Workspace {
        Workspace {
            out: Directory::parse("out").unwrap(),
            state: Directory::parse(".buildl").unwrap(),
            ceiling: Ceiling {
                run: programs(&["go", "protoc"]),
                env: env(&["HOME", "PATH"]),
                read: dirs(&["."]),
                write: dirs(&[".buildl", "out"]),
            },
            execution: Execution {
                workers,
                timeout: Timeout::parse("10m").unwrap(),
                strategy: Isolation::Sandbox,
            },
            approver,
            plugins: std::collections::BTreeMap::new(),
        }
    }

    /// A report of [`workspace`] wanting `wanted`, of which `outside` is outside the ceiling
    /// and `unapproved` is not approved.
    fn report(
        workspace: Workspace,
        wanted: WantedSet,
        outside: WantedSet,
        unapproved: WantedSet,
    ) -> String {
        doctor_text(&Doctor::new(
            Checkup::new(
                workspace,
                wanted,
                outside,
                Attribution::default(),
                Vec::new(),
            ),
            PathBuf::from("/path/to/myapp"),
            unapproved,
        ))
    }

    #[test]
    fn a_full_report_matches_the_documented_layout() {
        let wanted = WantedSet {
            programs: programs(&["curl", "go"]),
            env: env(&["HOME"]),
            write: dirs(&["out"]),
            network: labels(&["//:fetch"]),
        };
        let outside = WantedSet {
            programs: programs(&["curl"]),
            ..WantedSet::default()
        };
        let unapproved = WantedSet {
            programs: programs(&["curl"]),
            write: dirs(&["out"]),
            network: labels(&["//:fetch"]),
            ..WantedSet::default()
        };

        let text = report(
            workspace(ApproverMode::Interactive, Workers::PerCore),
            wanted,
            outside,
            unapproved,
        );

        assert_eq!(
            text,
            concat!(
                "workspace  /path/to/myapp\n",
                "approver   interactive\n",
                "execution  workers 0 (one per core), timeout 10m, strategy sandbox\n",
                "ceiling\n",
                "  proc.run  go, protoc\n",
                "  env.read  HOME, PATH\n",
                "  fs.read   .\n",
                "  fs.write  .buildl, out\n",
                "wants               ceiling  approved\n",
                "  run curl          REFUSED  -\n",
                "  run go            allowed  yes\n",
                "  read HOME         allowed  yes\n",
                "  write out         allowed  no\n",
                "  network //:fetch  -        no\n",
            )
        );
    }

    fn go() -> PluginRef {
        PluginRef {
            name: PluginName::parse("go").unwrap(),
            version: PluginVersion::parse("1.2.0").unwrap(),
            run: programs(&["go"]),
            env: BTreeSet::new(),
        }
    }

    #[test]
    fn doctor_text_names_plugins_and_violations() {
        let wanted = WantedSet {
            programs: programs(&["curl", "go"]),
            ..WantedSet::default()
        };
        let chain = PluginChain::new(vec![go()]).unwrap();
        let app = Label::parse("//:app").unwrap();
        let mut attribution = Attribution::default();
        attribution.record(Want::Program(Program::parse("curl").unwrap()), &chain, &app);
        let violation = PluginViolation {
            want: Want::Program(Program::parse("curl").unwrap()),
            site: DeclarationSite {
                label: app,
                provenance: Provenance::new(PathBuf::from("build.lua"), Directory::root()),
                origin: Origin::plugins(vec![go()]).unwrap(),
            },
            plugin: go(),
            allowed: BTreeSet::from(["go".to_owned()]),
        };
        let checkup = Checkup::new(
            workspace(ApproverMode::Manifest, Workers::PerCore),
            wanted,
            WantedSet::default(),
            attribution,
            vec![violation],
        );

        let text = doctor_text(&Doctor::new(
            checkup,
            PathBuf::from("/path/to/myapp"),
            WantedSet::default(),
        ));

        assert!(
            text.ends_with(concat!(
                "wants       ceiling  approved\n",
                "  run curl  allowed  yes\n",
                "    declared by plugin go@1.2.0 for //:app\n",
                "  run go    allowed  yes\n",
                "plugin violations\n",
                "  build.lua: //:app (plugin go@1.2.0): program curl is outside plugin go@1.2.0 ",
                "(proc.run allows: go)\n",
            )),
            "{text}"
        );
    }

    #[test]
    fn an_empty_ceiling_list_shows_a_dash() {
        let mut workspace = workspace(ApproverMode::Manifest, Workers::PerCore);
        workspace.ceiling.run.clear();
        workspace.ceiling.env.clear();

        let text = report(
            workspace,
            WantedSet::default(),
            WantedSet::default(),
            WantedSet::default(),
        );

        assert!(text.contains("  proc.run  -\n"), "{text}");
        assert!(text.contains("  env.read  -\n"), "{text}");
    }

    #[test]
    fn a_refused_want_has_no_approval() {
        let wanted = WantedSet {
            env: env(&["TOKEN"]),
            ..WantedSet::default()
        };

        let text = report(
            workspace(ApproverMode::Deny, Workers::PerCore),
            wanted.clone(),
            wanted.clone(),
            wanted,
        );

        assert!(text.ends_with("  read TOKEN  REFUSED  -\n"), "{text}");
    }

    #[test]
    fn per_core_workers_say_so() {
        let per_core = report(
            workspace(ApproverMode::Manifest, Workers::PerCore),
            WantedSet::default(),
            WantedSet::default(),
            WantedSet::default(),
        );
        let fixed = report(
            workspace(
                ApproverMode::Manifest,
                Workers::Fixed(NonZeroUsize::new(4).unwrap()),
            ),
            WantedSet::default(),
            WantedSet::default(),
            WantedSet::default(),
        );

        assert!(
            per_core
                .contains("execution  workers 0 (one per core), timeout 10m, strategy sandbox\n"),
            "{per_core}"
        );
        assert!(
            fixed.contains("execution  workers 4, timeout 10m, strategy sandbox\n"),
            "{fixed}"
        );
    }
}
