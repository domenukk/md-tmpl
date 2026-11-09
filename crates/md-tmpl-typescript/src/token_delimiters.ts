/**
 * LLM control token tables and single-pass delimiter matching.
 *
 * @module
 */

/** Minimum length of any rule in `TOKEN_DELIMITERS` (`"</s>"`, `4` chars). */
const MIN_RULE_LEN = 4;
/** Maximum inner length for generic `<|...|>` / `<｜...｜>` special control tokens. */
const MAX_PIPE_TOKEN_INNER_LEN = 64;
export const ESCAPED_LT = "&lt;";
export const ESCAPED_GT = "&gt;";
const FULLWIDTH_PIPE_OPEN = "<｜";

const ROLE_DELIMITER_COUNT = 79;

/** Delimiter pairs for LLM control token neutralization across all supported dialects. */
export const TOKEN_DELIMITERS: readonly [string, string][] = [
  // ChatML / OpenAI / Qwen
  ["<|im_start|>", "&lt;|im_start|&gt;"],
  ["<|im_end|>", "&lt;|im_end|&gt;"],
  ["<|endoftext|>", "&lt;|endoftext|&gt;"],
  // Llama 3
  ["<|start_header_id|>", "&lt;|start_header_id|&gt;"],
  ["<|end_header_id|>", "&lt;|end_header_id|&gt;"],
  ["<|eot_id|>", "&lt;|eot_id|&gt;"],
  // Llama 2
  ["[INST]", "&#91;INST&#93;"],
  ["[/INST]", "&#91;/INST&#93;"],
  ["<<SYS>>", "&lt;&lt;SYS&gt;&gt;"],
  ["<</SYS>>", "&lt;&lt;/SYS&gt;&gt;"],
  ["</s>", "&lt;/s&gt;"],
  // Gemma 2 & Gemma 4
  ["<start_of_turn>", "&lt;start_of_turn&gt;"],
  ["<end_of_turn>", "&lt;end_of_turn&gt;"],
  ["<|turn>", "&lt;|turn&gt;"],
  ["<turn|>", "&lt;turn|&gt;"],
  ["<|tool>", "&lt;|tool&gt;"],
  ["<tool|>", "&lt;tool|&gt;"],
  ["<|tool_call>", "&lt;|tool_call&gt;"],
  ["<tool_call|>", "&lt;tool_call|&gt;"],
  ["<|tool_response>", "&lt;|tool_response&gt;"],
  ["<tool_response|>", "&lt;tool_response|&gt;"],
  ["<|thought", "&lt;|thought"],
  ["<thought|>", "&lt;thought|&gt;"],
  ["<|think|>", "&lt;|think|&gt;"],
  ['<|"|>', '&lt;|"|&gt;'],
  ["<bos>", "&lt;bos&gt;"],
  ["<eos>", "&lt;eos&gt;"],
  // T5 / CodeGemma / ChatGLM / GLM-4 / Cohere / Yi / InternLM
  ["<extra_id_", "&lt;extra_id_"],
  ["[gMASK]", "&#91;gMASK&#93;"],
  ["<sop>", "&lt;sop&gt;"],
  ["<eop>", "&lt;eop&gt;"],
  ["<SPECIAL_", "&lt;SPECIAL_"],
  ["<beginning_of_sentence>", "&lt;beginning_of_sentence&gt;"],
  ["<end_of_sentence>", "&lt;end_of_sentence&gt;"],
  // DeepSeek V3
  ["<｜begin▁of▁sentence｜>", "&lt;｜begin▁of▁sentence｜&gt;"],
  ["<｜end▁of▁sentence｜>", "&lt;｜end▁of▁sentence｜&gt;"],
  ["<｜begin▁of▁thought｜>", "&lt;｜begin▁of▁thought｜&gt;"],
  ["<｜end▁of▁thought｜>", "&lt;｜end▁of▁thought｜&gt;"],
  ["<｜User｜>", "&lt;｜User｜&gt;"],
  ["<｜Assistant｜>", "&lt;｜Assistant｜&gt;"],
  ["<｜tool▁calls▁begin｜>", "&lt;｜tool▁calls▁begin｜&gt;"],
  ["<｜tool▁calls▁end｜>", "&lt;｜tool▁calls▁end｜&gt;"],
  ["<｜tool▁call▁begin｜>", "&lt;｜tool▁call▁begin｜&gt;"],
  ["<｜tool▁call▁end｜>", "&lt;｜tool▁call▁end｜&gt;"],
  ["<｜tool▁sep｜>", "&lt;｜tool▁sep｜&gt;"],
  ["<｜tool▁outputs▁begin｜>", "&lt;｜tool▁outputs▁begin｜&gt;"],
  ["<｜tool▁outputs▁end｜>", "&lt;｜tool▁outputs▁end｜&gt;"],
  ["<｜tool▁output▁begin｜>", "&lt;｜tool▁output▁begin｜&gt;"],
  ["<｜tool▁output▁end｜>", "&lt;｜tool▁output▁end｜&gt;"],
  // Phi-3/4, Command-R, Bailing & Harmony
  ["<|user|>", "&lt;|user|&gt;"],
  ["<|assistant|>", "&lt;|assistant|&gt;"],
  ["<|system|>", "&lt;|system|&gt;"],
  ["<|end|>", "&lt;|end|&gt;"],
  ["<|start|>", "&lt;|start|&gt;"],
  ["<|channel|>", "&lt;|channel|&gt;"],
  ["<|message|>", "&lt;|message|&gt;"],
  ["<|call|>", "&lt;|call|&gt;"],
  ["<|return|>", "&lt;|return|&gt;"],
  ["<|constrain|>", "&lt;|constrain|&gt;"],
  ["<|role_end|>", "&lt;|role_end|&gt;"],
  ["<|START_OF_TURN_TOKEN|>", "&lt;|START_OF_TURN_TOKEN|&gt;"],
  ["<|END_OF_TURN_TOKEN|>", "&lt;|END_OF_TURN_TOKEN|&gt;"],
  ["<role>", "&lt;role&gt;"],
  ["</role>", "&lt;/role&gt;"],
  // Mistral & Ministral
  ["[SYSTEM_PROMPT]", "&#91;SYSTEM_PROMPT&#93;"],
  ["[/SYSTEM_PROMPT]", "&#91;/SYSTEM_PROMPT&#93;"],
  ["[TOOL_CALLS]", "&#91;TOOL_CALLS&#93;"],
  ["[AVAILABLE_TOOLS]", "&#91;AVAILABLE_TOOLS&#93;"],
  ["[/TOOL_CALLS]", "&#91;/TOOL_CALLS&#93;"],
  ["[/AVAILABLE_TOOLS]", "&#91;/AVAILABLE_TOOLS&#93;"],
  ["[TOOL_RESULTS]", "&#91;TOOL_RESULTS&#93;"],
  ["[/TOOL_RESULTS]", "&#91;/TOOL_RESULTS&#93;"],
  ["[TOOL_CONTENT]", "&#91;TOOL_CONTENT&#93;"],
  ["[ARGS]", "&#91;ARGS&#93;"],
  ["[/ARGS]", "&#91;/ARGS&#93;"],
  ["[THINK]", "&#91;THINK&#93;"],
  ["[/THINK]", "&#91;/THINK&#93;"],
  // Anthropic
  ["\n\nHuman:", "\n\nHuman&#58;"],
  ["\n\nAssistant:", "\n\nAssistant&#58;"],
  // Tool Calling & Qwen3-Coder / Anthropic XML
  ["<tool_call>", "&lt;tool_call&gt;"],
  ["</tool_call>", "&lt;/tool_call&gt;"],
  ["<tool_response>", "&lt;tool_response&gt;"],
  ["</tool_response>", "&lt;/tool_response&gt;"],
  ["<function=", "&lt;function="],
  ["</function>", "&lt;/function&gt;"],
  ["<function>", "&lt;function&gt;"],
  ["<parameter=", "&lt;parameter="],
  ["</parameter>", "&lt;/parameter&gt;"],
  ["<tools>", "&lt;tools&gt;"],
  ["</tools>", "&lt;/tools&gt;"],
  ["<function_calls>", "&lt;function_calls&gt;"],
  ["</function_calls>", "&lt;/function_calls&gt;"],
  ["<function_results>", "&lt;function_results&gt;"],
  ["</function_results>", "&lt;/function_results&gt;"],
  // Reasoning XML
  ["<think>", "&lt;think&gt;"],
  ["</think>", "&lt;/think&gt;"],
  // Quarantine envelopes
  ["<untrusted_tool_output>", "&lt;untrusted_tool_output&gt;"],
  ["</untrusted_tool_output>", "&lt;/untrusted_tool_output&gt;"],
  ["<tool_output_quarantine>", "&lt;tool_output_quarantine&gt;"],
  ["</tool_output_quarantine>", "&lt;/tool_output_quarantine&gt;"],
  ["<untrusted_content>", "&lt;untrusted_content&gt;"],
  ["</untrusted_content>", "&lt;/untrusted_content&gt;"],
];

/** Subset of `TOKEN_DELIMITERS` covering role/turn headers and special control tokens. */
export const ROLE_TOKEN_DELIMITERS: readonly [string, string][] =
  TOKEN_DELIMITERS.slice(0, ROLE_DELIMITER_COUNT);

/** Anthropic `\n\nHuman:` / `\n\nAssistant:` turn markers (matched anywhere). */
const ANTHROPIC_TURN_RULES: readonly [string, string][] = [
  ["\n\nHuman:", "\n\nHuman&#58;"],
  ["\n\nAssistant:", "\n\nAssistant&#58;"],
];

/** Single-newline `\nHuman:` / `\nAssistant:` turn markers (matched only at input start). */
const ANTHROPIC_START_TURN_RULES: readonly [string, string][] = [
  ["\nHuman:", "\nHuman&#58;"],
  ["\nAssistant:", "\nAssistant&#58;"],
];

const BRACKET_TOKEN_RULES: readonly [string, string][] =
  ROLE_TOKEN_DELIMITERS.filter(([pat]) => pat.charCodeAt(0) === 91 /* '[' */);

function isBracketSecondCode(c1: number): boolean {
  return (
    c1 === 73 /* 'I' */ ||
    c1 === 47 /* '/' */ ||
    c1 === 103 /* 'g' */ ||
    c1 === 83 /* 'S' */ ||
    c1 === 84 /* 'T' */ ||
    c1 === 65 /* 'A' */
  );
}

function isAngleSecondCode(c1: number): boolean {
  return (
    c1 === 124 /* '|' */ ||
    c1 === 0xff5c /* '｜' */ ||
    c1 === 47 /* '/' */ ||
    c1 === 60 /* '<' */ ||
    c1 === 115 /* 's' */ ||
    c1 === 101 /* 'e' */ ||
    c1 === 116 /* 't' */ ||
    c1 === 98 /* 'b' */ ||
    c1 === 83 /* 'S' */ ||
    c1 === 114 /* 'r' */ ||
    c1 === 102 /* 'f' */ ||
    c1 === 112 /* 'p' */ ||
    c1 === 117 /* 'u' */
  );
}

export function isAsciiWhitespaceCode(c: number): boolean {
  return (
    c === 32 /* ' ' */ ||
    c === 9 /* '\t' */ ||
    c === 10 /* '\n' */ ||
    c === 12 /* '\f' */ ||
    c === 13 /* '\r' */
  );
}

function isPipeTokenForbiddenCode(code: number): boolean {
  return (
    isAsciiWhitespaceCode(code) ||
    code === 60 /* '<' */ ||
    code === 62 /* '>' */
  );
}

function matchGenericPipeToken(s: string, i: number): number | undefined {
  const ASCII_PIPE_CODE = 124;
  const FULLWIDTH_PIPE_CODE = 0xff5c;
  const ASCII_GT_CODE = 62;
  let openLen: number;
  let closePipeCode: number;
  if (s.startsWith("<|", i)) {
    openLen = 2;
    closePipeCode = ASCII_PIPE_CODE;
  } else if (s.startsWith(FULLWIDTH_PIPE_OPEN, i)) {
    openLen = FULLWIDTH_PIPE_OPEN.length;
    closePipeCode = FULLWIDTH_PIPE_CODE;
  } else {
    return undefined;
  }
  const start = i + openLen;
  const maxEnd = Math.min(s.length - 1, start + MAX_PIPE_TOKEN_INNER_LEN);
  let utf8Bytes = 0;
  for (let pos = start; pos <= maxEnd; pos++) {
    const code = s.charCodeAt(pos);
    if (code === closePipeCode && s.charCodeAt(pos + 1) === ASCII_GT_CODE) {
      return utf8Bytes > 0 ? pos + 2 : undefined;
    }
    if (isPipeTokenForbiddenCode(code)) {
      return undefined;
    }
    if (code < 0x80) {
      utf8Bytes += 1;
    } else if (code < 0x800) {
      utf8Bytes += 2;
    } else if (
      code >= 0xd800 &&
      code <= 0xdbff &&
      pos + 1 <= maxEnd &&
      s.charCodeAt(pos + 1) >= 0xdc00 &&
      s.charCodeAt(pos + 1) <= 0xdfff
    ) {
      utf8Bytes += 4;
      pos++;
    } else {
      utf8Bytes += 3;
    }
    if (utf8Bytes > MAX_PIPE_TOKEN_INNER_LEN) {
      return undefined;
    }
  }
  return undefined;
}

export type ControlTokenMatch =
  | {
      readonly kind: "rule";
      readonly len: number;
      readonly replacement: string;
    }
  | { readonly kind: "pipe"; readonly endIdx: number };

export function matchControlTokenAt(
  s: string,
  i: number,
  rules: readonly [string, string][],
): ControlTokenMatch | undefined {
  if (i + MIN_RULE_LEN > s.length) {
    return undefined;
  }
  const c0 = s.charCodeAt(i);
  const c1 = s.charCodeAt(i + 1);
  if (c0 === 10 /* '\n' */) {
    // `\n\nHuman:` anywhere, or `\nHuman:` at the very start of the input.
    let turnRules: readonly [string, string][] = [];
    if (c1 === 10) {
      turnRules = ANTHROPIC_TURN_RULES;
    } else if (i === 0) {
      turnRules = ANTHROPIC_START_TURN_RULES;
    }
    for (const [pat, replacement] of turnRules) {
      if (s.startsWith(pat, i)) {
        return { kind: "rule", len: pat.length, replacement };
      }
    }
    return undefined;
  }
  if (c0 === 91 /* '[' */) {
    if (!isBracketSecondCode(c1)) {
      return undefined;
    }
    const c2 = s.charCodeAt(i + 2);
    const c3 = s.charCodeAt(i + 3);
    for (const [pat, replacement] of BRACKET_TOKEN_RULES) {
      if (
        pat.charCodeAt(1) === c1 &&
        pat.charCodeAt(2) === c2 &&
        pat.charCodeAt(3) === c3 &&
        s.startsWith(pat, i)
      ) {
        return { kind: "rule", len: pat.length, replacement };
      }
    }
    return undefined;
  }
  if (c0 === 60 /* '<' */) {
    if (!isAngleSecondCode(c1)) {
      return undefined;
    }
    if (c1 === 124 /* '|' */ || c1 === 0xff5c /* '｜' */) {
      const endIdx = matchGenericPipeToken(s, i);
      if (endIdx !== undefined) {
        return { kind: "pipe", endIdx };
      }
    }
    const c2 = s.charCodeAt(i + 2);
    const c3 = s.charCodeAt(i + 3);
    for (const [pat, replacement] of rules) {
      if (
        pat.charCodeAt(0) === 60 &&
        pat.charCodeAt(1) === c1 &&
        pat.charCodeAt(2) === c2 &&
        pat.charCodeAt(3) === c3 &&
        s.startsWith(pat, i)
      ) {
        return { kind: "rule", len: pat.length, replacement };
      }
    }
  }
  return undefined;
}
