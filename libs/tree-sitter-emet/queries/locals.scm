; Scopes
(value_declaration) @local.scope
(let_expression) @local.scope
(lambda) @local.scope
(case_arm) @local.scope

; Definitions
(value_declaration
  name: (lower_identifier) @local.definition.function)

(value_declaration
  parameters: (variable (lower_identifier) @local.definition.parameter))

(lambda
  parameters: (variable (lower_identifier) @local.definition.parameter))

; References
(variable (lower_identifier) @local.reference)
