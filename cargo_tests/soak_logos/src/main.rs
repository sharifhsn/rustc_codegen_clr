//! H2 real-crate SOAK: logos (a derive-macro-driven lexer generator) on the dotnet PAL.
//! Exercises #[derive(Logos)] codegen: a generated DFA/jump-table state machine, the Lexer
//! iterator, &str slicing via spans, and the proc-macro-generated trait impl. Panic-safe:
//! fixed valid input string, all token results matched (no .unwrap()), counts printed.
//! SUCCESS = "== soak_logos done ==" with a token count + a couple of category checks.
use logos::Logos;

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[ \t\r\n]+")]
enum Token {
    #[token("let")]
    Let,

    #[token("=")]
    Eq,

    #[token(";")]
    Semi,

    #[token("+")]
    Plus,

    #[regex(r"[0-9]+")]
    Number,

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,
}

fn main() {
    println!("== soak_logos start ==");

    let src = "let x = 12 + foo; let y = x + 34;";
    let mut lex = Token::lexer(src);

    let mut total = 0usize;
    let mut errors = 0usize;
    let mut numbers = 0usize;
    let mut idents = 0usize;
    let mut keywords = 0usize;

    // Lexer yields Result<Token, ()>; match every item, never unwrap.
    while let Some(res) = lex.next() {
        total += 1;
        match res {
            Ok(tok) => {
                let span = lex.span();
                let slice = lex.slice();
                match tok {
                    Token::Number => numbers += 1,
                    Token::Ident => idents += 1,
                    Token::Let => keywords += 1,
                    _ => {}
                }
                // Touch the span so slicing codegen is exercised; print first few.
                if total <= 3 {
                    println!(
                        "  tok#{total} {:?} span={}..{} slice={:?}",
                        tok, span.start, span.end, slice
                    );
                }
            }
            Err(_) => errors += 1,
        }
    }

    println!("total tokens   = {total}");
    println!("numbers        = {numbers}");
    println!("idents         = {idents}");
    println!("keywords (let) = {keywords}");
    println!("errors         = {errors}");

    // Expected for the source above: 2 'let', 2 numbers (12, 34), idents x,foo,y,x.
    println!("keywords==2 ?  = {}", keywords == 2);
    println!("numbers==2  ?  = {}", numbers == 2);
    println!("no errors   ?  = {}", errors == 0);

    println!("== soak_logos done ==");
}
