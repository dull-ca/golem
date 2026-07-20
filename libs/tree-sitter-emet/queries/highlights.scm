; Comments
(comment) @comment @spell

; Literals
(number) @number

(string) @string
(escape_sequence) @string.escape

; String interpolation delimiters
(interpolation_start) @punctuation.special
(interpolation_end) @punctuation.special

; Keywords
[
  "let"
  "in"
] @keyword

[
  "if"
  "then"
  "else"
  "case"
  "of"
] @keyword.conditional

"\\" @keyword.function

; Reserved glyph / output constructors
(builtin) @function.builtin

; Prelude qualified functions (List.map, Maybe.withDefault, String.fromInt, ...)
(qualified_identifier
  module: (upper_identifier) @type)
((qualified_identifier
  module: (upper_identifier) @_mod
  name: (_) @function.builtin)
 (#any-of? @_mod "List" "Maybe" "String"))
(qualified_identifier
  name: (_) @function.call)

; Declarations
(value_declaration
  name: (lower_identifier) @function)

(type_signature
  name: (lower_identifier) @function)

; Function application head
(application
  function: (variable (lower_identifier) @function.call))

; Parameters
(value_declaration
  parameters: (variable (lower_identifier) @variable.parameter))

(lambda
  parameters: (variable (lower_identifier) @variable.parameter))

; Patterns
(constructor_pattern
  constructor: (constructor) @constructor)
(wildcard_pattern) @variable.parameter

; Types
(type_constructor (upper_identifier) @type)
(type_variable (lower_identifier) @type.parameter)
(field_type name: (lower_identifier) @property)

; Records
(field_assignment name: (lower_identifier) @property)
(field_access field: (lower_identifier) @property)

; Constructors (Just, Nothing, True, False, LT/EQ/GT, and any Upper-in-expr)
(constructor (upper_identifier) @constructor)

; Variables (fallback)
(variable (lower_identifier) @variable)

; Operators
(operator) @operator

[
  "="
  "->"
  ":"
  "|"
] @operator

; Punctuation
[ "," ] @punctuation.delimiter
"." @punctuation.delimiter

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket
