pub mod ast;

use crate::parser::ast::{Directive, Expr, ModuleSym, Rule};
use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, tag, take_until, take_while_m_n},
    character::complete::{char as c_char, multispace1, one_of},
    combinator::{cut, map, opt, peek, verify},
    error::{Error, ErrorKind, ParseError},
    multi::{many_till, many0},
    number::complete::double,
    sequence::{delimited, pair, preceded, terminated},
};

const MAX_RECURSION_DEPTH: usize = 64;
const MAX_IDENTIFIER_LEN: usize = 64;
const MAX_ARG_COUNT: usize = 32;
const MAX_SUCCESSORS: usize = 128;

fn space_or_comment<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, (), E> {
    let comment = alt((
        preceded(tag("/*"), terminated(take_until("*/"), tag("*/"))),
        preceded(tag("//"), is_not("\n\r")),
    ));
    let mut p = many0(alt((map(multispace1, |_| ()), map(comment, |_| ()))));
    p.parse(input).map(|(i, _)| (i, ()))
}

fn ws<'a, F, O, E: ParseError<&'a str>>(inner: F) -> impl Parser<&'a str, Output = O, Error = E>
where
    F: Parser<&'a str, Output = O, Error = E>,
{
    delimited(space_or_comment, inner, space_or_comment)
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn identifier(input: &str) -> IResult<&str, String> {
    let (input, slice) = verify(
        take_while_m_n(1, MAX_IDENTIFIER_LEN, is_ident_char),
        |s: &str| {
            if let Some(c) = s.chars().next() {
                let lower = s.to_lowercase();
                c.is_alphabetic() && lower != "nan" && lower != "inf" && lower != "infinity"
            } else {
                false
            }
        },
    )
    .parse(input)?;
    Ok((input, slice.to_string()))
}

fn finite_float(input: &str) -> IResult<&str, f64> {
    let (input, val) = verify(double, |x: &f64| x.is_finite()).parse(input)?;
    Ok((input, val))
}

pub fn parse_expr(input: &str) -> IResult<&str, Expr> {
    ws(|i| parse_expr_impl(i, 0)).parse(input)
}

fn parse_expr_impl(input: &str, depth: usize) -> IResult<&str, Expr> {
    parse_logical_or(input, depth)
}

fn parse_logical_or(input: &str, depth: usize) -> IResult<&str, Expr> {
    let (mut input, mut acc) = parse_logical_and(input, depth)?;
    while let Ok((next, _)) = ws(c_char::<&str, Error<&str>>('|')).parse(input) {
        let (next, rhs) = parse_logical_and(next, depth)?;
        acc = Expr::Or(Box::new(acc), Box::new(rhs));
        input = next;
    }
    Ok((input, acc))
}

fn parse_logical_and(input: &str, depth: usize) -> IResult<&str, Expr> {
    let (mut input, mut acc) = parse_relational(input, depth)?;
    while let Ok((next, _)) = ws(c_char::<&str, Error<&str>>('&')).parse(input) {
        let (next, rhs) = parse_relational(next, depth)?;
        acc = Expr::And(Box::new(acc), Box::new(rhs));
        input = next;
    }
    Ok((input, acc))
}

fn parse_relational(input: &str, depth: usize) -> IResult<&str, Expr> {
    let (input, lhs) = parse_addition(input, depth)?;
    let op_parser = alt((
        tag::<&str, &str, Error<&str>>("=="),
        tag::<&str, &str, Error<&str>>("!="),
        tag::<&str, &str, Error<&str>>("<="),
        tag::<&str, &str, Error<&str>>(">="),
        tag::<&str, &str, Error<&str>>("<"),
        tag::<&str, &str, Error<&str>>(">"),
        // Added support for single '=' as alias for '=='
        tag::<&str, &str, Error<&str>>("="),
    ));
    if let Ok((input, op)) = ws(op_parser).parse(input) {
        let (input, rhs) = parse_addition(input, depth)?;
        let res = match op {
            "==" | "=" => Expr::Eq(Box::new(lhs), Box::new(rhs)),
            "!=" => Expr::Ne(Box::new(lhs), Box::new(rhs)),
            "<=" => Expr::Le(Box::new(lhs), Box::new(rhs)),
            ">=" => Expr::Ge(Box::new(lhs), Box::new(rhs)),
            "<" => Expr::Lt(Box::new(lhs), Box::new(rhs)),
            ">" => Expr::Gt(Box::new(lhs), Box::new(rhs)),
            _ => unreachable!(),
        };
        Ok((input, res))
    } else {
        Ok((input, lhs))
    }
}

fn parse_addition(input: &str, depth: usize) -> IResult<&str, Expr> {
    let (mut input, mut acc) = parse_multiplication(input, depth)?;

    loop {
        let (trimmed, _) = space_or_comment::<Error<&str>>(input).unwrap_or((input, ()));

        if trimmed.starts_with("->") {
            break;
        }

        let op_char = trimmed.chars().next();
        let op = match op_char {
            Some('+') => '+',
            Some('-') => '-',
            _ => break,
        };

        let (next, _) = ws(c_char::<&str, Error<&str>>(op)).parse(input)?;
        let (next, rhs) = parse_multiplication(next, depth)?;

        acc = match op {
            '+' => Expr::Add(Box::new(acc), Box::new(rhs)),
            '-' => Expr::Sub(Box::new(acc), Box::new(rhs)),
            _ => unreachable!(),
        };
        input = next;
    }
    Ok((input, acc))
}

fn parse_multiplication(input: &str, depth: usize) -> IResult<&str, Expr> {
    let (mut input, mut acc) = parse_unary(input, depth)?;
    while let Ok((next, op)) = ws(one_of::<&str, &str, Error<&str>>("*/")).parse(input) {
        let (next, rhs) = parse_unary(next, depth)?;
        acc = match op {
            '*' => Expr::Mul(Box::new(acc), Box::new(rhs)),
            '/' => Expr::Div(Box::new(acc), Box::new(rhs)),
            _ => unreachable!(),
        };
        input = next;
    }
    Ok((input, acc))
}

fn parse_unary(input: &str, depth: usize) -> IResult<&str, Expr> {
    if let Ok((input, op)) = ws(one_of::<&str, &str, Error<&str>>("!-")).parse(input) {
        let (input, sub) = parse_pow(input, depth)?;
        let res = match op {
            '!' => Expr::Not(Box::new(sub)),
            '-' => Expr::Neg(Box::new(sub)),
            _ => unreachable!(),
        };
        Ok((input, res))
    } else {
        parse_pow(input, depth)
    }
}

fn parse_pow(input: &str, depth: usize) -> IResult<&str, Expr> {
    let (mut input, mut acc) = parse_atom(input, depth)?;
    while let Ok((next, _)) = ws(c_char::<&str, Error<&str>>('^')).parse(input) {
        let (next, val) = parse_pow(next, depth + 1)?;
        acc = Expr::Pow(Box::new(acc), Box::new(val));
        input = next;
    }
    Ok((input, acc))
}

fn parse_atom(input: &str, depth: usize) -> IResult<&str, Expr> {
    if depth > MAX_RECURSION_DEPTH {
        return Err(nom::Err::Failure(Error::new(input, ErrorKind::TooLarge)));
    }
    alt((
        map(ws(finite_float), Expr::Number),
        |i| parse_call_or_var_at_depth(i, depth),
        ws(delimited(
            c_char('('),
            |i| parse_expr_impl(i, depth + 1),
            c_char(')'),
        )),
    ))
    .parse(input)
}

fn parse_call_or_var_at_depth(input: &str, depth: usize) -> IResult<&str, Expr> {
    let (input, id) = ws(identifier).parse(input)?;
    if let Ok((_, '(')) = peek(ws(c_char::<&str, Error<&str>>('('))).parse(input) {
        let (input, args) = cut(|i| parse_arg_list(i, depth + 1)).parse(input)?;
        Ok((input, Expr::Call(id, args)))
    } else {
        Ok((input, Expr::Variable(id)))
    }
}

fn parse_arg_list(input: &str, depth: usize) -> IResult<&str, Vec<Expr>> {
    let (mut input, _) = ws(c_char('(')).parse(input)?;
    let mut args = Vec::new();
    loop {
        if args.len() >= MAX_ARG_COUNT {
            return Err(nom::Err::Failure(Error::from_error_kind(
                input,
                ErrorKind::TooLarge,
            )));
        }
        if let Ok((next_input, expr)) = parse_expr_impl(input, depth) {
            args.push(expr);
            input = next_input;
            if let Ok((next_input, _)) = ws(c_char::<&str, Error<&str>>(',')).parse(input) {
                input = next_input;
                continue;
            }
        }
        break;
    }
    let (input, _) = ws(c_char(')')).parse(input)?;
    Ok((input, args))
}

fn parse_symbol(input: &str) -> IResult<&str, String> {
    alt((identifier, map(one_of("+-/&^[]|\\!$%~"), |c| c.to_string()))).parse(input)
}

pub fn parse_module(input: &str) -> IResult<&str, ModuleSym> {
    ws(|i| parse_module_impl(i, 0)).parse(input)
}

fn parse_module_impl(input: &str, depth: usize) -> IResult<&str, ModuleSym> {
    let (input, symbol) = ws(parse_symbol).parse(input)?;
    let (input, params) =
        if let Ok((_, '(')) = peek(ws(c_char::<&str, Error<&str>>('('))).parse(input) {
            let (input, args) = cut(|i| parse_arg_list(i, depth + 1)).parse(input)?;
            (input, Some(args))
        } else {
            (input, None)
        };
    Ok((
        input,
        ModuleSym {
            symbol,
            params: params.unwrap_or_default(),
        },
    ))
}

fn parse_rule_structure(input: &str) -> IResult<&str, Rule> {
    let mut probability = 1.0;
    let mut input = input;

    // Optional Probability
    if let Ok((next, p)) =
        terminated(ws(finite_float), ws(c_char::<&str, Error<&str>>(':'))).parse(input)
    {
        probability = p;
        input = next;
    }

    // Left Context
    let (input, left_context) = if let Ok((next, (lc, _))) = many_till::<&str, Error<&str>, _, _>(
        ws(parse_module),
        peek(ws(c_char::<&str, Error<&str>>('<'))),
    )
    .parse(input)
    {
        let (next, _) = ws(c_char('<')).parse(next)?;
        (next, lc)
    } else {
        (input, Vec::new())
    };

    // Predecessor
    let (input, predecessor) = ws(parse_module).parse(input)?;

    // Right Context
    let (input, right_context) =
        if let Ok((next, _)) = ws(c_char::<&str, Error<&str>>('>')).parse(input) {
            let term = alt((map(ws(c_char(':')), |_| ()), map(ws(tag("->")), |_| ())));
            let (next, (rc, _)) =
                many_till::<&str, Error<&str>, _, _>(ws(parse_module), peek(term)).parse(next)?;
            (next, rc)
        } else {
            (input, Vec::new())
        };

    // Condition
    // Extended to support '*' as a wildcard for 1.0 (True)
    let (input, condition) = opt(preceded(
        ws(c_char(':')),
        alt((map(ws(c_char('*')), |_| Expr::Number(1.0)), parse_expr)),
    ))
    .parse(input)?;

    // Arrow
    let (input, _) = ws(tag("->")).parse(input)?;

    // Successors
    let mut successors = Vec::new();
    let mut curr = input;
    loop {
        let (next, _) = space_or_comment(curr)?;
        if successors.len() >= MAX_SUCCESSORS {
            return Err(nom::Err::Failure(Error::from_error_kind(
                next,
                ErrorKind::TooLarge,
            )));
        }
        if let Ok((next, m)) = parse_module(next) {
            successors.push(m);
            curr = next;
        } else {
            break;
        }
    }

    Ok((
        curr,
        Rule {
            label: None,
            probability,
            predecessor,
            left_context,
            right_context,
            condition,
            successors,
        },
    ))
}

pub fn parse_rule(input: &str) -> IResult<&str, Rule> {
    alt((
        map(
            pair(
                terminated(ws(identifier), ws(c_char::<&str, Error<&str>>(':'))),
                parse_rule_structure,
            ),
            |(l, mut r)| {
                r.label = Some(l);
                r
            },
        ),
        parse_rule_structure,
    ))
    .parse(input)
}

pub fn parse_directive(input: &str) -> IResult<&str, Directive> {
    preceded(
        ws(c_char('#')),
        alt((
            map(
                preceded(
                    pair(tag("ignore"), ws(c_char(':'))),
                    many0(ws(parse_symbol)),
                ),
                Directive::Ignore,
            ),
            map(
                preceded(tag("define"), pair(ws(identifier), parse_expr)),
                |(name, expr)| Directive::Define(name, expr),
            ),
        )),
    )
    .parse(input)
}
