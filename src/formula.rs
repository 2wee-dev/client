/// Simple arithmetic expression evaluator for grid column formulas.
/// Supports: +, -, *, /, parentheses, numeric literals, and column ID references.

use std::collections::HashMap;

#[derive(Debug)]
enum Token {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' => { chars.next(); }
            '+' => { chars.next(); tokens.push(Token::Plus); }
            '-' => { chars.next(); tokens.push(Token::Minus); }
            '*' => { chars.next(); tokens.push(Token::Star); }
            '/' => { chars.next(); tokens.push(Token::Slash); }
            '(' => { chars.next(); tokens.push(Token::LParen); }
            ')' => { chars.next(); tokens.push(Token::RParen); }
            '0'..='9' | '.' => {
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == '.' {
                        s.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Num(s.parse().unwrap_or(0.0)));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        s.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Ident(s));
            }
            _ => { chars.next(); } // skip unknown
        }
    }
    tokens
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let t = self.tokens.get(self.pos);
        self.pos += 1;
        t
    }

    // expr = term (('+' | '-') term)*
    fn expr(&mut self, vars: &HashMap<&str, f64>) -> f64 {
        let mut left = self.term(vars);
        loop {
            match self.peek() {
                Some(Token::Plus) => { self.advance(); left += self.term(vars); }
                Some(Token::Minus) => { self.advance(); left -= self.term(vars); }
                _ => break,
            }
        }
        left
    }

    // term = unary (('*' | '/') unary)*
    fn term(&mut self, vars: &HashMap<&str, f64>) -> f64 {
        let mut left = self.unary(vars);
        loop {
            match self.peek() {
                Some(Token::Star) => { self.advance(); left *= self.unary(vars); }
                Some(Token::Slash) => {
                    self.advance();
                    let right = self.unary(vars);
                    if right == 0.0 { left = 0.0; } else { left /= right; }
                }
                _ => break,
            }
        }
        left
    }

    // unary = '-' unary | primary
    fn unary(&mut self, vars: &HashMap<&str, f64>) -> f64 {
        if matches!(self.peek(), Some(Token::Minus)) {
            self.advance();
            -self.unary(vars)
        } else {
            self.primary(vars)
        }
    }

    // primary = Num | Ident | '(' expr ')'
    fn primary(&mut self, vars: &HashMap<&str, f64>) -> f64 {
        match self.advance() {
            Some(Token::Num(n)) => *n,
            Some(Token::Ident(name)) => {
                *vars.get(name.as_str()).unwrap_or(&0.0)
            }
            Some(Token::LParen) => {
                let val = self.expr(vars);
                self.advance(); // consume RParen
                val
            }
            _ => 0.0,
        }
    }
}

/// Evaluate a formula string with the given column values.
/// Returns the computed result, or 0.0 on any error.
pub fn evaluate(formula: &str, values: &HashMap<&str, f64>) -> f64 {
    let tokens = tokenize(formula);
    if tokens.is_empty() {
        return 0.0;
    }
    let mut parser = Parser::new(tokens);
    let result = parser.expr(values);
    if result.is_nan() || result.is_infinite() { 0.0 } else { result }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_arithmetic() {
        let vars = HashMap::new();
        assert_eq!(evaluate("2 + 3", &vars), 5.0);
        assert_eq!(evaluate("10 - 4", &vars), 6.0);
        assert_eq!(evaluate("3 * 4", &vars), 12.0);
        assert_eq!(evaluate("10 / 4", &vars), 2.5);
    }

    #[test]
    fn operator_precedence() {
        let vars = HashMap::new();
        assert_eq!(evaluate("2 + 3 * 4", &vars), 14.0);
        assert_eq!(evaluate("(2 + 3) * 4", &vars), 20.0);
    }

    #[test]
    fn variables() {
        let mut vars = HashMap::new();
        vars.insert("quantity", 5.0);
        vars.insert("unit_price", 100.0);
        vars.insert("line_discount_pct", 10.0);
        let result = evaluate("quantity * unit_price * (1 - line_discount_pct / 100)", &vars);
        assert_eq!(result, 450.0);
    }

    #[test]
    fn division_by_zero() {
        let vars = HashMap::new();
        assert_eq!(evaluate("10 / 0", &vars), 0.0);
    }

    #[test]
    fn missing_variable() {
        let vars = HashMap::new();
        assert_eq!(evaluate("missing_col * 5", &vars), 0.0);
    }
}
