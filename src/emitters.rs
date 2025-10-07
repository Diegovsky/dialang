use std::{collections::HashMap, fmt::Display, io::Write, path::PathBuf};

use crate::*;

pub fn emit_der(f: &mut dyn Write, Doc { links, defs }: &Doc) -> std::io::Result<()> {
    writeln!(f, "graph {{")?;
    writeln!(f, "node [shape=plaintext];")?;
    for def in defs {
        let name = &def.name;
        writeln!(
            f,
            r#"{name} [label=<
            <TABLE border="0" cellborder="1" cellspacing="0">
            <TR><TD colspan="2" bgcolor="gray">{name}</TD></TR>"#
        )?;
        for Field { field_type, name } in &def.fields {
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
        ..
    } in links
    {
        let label = label.as_deref().unwrap_or_default();
        let id = format!("{from}_{to}_{label}");
        writeln!(f, "\t{id} [label=<{label}>];")?;
        writeln!(f, "\t{from} -- {id} [taillabel=<{from_count}>];")?;
        writeln!(f, "\t{id} -- {to}   [headlabel=<{to_count}>];")?;
    }
    writeln!(f, "}}")?;
    Ok(())
}

fn table_fields(f: &mut dyn Write, fields: &[&str]) -> std::io::Result<()> {
    writeln!(f, "<TR>")?;
    for field in fields {
        writeln!(f, r#"<TD align="LEFT">{field}</TD>"#)?;
    }
    writeln!(f, "</TR>")?;
    Ok(())
}

pub fn emit_orm(f: &mut dyn Write, Doc { links, defs }: &Doc) -> std::io::Result<()> {
    writeln!(f, "digraph {{")?;
    writeln!(f, "graph [layout=dot];")?;
    writeln!(f, "node [shape=plaintext];")?;
    let mut def_links: HashMap<&str, Vec<&Link>> =
        defs.iter().map(|def| (&*def.name, vec![])).collect();
    for link in links {
        def_links.get_mut(&*link.from).unwrap().push(link)
    }
    let def_links = def_links;
    for def in defs {
        let name = def.name.as_str();
        writeln!(
            f,
            r#"{name} [label=<
            <TABLE border="1" ALIGN="LEFT" cellborder="0" cellspacing="0">
            <TR><TD colspan="2" border="1">{name}</TD></TR>"#
        )?;
        // writeln!(f, r#"<TR><TD ALIGN="LEFT" BALIGN="LEFT">"#)?;
        for Field { name, .. } in &def.fields {
            let name = name.as_str();
            table_fields(
                f,
                &[
                    match name {
                        "id" => "pk",
                        _ => "",
                    },
                    &format!("+{name}"),
                ],
            )?;
        }
        for Link { to, body, .. } in &def_links[name] {
            let name = format!("+{}_id", to.to_lowercase());

            table_fields(f, &[if body.is_pk { "fk_pk" } else { "fk" }, &*name])?;
        }
        writeln!(f, "</TABLE> >];")?;
        // writeln!(f, "</TD></TR></TABLE> >];")?;
    }
    for Link {
        from, to_count, to, ..
    } in links
    {
        writeln!(
            f,
            "\t{from} -> {to} [headlabel=<{to_count}>
            labelangle=45 labeldistance=2.1];"
        )?;
    }
    writeln!(f, "}}")?;
    Ok(())
}
