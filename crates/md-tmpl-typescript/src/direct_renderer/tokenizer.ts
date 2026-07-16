/**
 * Tokenizer for direct-renderer condition expressions.
 *
 * Re-exports the shared condition tokenizer with the legacy
 * Direct-prefixed names for backward compatibility.
 *
 * @module
 */

export {
  TokKind as DirectTokKind,
  type Token as DirectToken,
  tokenizeCondition as tokenizeDirectCondition,
  isIdentStart as isDirectIdentStart,
  isIdentChar as isDirectIdentChar,
  isOperatorToken as isDirectOperatorToken,
} from "../condition_tokenizer.js";
