//! Catalog layer: the document model stored inside a single `.tes` file.
//!
//! v0 scope covers the chunk index (`TIDX`). Catalog JSON, link table
//! (`TLNK`), chunk payloads, and the writer session land in later THI issues.

pub mod index;
