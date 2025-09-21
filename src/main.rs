use std::{fmt::Display, io::Write};

use pest::{
    Parser,
    iterators::{Pair, Pairs},
};

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
}

impl Display for LinkN {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LinkN::One => "1",
                LinkN::MaybeOne => "(0,1)",
                LinkN::Many => "(0,N)",
            }
        )
    }
}

impl Parse for LinkN {
    fn parse(tk: Token) -> ParseResult<Self> {
        ensure_rule!(tk, Rule::link_n);
        Ok(match tk.as_str() {
            "1" => Self::One,
            "n" => Self::Many,
            _ => Self::MaybeOne,
        })
    }
}

impl Parse for String {
    fn parse(tk: Token) -> ParseResult<Self> {
        ensure_rule!(tk, Rule::name);
        Ok(tk.as_str().to_owned())
    }
}
#[derive(Debug, Clone)]
struct Link {
    from: String,
    from_count: LinkN,
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
        let to_count: LinkN = tk.next_item()?;
        let to: String = tk.next_item()?;
        Ok(Self {
            from,
            from_count,
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
                name: field.next_item()?,
                field_type: field.next_item()?,
            })
        }
        let def = Def { name, fields };
        Ok(def)
    }
}

fn emit_graph(f: &mut dyn Write, links: Vec<Link>, defs: Vec<Def>) -> std::io::Result<()> {
    writeln!(f, "graph {{")?;
    writeln!(f, "node [shape=plaintext];")?;
    for def in defs {
        let name = def.name;
        writeln!(
            f,
            r#"{name} [label=<
            <TABLE border="0" cellborder="1" cellspacing="0">
            <TR><TD colspan="2" bgcolor="gray">{name}</TD></TR>"#
        )?;
        for Field { field_type, name } in def.fields {
            writeln!(f, "<TR><TD>{field_type}</TD><TD>{name}</TD></TR>")?;
        }
        writeln!(f, "</TABLE> >];")?;
    }
    writeln!(f, "node [shape=diamond, fontsize=11];")?;
    for Link {
        from,
        from_count,
        to_count,
        to,
        label,
    } in links
    {
        let label = label.unwrap_or_default();
        let id = format!("{from}_{to}_{label}");
        writeln!(f, "\t{id} [label=<{label}>];")?;
        writeln!(f, "\t{from} -- {id} [taillabel=<{from_count}>];")?;
        writeln!(f, "\t{id} -- {to}   [headlabel=<{to_count}>];")?;
    }
    writeln!(f, "}}")?;
    Ok(())
}

fn main() {
    let file = std::fs::read_to_string(std::env::args().nth(1).expect("Missing input file"))
        .expect("Failed to open input file");
    let mut parser = MyParser::parse(Rule::document, &file).expect("Failed to parse input file");
    let doc = parser.next().unwrap();
    let mut links: Vec<Link> = vec![];
    let mut defs: Vec<Def> = vec![];
    for tk in doc.into_inner() {
        match tk.as_rule() {
            Rule::link => links.push(Link::parse(tk).unwrap()),
            Rule::def => defs.push(Def::parse(tk).unwrap()),
            _ => unreachable!(),
        }
    }
    emit_graph(&mut std::io::stdout(), links.clone(), defs.clone()).unwrap();
    emit_graph(&mut std::io::stderr(), links.clone(), defs.clone()).unwrap();
}
