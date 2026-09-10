use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, bail};

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Number(f64),
    Variable(String),
    Unary {
        operator: UnaryOperator,
        value: Box<Expression>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Call {
        name: String,
        arguments: Vec<Expression>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
}

impl Expression {
    pub fn parse(source: &str) -> anyhow::Result<Self> {
        let source = source.trim();
        if source.is_empty() {
            bail!("expression is empty");
        }
        let mut parser = Parser::new(source)?;
        let expression = parser.parse_expression()?;
        if parser.current != Token::End {
            bail!("unexpected token after complete expression");
        }
        Ok(expression)
    }

    pub fn eval(&self, variables: &HashMap<String, f64>) -> anyhow::Result<f64> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Variable(name) => match name.as_str() {
                "pi" => Ok(std::f64::consts::PI),
                "e" => Ok(std::f64::consts::E),
                _ => variables
                    .get(name)
                    .copied()
                    .with_context(|| format!("unknown expression variable `{name}`")),
            },
            Self::Unary { operator, value } => {
                let value = value.eval(variables)?;
                Ok(match operator {
                    UnaryOperator::Positive => value,
                    UnaryOperator::Negative => -value,
                })
            }
            Self::Binary {
                operator,
                left,
                right,
            } => {
                let left = left.eval(variables)?;
                let right = right.eval(variables)?;
                Ok(match operator {
                    BinaryOperator::Add => left + right,
                    BinaryOperator::Subtract => left - right,
                    BinaryOperator::Multiply => left * right,
                    BinaryOperator::Divide => left / right,
                    BinaryOperator::Power => left.powf(right),
                })
            }
            Self::Call { name, arguments } => {
                let values = arguments
                    .iter()
                    .map(|argument| argument.eval(variables))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                evaluate_function(name, &values)
            }
        }
    }

    pub fn identifiers(&self) -> BTreeSet<String> {
        let mut identifiers = BTreeSet::new();
        self.collect_identifiers(&mut identifiers);
        identifiers
    }

    fn collect_identifiers(&self, identifiers: &mut BTreeSet<String>) {
        match self {
            Self::Variable(name) if name != "pi" && name != "e" => {
                identifiers.insert(name.clone());
            }
            Self::Unary { value, .. } => value.collect_identifiers(identifiers),
            Self::Binary { left, right, .. } => {
                left.collect_identifiers(identifiers);
                right.collect_identifiers(identifiers);
            }
            Self::Call { arguments, .. } => {
                for argument in arguments {
                    argument.collect_identifiers(identifiers);
                }
            }
            _ => {}
        }
    }

    pub fn to_wgsl(
        &self,
        resolve_variable: &impl Fn(&str) -> Option<String>,
    ) -> anyhow::Result<String> {
        match self {
            Self::Number(value) => Ok(wgsl_number(*value)),
            Self::Variable(name) => match name.as_str() {
                "pi" => Ok("3.141592653589793".to_owned()),
                "e" => Ok("2.718281828459045".to_owned()),
                _ => resolve_variable(name)
                    .with_context(|| format!("`{name}` cannot be lowered to the GPU expression set")),
            },
            Self::Unary { operator, value } => {
                let value = value.to_wgsl(resolve_variable)?;
                Ok(match operator {
                    UnaryOperator::Positive => format!("({value})"),
                    UnaryOperator::Negative => format!("(-({value}))"),
                })
            }
            Self::Binary {
                operator,
                left,
                right,
            } => {
                let left = left.to_wgsl(resolve_variable)?;
                let right = right.to_wgsl(resolve_variable)?;
                match operator {
                    BinaryOperator::Power => Ok(format!("pow(({left}), ({right}))")),
                    BinaryOperator::Add => Ok(format!("(({left}) + ({right}))")),
                    BinaryOperator::Subtract => Ok(format!("(({left}) - ({right}))")),
                    BinaryOperator::Multiply => Ok(format!("(({left}) * ({right}))")),
                    BinaryOperator::Divide => Ok(format!("(({left}) / ({right}))")),
                }
            }
            Self::Call { name, arguments } => {
                let allowed = match name.as_str() {
                    "ln" => "log",
                    "sign" => "sign",
                    "atan2" => "atan2",
                    "abs" | "sqrt" | "exp" | "log" | "sin" | "cos" | "tan"
                    | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh"
                    | "floor" | "ceil" | "round" | "min" | "max" | "pow"
                    | "clamp" => name.as_str(),
                    _ => bail!("function `{name}` is not available in the GPU expression set"),
                };
                let arguments = arguments
                    .iter()
                    .map(|argument| argument.to_wgsl(resolve_variable))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(format!("{allowed}({})", arguments.join(", ")))
            }
        }
    }
}

fn evaluate_function(name: &str, values: &[f64]) -> anyhow::Result<f64> {
    let unary = |function: fn(f64) -> f64| -> anyhow::Result<f64> {
        if values.len() != 1 {
            bail!("function `{name}` expects one argument");
        }
        Ok(function(values[0]))
    };
    let binary = |function: fn(f64, f64) -> f64| -> anyhow::Result<f64> {
        if values.len() != 2 {
            bail!("function `{name}` expects two arguments");
        }
        Ok(function(values[0], values[1]))
    };

    match name {
        "abs" => unary(f64::abs),
        "sqrt" => unary(f64::sqrt),
        "exp" => unary(f64::exp),
        "ln" | "log" => unary(f64::ln),
        "sin" => unary(f64::sin),
        "cos" => unary(f64::cos),
        "tan" => unary(f64::tan),
        "asin" => unary(f64::asin),
        "acos" => unary(f64::acos),
        "atan" => unary(f64::atan),
        "sinh" => unary(f64::sinh),
        "cosh" => unary(f64::cosh),
        "tanh" => unary(f64::tanh),
        "floor" => unary(f64::floor),
        "ceil" => unary(f64::ceil),
        "round" => unary(f64::round),
        "sign" => unary(f64::signum),
        "min" => binary(f64::min),
        "max" => binary(f64::max),
        "pow" => binary(f64::powf),
        "atan2" => binary(f64::atan2),
        "clamp" => {
            if values.len() != 3 {
                bail!("function `clamp` expects three arguments");
            }
            let (value, minimum, maximum) = (values[0], values[1], values[2]);
            if minimum.is_nan() || maximum.is_nan() || minimum > maximum {
                bail!("function `clamp` requires an ordered, non-NaN minimum and maximum");
            }
            Ok(value.max(minimum).min(maximum))
        }
        _ => bail!("unknown expression function `{name}`"),
    }
}

fn wgsl_number(value: f64) -> String {
    if value.is_nan() {
        return "0.0".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "3.402823466e+38".to_owned()
        } else {
            "-3.402823466e+38".to_owned()
        };
    }
    let mut rendered = format!("{value:.12}");
    while rendered.contains('.') && rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.push('0');
    }
    if !rendered.contains('.') && !rendered.contains('e') && !rendered.contains('E') {
        rendered.push_str(".0");
    }
    rendered
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Identifier(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LeftParen,
    RightParen,
    Comma,
    End,
}

struct Lexer<'a> {
    source: &'a [u8],
    offset: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source: source.as_bytes(),
            offset: 0,
        }
    }

    fn next_token(&mut self) -> anyhow::Result<Token> {
        while self.offset < self.source.len() && self.source[self.offset].is_ascii_whitespace() {
            self.offset += 1;
        }
        if self.offset >= self.source.len() {
            return Ok(Token::End);
        }
        let byte = self.source[self.offset];
        match byte {
            b'+' => {
                self.offset += 1;
                Ok(Token::Plus)
            }
            b'-' => {
                self.offset += 1;
                Ok(Token::Minus)
            }
            b'*' => {
                self.offset += 1;
                Ok(Token::Star)
            }
            b'/' => {
                self.offset += 1;
                Ok(Token::Slash)
            }
            b'^' => {
                self.offset += 1;
                Ok(Token::Caret)
            }
            b'(' => {
                self.offset += 1;
                Ok(Token::LeftParen)
            }
            b')' => {
                self.offset += 1;
                Ok(Token::RightParen)
            }
            b',' => {
                self.offset += 1;
                Ok(Token::Comma)
            }
            b'0'..=b'9' | b'.' => self.number(),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.identifier(),
            _ => bail!("unsupported character `{}` in expression", byte as char),
        }
    }

    fn number(&mut self) -> anyhow::Result<Token> {
        let start = self.offset;
        let mut saw_exponent = false;
        while self.offset < self.source.len() {
            match self.source[self.offset] {
                b'0'..=b'9' | b'.' => self.offset += 1,
                b'e' | b'E' if !saw_exponent => {
                    saw_exponent = true;
                    self.offset += 1;
                    if self.offset < self.source.len()
                        && matches!(self.source[self.offset], b'+' | b'-')
                    {
                        self.offset += 1;
                    }
                }
                _ => break,
            }
        }
        let value = std::str::from_utf8(&self.source[start..self.offset])?
            .parse::<f64>()
            .context("invalid numeric literal")?;
        Ok(Token::Number(value))
    }

    fn identifier(&mut self) -> anyhow::Result<Token> {
        let start = self.offset;
        while self.offset < self.source.len()
            && matches!(
                self.source[self.offset],
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'
            )
        {
            self.offset += 1;
        }
        Ok(Token::Identifier(
            std::str::from_utf8(&self.source[start..self.offset])?.to_owned(),
        ))
    }
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> anyhow::Result<Self> {
        let mut lexer = Lexer::new(source);
        let current = lexer.next_token()?;
        Ok(Self { lexer, current })
    }

    fn advance(&mut self) -> anyhow::Result<Token> {
        let previous = std::mem::replace(&mut self.current, self.lexer.next_token()?);
        Ok(previous)
    }

    fn parse_expression(&mut self) -> anyhow::Result<Expression> {
        self.parse_additive()
    }

    fn parse_additive(&mut self) -> anyhow::Result<Expression> {
        let mut expression = self.parse_multiplicative()?;
        loop {
            let operator = match self.current {
                Token::Plus => BinaryOperator::Add,
                Token::Minus => BinaryOperator::Subtract,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_multiplicative()?;
            expression = Expression::Binary {
                operator,
                left: Box::new(expression),
                right: Box::new(right),
            };
        }
        Ok(expression)
    }

    fn parse_multiplicative(&mut self) -> anyhow::Result<Expression> {
        let mut expression = self.parse_unary()?;
        loop {
            let operator = match self.current {
                Token::Star => BinaryOperator::Multiply,
                Token::Slash => BinaryOperator::Divide,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_unary()?;
            expression = Expression::Binary {
                operator,
                left: Box::new(expression),
                right: Box::new(right),
            };
        }
        Ok(expression)
    }

    fn parse_unary(&mut self) -> anyhow::Result<Expression> {
        match self.current {
            Token::Plus => {
                self.advance()?;
                Ok(Expression::Unary {
                    operator: UnaryOperator::Positive,
                    value: Box::new(self.parse_unary()?),
                })
            }
            Token::Minus => {
                self.advance()?;
                Ok(Expression::Unary {
                    operator: UnaryOperator::Negative,
                    value: Box::new(self.parse_unary()?),
                })
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> anyhow::Result<Expression> {
        let left = self.parse_primary()?;
        if self.current == Token::Caret {
            self.advance()?;
            let right = self.parse_unary()?;
            Ok(Expression::Binary {
                operator: BinaryOperator::Power,
                left: Box::new(left),
                right: Box::new(right),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_primary(&mut self) -> anyhow::Result<Expression> {
        match self.advance()? {
            Token::Number(value) => Ok(Expression::Number(value)),
            Token::Identifier(name) => {
                if self.current != Token::LeftParen {
                    return Ok(Expression::Variable(name));
                }
                self.advance()?;
                let mut arguments = Vec::new();
                if self.current != Token::RightParen {
                    loop {
                        arguments.push(self.parse_expression()?);
                        if self.current == Token::Comma {
                            self.advance()?;
                            continue;
                        }
                        break;
                    }
                }
                if self.current != Token::RightParen {
                    bail!("missing closing parenthesis for function `{name}`");
                }
                self.advance()?;
                validate_function_arity(&name, arguments.len())?;
                Ok(Expression::Call { name, arguments })
            }
            Token::LeftParen => {
                let expression = self.parse_expression()?;
                if self.current != Token::RightParen {
                    bail!("missing closing parenthesis");
                }
                self.advance()?;
                Ok(expression)
            }
            token => bail!("expected a number, variable, or parenthesized expression; found {token:?}"),
        }
    }
}

fn validate_function_arity(name: &str, count: usize) -> anyhow::Result<()> {
    let valid = match name {
        "abs" | "sqrt" | "exp" | "ln" | "log" | "sin" | "cos" | "tan"
        | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "floor"
        | "ceil" | "round" | "sign" => count == 1,
        "min" | "max" | "pow" | "atan2" => count == 2,
        "clamp" => count == 3,
        _ => false,
    };
    if !valid {
        bail!("unknown function or invalid argument count for `{name}`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_evaluates_scientific_expression() {
        let expression = Expression::parse("sin(theta) + gain * x^2").expect("parse");
        let variables = HashMap::from([
            ("theta".to_owned(), std::f64::consts::FRAC_PI_2),
            ("gain".to_owned(), 2.0),
            ("x".to_owned(), 3.0),
        ]);
        let value = expression.eval(&variables).expect("evaluate");
        assert!((value - 19.0).abs() < 1.0e-10);
    }

    #[test]
    fn exponentiation_precedes_unary_sign_and_remains_right_associative() {
        let empty = HashMap::new();
        let signed = Expression::parse("-2^2").expect("parse signed power");
        assert_eq!(signed.eval(&empty).expect("evaluate signed power"), -4.0);
        let chained = Expression::parse("2^3^2").expect("parse chained power");
        assert_eq!(chained.eval(&empty).expect("evaluate chained power"), 512.0);
        let negative_exponent = Expression::parse("2^-2").expect("parse negative exponent");
        assert!((negative_exponent.eval(&empty).expect("evaluate negative exponent") - 0.25).abs() < 1.0e-12);
    }

    #[test]
    fn invalid_clamp_bounds_return_an_error_instead_of_panicking() {
        let expression = Expression::parse("clamp(0, 2, 1)").expect("parse clamp");
        assert!(expression.eval(&HashMap::new()).is_err());
    }

    #[test]
    fn identifiers_do_not_include_named_constants() {
        let expression = Expression::parse("pi * radius^2 + e").expect("parse");
        assert_eq!(
            expression.identifiers(),
            BTreeSet::from(["radius".to_owned()])
        );
    }

    #[test]
    fn gpu_lowering_uses_resolved_variable_names() {
        let expression = Expression::parse("gain * x + cos(t)").expect("parse");
        let source = expression
            .to_wgsl(&|name| Some(format!("v_{name}")))
            .expect("lower");
        assert!(source.contains("v_gain"));
        assert!(source.contains("cos(v_t)"));
    }
}
