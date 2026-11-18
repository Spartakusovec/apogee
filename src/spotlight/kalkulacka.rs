//! Kalkulačka spotlightu: + − × ÷ ^ %, závorky, desetinná čárka i tečka, funkce a konstanty.
//!
//!     2+3*4 = 14        2^10 = 1024        (1,5 + 2) * 3 = 10,5
//!     200 + 10% = 220   15% z 80 = 12      15 % of 80 = 12
//!     sqrt(2), sin(30) (ve stupních), ln, log, abs, round, pi, e, 5! = 120

#[derive(Clone, Copy)]
struct Hodnota {
    cislo: f64,
    /// napsané jako procenta (200 + 10% = 220)
    procenta: bool,
}

struct Parser {
    znaky: Vec<char>,
    i: usize,
}

impl Parser {
    fn mezery(&mut self) {
        while self.znaky.get(self.i).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn dalsi(&mut self) -> Option<char> {
        self.mezery();
        self.znaky.get(self.i).copied()
    }

    fn slovo(&mut self) -> Option<String> {
        self.mezery();
        let start = self.i;
        while self.znaky.get(self.i).is_some_and(|c| c.is_alphabetic() || *c == 'π') {
            self.i += 1;
        }
        (self.i > start).then(|| self.znaky[start..self.i].iter().collect::<String>().to_lowercase())
    }

    fn vyraz(&mut self) -> Option<Hodnota> {
        let mut vlevo = self.clen()?;
        loop {
            let op = match self.dalsi() {
                Some('+') => 1.0,
                Some('-' | '−') => -1.0,
                _ => return Some(vlevo),
            };
            self.i += 1;
            let vpravo = self.clen()?;
            // 200 + 10 % = 220
            let pricist = if vpravo.procenta && !vlevo.procenta { vlevo.cislo * vpravo.cislo } else { vpravo.cislo };
            vlevo = Hodnota { cislo: vlevo.cislo + op * pricist, procenta: false };
        }
    }

    fn clen(&mut self) -> Option<Hodnota> {
        let mut vlevo = self.unarni()?;
        loop {
            let pozice = self.i;
            let op = match self.dalsi() {
                Some('*' | '×' | '·') => '*',
                Some('/' | '÷' | ':') => '/',
                Some(c) if c.is_alphabetic() => {
                    // „15 % z 80“, „15 % of 80“
                    match self.slovo().as_deref() {
                        Some("z" | "ze" | "of") => {
                            let vpravo = self.unarni()?;
                            vlevo = Hodnota { cislo: vlevo.cislo * vpravo.cislo, procenta: false };
                            continue;
                        }
                        _ => {
                            self.i = pozice;
                            return Some(vlevo);
                        }
                    }
                }
                _ => return Some(vlevo),
            };
            self.i += 1;
            let vpravo = self.unarni()?;
            vlevo = Hodnota {
                cislo: if op == '*' { vlevo.cislo * vpravo.cislo } else { vlevo.cislo / vpravo.cislo },
                procenta: false,
            };
        }
    }

    fn unarni(&mut self) -> Option<Hodnota> {
        match self.dalsi() {
            Some('-' | '−') => {
                self.i += 1;
                let h = self.unarni()?;
                Some(Hodnota { cislo: -h.cislo, ..h })
            }
            Some('+') => {
                self.i += 1;
                self.unarni()
            }
            _ => self.mocnina(),
        }
    }

    fn mocnina(&mut self) -> Option<Hodnota> {
        let zaklad = self.za()?;
        if self.dalsi() == Some('^') {
            self.i += 1;
            let exponent = self.unarni()?;
            return Some(Hodnota { cislo: zaklad.cislo.powf(exponent.cislo), procenta: false });
        }
        Some(zaklad)
    }

    /// číslo a za ním % nebo ! (faktoriál)
    fn za(&mut self) -> Option<Hodnota> {
        let mut h = self.zaklad()?;
        loop {
            match self.dalsi() {
                Some('%') => {
                    self.i += 1;
                    h = Hodnota { cislo: h.cislo / 100.0, procenta: true };
                }
                Some('!') => {
                    self.i += 1;
                    if h.cislo < 0.0 || h.cislo > 170.0 || h.cislo.fract() != 0.0 {
                        return None;
                    }
                    h = Hodnota { cislo: (1..=h.cislo as u64).map(|x| x as f64).product(), procenta: false };
                }
                _ => return Some(h),
            }
        }
    }

    fn zaklad(&mut self) -> Option<Hodnota> {
        let c = self.dalsi()?;
        if c == '(' {
            self.i += 1;
            let h = self.vyraz()?;
            if self.dalsi() != Some(')') {
                return None;
            }
            self.i += 1;
            return Some(h);
        }
        if c.is_ascii_digit() || c == '.' || c == ',' {
            let start = self.i;
            let mut oddelovac = false;
            while let Some(&z) = self.znaky.get(self.i) {
                if z.is_ascii_digit() {
                    self.i += 1;
                } else if (z == '.' || z == ',') && !oddelovac {
                    oddelovac = true;
                    self.i += 1;
                } else {
                    break;
                }
            }
            let text: String = self.znaky[start..self.i].iter().map(|z| if *z == ',' { '.' } else { *z }).collect();
            return Some(Hodnota { cislo: text.parse().ok()?, procenta: false });
        }
        let slovo = self.slovo()?;
        let konstanta = match slovo.as_str() {
            "pi" | "π" => Some(std::f64::consts::PI),
            "e" => Some(std::f64::consts::E),
            _ => None,
        };
        if let Some(k) = konstanta {
            return Some(Hodnota { cislo: k, procenta: false });
        }
        let x = self.za()?.cislo;
        let stupne = |x: f64| x.to_radians();
        let v = match slovo.as_str() {
            "sqrt" | "odmocnina" => x.sqrt(),
            "sin" => stupne(x).sin(),
            "cos" => stupne(x).cos(),
            "tan" | "tg" => stupne(x).tan(),
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
        Some(Hodnota { cislo: v, procenta: false })
    }
}

/// Je to příklad (aspoň jeden operátor nebo funkce), ne jen číslo nebo slovo?
fn vypada_jako_priklad(text: &str) -> bool {
    let ma_cislici = text.chars().any(|c| c.is_ascii_digit()) || text.contains("pi") || text.contains('π');
    let ma_operaci = text.chars().any(|c| "+-−*×·/÷^%!(".contains(c))
        || ["sqrt", "sin", "cos", "tan", "ln", "log", "abs", "round", "exp"].iter().any(|f| text.to_lowercase().contains(f));
    ma_cislici && ma_operaci
}

/// Výsledek příkladu, nebo None, když to příklad není.
pub fn spocitat(text: &str) -> Option<f64> {
    let text = text.trim().trim_end_matches('=');
    if !vypada_jako_priklad(text) {
        return None;
    }
    let mut p = Parser { znaky: text.chars().collect(), i: 0 };
    let h = p.vyraz()?;
    p.mezery();
    (p.i == p.znaky.len() && h.cislo.is_finite()).then_some(h.cislo)
}

/// Číslo hezky: bez zbytečných nul, nejvýš 10 platných číslic, s čárkou, když ji píše uživatel.
pub fn formatovat(x: f64, carka: bool) -> String {
    let text = if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        let platne = 10 - (x.abs().log10().floor() as i32 + 1).clamp(-5, 10);
        let t = format!("{:.*}", platne.max(0) as usize, x);
        let t = if t.contains('.') { t.trim_end_matches('0').trim_end_matches('.').to_string() } else { t };
        if t == "-0" { "0".into() } else { t }
    };
    if carka { text.replace('.', ",") } else { text }
}

#[cfg(test)]
mod testy {
    use super::*;

    #[test]
    fn priklady() {
        let s = |t: &str| spocitat(t).map(|x| formatovat(x, false));
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
