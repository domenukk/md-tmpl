/**
 * Options for the direct renderer.
 *
 * @module
 */

import { type Node } from "../parser.js";
import { type VarDecl } from "../frontmatter.js";

export interface DirectRenderOptions {
  inlineTemplates?: Map<
    string,
    {
      declarations: readonly VarDecl[];
      nodes: readonly Node[];
      consts: Map<string, unknown>;
    }
  >;
  templateLoader?: (
    path: string,
    basePath?: string,
  ) =>
    | [
        readonly Node[],
        ReadonlyMap<string, unknown>,
        readonly VarDecl[],
        string?,
      ]
    | undefined;
  maxIncludeDepth?: number;
  currentBasePath?: string;
}
