//! Internal, unpublished crate hosting the compile-time `template!` test
//! generator (`build.rs`) and the generated shared test suite
//! (`tests/shared_compile_time.rs`).
//!
//! This crate exists solely so the published `md-tmpl` crate carries no build
//! script and no `toml` build-dependency: downstream consumers previously had
//! to compile the `toml`/`toml_edit`/`winnow` tree and run a build script that
//! produced nothing outside this workspace. The generated tests run here via
//! `cargo test --workspace`, keeping coverage identical.
//!
//! It has no runtime API and is never published (`publish = false`).
