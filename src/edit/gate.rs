//! Public edit-read / edit-write / apply gate (lock → hash → verify → replace).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::catalog::TesFile;
use crate::error::{Result, TesError};
use crate::verify::{verify_bytes, verify_tes_file};

use super::compile::{compile_blocks_to_bytes, seal_with_history};
use super::diff::{normalize_tessprek_for_diff, simple_diff};
use super::lock::{AdvisoryLock, sibling_temp_path};
use super::{
    CatalogPatch, EditMediaBag, EditReadReport, EditWriteOptions, EditWriteReport, TesOp,
    apply_ops_to_blocks, decode_tessprek, encode_tessprek, file_source_hash,
};

/// Decode a `.tes` file to Tessera Markdown for an editor buffer.
///
/// # Errors
///
/// Returns I/O or decode errors from opening/encoding the file.
pub fn edit_read(path: impl AsRef<Path>) -> Result<EditReadReport> {
    let path = path.as_ref();
    let source_hash = file_source_hash(path)?;
    let file = TesFile::open(path)?;
    let tessprek = encode_tessprek(&file, &source_hash)?;
    Ok(EditReadReport {
        source_hash,
        tessprek,
    })
}

/// Compile Tessera Markdown and atomically replace `path` under lock + hash check.
///
/// New image/attachment bytes may be supplied via [`EditWriteOptions::media`]
/// (see [`edit_write_with_media`]).
///
/// # Errors
///
/// Returns lock, hash mismatch, parse, verify, or I/O errors. On failure the
/// original file is left untouched.
pub fn edit_write(
    path: impl AsRef<Path>,
    tessprek: &str,
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    edit_write_inner(path, tessprek, options)
}

/// Like [`edit_write`], with an explicit media bag for newly injected payloads.
///
/// Temporary ids referenced by figure `image=` / `media:N` or attachment
/// `chunk=` are resolved from `media` when absent from the source file.
///
/// # Errors
///
/// Same as [`edit_write`].
pub fn edit_write_with_media(
    path: impl AsRef<Path>,
    tessprek: &str,
    options: &EditWriteOptions,
    media: EditMediaBag,
) -> Result<EditWriteReport> {
    let mut options = options.clone();
    options.media = media;
    edit_write_inner(path, tessprek, &options)
}

fn edit_write_inner(
    path: impl AsRef<Path>,
    tessprek: &str,
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    let path = path.as_ref();
    let _lock = AdvisoryLock::acquire(path)?;
    let current_hash = file_source_hash(path)?;
    if current_hash != options.source_hash {
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: current_hash,
        });
    }

    let before = edit_read(path)?;
    let source = TesFile::open(path)?;
    let blocks = decode_tessprek(tessprek)?;
    let compiled = seal_with_history(
        &source,
        compile_blocks_to_bytes(&source, &blocks, None, &options.media)?,
    )?;

    let verify = verify_bytes(path, &compiled, true);
    if !verify.ok {
        let first = verify
            .findings
            .iter()
            .find(|f| matches!(f.severity, crate::verify::Severity::Error))
            .map_or_else(|| "deep verify failed".into(), |f| f.message.clone());
        return Err(TesError::EditVerifyFailed { message: first });
    }

    let after_tessprek = {
        // Encode from compiled bytes via a temp file for the diff projection.
        let tmp_for_diff = sibling_temp_path(path, "diff");
        fs::write(&tmp_for_diff, &compiled)?;
        let report = edit_read(&tmp_for_diff);
        let _ = fs::remove_file(&tmp_for_diff);
        report?.tessprek
    };
    let diff = simple_diff(
        &normalize_tessprek_for_diff(&before.tessprek),
        &normalize_tessprek_for_diff(&after_tessprek),
    );

    if options.dry_run {
        return Ok(EditWriteReport {
            path: path.to_path_buf(),
            source_hash: current_hash,
            new_source_hash: None,
            verify,
            diff,
            replaced: false,
        });
    }

    let tmp = sibling_temp_path(path, "tmp");
    {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(&compiled)?;
        file.sync_all()?;
    }
    // Re-check after compile (another writer could have raced before we locked
    // in pathological cases; lock covers the common editor path).
    let recheck = file_source_hash(path)?;
    if recheck != options.source_hash {
        let _ = fs::remove_file(&tmp);
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: recheck,
        });
    }
    fs::rename(&tmp, path)?;
    let new_source_hash = file_source_hash(path)?;
    // Confirm the replaced file still verifies on disk.
    let on_disk = verify_tes_file(path, true)?;
    if !on_disk.ok {
        return Err(TesError::EditVerifyFailed {
            message: "post-replace deep verify failed".into(),
        });
    }

    Ok(EditWriteReport {
        path: path.to_path_buf(),
        source_hash: current_hash,
        new_source_hash: Some(new_source_hash),
        verify: on_disk,
        diff,
        replaced: true,
    })
}

/// Apply a Tessprek patch file through the same mutation gate as [`edit_write`].
///
/// # Errors
///
/// Same as [`edit_write`].
pub fn apply_patch(
    path: impl AsRef<Path>,
    patch_tessprek: &str,
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    edit_write(path, patch_tessprek, options)
}

/// Apply typed JSON ops: project → mutate → compile → verify → replace.
///
/// # Errors
///
/// Returns op/parse/verify/I/O errors. Original untouched on failure.
pub fn apply_ops(
    path: impl AsRef<Path>,
    ops: &[TesOp],
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    let path = path.as_ref();
    let _lock = AdvisoryLock::acquire(path)?;
    let current_hash = file_source_hash(path)?;
    if current_hash != options.source_hash {
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: current_hash,
        });
    }

    let before = edit_read(path)?;
    let source = TesFile::open(path)?;
    let mut blocks = decode_tessprek(&before.tessprek)?;
    let mut catalog = CatalogPatch::from_catalog(source.catalog());
    apply_ops_to_blocks(&mut blocks, &mut catalog, ops)?;
    let compiled = seal_with_history(
        &source,
        compile_blocks_to_bytes(&source, &blocks, Some(&catalog), &EditMediaBag::default())?,
    )?;

    let verify = verify_bytes(path, &compiled, true);
    if !verify.ok {
        let first = verify
            .findings
            .iter()
            .find(|f| matches!(f.severity, crate::verify::Severity::Error))
            .map_or_else(|| "deep verify failed".into(), |f| f.message.clone());
        return Err(TesError::EditVerifyFailed { message: first });
    }

    let after_tessprek = {
        let tmp_for_diff = sibling_temp_path(path, "diff");
        fs::write(&tmp_for_diff, &compiled)?;
        let report = edit_read(&tmp_for_diff);
        let _ = fs::remove_file(&tmp_for_diff);
        report?.tessprek
    };
    let diff = simple_diff(
        &normalize_tessprek_for_diff(&before.tessprek),
        &normalize_tessprek_for_diff(&after_tessprek),
    );

    if options.dry_run {
        return Ok(EditWriteReport {
            path: path.to_path_buf(),
            source_hash: current_hash,
            new_source_hash: None,
            verify,
            diff,
            replaced: false,
        });
    }

    let tmp = sibling_temp_path(path, "tmp");
    {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(&compiled)?;
        file.sync_all()?;
    }
    let recheck = file_source_hash(path)?;
    if recheck != options.source_hash {
        let _ = fs::remove_file(&tmp);
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: recheck,
        });
    }
    fs::rename(&tmp, path)?;
    let new_source_hash = file_source_hash(path)?;
    let on_disk = verify_tes_file(path, true)?;
    if !on_disk.ok {
        return Err(TesError::EditVerifyFailed {
            message: "post-replace deep verify failed".into(),
        });
    }

    Ok(EditWriteReport {
        path: path.to_path_buf(),
        source_hash: current_hash,
        new_source_hash: Some(new_source_hash),
        verify: on_disk,
        diff,
        replaced: true,
    })
}
