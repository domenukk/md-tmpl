//! Compile-time check rejecting multi-line block filters (`fence`, `quarantine`,
//! and block `sanitize("tag")`) inside Markdown ATX heading lines (`# ...` through `###### ...`).

use super::{FilterKind, SanitizeFilterMode, Segment, sanitize_pass::update_heading_line_state};
use crate::error::TemplateError;

/// Reject multi-line block filters (`fence`, `quarantine`, and block `sanitize("tag")`) on
/// Markdown ATX heading lines (`# ...` through `###### ...`), starting from
/// `(in_atx_heading, at_line_start)`.
pub(super) fn check_heading_block_filters(
    segments: &[Segment],
    in_atx_heading: bool,
    at_line_start: bool,
) -> Result<(), TemplateError> {
    match check_heading_block_filters_from(segments, in_atx_heading, at_line_start) {
        Ok((_end_heading, _end_line_start)) => Ok(()),
        Err(err) => Err(err),
    }
}

fn reject_heading_block_filters(filters: &[super::ParsedFilter]) -> Result<(), TemplateError> {
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
                if matches!(f.sanitize_mode, Some(SanitizeFilterMode::Block { .. }))
                    || f.args.is_some() =>
            {
                return Err(TemplateError::syntax(
                    "multi-line block 'sanitize' cannot be used inside a Markdown ATX heading line",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_heading_block_filters_from(
    segments: &[Segment],
    mut in_atx_heading: bool,
    mut at_line_start: bool,
) -> Result<(bool, bool), TemplateError> {
    for seg in segments {
        match seg {
            Segment::Static(text) | Segment::Raw(text) => {
                update_heading_line_state(text, &mut in_atx_heading, &mut at_line_start);
            }
            Segment::Expr { filters, .. } => {
                if in_atx_heading {
                    reject_heading_block_filters(filters)?;
                }
                at_line_start = false;
            }
            Segment::ForLoop {
                body, else_body, ..
            } => {
                let (body_h, body_l) =
                    check_repeated_heading_body(body, in_atx_heading, at_line_start)?;
                let (else_h, else_l) = if else_body.is_empty() {
                    (in_atx_heading, at_line_start)
                } else {
                    check_heading_block_filters_from(else_body, in_atx_heading, at_line_start)?
                };
                in_atx_heading = body_h || else_h;
                at_line_start = body_l || else_l;
            }
            Segment::If {
                branches,
                else_body,
            } => {
                let mut any_heading = false;
                let mut any_line_start = false;
                for (_, branch_body) in branches {
                    let (h, l) = check_heading_block_filters_from(
                        branch_body,
                        in_atx_heading,
                        at_line_start,
                    )?;
                    any_heading |= h;
                    any_line_start |= l;
                }
                if else_body.is_empty() {
                    any_heading |= in_atx_heading;
                    any_line_start |= at_line_start;
                } else {
                    let (h, l) =
                        check_heading_block_filters_from(else_body, in_atx_heading, at_line_start)?;
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
                    let (h, l) =
                        check_heading_block_filters_from(&arm.body, in_atx_heading, at_line_start)?;
                    any_heading |= h;
                    any_line_start |= l;
                }
                if !arms.is_empty() {
                    in_atx_heading = any_heading;
                    at_line_start = any_line_start;
                }
            }
            Segment::Panic(inner) => {
                check_heading_block_filters(inner, in_atx_heading, at_line_start)?;
            }
            Segment::Include(inc) => {
                if let Some(ic) = &inc.inline_compiled {
                    let (h, l) = if inc.for_each.is_some() {
                        let (body_h, body_l) = check_repeated_heading_body(
                            &ic.segments,
                            in_atx_heading,
                            at_line_start,
                        )?;
                        (body_h || in_atx_heading, body_l || at_line_start)
                    } else {
                        check_heading_block_filters_from(
                            &ic.segments,
                            in_atx_heading,
                            at_line_start,
                        )?
                    };
                    in_atx_heading = h;
                    at_line_start = l;
                } else {
                    at_line_start = false;
                }
            }
            Segment::Comment(_) => {}
        }
    }
    Ok((in_atx_heading, at_line_start))
}

fn check_repeated_heading_body(
    segments: &[Segment],
    initial_heading: bool,
    initial_line_start: bool,
) -> Result<(bool, bool), TemplateError> {
    let mut iter_h = initial_heading;
    let mut iter_l = initial_line_start;
    let mut out_h = false;
    let mut out_l = false;
    loop {
        let (nh, nl) = check_heading_block_filters_from(segments, iter_h, iter_l)?;
        out_h |= nh;
        out_l |= nl;
        if (nh && !iter_h) || (nl && !iter_l) {
            iter_h |= nh;
            iter_l |= nl;
        } else {
            break;
        }
    }
    Ok((out_h, out_l))
}
