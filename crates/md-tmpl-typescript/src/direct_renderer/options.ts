/**
 * Options for the direct renderer.
 *
 * @module
 */

import { type Node } from "../parser.js";
import { type VarDecl } from "../frontmatter.js";

export interface DirectInlineTemplate {
  readonly declarations: readonly VarDecl[];
  readonly nodes: readonly Node[];
  readonly consts?: ReadonlyMap<string, unknown>;
  readonly constsJs?: ReadonlyMap<string, unknown>;
}

export interface DirectRenderOptions {
  inlineTemplates?: ReadonlyMap<string, DirectInlineTemplate>;
  templateLoader?: (
    path: string,
    basePath?: string,
  ) =>
    | [
        readonly Node[],
        ReadonlyMap<string, unknown>,
        readonly VarDecl[],
        string?,
        ReadonlyMap<string, DirectInlineTemplate>?,
      ]
    | undefined;
  maxIncludeDepth?: number;
  currentBasePath?: string;
}
