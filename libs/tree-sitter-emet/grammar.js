const PREC = {
  or: 2,
  and: 3,
  compare: 4,
  append: 5,
  add: 6,
  mul: 7,
  power: 8,
  apply: 10,
  access: 11,
};

module.exports = grammar({
  name: 'emet',

  extras: ($) => [/\s/, $.comment],

  externals: ($) => [$._decl_boundary, $._line_boundary],

  word: ($) => $.lower_identifier,

  rules: {
    source_file: ($) => repeat(choice($._decl_boundary, $._declaration)),

    comment: ($) => token(seq('--', /.*/)),

    _declaration: ($) =>
      choice($.type_signature, $.value_declaration),

    type_signature: ($) =>
      seq(field('name', $.lower_identifier), ':', field('type', $._type)),

    value_declaration: ($) =>
      seq(
        field('name', $.lower_identifier),
        field('parameters', repeat($._pattern_atom)),
        '=',
        field('body', $._expression),
      ),

    // ---------------------------------------------------------------- types
    _type: ($) => choice($.function_type, $._type_application),

    function_type: ($) =>
      prec.right(seq($._type_application, '->', $._type)),

    _type_application: ($) =>
      choice($.type_application, $._type_atom),

    type_application: ($) =>
      prec.left(
        PREC.apply,
        seq($._type_application, $._type_atom),
      ),

    _type_atom: ($) =>
      choice(
        $.type_constructor,
        $.type_variable,
        $.record_type,
        $.parenthesized_type,
      ),

    parenthesized_type: ($) => seq('(', $._type, ')'),

    type_constructor: ($) => $.upper_identifier,
    type_variable: ($) => $.lower_identifier,

    record_type: ($) =>
      seq(
        '{',
        optional(seq(field('base', $.type_variable), '|')),
        commaSep($.field_type),
        '}',
      ),

    field_type: ($) =>
      seq(field('name', $.lower_identifier), ':', field('type', $._type)),

    // ---------------------------------------------------------- expressions
    _expression: ($) => choice($.binary_expression, $._application),

    binary_expression: ($) => {
      const table = [
        [prec.right, PREC.power, '^'],
        [prec.left, PREC.mul, choice('*', '//', '/')],
        [prec.left, PREC.add, choice('+', '-')],
        [prec.right, PREC.append, '++'],
        [prec.left, PREC.compare, choice('==', '/=', '<=', '>=', '<', '>')],
        [prec.right, PREC.and, '&&'],
        [prec.right, PREC.or, '||'],
      ];
      return choice(
        ...table.map(([fn, precedence, operator]) =>
          fn(
            precedence,
            seq(
              field('left', $._expression),
              field('operator', alias(operator, $.operator)),
              field('right', $._expression),
            ),
          ),
        ),
      );
    },

    _application: ($) => choice($.application, $._expression_atom),

    application: ($) =>
      prec.left(
        PREC.apply,
        seq(
          field('function', $._application),
          field('argument', $._expression_atom),
        ),
      ),

    _expression_atom: ($) =>
      choice(
        $.field_access,
        $._simple_atom,
      ),

    field_access: ($) =>
      prec.left(
        PREC.access,
        seq($._simple_atom, repeat1(seq('.', field('field', $.lower_identifier)))),
      ),

    _simple_atom: ($) =>
      choice(
        $.let_expression,
        $.if_expression,
        $.case_expression,
        $.lambda,
        $.record_expression,
        $.list_expression,
        $.parenthesized_expression,
        $.string,
        $.number,
        $.builtin,
        $.constructor,
        $.qualified_identifier,
        $.variable,
      ),

    parenthesized_expression: ($) => seq('(', $._expression, ')'),

    let_expression: ($) =>
      seq(
        'let',
        repeat1($.value_declaration),
        'in',
        field('body', $._expression),
      ),

    if_expression: ($) =>
      seq(
        'if',
        field('condition', $._expression),
        'then',
        field('consequence', $._expression),
        'else',
        field('alternative', $._expression),
      ),

    case_expression: ($) =>
      prec.right(
        seq(
          'case',
          field('value', $._expression),
          'of',
          $.case_arm,
          repeat(seq($._line_boundary, $.case_arm)),
        ),
      ),

    case_arm: ($) =>
      seq(
        field('pattern', $._pattern),
        '->',
        field('body', $._expression),
      ),

    lambda: ($) =>
      seq(
        '\\',
        field('parameters', repeat1($._pattern_atom)),
        '->',
        field('body', $._expression),
      ),

    record_expression: ($) =>
      seq('{', commaSep($.field_assignment), '}'),

    field_assignment: ($) =>
      seq(field('name', $.lower_identifier), '=', field('value', $._expression)),

    list_expression: ($) =>
      seq('[', commaSep($._expression), ']'),

    // ------------------------------------------------------------- patterns
    _pattern: ($) => choice($.constructor_pattern, $._pattern_atom),

    constructor_pattern: ($) =>
      prec.left(
        PREC.apply,
        seq(field('constructor', $.constructor), repeat1($._pattern_atom)),
      ),

    _pattern_atom: ($) =>
      choice(
        $.constructor,
        $.variable,
        $.wildcard_pattern,
        $.record_expression,
        $.list_expression,
        $.string,
        $.number,
        seq('(', $._pattern, ')'),
      ),

    wildcard_pattern: ($) => '_',

    // ---------------------------------------------------------- identifiers
    builtin: ($) =>
      choice(
        'aptPackage',
        'systemdService',
        'file',
        'lineInFile',
        'scroll',
      ),

    qualified_identifier: ($) =>
      seq(
        field('module', $.upper_identifier),
        token.immediate('.'),
        field('name', token.immediate(/[a-z][a-zA-Z0-9_]*/)),
      ),

    constructor: ($) => $.upper_identifier,
    variable: ($) => $.lower_identifier,

    lower_identifier: ($) => /[a-z][a-zA-Z0-9_]*/,
    upper_identifier: ($) => /[A-Z][a-zA-Z0-9_]*/,

    // --------------------------------------------------------------- number
    number: ($) => token(seq(/[0-9]+/, optional(seq('.', /[0-9]+/)))),

    // --------------------------------------------------------------- string
    string: ($) =>
      seq(
        '"',
        repeat(
          choice(
            $.string_content,
            $.escape_sequence,
            $.interpolation,
          ),
        ),
        '"',
      ),

    string_content: ($) =>
      choice(
        token.immediate(prec(1, /[^"\\$]+/)),
        token.immediate('$'),
      ),

    escape_sequence: ($) => token.immediate(/\\./),

    interpolation: ($) =>
      seq(
        alias(token.immediate('${'), $.interpolation_start),
        $._expression,
        alias('}', $.interpolation_end),
      ),
  },
});

function sepBy(sep, rule) {
  return seq(rule, repeat(seq(sep, rule)));
}

function commaSep(rule) {
  return optional(commaSep1(rule));
}

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}
