//! Copies of the Lua tables that code under a different authority must not share.
//!
//! Its own file because three places need it: the snapshot taken at install, the curated modules
//! recorded as they install, and every plugin instance's environment. A table shared by reference
//! is a table either side can write, and what one side writes the other then calls.
//!
//! Responsibilities: [`copy_table`] and [`copy_tables`].
//!
//! Non-responsibilities: deciding which values are worth copying, which the callers do.

use airsl::mlua::{self, Lua, Table, Value};

/// A new table holding the entries of `table`.
///
/// The copy is one level deep: a table inside `table` is shared. The library and module tables it
/// is used for hold only functions, so one level is the whole table. A metatable is not carried
/// over.
///
/// # Errors
///
/// Returns mlua's error when a table cannot be created or read.
pub(crate) fn copy_table(lua: &Lua, table: &Table) -> mlua::Result<Table> {
    let copy = lua.create_table()?;
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        copy.set(key, value)?;
    }
    Ok(copy)
}

/// Writes every entry of `from` into `into`, replacing each table-valued entry with a
/// [`copy_table`] of it.
///
/// # Errors
///
/// Returns mlua's error when a table cannot be created or read.
pub(crate) fn copy_tables(lua: &Lua, from: &Table, into: &Table) -> mlua::Result<()> {
    for pair in from.pairs::<Value, Value>() {
        let (name, value) = pair?;
        let value = match value {
            Value::Table(table) => Value::Table(copy_table(lua, &table)?),
            other => other,
        };
        into.set(name, value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use airsl::mlua::{Lua, Table};

    use super::{copy_table, copy_tables};

    #[test]
    fn a_copied_table_holds_the_entries_and_is_a_table_of_its_own() {
        let lua = Lua::new();
        let original: Table = lua.load("return { a = 1, f = print }").eval().unwrap();
        let copy = copy_table(&lua, &original).unwrap();
        assert!(!original.equals(&copy).unwrap());
        assert_eq!(copy.get::<i64>("a").unwrap(), 1);

        copy.set("a", 2).unwrap();
        copy.set("b", 3).unwrap();
        assert_eq!(original.get::<i64>("a").unwrap(), 1);
        assert!(!original.contains_key("b").unwrap());
    }

    #[test]
    fn a_table_valued_entry_is_copied_and_the_rest_is_shared() {
        let lua = Lua::new();
        let from: Table = lua
            .load("return { n = 1, f = print, t = { a = 1 } }")
            .eval()
            .unwrap();
        let into = lua.create_table().unwrap();
        copy_tables(&lua, &from, &into).unwrap();

        assert_eq!(into.get::<i64>("n").unwrap(), 1);
        let original: Table = from.get("t").unwrap();
        let copy: Table = into.get("t").unwrap();
        assert!(!original.equals(&copy).unwrap());
        assert_eq!(copy.get::<i64>("a").unwrap(), 1);
        copy.set("a", 2).unwrap();
        assert_eq!(original.get::<i64>("a").unwrap(), 1);
    }
}
