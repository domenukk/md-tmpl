/*
 * md_tmpl.h - C FFI bindings for the md-tmpl engine.
 *
 * All types are opaque pointers; callers allocate and free handles
 * through the pt_* function family.
 *
 * Error convention: functions that can fail return a char* error string.
 * A NULL return means success. The caller owns the error string and must
 * free it with pt_free_string().
 *
 * Typed errors: error strings produced from a template error are formatted as
 * "<kind>\x1f<message>", where <kind> is a stable machine-readable id (e.g.
 * "missing_params", "type_mismatch", "syntax") and \x1f is the ASCII Unit
 * Separator (0x1F), which never appears in messages. Bindings may split on it
 * to recover the kind; errors without the separator are plain messages.
 */

#ifndef MD_TMPL_H
#define MD_TMPL_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handles. */
typedef struct PtTemplate PtTemplate;
typedef struct PtContext PtContext;
typedef struct PtCache PtCache;

/* ---- String lifecycle --------------------------------------------------- */

void pt_free_string(char *ptr);

/* ---- Template lifecycle ------------------------------------------------- */

char *pt_template_from_source(const char *source, PtTemplate **out);
char *pt_template_from_source_allowing_unused(const char *source,
                                              PtTemplate **out);
char *pt_template_from_source_with_base_dir(const char *source,
                                            const char *base_dir,
                                            PtTemplate **out);
char *pt_template_from_source_with_frontmatter(const char *source,
                                               PtTemplate **out_tmpl,
                                               char **out_fm);
/*
 * Unified constructors: base_dir and env_json are optional (pass NULL to omit),
 * allow_unused toggles the unused-parameter check. Unlike the single-purpose
 * constructors above, these allow all options to be combined in one call.
 */
char *pt_template_from_source_with_options(const char *source,
                                           const char *base_dir,
                                           const char *env_json,
                                           bool allow_unused, PtTemplate **out);
char *pt_template_from_file_with_options(const char *path,
                                         const char *base_dir,
                                         const char *env_json,
                                         bool allow_unused, PtTemplate **out);
char *pt_template_from_file(const char *path, PtTemplate **out);
void pt_template_free(PtTemplate *tmpl);

/* ---- Context lifecycle -------------------------------------------------- */

PtContext *pt_context_new(void);
void pt_context_free(PtContext *ctx);
char *pt_context_set_str(PtContext *ctx, const char *key, const char *value);
char *pt_context_set_int(PtContext *ctx, const char *key, int64_t value);
char *pt_context_set_float(PtContext *ctx, const char *key, double value);
char *pt_context_set_bool(PtContext *ctx, const char *key, bool value);
char *pt_context_set_none(PtContext *ctx, const char *key);
char *pt_context_set_json(PtContext *ctx, const char *key, const char *json);
char *pt_context_set_tmpl(PtContext *ctx, const char *key,
                          const PtTemplate *tmpl);
char *pt_context_merge_json(PtContext *ctx, const char *json);
char *pt_context_set_flexbuffers(PtContext *ctx, const char *key,
                                 const uint8_t *data, size_t len);
char *pt_context_merge_flexbuffers(PtContext *ctx, const uint8_t *data,
                                   size_t len);

/* ---- Rendering ---------------------------------------------------------- */

char *pt_template_render(const PtTemplate *tmpl, const PtContext *ctx,
                         char **out_err);
char *pt_template_render_allowing_extra(const PtTemplate *tmpl,
                                        const PtContext *ctx,
                                        char **out_err);

/**
 * Single-shot render from a JSON object string.
 *
 * Parses JSON, builds a context, and renders — all in one FFI call.
 * When allow_extra is true, undeclared keys are silently ignored.
 *
 * Returns the rendered string (caller frees with pt_free_string) or NULL
 * on error (error written to *out_err, caller frees).
 */
char *pt_template_render_json(const PtTemplate *tmpl, const char *json,
                              bool allow_extra, char **out_err);

/**
 * Single-shot render from a FlexBuffers binary map.
 *
 * Deserializes FlexBuffers, builds a context, and renders — all in one FFI call.
 * When allow_extra is true, undeclared keys are silently ignored.
 *
 * Returns the rendered string (caller frees with pt_free_string) or NULL
 * on error (error written to *out_err, caller frees).
 */
char *pt_template_render_flexbuffers(const PtTemplate *tmpl,
                                     const uint8_t *data, size_t len,
                                     bool allow_extra, char **out_err);

/* Render a parameter-less template with an empty context. */
char *pt_template_render_empty(const PtTemplate *tmpl, char **out_err);

/* Render without context validation (caller guarantees a valid context). */
char *pt_template_render_unchecked(const PtTemplate *tmpl, const PtContext *ctx,
                                  char **out_err);

/* Render, resolving {% include %} directives through the given cache. */
char *pt_template_render_cached(const PtTemplate *tmpl, const PtContext *ctx,
                               const PtCache *cache, char **out_err);

/* ---- Template metadata -------------------------------------------------- */

uint64_t pt_template_source_hash(const PtTemplate *tmpl);
/* Name/description from frontmatter; NULL if absent (caller frees non-NULL). */
char *pt_template_name(const PtTemplate *tmpl);
char *pt_template_description(const PtTemplate *tmpl);
char *pt_template_body(const PtTemplate *tmpl);
char *pt_template_declarations(const PtTemplate *tmpl);
void pt_template_set_max_include_depth(PtTemplate *tmpl, size_t depth);
char *pt_template_defaults_json(const PtTemplate *tmpl);
char *pt_template_consts_json(const PtTemplate *tmpl);
char *pt_template_imported_consts_json(const PtTemplate *tmpl);
PtContext *pt_template_defaults_context(const PtTemplate *tmpl);
char *pt_template_validate_declarations(const PtTemplate *tmpl,
                                        const char *expected_json);

/* ---- Cache lifecycle ---------------------------------------------------- */

PtCache *pt_cache_new(void);
void pt_cache_free(PtCache *cache);
char *pt_cache_load(const PtCache *cache, const char *path, PtTemplate **out);
void pt_cache_clear(const PtCache *cache);
size_t pt_cache_template_count(const PtCache *cache);
size_t pt_cache_include_count(const PtCache *cache);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MD_TMPL_H */
