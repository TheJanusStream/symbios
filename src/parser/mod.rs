pub mod ast;

use crate::parser::ast::{Expr, ModuleSym, Rule};
use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::tag,
    character::complete::{alpha1, char as c_char, digit1, multispace0, one_of},
    combinator::{map, map_res, opt, recognize},
    multi::{many0, separated_list0},
    sequence::{delimited, pair, preceded, terminated},
};

// --- Lexical Tokens ---

/// Whitespace consumer
/// Fixed for nom 8: Use associated types for Output and Error
fn ws<'a, F, O, E: nom::error::ParseError<&'a str>>(
    inner: F,
) -> impl Parser<&'a str, Output = O, Error = E>
where
    F: Parser<&'a str, Output = O, Error = E>,
{
    delimited(multispace0, inner, multispace0)
}

fn float(input: &str) -> IResult<&str, f32> {
    map_res(
        recognize(pair(
            opt(one_of("+-")),
            pair(digit1, opt(pair(c_char('.'), digit1))),
        )),
        |s: &str| s.parse::<f32>(),
    )
    .parse(input)
}

fn identifier(input: &str) -> IResult<&str, String> {
    map(alpha1, |s: &str| s.to_string()).parse(input)
}

// --- Expressions ---

fn parse_factor(input: &str) -> IResult<&str, Expr> {
    alt((
        map(float, Expr::Number),
        map(identifier, Expr::Variable),
        delimited(c_char('('), parse_expr, c_char(')')),
    ))
    .parse(input)
}

fn parse_expr(input: &str) -> IResult<&str, Expr> {
    let (input, lhs) = parse_factor(input)?;

    // Parse optional operator and RHS
    let (input, op) = opt(pair(ws(one_of("+-*/><=")), parse_expr)).parse(input)?;

    match op {
        Some(('+', rhs)) => Ok((input, Expr::Add(Box::new(lhs), Box::new(rhs)))),
        Some(('-', rhs)) => Ok((input, Expr::Sub(Box::new(lhs), Box::new(rhs)))),
        Some(('*', rhs)) => Ok((input, Expr::Mul(Box::new(lhs), Box::new(rhs)))),
        Some(('/', rhs)) => Ok((input, Expr::Div(Box::new(lhs), Box::new(rhs)))),
        Some(('>', rhs)) => Ok((input, Expr::Gt(Box::new(lhs), Box::new(rhs)))),
        Some(('<', rhs)) => Ok((input, Expr::Lt(Box::new(lhs), Box::new(rhs)))),
        Some(('=', rhs)) => Ok((input, Expr::Eq(Box::new(lhs), Box::new(rhs)))),
        _ => Ok((input, lhs)),
    }
}

// --- L-System Grammar ---

fn parse_symbol(input: &str) -> IResult<&str, char> {
    one_of("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz+-/&^[]")(input)
}

fn parse_params(input: &str) -> IResult<&str, Vec<Expr>> {
    delimited(
        c_char('('),
        separated_list0(ws(c_char(',')), parse_expr),
        c_char(')'),
    )
    .parse(input)
}

pub fn parse_module(input: &str) -> IResult<&str, ModuleSym> {
    let (input, symbol) = parse_symbol(input)?;
    let (input, params) = opt(parse_params).parse(input)?;

    Ok((
        input,
        ModuleSym {
            symbol,
            params: params.unwrap_or_default(),
        },
    ))
}

// Public for testing
pub fn parse_context(input: &str) -> IResult<&str, Option<ModuleSym>> {
    opt(parse_module).parse(input)
}

pub fn parse_rule(input: &str) -> IResult<&str, Rule> {
    // Optional Left Context "LC <"
    let (input, left_ctx) = opt(terminated(parse_module, ws(c_char('<')))).parse(input)?;

    // Predecessor "P"
    let (input, predecessor) = parse_module(input)?;

    // Optional Right Context "> RC"
    let (input, right_ctx) = opt(preceded(ws(c_char('>')), parse_module)).parse(input)?;

    // Optional Condition ": Cond"
    let (input, condition) = opt(preceded(ws(c_char(':')), parse_expr)).parse(input)?;

    // Arrow "->"
    let (input, _) = ws(tag("->")).parse(input)?;

    // Successors
    let (input, successors) = many0(ws(parse_module)).parse(input)?;

    Ok((
        input,
        Rule {
            probability: 1.0,
            predecessor,
            left_context: left_ctx,
            right_context: right_ctx,
            condition,
            successors,
        },
    ))
}
