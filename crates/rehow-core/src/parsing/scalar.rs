use crate::domain::{CanonicalYamlFloat, CanonicalYamlInteger, UnknownYamlEvent};

use super::reconcile::{ScalarPresentationStyle, SourceTag};
use super::source::{ParseFailure, ParseFailureKind, ParseLimitScope};

const MAX_SCALAR: usize = 65_536;
const MAX_NUMERIC_WORK: u64 = 16 * 1024 * 1024;
const MAX_NUMERIC_DIGITS: u64 = 80 * 1024;

pub(super) struct NumericMeter {
    work: u64,
    digits: u64,
}

impl NumericMeter {
    pub(super) fn new() -> Self {
        Self { work: 0, digits: 0 }
    }

    fn charge_work(&mut self, amount: u64) -> Result<(), ParseFailure> {
        let next = self
            .work
            .checked_add(amount)
            .ok_or_else(|| arithmetic_overflow(ParseLimitScope::NumericWork, MAX_NUMERIC_WORK))?;
        if next > MAX_NUMERIC_WORK {
            return Err(ParseFailure::limited(
                ParseFailureKind::NumericWorkLimit,
                ParseLimitScope::NumericWork,
                MAX_NUMERIC_WORK,
                next,
            ));
        }
        self.work = next;
        Ok(())
    }

    fn charge_digits(&mut self, amount: usize) -> Result<(), ParseFailure> {
        let amount = u64::try_from(amount)
            .map_err(|_| arithmetic_overflow(ParseLimitScope::NumericDigits, MAX_NUMERIC_DIGITS))?;
        let next = self.digits.checked_add(amount).ok_or_else(|| {
            arithmetic_overflow(ParseLimitScope::NumericDigits, MAX_NUMERIC_DIGITS)
        })?;
        if next > MAX_NUMERIC_DIGITS {
            return Err(ParseFailure::limited(
                ParseFailureKind::NumericDigitLimit,
                ParseLimitScope::NumericDigits,
                MAX_NUMERIC_DIGITS,
                next,
            ));
        }
        self.digits = next;
        Ok(())
    }

    fn precharge(&mut self, source_digits: usize, radix: bool) -> Result<(), ParseFailure> {
        let source_digits = u64::try_from(source_digits)
            .map_err(|_| arithmetic_overflow(ParseLimitScope::NumericWork, MAX_NUMERIC_WORK))?;
        let quadratic = if radix {
            source_digits.checked_mul(source_digits).ok_or_else(|| {
                arithmetic_overflow(ParseLimitScope::NumericWork, MAX_NUMERIC_WORK)
            })?
        } else {
            0
        };
        let linear = source_digits
            .checked_mul(4)
            .ok_or_else(|| arithmetic_overflow(ParseLimitScope::NumericWork, MAX_NUMERIC_WORK))?;
        let amount = quadratic
            .checked_add(linear)
            .ok_or_else(|| arithmetic_overflow(ParseLimitScope::NumericWork, MAX_NUMERIC_WORK))?;
        self.charge_work(amount)
    }

    fn charge_result(&mut self, canonical_digits: usize) -> Result<(), ParseFailure> {
        let canonical_digits = u64::try_from(canonical_digits)
            .map_err(|_| arithmetic_overflow(ParseLimitScope::NumericWork, MAX_NUMERIC_WORK))?;
        let amount = canonical_digits
            .checked_mul(4)
            .ok_or_else(|| arithmetic_overflow(ParseLimitScope::NumericWork, MAX_NUMERIC_WORK))?;
        self.charge_work(amount)
    }

    #[cfg(test)]
    pub(super) fn seed_for_test(&mut self, work: u64, digits: u64) {
        self.work = work;
        self.digits = digits;
    }

    #[cfg(test)]
    pub(super) fn values_for_test(&self) -> (u64, u64) {
        (self.work, self.digits)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Form {
    Null,
    Boolean(bool),
    DecimalInteger,
    OctalInteger,
    HexInteger,
    FiniteFloat,
    PositiveInfinity,
    NegativeInfinity,
    Nan,
    String,
}

pub(super) struct BorrowedScalarAnalysis {
    form: Form,
    noncharacter: bool,
    merge: bool,
    tilde: bool,
    visited_bytes: u64,
}

impl BorrowedScalarAnalysis {
    pub(super) fn inspect(
        text: &str,
        mut retained_copy: Option<&mut String>,
        source_byte_visits: &mut u64,
    ) -> Result<Self, ParseFailure> {
        const LITERALS: [&[u8]; 24] = [
            b"", b"~", b"null", b"Null", b"NULL", b"true", b"True", b"TRUE", b"false", b"False",
            b"FALSE", b".inf", b".Inf", b".INF", b"+.inf", b"+.Inf", b"+.INF", b"-.inf", b"-.Inf",
            b"-.INF", b".nan", b".NaN", b".NAN", b"<<",
        ];
        let mut literal = [true; LITERALS.len()];
        let mut noncharacter = false;
        let mut decimal = true;
        let mut decimal_digits = 0usize;
        let mut octal = true;
        let mut hex = true;
        let mut float = FloatScan::new();
        let mut byte_index = 0usize;

        for character in text.chars() {
            let width = character.len_utf8();
            *source_byte_visits = source_byte_visits
                .checked_add(u64::try_from(width).map_err(|_| invariant())?)
                .ok_or_else(invariant)?;
            if let Some(copy) = retained_copy.as_deref_mut() {
                copy.push(character);
            }
            noncharacter |= is_noncharacter(character);
            let ascii = character.is_ascii().then_some(character as u8);
            for (matches, candidate) in literal.iter_mut().zip(LITERALS) {
                *matches &= ascii.is_some_and(|byte| candidate.get(byte_index) == Some(&byte));
            }
            if let Some(byte) = ascii {
                let index = byte_index;
                if index == 0 && matches!(byte, b'+' | b'-') {
                } else if byte.is_ascii_digit() {
                    decimal_digits = decimal_digits.checked_add(1).ok_or_else(invariant)?;
                } else {
                    decimal = false;
                }
                if index < 2 {
                    octal &= matches!((index, byte), (0, b'0') | (1, b'o'));
                    hex &= matches!((index, byte), (0, b'0') | (1, b'x'));
                } else {
                    octal &= matches!(byte, b'0'..=b'7');
                    hex &= byte.is_ascii_hexdigit();
                }
                float.accept(index, byte);
            } else {
                decimal = false;
                octal = false;
                hex = false;
                float.invalid = true;
            }
            byte_index = byte_index.checked_add(width).ok_or_else(invariant)?;
        }
        for (matches, candidate) in literal.iter_mut().zip(LITERALS) {
            *matches &= candidate.len() == byte_index;
        }
        let form = if literal[0] || literal[1] || literal[2] || literal[3] || literal[4] {
            Form::Null
        } else if literal[5] || literal[6] || literal[7] {
            Form::Boolean(true)
        } else if literal[8] || literal[9] || literal[10] {
            Form::Boolean(false)
        } else if octal && byte_index > 2 {
            Form::OctalInteger
        } else if hex && byte_index > 2 {
            Form::HexInteger
        } else if decimal && decimal_digits > 0 {
            Form::DecimalInteger
        } else if literal[11]
            || literal[12]
            || literal[13]
            || literal[14]
            || literal[15]
            || literal[16]
        {
            Form::PositiveInfinity
        } else if literal[17] || literal[18] || literal[19] {
            Form::NegativeInfinity
        } else if literal[20] || literal[21] || literal[22] {
            Form::Nan
        } else if float.valid() {
            Form::FiniteFloat
        } else {
            Form::String
        };
        Ok(Self {
            form,
            noncharacter,
            merge: literal[23],
            tilde: literal[1],
            visited_bytes: u64::try_from(byte_index).map_err(|_| invariant())?,
        })
    }

    pub(super) fn form_valid_for(&self, tag: SourceTag) -> bool {
        match tag {
            SourceTag::Null => self.form == Form::Null,
            SourceTag::Boolean => matches!(self.form, Form::Boolean(_)),
            SourceTag::Integer => matches!(
                self.form,
                Form::DecimalInteger | Form::OctalInteger | Form::HexInteger
            ),
            SourceTag::Float => matches!(
                self.form,
                Form::FiniteFloat | Form::PositiveInfinity | Form::NegativeInfinity | Form::Nan
            ),
            SourceTag::String => true,
            SourceTag::Sequence | SourceTag::Mapping | SourceTag::Implicit => false,
        }
    }

    pub(super) fn resolves_to_string(
        &self,
        style: ScalarPresentationStyle,
        tag: SourceTag,
    ) -> bool {
        tag == SourceTag::String
            || (tag == SourceTag::Implicit
                && (style != ScalarPresentationStyle::Plain || self.form == Form::String))
    }

    pub(super) fn contains_noncharacter(&self) -> bool {
        self.noncharacter
    }

    pub(super) fn is_merge(&self) -> bool {
        self.merge
    }

    pub(super) fn is_tilde(&self) -> bool {
        self.tilde
    }

    pub(super) fn visited_bytes(&self) -> u64 {
        self.visited_bytes
    }
}

struct FloatScan {
    invalid: bool,
    mantissa_digits: usize,
    fraction_digits: usize,
    dot: bool,
    exponent: bool,
    exponent_sign_allowed: bool,
    exponent_digits: usize,
    first_body: Option<u8>,
}

impl FloatScan {
    fn new() -> Self {
        Self {
            invalid: false,
            mantissa_digits: 0,
            fraction_digits: 0,
            dot: false,
            exponent: false,
            exponent_sign_allowed: false,
            exponent_digits: 0,
            first_body: None,
        }
    }

    fn accept(&mut self, index: usize, byte: u8) {
        if index == 0 && matches!(byte, b'+' | b'-') {
            return;
        }
        self.first_body.get_or_insert(byte);
        if self.exponent {
            if self.exponent_sign_allowed && matches!(byte, b'+' | b'-') {
                self.exponent_sign_allowed = false;
            } else if byte.is_ascii_digit() {
                self.exponent_sign_allowed = false;
                self.exponent_digits += 1;
            } else {
                self.invalid = true;
            }
        } else if byte.is_ascii_digit() {
            self.mantissa_digits += 1;
            if self.dot {
                self.fraction_digits += 1;
            }
        } else if byte == b'.' && !self.dot {
            self.dot = true;
        } else if matches!(byte, b'e' | b'E') && self.mantissa_digits > 0 {
            self.exponent = true;
            self.exponent_sign_allowed = true;
        } else {
            self.invalid = true;
        }
    }

    fn valid(&self) -> bool {
        !self.invalid
            && self.mantissa_digits > 0
            && (self.first_body != Some(b'.') || self.fraction_digits > 0)
            && (!self.exponent || self.exponent_digits > 0)
            && (self.dot || self.exponent)
    }
}

pub(super) fn resolve(
    text: String,
    style: ScalarPresentationStyle,
    tag: SourceTag,
    meter: &mut NumericMeter,
) -> Result<UnknownYamlEvent, ParseFailure> {
    if resolves_to_string(&text, style, tag) && contains_noncharacter(&text) {
        return Err(ParseFailure::plain(ParseFailureKind::InvalidUnicodeScalar));
    }
    let form = selected_form(&text, style, tag)
        .ok_or_else(|| ParseFailure::plain(ParseFailureKind::ExplicitTagValueMismatch))?;
    match form {
        Form::Null => Ok(UnknownYamlEvent::Null),
        Form::Boolean(value) => Ok(UnknownYamlEvent::Boolean(value)),
        Form::String => Ok(UnknownYamlEvent::String(text)),
        Form::DecimalInteger => decimal_integer(&text, meter),
        Form::OctalInteger => radix_integer(&text[2..], 8, meter),
        Form::HexInteger => radix_integer(&text[2..], 16, meter),
        Form::FiniteFloat => finite_float(&text, meter),
        Form::PositiveInfinity => Ok(UnknownYamlEvent::Float(
            CanonicalYamlFloat::positive_infinity(),
        )),
        Form::NegativeInfinity => Ok(UnknownYamlEvent::Float(
            CanonicalYamlFloat::negative_infinity(),
        )),
        Form::Nan => Ok(UnknownYamlEvent::Float(CanonicalYamlFloat::nan())),
    }
}

pub(super) fn explicit_form_valid(text: &str, tag: SourceTag) -> bool {
    match tag {
        SourceTag::Null => is_null(text),
        SourceTag::Boolean => boolean(text).is_some(),
        SourceTag::Integer => integer_form(text).is_some(),
        SourceTag::Float => float_form(text).is_some(),
        SourceTag::String => true,
        SourceTag::Sequence | SourceTag::Mapping | SourceTag::Implicit => false,
    }
}

pub(super) fn resolves_to_string(
    text: &str,
    style: ScalarPresentationStyle,
    tag: SourceTag,
) -> bool {
    match tag {
        SourceTag::String => true,
        SourceTag::Implicit if style != ScalarPresentationStyle::Plain => true,
        SourceTag::Implicit => matches!(implicit_form(text), Form::String),
        _ => false,
    }
}

pub(super) fn contains_noncharacter(text: &str) -> bool {
    text.chars().any(is_noncharacter)
}

fn selected_form(text: &str, style: ScalarPresentationStyle, tag: SourceTag) -> Option<Form> {
    if tag == SourceTag::Implicit {
        if style == ScalarPresentationStyle::Plain {
            Some(implicit_form(text))
        } else {
            Some(Form::String)
        }
    } else {
        match tag {
            SourceTag::Null if is_null(text) => Some(Form::Null),
            SourceTag::Boolean => boolean(text).map(Form::Boolean),
            SourceTag::Integer => integer_form(text),
            SourceTag::Float => float_form(text),
            SourceTag::String => Some(Form::String),
            SourceTag::Null | SourceTag::Sequence | SourceTag::Mapping | SourceTag::Implicit => {
                None
            }
        }
    }
}

fn implicit_form(text: &str) -> Form {
    if is_null(text) {
        Form::Null
    } else if let Some(value) = boolean(text) {
        Form::Boolean(value)
    } else if let Some(form) = integer_form(text) {
        form
    } else if let Some(form) = float_form(text) {
        form
    } else {
        Form::String
    }
}

fn is_null(text: &str) -> bool {
    matches!(text, "" | "~" | "null" | "Null" | "NULL")
}

fn boolean(text: &str) -> Option<bool> {
    match text {
        "true" | "True" | "TRUE" => Some(true),
        "false" | "False" | "FALSE" => Some(false),
        _ => None,
    }
}

fn nonempty_digits(text: &str, predicate: impl Fn(u8) -> bool) -> bool {
    !text.is_empty() && text.bytes().all(predicate)
}

fn decimal_grammar(text: &str) -> bool {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    nonempty_digits(digits, |byte| byte.is_ascii_digit())
}

fn integer_form(text: &str) -> Option<Form> {
    if let Some(digits) = text.strip_prefix("0o")
        && nonempty_digits(digits, |byte| matches!(byte, b'0'..=b'7'))
    {
        Some(Form::OctalInteger)
    } else if let Some(digits) = text.strip_prefix("0x")
        && nonempty_digits(digits, |byte| byte.is_ascii_hexdigit())
    {
        Some(Form::HexInteger)
    } else if decimal_grammar(text) {
        Some(Form::DecimalInteger)
    } else {
        None
    }
}

fn finite_float_grammar(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let mut mantissa_digits = 0usize;
    let mut fraction_digits = 0usize;
    let mut dot = false;
    let mut exponent = false;
    let mut exponent_sign_allowed = false;
    let mut exponent_digits = 0usize;
    for byte in body.bytes() {
        if exponent {
            if exponent_sign_allowed && matches!(byte, b'+' | b'-') {
                exponent_sign_allowed = false;
            } else if byte.is_ascii_digit() {
                exponent_sign_allowed = false;
                exponent_digits += 1;
            } else {
                return false;
            }
        } else if byte.is_ascii_digit() {
            mantissa_digits += 1;
            if dot {
                fraction_digits += 1;
            }
        } else if byte == b'.' && !dot {
            dot = true;
        } else if matches!(byte, b'e' | b'E') && mantissa_digits > 0 {
            exponent = true;
            exponent_sign_allowed = true;
        } else {
            return false;
        }
    }
    mantissa_digits > 0
        && (!body.starts_with('.') || fraction_digits > 0)
        && (!exponent || exponent_digits > 0)
        && (dot || exponent)
}

fn float_form(text: &str) -> Option<Form> {
    match text {
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => Some(Form::PositiveInfinity),
        "-.inf" | "-.Inf" | "-.INF" => Some(Form::NegativeInfinity),
        ".nan" | ".NaN" | ".NAN" => Some(Form::Nan),
        _ if finite_float_grammar(text) => Some(Form::FiniteFloat),
        _ => None,
    }
}

fn decimal_integer(text: &str, meter: &mut NumericMeter) -> Result<UnknownYamlEvent, ParseFailure> {
    let negative = text.starts_with('-');
    let source = text.strip_prefix(['+', '-']).unwrap_or(text);
    meter.precharge(source.len(), false)?;
    let magnitude = source.trim_start_matches('0');
    let canonical_len = magnitude.len().max(1);
    meter.charge_result(canonical_len)?;
    enforce_canonical_limit(canonical_len)?;
    meter.charge_digits(canonical_len)?;

    let mut digits = String::new();
    digits
        .try_reserve_exact(canonical_len)
        .map_err(|_| invariant())?;
    if magnitude.is_empty() {
        digits.push('0');
    } else {
        digits.push_str(magnitude);
    }
    let value = CanonicalYamlInteger::new(negative, digits).map_err(|_| invariant())?;
    Ok(UnknownYamlEvent::Integer(value))
}

fn radix_integer(
    source: &str,
    radix: u8,
    meter: &mut NumericMeter,
) -> Result<UnknownYamlEvent, ParseFailure> {
    meter.precharge(source.len(), true)?;
    let capacity = source
        .len()
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| arithmetic_overflow(ParseLimitScope::NumericWork, MAX_NUMERIC_WORK))?;
    let mut decimal = Vec::new();
    decimal
        .try_reserve_exact(capacity)
        .map_err(|_| invariant())?;
    decimal.push(0u8);
    for byte in source.bytes() {
        let digit = radix_digit(byte).ok_or_else(invariant)?;
        let mut carry = u16::from(digit);
        for value in &mut decimal {
            let next = u16::from(*value)
                .checked_mul(u16::from(radix))
                .and_then(|value| value.checked_add(carry))
                .ok_or_else(invariant)?;
            *value = u8::try_from(next % 10).map_err(|_| invariant())?;
            carry = next / 10;
        }
        while carry != 0 {
            decimal.push(u8::try_from(carry % 10).map_err(|_| invariant())?);
            carry /= 10;
        }
    }
    while decimal.len() > 1 && decimal.last() == Some(&0) {
        decimal.pop();
    }
    let canonical_len = decimal.len();
    meter.charge_result(canonical_len)?;
    enforce_canonical_limit(canonical_len)?;
    meter.charge_digits(canonical_len)?;
    decimal.reverse();
    for digit in &mut decimal {
        *digit = digit.checked_add(b'0').ok_or_else(invariant)?;
    }
    let digits = String::from_utf8(decimal).map_err(|_| invariant())?;
    let value = CanonicalYamlInteger::new(false, digits).map_err(|_| invariant())?;
    Ok(UnknownYamlEvent::Integer(value))
}

fn radix_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

struct SignedDecimal {
    negative: bool,
    digits: String,
}

fn finite_float(text: &str, meter: &mut NumericMeter) -> Result<UnknownYamlEvent, ParseFailure> {
    let negative = text.starts_with('-');
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent_text) = match body.find(['e', 'E']) {
        Some(index) => (&body[..index], Some(&body[index + 1..])),
        None => (body, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let source_digits = text.bytes().filter(|byte| byte.is_ascii_digit()).count();
    meter.precharge(source_digits, false)?;

    let mantissa_digits = whole
        .len()
        .checked_add(fraction.len())
        .ok_or_else(|| arithmetic_overflow(ParseLimitScope::NumericWork, MAX_NUMERIC_WORK))?;
    let mut coefficient = String::new();
    coefficient
        .try_reserve_exact(mantissa_digits)
        .map_err(|_| invariant())?;
    let mut leading = true;
    for byte in whole.bytes().chain(fraction.bytes()) {
        if leading && byte == b'0' {
            continue;
        }
        leading = false;
        coefficient.push(char::from(byte));
    }
    if coefficient.is_empty() {
        coefficient.push('0');
    }

    let mut exponent = parse_signed_decimal(exponent_text.unwrap_or("0"))?;
    exponent = add_unsigned(exponent, true, fraction.len())?;
    if coefficient != "0" {
        let trailing = coefficient
            .as_bytes()
            .iter()
            .rev()
            .take_while(|byte| **byte == b'0')
            .count();
        coefficient.truncate(coefficient.len() - trailing);
        exponent = add_unsigned(exponent, false, trailing)?;
    } else {
        exponent = SignedDecimal {
            negative: false,
            digits: "0".to_owned(),
        };
    }

    let canonical_len = coefficient
        .len()
        .checked_add(exponent.digits.len())
        .ok_or_else(|| arithmetic_overflow(ParseLimitScope::NumericWork, MAX_NUMERIC_WORK))?;
    meter.charge_result(canonical_len)?;
    enforce_canonical_limit(canonical_len)?;
    meter.charge_digits(canonical_len)?;

    let exponent =
        CanonicalYamlInteger::new(exponent.negative, exponent.digits).map_err(|_| invariant())?;
    let value =
        CanonicalYamlFloat::finite(negative, coefficient, exponent).map_err(|_| invariant())?;
    Ok(UnknownYamlEvent::Float(value))
}

fn parse_signed_decimal(text: &str) -> Result<SignedDecimal, ParseFailure> {
    let negative = text.starts_with('-');
    let source = text.strip_prefix(['+', '-']).unwrap_or(text);
    let magnitude = source.trim_start_matches('0');
    let canonical_len = magnitude.len().max(1);
    let mut digits = String::new();
    digits
        .try_reserve_exact(canonical_len)
        .map_err(|_| invariant())?;
    if magnitude.is_empty() {
        digits.push('0');
    } else {
        digits.push_str(magnitude);
    }
    Ok(SignedDecimal {
        negative: negative && digits != "0",
        digits,
    })
}

fn add_unsigned(
    value: SignedDecimal,
    negative: bool,
    magnitude: usize,
) -> Result<SignedDecimal, ParseFailure> {
    if magnitude == 0 {
        return Ok(value);
    }
    let mut buffer = [0u8; 40];
    let rhs = usize_decimal(magnitude, &mut buffer)?;
    if value.negative == negative {
        return Ok(SignedDecimal {
            negative,
            digits: add_magnitudes(&value.digits, rhs)?,
        });
    }
    match compare_magnitudes(&value.digits, rhs) {
        core::cmp::Ordering::Greater => Ok(SignedDecimal {
            negative: value.negative,
            digits: subtract_magnitudes(&value.digits, rhs)?,
        }),
        core::cmp::Ordering::Less => Ok(SignedDecimal {
            negative,
            digits: subtract_magnitudes(rhs, &value.digits)?,
        }),
        core::cmp::Ordering::Equal => Ok(SignedDecimal {
            negative: false,
            digits: "0".to_owned(),
        }),
    }
}

fn usize_decimal(mut value: usize, buffer: &mut [u8; 40]) -> Result<&str, ParseFailure> {
    let mut cursor = buffer.len();
    while value != 0 {
        cursor = cursor.checked_sub(1).ok_or_else(invariant)?;
        buffer[cursor] = b'0' + u8::try_from(value % 10).map_err(|_| invariant())?;
        value /= 10;
    }
    core::str::from_utf8(&buffer[cursor..]).map_err(|_| invariant())
}

fn compare_magnitudes(left: &str, right: &str) -> core::cmp::Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn add_magnitudes(left: &str, right: &str) -> Result<String, ParseFailure> {
    let capacity = left
        .len()
        .max(right.len())
        .checked_add(1)
        .ok_or_else(invariant)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| invariant())?;
    let mut left = left.bytes().rev();
    let mut right = right.bytes().rev();
    let mut carry = 0u8;
    loop {
        let left_digit = left.next().map(|byte| byte - b'0');
        let right_digit = right.next().map(|byte| byte - b'0');
        if left_digit.is_none() && right_digit.is_none() && carry == 0 {
            break;
        }
        let sum = left_digit.unwrap_or(0) + right_digit.unwrap_or(0) + carry;
        output.push(b'0' + sum % 10);
        carry = sum / 10;
    }
    output.reverse();
    String::from_utf8(output).map_err(|_| invariant())
}

fn subtract_magnitudes(left: &str, right: &str) -> Result<String, ParseFailure> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(left.len())
        .map_err(|_| invariant())?;
    let mut right = right.bytes().rev();
    let mut borrow = 0i8;
    for left_byte in left.bytes().rev() {
        let right_digit = right.next().map_or(0, |byte| (byte - b'0') as i8);
        let mut digit = (left_byte - b'0') as i8 - borrow - right_digit;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        output.push(b'0' + u8::try_from(digit).map_err(|_| invariant())?);
    }
    while output.len() > 1 && output.last() == Some(&b'0') {
        output.pop();
    }
    output.reverse();
    String::from_utf8(output).map_err(|_| invariant())
}

fn enforce_canonical_limit(canonical_len: usize) -> Result<(), ParseFailure> {
    if canonical_len > MAX_SCALAR {
        Err(ParseFailure::limited(
            ParseFailureKind::CanonicalScalarLimit,
            ParseLimitScope::CanonicalScalarBytes,
            MAX_SCALAR as u64,
            (MAX_SCALAR + 1) as u64,
        ))
    } else {
        Ok(())
    }
}

fn arithmetic_overflow(scope: ParseLimitScope, limit: u64) -> ParseFailure {
    ParseFailure::limited(
        ParseFailureKind::MeterArithmeticOverflow,
        scope,
        limit,
        u64::MAX,
    )
}

fn invariant() -> ParseFailure {
    ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant)
}

fn is_noncharacter(character: char) -> bool {
    let value = character as u32;
    (0xFDD0..=0xFDEF).contains(&value) || value & 0xFFFF >= 0xFFFE
}

#[cfg(test)]
pub(super) fn grammar_form_for_test(text: &str, plain: bool, tag: SourceTag) -> u8 {
    let style = if plain {
        ScalarPresentationStyle::Plain
    } else {
        ScalarPresentationStyle::DoubleQuoted
    };
    match selected_form(text, style, tag) {
        Some(Form::Null) => 0,
        Some(Form::Boolean(_)) => 1,
        Some(Form::DecimalInteger) => 2,
        Some(Form::OctalInteger) => 3,
        Some(Form::HexInteger) => 4,
        Some(Form::FiniteFloat) => 5,
        Some(Form::PositiveInfinity) => 6,
        Some(Form::NegativeInfinity) => 7,
        Some(Form::Nan) => 8,
        Some(Form::String) => 9,
        None => 10,
    }
}
