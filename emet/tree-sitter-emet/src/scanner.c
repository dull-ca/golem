#include "tree_sitter/parser.h"

enum TokenType {
  DECL_BOUNDARY,
  LINE_BOUNDARY,
};

void *tree_sitter_emet_external_scanner_create(void) { return NULL; }
void tree_sitter_emet_external_scanner_destroy(void *payload) {}
unsigned tree_sitter_emet_external_scanner_serialize(void *payload, char *buffer) { return 0; }
void tree_sitter_emet_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {}

static bool is_horizontal_space(int32_t c) { return c == ' ' || c == '\t' || c == '\r'; }

bool tree_sitter_emet_external_scanner_scan(void *payload, TSLexer *lexer,
                                             const bool *valid_symbols) {
  if (!valid_symbols[DECL_BOUNDARY] && !valid_symbols[LINE_BOUNDARY]) return false;

  bool saw_newline = false;

  for (;;) {
    if (lexer->eof(lexer)) break;
    int32_t c = lexer->lookahead;
    if (c == '\n') {
      saw_newline = true;
      lexer->advance(lexer, true);
    } else if (is_horizontal_space(c)) {
      lexer->advance(lexer, true);
    } else {
      break;
    }
  }

  if (!saw_newline) return false;

  bool at_column_zero = lexer->eof(lexer) || lexer->get_column(lexer) == 0;

  if (valid_symbols[DECL_BOUNDARY] && at_column_zero) {
    lexer->result_symbol = DECL_BOUNDARY;
    return true;
  }

  if (valid_symbols[LINE_BOUNDARY]) {
    lexer->result_symbol = LINE_BOUNDARY;
    return true;
  }

  return false;
}
