//! The graph record on disk: the graph the last build resolved, as `graph.json`.
//!
//! Its own module because the graph on disk is what a later reader holds beside the rows and
//! the content a build left: it is written by every build, before anything is planned, so the
//! three always describe the same build files.
//!
//! Responsibilities: [`GraphFile`], the [`GraphRecord`](buildl_core::GraphRecord) of a local
//! workspace.
//!
//! Non-responsibilities: resolving the graph, which the pipeline does; and where the state
//! directory keeps the record, which [`StateLayout`](crate::state::StateLayout) says.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod graph_file;

pub use graph_file::GraphFile;
