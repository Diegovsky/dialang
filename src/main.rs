use std::{
    fmt::Display,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, bail};
use notify::Watcher;
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

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl std::error::Error for Error {}

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
    /// input mode
    mode: Mode,

    #[argh(positional)]
    /// output file. provide a `.png` or `.svg` to automatically pass through `dot`
    output: Option<PathBuf>,

    #[argh(switch, short = 'w')]
    /// enables watching file for changes
    watch: bool,
}

struct Doc {
    links: Vec<Link>,
    defs: Vec<Def>,
}

fn parse_doc(path: &Path) -> anyhow::Result<Doc> {
    let file = std::fs::read_to_string(path).context("Failed to open input file")?;
    let mut parser =
        MyParser::parse(Rule::document, &file).context("Failed to parse input file")?;
    let doc = parser.next().unwrap();
    let mut links: Vec<Link> = vec![];
    let mut defs: Vec<Def> = vec![];
    for tk in doc.into_inner() {
        match tk.as_rule() {
            Rule::link => links.push(Link::parse(tk)?),
            Rule::def => defs.push(Def::parse(tk)?),
            Rule::EOI => break,
            _ => unreachable!("Got token {:?}", tk.as_rule()),
        }
    }
    Ok(Doc { links, defs })
}

fn app(args: &Args) -> anyhow::Result<()> {
    let doc = parse_doc(&args.input)?;
    let emitter = match args.mode {
        Mode::DER => emit_der,
        Mode::ORM => emit_orm,
    };
    match &args.output {
        None => emitter(&mut std::io::stdout(), &doc),
        Some(path) => {
            let mut out_file = std::fs::File::create(path).map(BufWriter::new)?;
            if let Some(ext) = path.extension().and_then(|ext| ext.to_str())
                && matches!(ext, "svg" | "png")
            {
                let mut dot = Command::new("dot")
                    .args(&[&format!("-T{ext}")])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                    .context("dot not found. can't output images")?;
                emitter(&mut dot.stdin.take().unwrap(), &doc)?;
                if !dot.wait()?.success() {
                    bail!("Dot failed.");
                };

                let mut cmd_output = dot.stdout.take().unwrap();
                std::io::copy(&mut cmd_output, &mut out_file)?;
                Ok(())
            } else {
                emitter(&mut out_file, &doc)
            }
        }
    }?;
    Ok(())
}

fn watch(args: Args) -> anyhow::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;

    watcher.watch(Path::new("."), notify::RecursiveMode::NonRecursive)?;

    let input = args.input.to_str().context("Not utf-8 path")?;
    if let Err(e) = app(&args) {
        eprintln!("Error: {e}")
    }
    for res in rx {
        let ev = match res {
            Err(e) => {
                eprintln!("Error watching stuff: {e}");
                break;
            }
            Ok(ev) => ev,
        };
        if !(ev.kind.is_create() || ev.kind.is_modify()) {
            continue;
        }

        if !ev.paths.iter().any(|path| {
            path.to_str()
                .map(|path| path.contains(input))
                .unwrap_or(false)
        }) {
            continue;
        }

        if let Err(e) = app(&args) {
            eprintln!("Error: {e}")
        }
    }
    Ok(())
}

fn main() {
    let args = argh::from_env::<Args>();
    if args.watch {
        watch(args).unwrap()
    } else {
        app(&args).unwrap()
    }
}
