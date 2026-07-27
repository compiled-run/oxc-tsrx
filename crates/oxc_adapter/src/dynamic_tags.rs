use std::{error::Error, fmt};

use oxc_ast::ast::{
    ArrayExpression, BinaryExpression, CallExpression, ChainElement, Expression, JSXAttributeValue,
    JSXOpeningElement, NewExpression, ObjectExpression, SpreadElement, TaggedTemplateExpression,
    TemplateLiteral,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::{BinaryOperator, UnaryOperator};

use crate::DynamicTagContract;

#[cfg(feature = "toolchain")]
pub(crate) fn validate_dynamic_tags(
    program: &oxc_ast::ast::Program<'_>,
    contract: Option<DynamicTagContract<'_>>,
) -> Result<(), DynamicTagError> {
    validate_dynamic_tags_with_synthetic_calls(program, contract, &[])
}

/// Why a TSRX dynamic-tag scaffold did not validate against the parsed OXC AST.
///
/// Exactly one variant, [`Self::AuthoredGrammar`], is the user's defect: it names a tag expression
/// the TSRX grammar does not accept and positions it in the authored source. Every other variant
/// describes an inconsistent scaffold contract, which is a projector or adapter defect rather than
/// anything an author wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicTagError {
    /// The authored expression is not one of the shapes a TSRX dynamic tag may hold.
    AuthoredGrammar { index: usize, offset: u32 },
    /// The synthetic callee spans were not handed over in ascending order.
    UnorderedSyntheticCallees,
    /// The contract claims more dynamic tags than this target can address.
    CountExceedsAddressableMemory,
    /// The contract has no prefix, no tags, or an offset list that does not match its count.
    EmptyContract,
    /// A prefixed element name did not parse as a scaffold ordinal.
    MalformedScaffold,
    /// A scaffold ordinal is out of range or was seen twice.
    InvalidScaffold { index: usize },
    /// Canonical OXC's parse did not preserve one of the scaffolds the projector emitted.
    LostScaffold { index: usize },
    /// The end sentinel does not belong to the scaffold that opened it.
    MismatchedEndScaffold { index: usize },
    /// A prefixed attribute name did not parse as a scaffold ordinal.
    MalformedAttribute { index: usize },
    /// The expression attribute does not belong to the scaffold that opened it.
    MismatchedAttribute { index: usize },
    /// The scaffold carries no dynamic-tag expression.
    MissingExpression { index: usize },
    /// The scaffold was never closed by its end sentinel.
    MissingEndScaffold { index: usize },
}

impl DynamicTagError {
    /// The authored-source UTF-8 byte offset this failure points at, when it has one.
    ///
    /// Only [`Self::AuthoredGrammar`] is positioned; the rest describe a whole-contract defect
    /// with no authored location. This accessor exists so a caller that needs the position never
    /// has to scrape it back out of the [`fmt::Display`] text.
    #[must_use]
    pub const fn byte_offset(&self) -> Option<u32> {
        match self {
            Self::AuthoredGrammar { offset, .. } => Some(*offset),
            _ => None,
        }
    }
}

impl fmt::Display for DynamicTagError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthoredGrammar { index, offset } => write!(
                formatter,
                "TSRX dynamic tag {index} at source byte {offset} must be an identifier, member, static string, or runtime expression without calls, construction, spreads, concatenation, interpolation, objects, or arrays"
            ),
            Self::UnorderedSyntheticCallees => {
                formatter.write_str("unordered synthetic callee span contract")
            }
            Self::CountExceedsAddressableMemory => {
                formatter.write_str("TSRX dynamic-tag count exceeds addressable memory")
            }
            Self::EmptyContract => {
                formatter.write_str("invalid empty TSRX dynamic-tag scaffold contract")
            }
            Self::MalformedScaffold => formatter.write_str("malformed TSRX dynamic-tag scaffold"),
            Self::InvalidScaffold { index } => {
                write!(formatter, "invalid TSRX dynamic-tag scaffold {index}")
            }
            Self::LostScaffold { index } => {
                write!(formatter, "OXC parse lost TSRX dynamic-tag scaffold {index}")
            }
            Self::MismatchedEndScaffold { index } => {
                write!(formatter, "mismatched TSRX dynamic-tag end scaffold {index}")
            }
            Self::MalformedAttribute { index } => {
                write!(formatter, "malformed TSRX dynamic-tag attribute {index}")
            }
            Self::MismatchedAttribute { index } => {
                write!(formatter, "mismatched TSRX dynamic-tag attribute {index}")
            }
            Self::MissingExpression { index } => {
                write!(formatter, "missing TSRX dynamic-tag expression {index}")
            }
            Self::MissingEndScaffold { index } => {
                write!(formatter, "missing TSRX dynamic-tag end scaffold {index}")
            }
        }
    }
}

impl Error for DynamicTagError {}

pub(crate) fn validate_dynamic_tags_with_synthetic_calls(
    program: &oxc_ast::ast::Program<'_>,
    contract: Option<DynamicTagContract<'_>>,
    synthetic_callee_spans: &[(u32, u32)],
) -> Result<(), DynamicTagError> {
    if synthetic_callee_spans.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(DynamicTagError::UnorderedSyntheticCallees);
    }
    let Some(contract) = contract else {
        return Ok(());
    };
    let count = usize::try_from(contract.count)
        .map_err(|_| DynamicTagError::CountExceedsAddressableMemory)?;
    if count == 0 || contract.prefix.is_empty() || contract.original_offsets.len() != count {
        return Err(DynamicTagError::EmptyContract);
    }
    let mut validator = DynamicTagValidator {
        prefix: contract.prefix,
        original_offsets: contract.original_offsets,
        synthetic_callee_spans,
        seen: vec![false; count],
        validated_expression: None,
        error: None,
    };
    validator.visit_program(program);
    if let Some(error) = validator.error {
        return Err(error);
    }
    if let Some(index) = validator.seen.iter().position(|seen| !seen) {
        return Err(DynamicTagError::LostScaffold { index });
    }
    Ok(())
}

struct DynamicTagValidator<'c> {
    prefix: &'c str,
    original_offsets: &'c [u32],
    synthetic_callee_spans: &'c [(u32, u32)],
    seen: Vec<bool>,
    validated_expression: Option<Span>,
    error: Option<DynamicTagError>,
}

impl<'a> Visit<'a> for DynamicTagValidator<'_> {
    fn visit_jsx_opening_element(&mut self, element: &JSXOpeningElement<'a>) {
        if self.error.is_some() {
            return;
        }
        let Some(name) = element.name.get_identifier_name() else {
            walk::walk_jsx_opening_element(self, element);
            return;
        };
        let Some(index) = scaffold_ordinal(name.as_str(), self.prefix, 'D', false) else {
            if name.as_str().starts_with(self.prefix) {
                self.error = Some(DynamicTagError::MalformedScaffold);
                return;
            }
            walk::walk_jsx_opening_element(self, element);
            return;
        };
        let index = index as usize;
        if index >= self.seen.len() || self.seen[index] {
            self.error = Some(DynamicTagError::InvalidScaffold { index });
            return;
        }

        let expression = match dynamic_tag_expression(element, self.prefix, index) {
            Ok(expression) => expression,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let expression_span = expression.span();
        let contained = self.validated_expression.is_some_and(|validated| {
            validated.start <= expression_span.start && expression_span.end <= validated.end
        });
        let overlapping = self
            .validated_expression
            .is_some_and(|validated| expression_span.start < validated.end && !contained);
        if !is_valid_dynamic_root(expression)
            || root_is_synthetic_call(expression, self.synthetic_callee_spans)
            || overlapping
            || !contained
                && has_disallowed_dynamic_syntax(
                    expression,
                    self.prefix,
                    self.synthetic_callee_spans,
                )
        {
            self.error = Some(DynamicTagError::AuthoredGrammar {
                index,
                offset: self.original_offsets[index],
            });
            return;
        }
        if !contained {
            self.validated_expression = Some(expression_span);
        }
        self.seen[index] = true;
        walk::walk_jsx_opening_element(self, element);
    }
}

fn dynamic_tag_expression<'a, 'element>(
    element: &'element JSXOpeningElement<'a>,
    prefix: &str,
    index: usize,
) -> Result<&'element Expression<'a>, DynamicTagError> {
    let mut expression = None;
    let mut end_sentinel = false;
    for item in &element.attributes {
        let Some(attribute) = item.as_attribute() else {
            continue;
        };
        let Some(identifier) = attribute.name.as_identifier() else {
            continue;
        };
        let name = identifier.name.as_str();
        if let Some(attribute_index) = scaffold_ordinal(name, prefix, 'Z', true) {
            let valid_value = attribute.value.as_ref().is_some_and(|value| {
                matches!(
                    value,
                    JSXAttributeValue::ExpressionContainer(container)
                        if matches!(container.expression.as_expression(), Some(Expression::NullLiteral(_)))
                )
            });
            if attribute_index as usize != index || end_sentinel || !valid_value {
                return Err(DynamicTagError::MismatchedEndScaffold { index });
            }
            end_sentinel = true;
            continue;
        }
        let Some(attribute_index) = scaffold_ordinal(name, prefix, 'A', true) else {
            if name.starts_with(prefix) {
                return Err(DynamicTagError::MalformedAttribute { index });
            }
            continue;
        };
        if attribute_index as usize != index || expression.is_some() {
            return Err(DynamicTagError::MismatchedAttribute { index });
        }
        expression = attribute.value.as_ref().and_then(|value| match value {
            JSXAttributeValue::ExpressionContainer(container) => {
                container.expression.as_expression()
            }
            _ => None,
        });
    }
    let expression = expression.ok_or(DynamicTagError::MissingExpression { index })?;
    if !end_sentinel {
        return Err(DynamicTagError::MissingEndScaffold { index });
    }
    Ok(expression)
}

fn scaffold_ordinal(name: &str, prefix: &str, kind: char, suffix: bool) -> Option<u32> {
    let rest = name.strip_prefix(prefix)?.strip_prefix(kind)?;
    let digits = if suffix { rest.strip_suffix('_')? } else { rest };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn has_disallowed_dynamic_syntax(
    expression: &Expression<'_>,
    prefix: &str,
    synthetic_callee_spans: &[(u32, u32)],
) -> bool {
    let mut validator = DisallowedDynamicSyntax {
        prefix,
        synthetic_callee_spans,
        found: false,
        allow_synthetic_object: false,
    };
    validator.visit_expression(expression);
    validator.found
}

fn root_is_synthetic_call(
    mut expression: &Expression<'_>,
    synthetic_callee_spans: &[(u32, u32)],
) -> bool {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(wrapper) => &wrapper.expression,
            Expression::TSAsExpression(wrapper) => &wrapper.expression,
            Expression::TSTypeAssertion(wrapper) => &wrapper.expression,
            Expression::TSNonNullExpression(wrapper) => &wrapper.expression,
            _ => break,
        };
    }
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    let span = call.callee.span();
    synthetic_callee_spans.binary_search(&(span.start, span.end)).is_ok()
}

fn is_valid_dynamic_root(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::ParenthesizedExpression(wrapper) => is_valid_dynamic_root(&wrapper.expression),
        Expression::TSAsExpression(wrapper) => is_valid_dynamic_root(&wrapper.expression),
        Expression::TSTypeAssertion(wrapper) => is_valid_dynamic_root(&wrapper.expression),
        Expression::TSNonNullExpression(wrapper) => is_valid_dynamic_root(&wrapper.expression),
        Expression::ChainExpression(wrapper) => match &wrapper.expression {
            ChainElement::CallExpression(_) => false,
            ChainElement::TSNonNullExpression(inner) => is_valid_dynamic_root(&inner.expression),
            _ => true,
        },
        Expression::Identifier(identifier) => identifier.name != "undefined",
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::JSXElement(_)
        | Expression::JSXFragment(_) => false,
        Expression::UnaryExpression(unary) => unary.operator != UnaryOperator::Void,
        _ => true,
    }
}

struct DisallowedDynamicSyntax<'a> {
    prefix: &'a str,
    synthetic_callee_spans: &'a [(u32, u32)],
    found: bool,
    allow_synthetic_object: bool,
}

impl<'a> Visit<'a> for DisallowedDynamicSyntax<'_> {
    fn visit_array_expression(&mut self, _expression: &ArrayExpression<'a>) {
        self.found = true;
    }

    fn visit_object_expression(&mut self, expression: &ObjectExpression<'a>) {
        if std::mem::take(&mut self.allow_synthetic_object) {
            walk::walk_object_expression(self, expression);
        } else {
            self.found = true;
        }
    }

    fn visit_call_expression(&mut self, expression: &CallExpression<'a>) {
        let callee_span = expression.callee.span();
        let synthetic = self
            .synthetic_callee_spans
            .binary_search(&(callee_span.start, callee_span.end))
            .is_ok();
        if synthetic {
            self.allow_synthetic_object = match &expression.callee {
                Expression::Identifier(identifier) => {
                    let name = identifier.name.as_str();
                    scaffold_ordinal(name, self.prefix, 'W', true).is_some()
                        || scaffold_ordinal(name, self.prefix, 'T', true).is_some()
                }
                _ => false,
            };
            walk::walk_call_expression(self, expression);
            if self.allow_synthetic_object {
                self.allow_synthetic_object = false;
                self.found = true;
            }
        } else {
            self.found = true;
        }
    }

    fn visit_new_expression(&mut self, _expression: &NewExpression<'a>) {
        self.found = true;
    }

    fn visit_spread_element(&mut self, _spread: &SpreadElement<'a>) {
        self.found = true;
    }

    fn visit_tagged_template_expression(&mut self, _expression: &TaggedTemplateExpression<'a>) {
        self.found = true;
    }

    fn visit_template_literal(&mut self, template: &TemplateLiteral<'a>) {
        if template.expressions.is_empty() {
            walk::walk_template_literal(self, template);
        } else {
            self.found = true;
        }
    }

    fn visit_binary_expression(&mut self, expression: &BinaryExpression<'a>) {
        if expression.operator == BinaryOperator::Addition {
            self.found = true;
        } else {
            walk::walk_binary_expression(self, expression);
        }
    }
}
