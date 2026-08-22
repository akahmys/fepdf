//! Type 4 PostScript calculator functions (ISO 32000-2, 7.10.5).
//!
//! A calculator function is a small stack program in braces. It has no loops and no
//! variables — `if` and `ifelse` are the only control flow and both take literal
//! procedure blocks — so evaluation always terminates, and the only bounds this module
//! needs are on nesting depth and stack size.
//!
//! **An unknown operator fails the evaluation rather than being skipped.** A calculator
//! function is arithmetic: skipping a token leaves the stack the wrong depth and the
//! remaining operators consume the wrong operands, so the result would be a plausible
//! colour computed from nonsense. The caller falls back to something it can explain.

use super::Bounds;

/// How deeply `{ }` may nest. RR-15 Rule 6: the parser and the evaluator both recurse
/// over blocks, and a file can nest braces as deeply as it likes.
const MAX_BLOCK_DEPTH: usize = 32;

/// The operand stack limit. 7.10.5 caps a calculator's stack at 100 entries; this is
/// well above that and is here so a `copy` with a large operand cannot grow memory.
const MAX_STACK: usize = 1000;

/// A value on the operand stack.
///
/// Not a bare `f64`: Table 42 has the relational operators produce booleans and
/// `if`/`ifelse` consume them, and `and`/`or`/`xor`/`not` mean *logical* on booleans and
/// *bitwise* on integers. Encoding truth as a number loses the distinction that decides
/// which of the two an operator is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PsValue {
    /// A number. PostScript's integer/real distinction is kept in the value, not the
    /// type: `cvi` truncates and the integer operators convert on entry.
    Num(f64),
    /// A boolean, produced by a relational or logical operator or by `true`/`false`.
    Bool(bool),
}

/// A token of a calculator program.
#[derive(Debug, Clone)]
enum PsToken {
    /// A numeric literal.
    Number(f64),
    /// An operator name, resolved against Table 42 at evaluation time.
    Operator(String),
    /// A `{ }` procedure, which is only ever an operand of `if` or `ifelse`.
    Block(Vec<PsToken>),
}

/// A type 4 function: a parsed calculator program plus the bounds it runs under.
#[derive(Debug, Clone)]
pub struct PostScriptFunction {
    bounds: Bounds,
    program: Vec<PsToken>,
    outputs: usize,
}

impl PostScriptFunction {
    pub(super) fn parse(bounds: Bounds, data: &[u8]) -> Option<Self> {
        // `/Range` is required for a type 4 function (Table 39). It is also the only
        // statement of how many of the values left on the stack are the result, so
        // without it there is nothing to return.
        let outputs = bounds.range.as_ref()?.len() / 2;
        if outputs == 0 {
            return None;
        }
        let text = String::from_utf8_lossy(data);
        let lexemes = lex(&text);
        let mut at = 0;
        let top = build(&lexemes, &mut at, 0)?;
        // The whole program is wrapped in one `{ }`. A file missing the outer pair is
        // taken at face value rather than rejected, which costs nothing and is what the
        // readers this is checked against do.
        let program = match top.as_slice() {
            [PsToken::Block(body)] => body.clone(),
            _ => top,
        };
        Some(Self { bounds, program, outputs })
    }

    pub(super) fn eval(&self, inputs: &[f64]) -> Option<Vec<f64>> {
        let clipped = self.bounds.clip_inputs(inputs)?;
        let mut stack: Vec<PsValue> = clipped.iter().map(|v| PsValue::Num(*v)).collect();
        exec(&self.program, &mut stack, 0)?;
        if stack.len() < self.outputs {
            return None;
        }
        let start = stack.len() - self.outputs;
        let mut out = Vec::with_capacity(self.outputs);
        for value in &stack[start..] {
            match value {
                PsValue::Num(n) => out.push(*n),
                // A program that leaves a boolean where a colour component belongs has
                // gone wrong in a way no clipping repairs.
                PsValue::Bool(_) => return None,
            }
        }
        Some(self.bounds.clip_outputs(out))
    }

    pub(super) fn bounds(&self) -> &Bounds {
        &self.bounds
    }
}

/// Splits the program text into lexemes, treating `{` and `}` as self-delimiting and
/// dropping `%` comments to end of line.
fn lex(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_comment = false;
    for ch in text.chars() {
        if in_comment {
            if ch == '\n' || ch == '\r' {
                in_comment = false;
            }
            continue;
        }
        match ch {
            '%' => {
                flush(&mut current, &mut out);
                in_comment = true;
            }
            '{' | '}' => {
                flush(&mut current, &mut out);
                out.push(ch.to_string());
            }
            c if c.is_whitespace() => flush(&mut current, &mut out),
            c => current.push(c),
        }
    }
    flush(&mut current, &mut out);
    out
}

fn flush(current: &mut String, out: &mut Vec<String>) {
    if !current.is_empty() {
        out.push(std::mem::take(current));
    }
}

/// Builds the nested token tree, consuming lexemes from `at` until a `}` or the end.
fn build(lexemes: &[String], at: &mut usize, depth: usize) -> Option<Vec<PsToken>> {
    if depth > MAX_BLOCK_DEPTH {
        return None;
    }
    let mut tokens = Vec::new();
    while *at < lexemes.len() {
        let lexeme = lexemes.get(*at)?.clone();
        *at += 1;
        match lexeme.as_str() {
            "{" => tokens.push(PsToken::Block(build(lexemes, at, depth + 1)?)),
            "}" => return Some(tokens),
            other => tokens.push(match other.parse::<f64>() {
                Ok(n) => PsToken::Number(n),
                Err(_) => PsToken::Operator(other.to_string()),
            }),
        }
    }
    Some(tokens)
}

fn exec(tokens: &[PsToken], stack: &mut Vec<PsValue>, depth: usize) -> Option<()> {
    if depth > MAX_BLOCK_DEPTH {
        return None;
    }
    let mut at = 0;
    while at < tokens.len() {
        if stack.len() > MAX_STACK {
            return None;
        }
        match tokens.get(at)? {
            PsToken::Number(n) => {
                stack.push(PsValue::Num(*n));
                at += 1;
            }
            PsToken::Operator(op) => {
                apply(op, stack)?;
                at += 1;
            }
            PsToken::Block(body) => at += conditional(tokens, at, body, stack, depth)?,
        }
    }
    Some(())
}

/// Runs the `{proc} if` or `{proc1} {proc2} ifelse` starting at `at`, returning how many
/// tokens it consumed. A block in any other position is a program this engine will not
/// guess at.
fn conditional(
    tokens: &[PsToken],
    at: usize,
    body: &[PsToken],
    stack: &mut Vec<PsValue>,
    depth: usize,
) -> Option<usize> {
    if is_operator(tokens.get(at + 1), "if") {
        if pop_bool(stack)? {
            exec(body, stack, depth + 1)?;
        }
        return Some(2);
    }
    let Some(PsToken::Block(other)) = tokens.get(at + 1) else {
        return None;
    };
    if !is_operator(tokens.get(at + 2), "ifelse") {
        return None;
    }
    let taken = if pop_bool(stack)? { body } else { other.as_slice() };
    exec(taken, stack, depth + 1)?;
    Some(3)
}

fn is_operator(token: Option<&PsToken>, name: &str) -> bool {
    matches!(token, Some(PsToken::Operator(op)) if op == name)
}

/// Dispatches one operator name against Table 42.
// RR-15 Limit: Dispatcher
fn apply(op: &str, stack: &mut Vec<PsValue>) -> Option<()> {
    match op {
        "true" => {
            stack.push(PsValue::Bool(true));
            Some(())
        }
        "false" => {
            stack.push(PsValue::Bool(false));
            Some(())
        }
        "add" | "sub" | "mul" | "div" | "idiv" | "mod" | "exp" | "atan" => binary_math(op, stack),
        "neg" | "abs" | "sqrt" | "sin" | "cos" | "ln" | "log" | "ceiling" | "floor" | "round"
        | "truncate" | "cvi" | "cvr" => unary_math(op, stack),
        "eq" | "ne" | "gt" | "ge" | "lt" | "le" => comparison(op, stack),
        "and" | "or" | "xor" | "bitshift" => logical_binary(op, stack),
        "not" => logical_not(stack),
        "pop" | "exch" | "dup" | "copy" | "index" | "roll" => stack_op(op, stack),
        _ => None,
    }
}

fn binary_math(op: &str, stack: &mut Vec<PsValue>) -> Option<()> {
    let b = pop_num(stack)?;
    let a = pop_num(stack)?;
    let value = match op {
        "add" => a + b,
        "sub" => a - b,
        "mul" => a * b,
        "div" => divide(a, b)?,
        "idiv" => int_to_f64(to_int(a)?.checked_div(to_int(b)?)?),
        "mod" => int_to_f64(to_int(a)?.checked_rem(to_int(b)?)?),
        // `base exponent exp`, not the exponential of one operand.
        "exp" => a.powf(b),
        // `num den atan`, in degrees on [0, 360) rather than radians on (−π, π].
        "atan" => atan_degrees(a, b),
        _ => return None,
    };
    push_finite(stack, value)
}

fn unary_math(op: &str, stack: &mut Vec<PsValue>) -> Option<()> {
    let a = pop_num(stack)?;
    let value = match op {
        "neg" => -a,
        "abs" => a.abs(),
        "sqrt" => positive_only(a)?.sqrt(),
        "sin" => a.to_radians().sin(),
        "cos" => a.to_radians().cos(),
        "ln" => strictly_positive(a)?.ln(),
        "log" => strictly_positive(a)?.log10(),
        "ceiling" => a.ceil(),
        "floor" => a.floor(),
        "round" => a.round(),
        // `cvi` truncates toward zero, which is `truncate`; the difference between them
        // is the resulting *type*, and this stack keeps numbers in one.
        "truncate" | "cvi" => a.trunc(),
        "cvr" => a,
        _ => return None,
    };
    push_finite(stack, value)
}

fn comparison(op: &str, stack: &mut Vec<PsValue>) -> Option<()> {
    let b = stack.pop()?;
    let a = stack.pop()?;
    let result = match (a, b) {
        (PsValue::Bool(x), PsValue::Bool(y)) => match op {
            "eq" => x == y,
            "ne" => x != y,
            // Table 42 defines the ordering comparisons on numbers only.
            _ => return None,
        },
        (PsValue::Num(x), PsValue::Num(y)) => match op {
            "eq" => (x - y).abs() < f64::EPSILON,
            "ne" => (x - y).abs() >= f64::EPSILON,
            "gt" => x > y,
            "ge" => x >= y,
            "lt" => x < y,
            "le" => x <= y,
            _ => return None,
        },
        (PsValue::Bool(_), PsValue::Num(_)) | (PsValue::Num(_), PsValue::Bool(_)) => return None,
    };
    stack.push(PsValue::Bool(result));
    Some(())
}

fn logical_binary(op: &str, stack: &mut Vec<PsValue>) -> Option<()> {
    let b = stack.pop()?;
    let a = stack.pop()?;
    match (a, b) {
        (PsValue::Bool(x), PsValue::Bool(y)) => {
            let value = match op {
                "and" => x && y,
                "or" => x || y,
                "xor" => x != y,
                // `bitshift` has no boolean form.
                _ => return None,
            };
            stack.push(PsValue::Bool(value));
        }
        (PsValue::Num(x), PsValue::Num(y)) => {
            let (i, j) = (to_int(x)?, to_int(y)?);
            let value = match op {
                "and" => i & j,
                "or" => i | j,
                "xor" => i ^ j,
                "bitshift" => shift(i, j),
                _ => return None,
            };
            stack.push(PsValue::Num(int_to_f64(value)));
        }
        (PsValue::Bool(_), PsValue::Num(_)) | (PsValue::Num(_), PsValue::Bool(_)) => return None,
    }
    Some(())
}

fn logical_not(stack: &mut Vec<PsValue>) -> Option<()> {
    let value = match stack.pop()? {
        PsValue::Bool(b) => PsValue::Bool(!b),
        PsValue::Num(n) => PsValue::Num(int_to_f64(!to_int(n)?)),
    };
    stack.push(value);
    Some(())
}

fn stack_op(op: &str, stack: &mut Vec<PsValue>) -> Option<()> {
    match op {
        "pop" => {
            stack.pop()?;
        }
        "exch" => {
            let len = stack.len();
            if len < 2 {
                return None;
            }
            stack.swap(len - 1, len - 2);
        }
        "dup" => {
            let top = *stack.last()?;
            stack.push(top);
        }
        "copy" => return copy_op(stack),
        "index" => return index_op(stack),
        "roll" => return roll_op(stack),
        _ => return None,
    }
    Some(())
}

fn copy_op(stack: &mut Vec<PsValue>) -> Option<()> {
    let count = to_count(pop_num(stack)?)?;
    let len = stack.len();
    if count > len || len + count > MAX_STACK {
        return None;
    }
    for i in (len - count)..len {
        let value = *stack.get(i)?;
        stack.push(value);
    }
    Some(())
}

fn index_op(stack: &mut Vec<PsValue>) -> Option<()> {
    let depth = to_count(pop_num(stack)?)?;
    let len = stack.len();
    if depth >= len {
        return None;
    }
    let value = *stack.get(len - 1 - depth)?;
    stack.push(value);
    Some(())
}

/// `any_(n−1) … any_0 n j roll`: a circular shift of the top `n`, positive `j` moving
/// elements toward the top of the stack.
fn roll_op(stack: &mut Vec<PsValue>) -> Option<()> {
    let by = to_int(pop_num(stack)?)?;
    let count = to_count(pop_num(stack)?)?;
    let len = stack.len();
    if count == 0 {
        return Some(());
    }
    if count > len {
        return None;
    }
    let modulus = i64::try_from(count).ok()?;
    let places = usize::try_from(by.rem_euclid(modulus)).ok()?;
    stack.get_mut(len - count..)?.rotate_right(places);
    Some(())
}

fn divide(a: f64, b: f64) -> Option<f64> {
    if b == 0.0_f64 { None } else { Some(a / b) }
}

fn positive_only(a: f64) -> Option<f64> {
    if a < 0.0_f64 { None } else { Some(a) }
}

fn strictly_positive(a: f64) -> Option<f64> {
    if a <= 0.0_f64 { None } else { Some(a) }
}

fn atan_degrees(num: f64, den: f64) -> f64 {
    let degrees = num.atan2(den).to_degrees();
    if degrees < 0.0_f64 { degrees + 360.0_f64 } else { degrees }
}

/// Shifts left for positive `by` and right for negative, wrapping rather than panicking:
/// a shift count out of range is a malformed program, not a reason to abort a render.
fn shift(value: i64, by: i64) -> i64 {
    let places = by.unsigned_abs().min(u64::from(u32::MAX));
    let places = u32::try_from(places).unwrap_or(u32::MAX);
    if places >= 64 {
        return 0;
    }
    if by >= 0 { value.wrapping_shl(places) } else { value.wrapping_shr(places) }
}

fn push_finite(stack: &mut Vec<PsValue>, value: f64) -> Option<()> {
    if !value.is_finite() {
        return None;
    }
    stack.push(PsValue::Num(value));
    Some(())
}

fn pop_num(stack: &mut Vec<PsValue>) -> Option<f64> {
    match stack.pop()? {
        PsValue::Num(n) => Some(n),
        PsValue::Bool(_) => None,
    }
}

fn pop_bool(stack: &mut Vec<PsValue>) -> Option<bool> {
    match stack.pop()? {
        PsValue::Bool(b) => Some(b),
        PsValue::Num(_) => None,
    }
}

/// Truncates toward zero into the integer domain the bitwise operators work in.
///
/// The bound is 2^53, where `f64` stops representing consecutive integers: past it the
/// truncation is not the number the program wrote, so it is refused rather than
/// approximated. Within the bound the `as` cast is exact, and Rust's float-to-int cast
/// saturates rather than being undefined — the lint is allowed here because the guard
/// above is what makes the *value* right, not what makes the cast safe.
#[allow(clippy::cast_possible_truncation)]
fn to_int(value: f64) -> Option<i64> {
    let truncated = value.trunc();
    if !truncated.is_finite() || truncated.abs() > 9_007_199_254_740_992.0_f64 {
        return None;
    }
    Some(truncated as i64)
}

/// The inverse of `to_int`, exact for every value `to_int` accepts.
#[allow(clippy::cast_precision_loss)]
fn int_to_f64(value: i64) -> f64 {
    value as f64
}

fn to_count(value: f64) -> Option<usize> {
    usize::try_from(to_int(value)?).ok()
}
