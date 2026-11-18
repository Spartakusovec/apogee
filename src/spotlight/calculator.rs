//! Spotlight calculator: + − × ÷ ^ %, brackets, decimal comma and point, functions and constants.
//!
//!     2+3*4 = 14        2^10 = 1024        (1,5 + 2) * 3 = 10,5
//!     200 + 10% = 220   15% z 80 = 12      15 % of 80 = 12
//!     sqrt(2), sin(30) (in degrees), ln, log, abs, round, pi, e, 5! = 120

#[derive(Clone, Copy)]
struct Value {
    number: f64,
    /// written as a percentage (200 + 10% = 220)
    percent: bool,
}

struct Parser {
    chars: Vec<char>,
    i: usize,
}

impl Parser {
    fn gaps(&mut self) {
        while self.chars.get(self.i).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn next(&mut self) -> Option<char> {
        self.gaps();
        self.chars.get(self.i).copied()
    }

    fn word(&mut self) -> Option<String> {
        self.gaps();
        let start = self.i;
        while self.chars.get(self.i).is_some_and(|c| c.is_alphabetic() || *c == 'π') {
            self.i += 1;
        }
        (self.i > start).then(|| self.chars[start..self.i].iter().collect::<String>().to_lowercase())
    }

    fn expression(&mut self) -> Option<Value> {
        let mut left = self.term()?;
        loop {
            let op = match self.next() {
                Some('+') => 1.0,
                Some('-' | '−') => -1.0,
                _ => return Some(left),
            };
            self.i += 1;
            let right = self.term()?;
            // 200 + 10 % = 220
            let add = if right.percent && !left.percent { left.number * right.number } else { right.number };
            left = Value { number: left.number + op * add, percent: false };
        }
    }

    fn term(&mut self) -> Option<Value> {
        let mut left = self.unary()?;
        loop {
            let position = self.i;
            let op = match self.next() {
                Some('*' | '×' | '·') => '*',
                Some('/' | '÷' | ':') => '/',
                Some(c) if c.is_alphabetic() => {
                    // „15 % z 80“, „15 % of 80“
                    match self.word().as_deref() {
                        Some("z" | "ze" | "of") => {
                            let right = self.unary()?;
                            left = Value { number: left.number * right.number, percent: false };
                            continue;
                        }
                        _ => {
                            self.i = position;
                            return Some(left);
                        }
                    }
                }
                _ => return Some(left),
            };
            self.i += 1;
            let right = self.unary()?;
            left = Value {
                number: if op == '*' { left.number * right.number } else { left.number / right.number },
                percent: false,
            };
        }
    }

    fn unary(&mut self) -> Option<Value> {
        match self.next() {
            Some('-' | '−') => {
                self.i += 1;
                let h = self.unary()?;
                Some(Value { number: -h.number, ..h })
            }
            Some('+') => {
                self.i += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Option<Value> {
        let base = self.after()?;
        if self.next() == Some('^') {
            self.i += 1;
            let exponent = self.unary()?;
            return Some(Value { number: base.number.powf(exponent.number), percent: false });
        }
        Some(base)
    }

    /// a number followed by % or ! (factorial)
    fn after(&mut self) -> Option<Value> {
        let mut h = self.base()?;
        loop {
            match self.next() {
                Some('%') => {
                    self.i += 1;
                    h = Value { number: h.number / 100.0, percent: true };
                }
                Some('!') => {
                    self.i += 1;
                    if h.number < 0.0 || h.number > 170.0 || h.number.fract() != 0.0 {
                        return None;
                    }
                    h = Value { number: (1..=h.number as u64).map(|x| x as f64).product(), percent: false };
                }
                _ => return Some(h),
            }
        }
    }

    fn base(&mut self) -> Option<Value> {
        let c = self.next()?;
        if c == '(' {
            self.i += 1;
            let h = self.expression()?;
            if self.next() != Some(')') {
                return None;
            }
            self.i += 1;
            return Some(h);
        }
        if c.is_ascii_digit() || c == '.' || c == ',' {
            let start = self.i;
            let mut separator = false;
            while let Some(&z) = self.chars.get(self.i) {
                if z.is_ascii_digit() {
                    self.i += 1;
                } else if (z == '.' || z == ',') && !separator {
                    separator = true;
                    self.i += 1;
                } else {
                    break;
                }
            }
            let text: String = self.chars[start..self.i].iter().map(|z| if *z == ',' { '.' } else { *z }).collect();
            return Some(Value { number: text.parse().ok()?, percent: false });
        }
        let word = self.word()?;
        let constant = match word.as_str() {
            "pi" | "π" => Some(std::f64::consts::PI),
            "e" => Some(std::f64::consts::E),
            _ => None,
        };
        if let Some(k) = constant {
            return Some(Value { number: k, percent: false });
        }
        let x = self.after()?.number;
        let degrees = |x: f64| x.to_radians();
        let v = match word.as_str() {
            "sqrt" | "odmocnina" => x.sqrt(),
            "sin" => degrees(x).sin(),
            "cos" => degrees(x).cos(),
            "tan" | "tg" => degrees(x).tan(),
            "asin" => x.asin().to_degrees(),
            "acos" => x.acos().to_degrees(),
            "atan" => x.atan().to_degrees(),
            "ln" => x.ln(),
            "log" => x.log10(),
            "abs" => x.abs(),
            "round" => x.round(),
            "floor" => x.floor(),
            "ceil" => x.ceil(),
            "exp" => x.exp(),
            _ => return None,
        };
        Some(Value { number: v, percent: false })
    }
}

/// Is it an expression (at least one operator or function), not just a number or a word?
fn looks_jako_example(text: &str) -> bool {
    let ma_digit = text.chars().any(|c| c.is_ascii_digit()) || text.contains("pi") || text.contains('π');
    let ma_operator = text.chars().any(|c| "+-−*×·/÷^%!(".contains(c))
        || ["sqrt", "sin", "cos", "tan", "ln", "log", "abs", "round", "exp"].iter().any(|f| text.to_lowercase().contains(f));
    ma_digit && ma_operator
}

/// The result of the expression, or None when it isn't one.
pub fn compute(text: &str) -> Option<f64> {
    let text = text.trim().trim_end_matches('=');
    if !looks_jako_example(text) {
        return None;
    }
    let mut p = Parser { chars: text.chars().collect(), i: 0 };
    let h = p.expression()?;
    p.gaps();
    (p.i == p.chars.len() && h.number.is_finite()).then_some(h.number)
}

/// A number nicely: no needless zeros, at most 10 significant digits, with a comma if the user typed one.
pub fn format(x: f64, comma: bool) -> String {
    let text = if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        let valid = 10 - (x.abs().log10().floor() as i32 + 1).clamp(-5, 10);
        let t = format!("{:.*}", valid.max(0) as usize, x);
        let t = if t.contains('.') { t.trim_end_matches('0').trim_end_matches('.').to_string() } else { t };
        if t == "-0" { "0".into() } else { t }
    };
    if comma { text.replace('.', ",") } else { text }
}

#[cfg(test)]
mod testy {
    use super::*;

    #[test]
    fn examples() {
        let s = |t: &str| compute(t).map(|x| format(x, false));
        assert_eq!(s("2+3*4").as_deref(), Some("14"));
        assert_eq!(s("2^10").as_deref(), Some("1024"));
        assert_eq!(s("(1,5 + 2) * 3").as_deref(), Some("10.5"));
        assert_eq!(s("200 + 10%").as_deref(), Some("220"));
        assert_eq!(s("200 - 10 %").as_deref(), Some("180"));
        assert_eq!(s("15% z 80").as_deref(), Some("12"));
        assert_eq!(s("15 % of 80").as_deref(), Some("12"));
        assert_eq!(s("sqrt(2)").as_deref(), Some("1.414213562"));
        assert_eq!(s("sin(30)").as_deref(), Some("0.5"));
        assert_eq!(s("5!").as_deref(), Some("120"));
        assert_eq!(s("10/4").as_deref(), Some("2.5"));
        assert_eq!(s("-3^2").as_deref(), Some("-9"));
        assert_eq!(s("2*pi").as_deref(), Some("6.283185307"));
        assert_eq!(s("1/3").as_deref(), Some("0.3333333333"));
        assert_eq!(s("firefox"), None);
        assert_eq!(s("42"), None);
        assert_eq!(s("2+"), None);
    }
}
