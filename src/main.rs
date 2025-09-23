use std::{fmt::Display, io::Write, path::PathBuf};

use pest::{
    Parser,
    iterators::{Pair, Pairs},
};

use crate::emitters::{emit_der, emit_orm};

#[derive(pest_derive::Parser)]
#[grammar = "rules.pest"]
struct MyParser;

macro_rules! ensure_rule {
    ($tk:ident, $rule:expr) => {
        if $tk.as_rule() != $rule {
            return Err(Error {
                cause: format!("Expected {:?}, got {:?}", $rule, $tk.as_rule()),
            });
        }
    };
}

#[derive(Debug)]
struct Error {
    cause: String,
}

type ParseResult<T> = Result<T, Error>;

trait Parse: Sized {
    fn parse(tk: Token) -> ParseResult<Self>;
}
#[easy_ext::ext]
impl Pairs<'_, Rule> {
    fn next_item<T: Parse>(&mut self) -> ParseResult<T> {
        T::parse(self.next().ok_or_else(|| Error {
            cause: "Missing".into(),
        })?)
    }
}

#[derive(Debug, Clone, Copy)]
enum LinkN {
    One,
    MaybeOne,
    Many,
    MaybeMany,
}

impl Display for LinkN {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LinkN::One => "1",
                LinkN::MaybeOne => "(0,1)",
                LinkN::Many => "(1,N)",
                LinkN::MaybeMany => "(0,N)",
            }
        )
    }
}

impl Parse for LinkN {
    fn parse(tk: Token) -> ParseResult<Self> {
        ensure_rule!(tk, Rule::link_n);
        Ok(match tk.as_str().trim() {
            "1" => Self::One,
            "1?" => Self::MaybeOne,
            "n" => Self::Many,
            "n?" => Self::MaybeMany,

            _ => unreachable!("Unknown"),
        })
    }
}

impl Parse for String {
    fn parse(tk: Token) -> ParseResult<Self> {
        ensure_rule!(tk, Rule::name);
        Ok(tk.as_str().to_owned())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkBody {
    is_pk: bool,
}

impl Parse for LinkBody {
    fn parse(tk: Token) -> ParseResult<Self> {
        ensure_rule!(tk, Rule::ARROW_BODY);
        let is_pk = tk.as_str().contains('=');
        Ok(Self { is_pk })
    }
}

#[derive(Debug, Clone)]
struct Link {
    from: String,
    from_count: LinkN,
    body: LinkBody,
    to_count: LinkN,
    to: String,
    label: Option<String>,
}

impl Parse for Link {
    fn parse(tk: Token) -> ParseResult<Self> {
        ensure_rule!(tk, Rule::link);
        let mut tk = tk.into_inner();
        let from: String = tk.next_item()?;
        let from_count: LinkN = tk.next_item()?;
        let body: LinkBody = tk.next_item()?;
        let to_count: LinkN = tk.next_item()?;
        let to: String = tk.next_item()?;
        Ok(Self {
            from,
            from_count,
            body,
            to_count,
            to,
            label: tk.next_item().ok(),
        })
    }
}

#[derive(Debug, Clone)]
struct Field {
    field_type: String,
    name: String,
}

#[derive(Debug, Clone)]
pub struct Def {
    name: String,
    fields: Vec<Field>,
}

type Token<'a> = Pair<'a, Rule>;

impl Parse for Def {
    fn parse(tk: Token) -> ParseResult<Self> {
        ensure_rule!(tk, Rule::def);
        let mut def = tk.into_inner();
        let name: String = def.next_item()?;
        let mut fields = vec![];
        for field in def {
            let mut field = field.into_inner();
            fields.push(Field {
                field_type: field.next_item()?,
                name: field.next_item()?,
            })
        }
        let def = Def { name, fields };
        Ok(def)
    }
}

mod emitters;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
enum Mode {
    #[default]
    DER,
    ORM,
}

#[derive(argh::FromArgs)]
/// DiaLang compiler
struct Args {
    #[argh(positional)]
    input: PathBuf,
    #[argh(option, short = 'm', default = "Mode::default()")]
    /// output mode
    mode: Mode,
}

fn main() {
    let args = argh::from_env::<Args>();
    let file = std::fs::read_to_string(args.input).expect("Failed to open input file");
    let mut parser = MyParser::parse(Rule::document, &file).expect("Failed to parse input file");
    let doc = parser.next().unwrap();
    let mut links: Vec<Link> = vec![];
    let mut defs: Vec<Def> = vec![];
    for tk in doc.into_inner() {
        match tk.as_rule() {
            Rule::link => links.push(Link::parse(tk).unwrap()),
            Rule::def => defs.push(Def::parse(tk).unwrap()),
            Rule::EOI => break,
            _ => unreachable!("Got token {:?}", tk.as_rule()),
        }
    }
    let emitter = match args.mode {
        Mode::DER => emit_der,
        Mode::ORM => emit_orm,
    };
    emitter(&mut std::io::stdout(), links.clone(), defs.clone()).unwrap();
    emitter(
        &mut std::fs::File::create("out.dot").unwrap(),
        links.clone(),
        defs.clone(),
    )
    .unwrap();
}
