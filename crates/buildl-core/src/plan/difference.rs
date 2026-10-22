//! How a target's key differs from the key its last build recorded.
//!
//! Its own file because this comparison is what explains a rebuild: a digest says only that two
//! keys differ, and the components kept beside it say where.
//!
//! Responsibilities: [`Known`] and [`between`].
//!
//! Non-responsibilities: deciding what a difference means for a target, which is its verdict.

use std::collections::BTreeMap;

use crate::types::{Difference, Digest, KeyComponents, Label, OwnComponents, Produced, SourcePath};

/// One dependency as a comparison sees it: its label, and its products when they are known.
///
/// A borrowed view, made for one comparison. The value a verdict is decided from also carries
/// the dependency's id and owns its products; a comparison needs neither.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Known<'a> {
    /// The dependency.
    pub(crate) label: &'a Label,
    /// What it produced; `None` when that is not known before the build.
    pub(crate) products: Option<&'a [Produced]>,
}

/// Every way a key made of `own` and `deps` is known to differ from `previous`.
///
/// The differences come in a fixed order: the command; each input that was added, removed or
/// changed, in path order; the inputs' order; the outputs; each environment variable, in name
/// order; the tool; each setting, in name order; the network; the platform; each dependency
/// that was added, removed or produced something else, in label order; the dependencies' order.
///
/// A dependency whose products are not known cannot be said to have produced something else, so
/// it is reported only when it was added. When every dependency's products are known the list
/// is empty exactly when the two keys are equal.
pub(crate) fn between(
    own: &OwnComponents,
    deps: &[Known<'_>],
    previous: &KeyComponents,
) -> Vec<Difference> {
    let old = &previous.own;
    let mut found = Vec::new();

    if own.command != old.command {
        found.push(Difference::Command);
    }
    found.extend(
        changed(&input_digests(own), &input_digests(old))
            .into_iter()
            .map(|path| Difference::Input(path.clone())),
    );
    if reordered(&input_paths(own), &input_paths(old)) {
        found.push(Difference::InputOrder);
    }
    if own.outputs != old.outputs {
        found.push(Difference::Outputs);
    }
    found.extend(changed(&own.env, &old.env).into_iter().map(Difference::Env));
    if own.tool != old.tool {
        found.push(Difference::Tool);
    }
    found.extend(
        changed(&own.settings, &old.settings)
            .into_iter()
            .map(Difference::Setting),
    );
    if own.network != old.network {
        found.push(Difference::Network);
    }
    if own.platform != old.platform {
        found.push(Difference::Platform);
    }

    let then: BTreeMap<&Label, &[Produced]> = previous
        .deps
        .iter()
        .map(|dep| (&dep.label, dep.outputs.as_slice()))
        .collect();
    // A dependency whose products are not known is given the recorded ones, so that it compares
    // equal when it was there before and is missing from `now` when it was not.
    let now: BTreeMap<&Label, &[Produced]> = deps
        .iter()
        .filter_map(|dep| {
            let products = dep.products.or_else(|| then.get(dep.label).copied())?;
            Some((dep.label, products))
        })
        .collect();
    let added_unknown = deps
        .iter()
        .filter(|dep| dep.products.is_none() && !then.contains_key(dep.label))
        .map(|dep| dep.label);
    let mut differing: Vec<&Label> = changed(&now, &then)
        .into_iter()
        .chain(added_unknown)
        .collect();
    differing.sort();
    found.extend(
        differing
            .into_iter()
            .map(|label| Difference::Dep(label.clone())),
    );
    let labels_now: Vec<&Label> = deps.iter().map(|dep| dep.label).collect();
    let labels_then: Vec<&Label> = previous.deps.iter().map(|dep| &dep.label).collect();
    if reordered(&labels_now, &labels_then) {
        found.push(Difference::DepOrder);
    }

    found
}

/// The inputs of `components`, each path with its digest.
fn input_digests(components: &OwnComponents) -> BTreeMap<&SourcePath, Digest> {
    components
        .inputs
        .iter()
        .map(|input| (&input.path, input.digest))
        .collect()
}

/// The paths of the inputs of `components`, in written order.
fn input_paths(components: &OwnComponents) -> Vec<&SourcePath> {
    components.inputs.iter().map(|input| &input.path).collect()
}

/// The keys held by only one of two maps, or by both with different values, in key order.
fn changed<K: Ord + Clone, V: PartialEq>(now: &BTreeMap<K, V>, then: &BTreeMap<K, V>) -> Vec<K> {
    let mut keys: Vec<K> = now
        .keys()
        .chain(then.keys())
        .filter(|key| now.get(*key) != then.get(*key))
        .cloned()
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Whether two lists hold the same items in a different order.
fn reordered<T: Ord + Clone>(now: &[T], then: &[T]) -> bool {
    let sorted = |items: &[T]| {
        let mut items = items.to_vec();
        items.sort();
        items
    };
    now != then && sorted(now) == sorted(then)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{Known, between, changed, reordered};
    use crate::grants::fixtures::{command, env_name, label, path, program, settings};
    use crate::plan::fixtures::{components, produced};
    use crate::types::{
        DepProducts, Difference, Digest, EnvValue, InputDigest, KeyComponents, Label,
        NetworkAccess, OutputName, OwnComponents, Platform, Produced, SettingName, ToolIdentity,
    };
    use std::collections::BTreeMap;

    fn input(raw: &str, content: &str) -> InputDigest {
        InputDigest {
            path: path(raw),
            digest: Digest::of(content.as_bytes()),
        }
    }

    fn tool(fingerprint: &str) -> ToolIdentity {
        ToolIdentity {
            program: program("cc"),
            fingerprint: Digest::of(fingerprint.as_bytes()),
        }
    }

    /// One edit to a target's own components.
    type Change = fn(&mut OwnComponents);

    /// The components of a target with something in every field.
    fn full() -> OwnComponents {
        components("app", |own| {
            own.inputs = vec![input("src/z.c", "z"), input("src/a.c", "a")];
            own.env = BTreeMap::from([
                (env_name("HOME"), None),
                (env_name("PATH"), Some(EnvValue::parse("/bin").unwrap())),
            ]);
            own.tool = Some(tool("cc 14.2"));
            own.settings = settings(&[("mode", "debug"), ("test_filter", "T")]);
        })
    }

    /// The recorded key: [`full`], depending on `//lib:text` then `//lib:util`.
    fn recorded() -> KeyComponents {
        KeyComponents {
            own: full(),
            deps: vec![
                DepProducts {
                    label: label("//lib:text"),
                    outputs: vec![produced("text", "text v1")],
                },
                DepProducts {
                    label: label("//lib:util"),
                    outputs: vec![produced("util", "util v1")],
                },
            ],
        }
    }

    /// The dependencies of [`recorded`], all with the products it recorded.
    fn same_deps(previous: &KeyComponents) -> Vec<(Label, Option<Vec<Produced>>)> {
        previous
            .deps
            .iter()
            .map(|dep| (dep.label.clone(), Some(dep.outputs.clone())))
            .collect()
    }

    fn known(deps: &[(Label, Option<Vec<Produced>>)]) -> Vec<Known<'_>> {
        deps.iter()
            .map(|(label, products)| Known {
                label,
                products: products.as_deref(),
            })
            .collect()
    }

    /// The differences of `own` with the recorded dependencies unchanged.
    fn own_differences(own: &OwnComponents) -> Vec<Difference> {
        let previous = recorded();
        let deps = same_deps(&previous);
        between(own, &known(&deps), &previous)
    }

    /// The differences of the recorded own components with `deps`.
    fn dep_differences(deps: &[(Label, Option<Vec<Produced>>)]) -> Vec<Difference> {
        between(&full(), &known(deps), &recorded())
    }

    #[test]
    fn equal_keys_have_no_difference() {
        assert_eq!(own_differences(&full()), []);
    }

    #[test]
    fn each_own_component_is_reported_alone_when_it_alone_changes() {
        let cases: [(Change, Difference); 12] = [
            (
                |own| own.command = command(&["cc", "-O2"]),
                Difference::Command,
            ),
            (
                |own| own.inputs = vec![input("src/z.c", "z2"), input("src/a.c", "a")],
                Difference::Input(path("src/z.c")),
            ),
            (
                |own| own.inputs.push(input("src/new.c", "n")),
                Difference::Input(path("src/new.c")),
            ),
            (
                |own| own.inputs = vec![input("src/z.c", "z")],
                Difference::Input(path("src/a.c")),
            ),
            (|own| own.inputs.reverse(), Difference::InputOrder),
            (
                |own| own.outputs = vec![OutputName::parse("app2").unwrap()],
                Difference::Outputs,
            ),
            (
                |own| {
                    own.env
                        .insert(env_name("HOME"), Some(EnvValue::parse("").unwrap()));
                },
                Difference::Env(env_name("HOME")),
            ),
            (
                |own| {
                    own.env.remove(&env_name("PATH"));
                },
                Difference::Env(env_name("PATH")),
            ),
            (|own| own.tool = Some(tool("cc 15.1")), Difference::Tool),
            (
                |own| own.settings = settings(&[("mode", "release"), ("test_filter", "T")]),
                Difference::Setting(SettingName::parse("mode").unwrap()),
            ),
            (
                |own| own.network = NetworkAccess::Declared,
                Difference::Network,
            ),
            (
                |own| own.platform = Platform::parse("macos/aarch64").unwrap(),
                Difference::Platform,
            ),
        ];
        for (change, difference) in cases {
            let mut own = full();
            change(&mut own);
            assert_eq!(own_differences(&own), [difference]);
        }
    }

    #[test]
    fn a_dependency_is_reported_when_added_removed_or_producing_something_else() {
        let previous = recorded();
        let mut produced_other = same_deps(&previous);
        produced_other[1].1 = Some(vec![produced("util", "util v2")]);
        assert_eq!(
            dep_differences(&produced_other),
            [Difference::Dep(label("//lib:util"))]
        );

        let mut added = same_deps(&previous);
        added.push((label("//lib:new"), Some(vec![produced("new", "new v1")])));
        assert_eq!(
            dep_differences(&added),
            [Difference::Dep(label("//lib:new"))]
        );

        let removed = &same_deps(&previous)[..1];
        assert_eq!(
            dep_differences(removed),
            [Difference::Dep(label("//lib:util"))]
        );
    }

    #[test]
    fn the_same_dependencies_in_another_order_are_reported_as_an_order_change() {
        let mut deps = same_deps(&recorded());
        deps.reverse();
        assert_eq!(dep_differences(&deps), [Difference::DepOrder]);
    }

    #[test]
    fn a_dependency_with_unknown_products_is_reported_only_when_it_was_added() {
        let previous = recorded();
        let mut unknown = same_deps(&previous);
        unknown[0].1 = None;
        assert_eq!(dep_differences(&unknown), []);

        unknown.push((label("//lib:new"), None));
        assert_eq!(
            dep_differences(&unknown),
            [Difference::Dep(label("//lib:new"))]
        );
    }

    #[test]
    fn differences_come_in_one_fixed_order() {
        let own = components("app2", |own| {
            own.command = command(&["cc", "-O2"]);
            own.inputs = vec![
                input("src/b.c", "b"),
                input("src/a.c", "a2"),
                input("src/z.c", "z"),
            ];
            own.env = BTreeMap::from([(env_name("PATH"), None), (env_name("CC"), None)]);
            own.tool = Some(tool("cc 15.1"));
            own.settings = settings(&[("arch", "arm"), ("test_filter", "U")]);
            own.network = NetworkAccess::Declared;
            own.platform = Platform::parse("macos/aarch64").unwrap();
        });
        let deps = [
            (label("//lib:util"), Some(vec![produced("util", "util v2")])),
            (label("//lib:text"), None),
            (label("//lib:added"), None),
        ];
        assert_eq!(
            between(&own, &known(&deps), &recorded()),
            [
                Difference::Command,
                Difference::Input(path("src/a.c")),
                Difference::Input(path("src/b.c")),
                Difference::Outputs,
                Difference::Env(env_name("CC")),
                Difference::Env(env_name("HOME")),
                Difference::Env(env_name("PATH")),
                Difference::Tool,
                Difference::Setting(SettingName::parse("arch").unwrap()),
                Difference::Setting(SettingName::parse("mode").unwrap()),
                Difference::Setting(SettingName::parse("test_filter").unwrap()),
                Difference::Network,
                Difference::Platform,
                Difference::Dep(label("//lib:added")),
                Difference::Dep(label("//lib:util")),
            ]
        );
    }

    #[test]
    fn an_order_change_is_reported_beside_a_changed_member() {
        let mut own = full();
        own.inputs = vec![input("src/a.c", "a2"), input("src/z.c", "z")];
        assert_eq!(
            own_differences(&own),
            [Difference::Input(path("src/a.c")), Difference::InputOrder]
        );

        let mut deps = same_deps(&recorded());
        deps.reverse();
        deps[0].1 = Some(vec![produced("util", "util v2")]);
        assert_eq!(
            dep_differences(&deps),
            [Difference::Dep(label("//lib:util")), Difference::DepOrder]
        );
    }

    #[test]
    fn changed_lists_each_differing_key_once_in_key_order() {
        let now = BTreeMap::from([("a", 1), ("b", 2), ("d", 4)]);
        let then = BTreeMap::from([("b", 3), ("c", 3), ("d", 4)]);
        assert_eq!(changed(&now, &then), ["a", "b", "c"]);
        assert_eq!(changed(&now, &now), Vec::<&str>::new());
    }

    #[test]
    fn reordered_means_the_same_items_in_another_order() {
        assert!(reordered(&[1, 2, 3], &[3, 1, 2]));
        assert!(!reordered(&[1, 2, 3], &[1, 2, 3]));
        assert!(!reordered(&[1, 2, 3], &[1, 2]));
        assert!(!reordered(&[1, 2, 3], &[1, 2, 4]));
        assert!(!reordered::<u8>(&[], &[]));
    }
}
