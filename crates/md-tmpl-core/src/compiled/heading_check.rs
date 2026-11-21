//! Compile-time check rejecting multi-line block filters (`fence`, `quarantine`,
//! and block `sanitize("tag")`) inside Markdown ATX heading lines (`# ...` through `###### ...`).

use super::{FilterKind, SanitizeFilterMode, Segment, sanitize_pass::update_heading_line_state};
use crate::error::TemplateError;

/// Reject multi-line block filters (`fence`, `quarantine`, and block `sanitize("tag")`) on
/// Markdown ATX heading lines (`# ...` through `###### ...`), starting from `(in_atx_heading, at_line_start)`.
pub(super) fn check_heading_block_filters_from(
    segments: &[Segment],
    mut in_atx_heading: bool,
    mut at_line_start: bool,
) -> Result<(), TemplateError> {
    for seg in segments {
        match seg {
            Segment::Static(text) | Segment::Raw(text) => {
                update_heading_line_state(text, &mut in_atx_heading, &mut at_line_start);
            }
            Segment::Expr { filters, .. } => {
                if in_atx_heading {
                    for f in filters {
                        match f.kind {
                            FilterKind::Fence => {
                                return Err(TemplateError::syntax(
                                    "multi-line filter 'fence' cannot be used inside a Markdown ATX heading line",
                                ));
                            }
                            FilterKind::Quarantine => {
                                return Err(TemplateError::syntax(
                                    "multi-line filter 'quarantine' cannot be used inside a Markdown ATX heading line",
                                ));
                            }
                            FilterKind::Sanitize
                                if matches!(
                                    f.sanitize_mode,
                                    Some(SanitizeFilterMode::Block { .. })
                                ) || f.args.is_some() =>
                            {
                                return Err(TemplateError::syntax(
                                    "multi-line block 'sanitize' cannot be used inside a Markdown ATX heading line",
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                at_line_start = false;
            }
            Segment::ForLoop {
                body, else_body, ..
            } => {
                check_heading_block_filters_from(body, in_atx_heading, at_line_start)?;
                check_heading_block_filters_from(else_body, in_atx_heading, at_line_start)?;
            }
            Segment::If {
                branches,
                else_body,
            } => {
                let mut any_heading = false;
                let mut any_line_start = false;
                for (_, branch_body) in branches {
                    let (h, l) = scan_heading_branch(branch_body, in_atx_heading, at_line_start)?;
                    any_heading |= h;
                    any_line_start |= l;
                }
                if else_body.is_empty() {
                    any_heading |= in_atx_heading;
                    any_line_start |= at_line_start;
                } else {
                    let (h, l) = scan_heading_branch(else_body, in_atx_heading, at_line_start)?;
                    any_heading |= h;
                    any_line_start |= l;
                }
                in_atx_heading = any_heading;
                at_line_start = any_line_start;
            }
            Segment::Match { arms, .. } => {
                let mut any_heading = false;
                let mut any_line_start = false;
                for arm in arms {
                    let (h, l) = scan_heading_branch(&arm.body, in_atx_heading, at_line_start)?;
                    any_heading |= h;
                    any_line_start |= l;
                }
                if !arms.is_empty() {
                    in_atx_heading = any_heading;
                    at_line_start = any_line_start;
                }
            }
            Segment::Panic(inner) => {
                check_heading_block_filters_from(inner, in_atx_heading, at_line_start)?;
            }
            Segment::Include(inc) => {
                if let Some(ic) = &inc.inline_compiled {
                    check_heading_block_filters_from(&ic.segments, in_atx_heading, at_line_start)?;
                }
                at_line_start = false;
            }
            Segment::Comment(_) => {}
        }
    }
    Ok(())
}

fn scan_heading_branch(
    segments: &[Segment],
    in_atx_heading: bool,
    at_line_start: bool,
) -> Result<(bool, bool), TemplateError> {
    check_heading_block_filters_from(segments, in_atx_heading, at_line_start)?;
    let mut h = in_atx_heading;
    let mut l = at_line_start;
    for seg in segments {
        match seg {
            Segment::Static(text) | Segment::Raw(text) => {
                update_heading_line_state(text, &mut h, &mut l);
            }
            Segment::Expr { .. } | Segment::Include(_) => {
                l = false;
            }
            _ => {}
        }
    }
    Ok((h, l))
}
