use super::{Command, IrObject};
use crate::graphics::{LineCap, LineJoin, StrokeStyle};
use kurbo::{Affine, Point};
use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, take_until},
    character::complete::{char, digit1, multispace0},
    combinator::{map, map_res, opt, recognize},
    multi::separated_list0,
    sequence::{delimited, preceded, tuple},
};

/// Attempt to resurrect a sequence of commands from corrupted debug strings.
pub fn resurrect_commands(data: &[u8]) -> Option<Vec<Command>> {
    let s = std::str::from_utf8(data).ok()?;
    let mut commands = Vec::new();

    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok((_, cmd)) = parse_command(line) {
            commands.push(cmd);
        } else {
            // If we find something that looks like a debug string but fails to parse,
            // we might want to log it, but for now just skip to be robust.
        }
    }

    if commands.is_empty() { None } else { Some(commands) }
}

fn parse_f64(input: &str) -> IResult<&str, f64> {
    map_res(
        recognize(tuple((opt(char('-')), digit1, opt(tuple((char('.'), digit1)))))),
        |s: &str| s.parse::<f64>(),
    )(input)
}

fn parse_i64(input: &str) -> IResult<&str, i64> {
    map_res(recognize(tuple((opt(char('-')), digit1))), |s: &str| s.parse::<i64>())(input)
}

fn parse_point(input: &str) -> IResult<&str, Point> {
    let (input, _) = char('(')(input)?;
    let (input, x) = parse_f64(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, y) = parse_f64(input)?;
    let (input, _) = char(')')(input)?;
    Ok((input, Point::new(x, y)))
}

fn parse_affine(input: &str) -> IResult<&str, Affine> {
    let (input, _) = tag("Affine([")(input)?;
    let (input, coeffs) = separated_list0(tuple((char(','), multispace0)), parse_f64)(input)?;
    let (input, _) = tag("])")(input)?;
    if coeffs.len() == 6 {
        Ok((input, Affine::new([coeffs[0], coeffs[1], coeffs[2], coeffs[3], coeffs[4], coeffs[5]])))
    } else {
        Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Verify)))
    }
}

fn parse_line_cap(input: &str) -> IResult<&str, LineCap> {
    alt((
        map(tag("Butt"), |_| LineCap::Butt),
        map(tag("Round"), |_| LineCap::Round),
        map(tag("Square"), |_| LineCap::Square),
    ))(input)
}

fn parse_line_join(input: &str) -> IResult<&str, LineJoin> {
    alt((
        map(tag("Miter"), |_| LineJoin::Miter),
        map(tag("Round"), |_| LineJoin::Round),
        map(tag("Bevel"), |_| LineJoin::Bevel),
    ))(input)
}

fn parse_dash_pattern(input: &str) -> IResult<&str, Option<(Vec<f64>, f64)>> {
    alt((
        map(tag("dash_pattern: None"), |_| None),
        map(
            tuple((
                tag("dash_pattern: Some((["),
                separated_list0(tuple((char(','), multispace0)), parse_f64),
                tag("],"),
                multispace0,
                parse_f64,
                tag("))"),
            )),
            |(_, dash, _, _, phase, _)| Some((dash, phase)),
        ),
    ))(input)
}

fn parse_stroke_style(input: &str) -> IResult<&str, StrokeStyle> {
    let (input, _) = tag("StrokeStyle {")(input)?;
    let (input, _) = multispace0(input)?;

    // Simple key-value parser for StrokeStyle fields
    let (input, _) = tag("width:")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, width) = parse_f64(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = multispace0(input)?;

    let (input, _) = tag("cap:")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, cap) = parse_line_cap(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = multispace0(input)?;

    let (input, _) = tag("join:")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, join) = parse_line_join(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = multispace0(input)?;

    let (input, _) = tag("miter_limit:")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, miter_limit) = parse_f64(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = multispace0(input)?;

    let (input, dash_pattern) = parse_dash_pattern(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char('}')(input)?;

    Ok((input, StrokeStyle { width, cap, join, miter_limit, dash_pattern }))
}

fn parse_string_literal(input: &str) -> IResult<&str, String> {
    delimited(char('\"'), map(take_until("\""), |s: &str| s.to_string()), char('\"'))(input)
}

fn parse_ir_object(input: &str) -> IResult<&str, IrObject> {
    alt((
        map(tag("Null"), |_| IrObject::Null),
        map(
            preceded(tag("Boolean("), terminated_char(alt((tag("true"), tag("false"))), ')')),
            |s| IrObject::Boolean(s == "true"),
        ),
        map(preceded(tag("Integer("), terminated_char(parse_i64, ')')), IrObject::Integer),
        map(preceded(tag("Real("), terminated_char(parse_f64, ')')), IrObject::Real),
        map(preceded(tag("Name("), terminated_char(parse_string_literal, ')')), IrObject::Name),
        map(
            preceded(
                tag("Array(["),
                nom::sequence::terminated(
                    separated_list0(tuple((char(','), multispace0)), parse_ir_object),
                    tag("])"),
                ),
            ),
            IrObject::Array,
        ),
    ))(input)
}

fn terminated_char<'a, T, F>(mut parser: F, c: char) -> impl FnMut(&'a str) -> IResult<&'a str, T>
where
    F: FnMut(&'a str) -> IResult<&'a str, T>,
{
    move |input| {
        let (input, res) = parser(input)?;
        let (input, _) = char(c)(input)?;
        Ok((input, res))
    }
}

fn parse_command(input: &str) -> IResult<&str, Command> {
    alt((
        map(tag("PushState"), |_| Command::PushState),
        map(tag("PopState"), |_| Command::PopState),
        map(preceded(tag("Transform("), terminated_char(parse_affine, ')')), Command::Transform),
        map(preceded(tag("MoveTo("), terminated_char(parse_point, ')')), Command::MoveTo),
        map(preceded(tag("LineTo("), terminated_char(parse_point, ')')), Command::LineTo),
        map(preceded(tag("Stroke("), terminated_char(parse_stroke_style, ')')), Command::Stroke),
        map(
            preceded(tag("DrawXObject("), terminated_char(parse_string_literal, ')')),
            Command::DrawXObject,
        ),
        parse_raw_operator,
    ))(input)
}

fn parse_raw_operator(input: &str) -> IResult<&str, Command> {
    let (input, _) = tag("RawOperator {")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("name:")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, name) = parse_string_literal(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("operands: [")(input)?;
    let (input, operands) =
        separated_list0(tuple((char(','), multispace0)), parse_ir_object)(input)?;
    let (input, _) = tag("] }")(input)?;
    Ok((input, Command::RawOperator { name, operands }))
}
